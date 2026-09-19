"""
World in Conflict Replay Parser v6.0

Extracts player names, scores, team assignments, and game metadata from .wicdemo files.

Features:
- Player information: names (with Unicode support), IDs, scores, and team/faction
- Team detection via BinTag event stream (PlayerJoinedTeam / UnitCreate events)
- Faction identification: 0=Spectator, 1=USA, 2=NATO, 3=USSR
- Game metadata: map name, server, date/time, mode, duration
- Accurate score extraction for 2-16 players
- Human-readable map names (e.g., "do_Hometown" instead of "maps/ustown1/ustown1.ice")
"""

import struct
import zlib
import re
from typing import Dict, List, Optional, Tuple
from dataclasses import dataclass, field
from pathlib import Path


# Map ID to human-readable name translation with game mode prefixes.
# The community table is the compatibility baseline, with verified corrections
# and revision paths normalized to their canonical public name. Unknown maps stay raw.
# Format: do_ = Domination, as_ = Assault, tw_ = Tug of War
MAP_NAMES = {
    "maps/europe4/europe4.ice": "do_Riviera",
    "maps/ustown4/ustown4.ice": "do_Seaside",
    "maps/russia2/russia2.ice": "do_Powerplant",
    "maps/ustown1/ustown1.ice": "do_Hometown",
    "maps/usfarmland1/usfarmland1.ice": "do_Farmland",
    "maps/seattle1/seattle1.ice": "do_SpaceNeedle",
    "maps/seattle3/seattle3.ice": "as_Bridge",
    "maps/russia3/russia3.ice": "do_Quarry",
    "maps/ustown3/ustown3.ice": "do_Xmas",
    "maps/usfarmland5/usfarmland5.ice": "do_Countryside",
    "maps/newyork1/newyork1.ice": "do_Liberty",
    "maps/seattle4/seattle4.ice": "do_Island",
    "maps/usdesert1/usdesert1.ice": "do_Silo",
    "maps/europe1/europe1.ice": "do_Ruins",
    "maps/usdesert2/usdesert2.ice": "as_AirBase",
    "maps/usfarmland3/usfarmland3.ice": "do_Riverbed",
    "maps/europe3/europe3.ice": "do_Vineyard",
    "maps/do_airport/do_airport.ice": "do_Airport",
    "maps/do_paradise/do_paradise.ice": "do_Paradise",
    "maps/do_wake/do_wake.ice": "do_Wake",
    "maps/europe2/europe2.ice": "as_Hillside",
    "maps/seattle2/seattle2.ice": "as_Dome",
    "maps/europe5/europe5.ice": "do_Canal",
    "maps/berlin1/berlin1.ice": "do_Mauer",
    "maps/do_apocalypse/do_apocalypse.ice": "do_Apocalypse",
    "maps/do_studio/do_studio.ice": "do_Studio",
    "maps/norway1/norway1.ice": "do_Fjord",
    "maps/do_tequila/do_tequila.ice": "do_Tequila",
    "maps/virginia/virginia.ice": "do_Virginia",
    "maps/as_ozzault/as_ozzault.ice": "as_Ozzault",
    "maps/russia4/russia4.ice": "as_Typhoon",
    "maps/caspian_border_chepoint2/caspian_border_chepoint2.ice": "do_CaspianWinter",
    "maps/caspian_border_chepoint1/caspian_border_chepoint1.ice": "do_Caspianborder",
    "maps/do_valley/do_valley.ice": "do_Valley",
    "maps/do_alaska/do_alaska.ice": "do_Alaska",
    "maps/do_caspianborder/do_caspianborder.ice": "do_Caspianborder",
    "maps/do_farmland_night/do_farmland_night.ice": "do_Farmland_Night",
    "maps/do_hometown_night/do_hometown_night.ice": "do_Hometown_Night",
    "maps/do_powerplant_night/do_powerplant_night.ice": "do_Powerplant_Night",
    "maps/do_riverport/do_riverport.ice": "do_Riverport",
    "maps/do_riviera_night/do_riviera_night.ice": "do_Riviera_Night",
    "maps/do_seaside_night/do_seaside_night.ice": "do_Seaside_Night",
    "maps/tw_arizona/tw_arizona.ice": "tw_Arizona",
    "maps/tw_bocage/tw_bocage.ice": "tw_Bocage",
    "maps/russia1/russia1.ice": "tw_Radar",
    "maps/usfarmland4/usfarmland4.ice": "tw_Highway",
    "maps/airport_03/airport_03.ice": "do_Airport",
    "maps/bllack_forest/bllack_forest.ice": "do_BlackForest",
    "maps/airport_v2/airport_v2.ice": "do_Airport",
    "maps/usfarmland2/usfarmland2.ice": "tw_Wasteland",
    "maps/helgoland/helgoland.ice": "do_Helgoland",
    "maps/do_wakebeta2/do_wakebeta2.ice": "do_Wake",
}


# BinTag hash constants (computed from wic.exe hash function at VMA 0xa03830)
# Event container hashes
_HASH_PLAYER_JOINED_TEAM = b"\x4e\x06\x8e\x35"  # "PlayerJoinedTeam"
_HASH_SPECTATOR_JOINED_TEAM = b"\x96\x07\x2e\x4c"  # "SpectatorJoinedTeam"
_HASH_PLAYER_ENTERS_GAME = b"\x59\x06\xd5\x35"  # "PlayerEntersGame"
_HASH_PLAYER_LEAVES_GAME = b"\x48\x06\x5b\x35"  # "PlayerLeavesGame"
_HASH_PLAYER_SET_ROLE = b"\x2c\x05\xe8\x23"  # "PlayerSetRole"
_HASH_ASPECTATOR_LOS = b"\x45\x05\x42\x24"  # "aSpectatorLOS"
_HASH_UNIT_CREATE = b"\xf5\x03\x7e\x15"  # "UnitCreate"
_HASH_TEAM_WINS = b"\x29\x03\xb8\x0d"  # "TeamWins"
# Field hashes
_HASH_AFACTOR = b"\xc1\x02\x6f\x0a"  # "aFactor" (domination bar position, 0.0-1.0)
_HASH_ASLOT = b"\x04\x02\xcc\x05"  # "aSlot"
_HASH_ATEAM = b"\xe9\x01\x98\x05"  # "aTeam"
_HASH_AROLE_ID = b"\xa1\x02\x9a\x0a"  # "aRoleId"
_HASH_APLAYER = b"\xcf\x02\xd5\x0a"  # "aPlayer"
_HASH_SCORE = b"\xcb\x04\xa1\x1e"  # score BinTag field hash
_HASH_APOS = b"\x94\x01\xc9\x03"  # "aPos" (0-based player index in score summary)
_HASH_SCORE_ROLE0 = b"\x20\x04\x77\x19"  # "aScoreRole0" = Infantry
_HASH_SCORE_ROLE1 = b"\x21\x04\x78\x19"  # "aScoreRole1" = Support
_HASH_SCORE_ROLE2 = b"\x22\x04\x79\x19"  # "aScoreRole2" = Armor
_HASH_SCORE_ROLE3 = b"\x23\x04\x7a\x19"  # "aScoreRole3" = Air
_HASH_CAPTURING_SCORE = b"\x0b\x06\x9a\x2f"
_HASH_FORTIFICATION_SCORE = b"\xaf\x07\x1a\x4c"
_HASH_TRANSPORTATION_SCORE = b"\x46\x08\x91\x56"
_HASH_REPAIR_SCORE = b"\xc1\x04\x54\x1e"
_HASH_BRIDGE_LAYING_SCORE = b"\x0f\x07\xc0\x41"
_HASH_UNIT_DAMAGE_SCORE = b"\x3d\x06\x67\x34"
_HASH_TACTICAL_AID_SCORE = b"\x91\x06\x82\x3a"
_HASH_TOTAL_SCORE = b"\x62\x04\xf5\x19"
_HASH_POV_PLAYER = b"\xad\x07\x93\x4b"  # Recorder's aSlot value in metadata
# Name-bearing type-5 field inside PlayerEntersGame.
_HASH_PLAYER_ENTRY_NAME = b"\xa2\x01\x1e\x04"
# "SetGameModeData_Float", with an underscore: the literal "SetGameModeDataFloat"
# hashes to 0x4eb1079c, which appears in no replay. Confirmed against the client's
# own writer; see the parent workspace's bintag-message-inventory findings.
_HASH_SET_GAME_MODE_DATA_FLOAT = b"\xfb\x07\x91\x56"
_HASH_ADATA_TYPE = b"\x7e\x03\xd6\x10"  # "aDataType" (1 = countdown clock)
_HASH_AFLOAT = b"\x58\x02\xdd\x07"  # "aFloat"
_HASH_EVENT = b"\x03\x02\xb5\x05"  # "Event" envelope tag
_ENVELOPE_TYPE = b"\x15\x00\x00\x00"
_ENVELOPE_ARRAY_FLAG = 0x06
# Bytes from an envelope's start to its message-name hash.
_ENVELOPE_MESSAGE_OFFSET = 17

# Configured Domination round lengths observed in the corpus, ascending.
# Used to recover the round length when a recording joined mid-match and
# therefore never observed the countdown's starting value.
STANDARD_ROUND_LENGTHS = (600.0, 900.0, 1200.0, 1800.0)
# Clock remaining at or below this means the countdown expired.
# A countdown jump larger than this means a new timed round started.
CLOCK_RESET_THRESHOLD_SECONDS = 5.0
# A backward step in envelope time larger than this starts a new pass: after
# TeamWins the recorder writes a second complete pass over the match, the
# end-of-match summary, whose clock restarts at zero. The envelope clock is
# otherwise strictly monotonic — a 156-replay scan found zero backward steps of
# any size before that reset.
RECORDING_RESET_DROP_SECONDS = 1.0
# Envelopes that must chain end-to-end before a candidate offset is accepted as
# the start of the event chain.
_CHAIN_ANCHOR_CONFIRM_LINKS = 6
TIMEOUT_REMAINING_SECONDS = 2.0
# The bar is pinned to a stop, i.e. one team reached total domination.
DOMINATION_EXTREME_EPSILON = 0.0005
_HASH_MY_GAME_NAME = b"\xe2\x03\x8d\x15"  # "myGameName" server/session title
_HASH_REPLAY_NAME = b"\xef\x03\x7c\x15"  # "ReplayName" in-game replay title
_HASH_MY_FPM_MODE_FLAG = b"\xc9\x04\x60\x21"
_HASH_MY_MATCH_MODE_FLAG = b"\xd3\x05\x4e\x2f"
_HASH_MY_IS_TOURNAMENT_MATCH_FLAG = b"\x37\x09\x3f\x70"
_HASH_MY_IS_CLAN_MATCH_FLAG = b"\x88\x06\x25\x3b"
_HASH_MY_TYPE = b"\x89\x02\xf1\x08"
# BinTag int32 separator
_BINTAG_SEP = b"\x11\x00\x00\x00"

# Faction ID to name mapping (determined from map analysis:
# team 1 appears on American maps, team 2 on European maps, team 3 always)
FACTION_NAMES = {0: "Spectator", 1: "USA", 2: "NATO", 3: "USSR"}


def format_share(pct: float) -> str:
    """Percentage text carrying every digit the bar actually resolves.

    The bar is quantised to 1/3000, i.e. steps of 0.0333%, so rounding to whole
    percent collapses a real result like 50.07 / 49.93 into a false 50 / 50 tie.
    Two decimals is the game's own precision; trailing zeros are dropped so a
    clean value still reads as "58" rather than "58.00".
    """
    text = f"{pct * 100:.2f}"
    return text.rstrip("0").rstrip(".") if "." in text else text


def publishes_control_split(game_mode: str) -> bool:
    """Whether aFactor is a two-sided control split for this mode.

    True for Domination (continuous lead bar) and Tug of War (discrete front
    line). False for Assault, whose value tracks attacker progress instead.
    """
    return game_mode in ("Domination", "Tug of War")


MATCH_ENDING_LABELS = {
    "totalDomination": "total domination",
    "timeout": "on the timer",
    "forfeit": "forfeit",
    "unknown": "unknown ending",
}


@dataclass
class Player:
    """Represents a player in the game"""

    id: int
    name: str
    team: Optional[int] = None
    faction: Optional[str] = None
    score: Optional[int] = None
    role: Optional[str] = None
    score_infantry: Optional[int] = None
    score_support: Optional[int] = None
    score_armor: Optional[int] = None
    score_air: Optional[int] = None
    score_capturing: Optional[int] = None
    score_fortification: Optional[int] = None
    score_transportation: Optional[int] = None
    score_repair: Optional[int] = None
    score_bridge_laying: Optional[int] = None
    score_unit_damage: Optional[int] = None
    score_tactical_aid: Optional[int] = None
    score_total: Optional[int] = None
    left_at_seconds: Optional[float] = None


@dataclass
class GameInfo:
    """Represents game metadata"""

    map_name: str  # Internal map path (e.g., "maps/ustown1/ustown1.ice")
    map_display_name: str  # Human-readable name (e.g., "Hometown")
    server_name: str
    date_time: str
    replay_name: Optional[str]
    game_mode: str = "Unknown"


@dataclass
class RawServerFlags:
    """Exact boolean-like values serialized in DemoHeader."""

    few_player_mode_flag: Optional[bool]
    match_mode_flag: Optional[bool]
    tournament_match_flag: Optional[bool]
    clan_match_flag: Optional[bool]


@dataclass
class ServerClassification:
    """Orthogonal server properties supported by replay evidence."""

    few_player_mode: Optional[bool]
    match_mode: Optional[bool]
    has_bots: Optional[bool]
    clan_match: Optional[bool]
    tournament_match: Optional[bool]
    ranked: Optional[bool]


@dataclass
class MatchTiming:
    """Match timing separated from recording timing.

    A recording that joins late still observes the true remaining clock, but not
    how long the match had already been running, so ``observed_gameplay_seconds``
    and ``match_elapsed_seconds`` differ for a late join.

    ``recording_seconds`` is a different question again: the real length of the
    replay file, read from the ``Event`` envelope clock. It counts the pre-match
    lobby and the post-``TeamWins`` tail, neither of which the countdown sees.
    """

    captured_match_start: bool = False
    recording_seconds: float = 0.0
    observed_gameplay_seconds: Optional[float] = None
    match_elapsed_seconds: Optional[float] = None
    round_length_seconds: Optional[float] = None
    round_length_exact: bool = False
    joined_at_remaining_seconds: Optional[float] = None
    final_remaining_seconds: Optional[float] = None


@dataclass
class DominationShare:
    """One faction's share of the final domination bar."""

    faction: str
    pct: float


@dataclass
class ReplayData:
    """Complete replay data"""

    game_info: GameInfo
    raw_server_flags: RawServerFlags
    server_classification: ServerClassification
    players: List[Player]
    player_scores: List[int]
    duration_seconds: Optional[float] = None
    timing: MatchTiming = field(default_factory=MatchTiming)
    # "totalDomination" | "timeout" | "forfeit" | "unknown"
    match_ending: str = "unknown"
    winner_domination_pct: Optional[float] = None
    loser_domination_pct: Optional[float] = None
    domination_shares: Optional[List[DominationShare]] = None
    # "povTeam" | "winnerInferred"
    domination_anchor: Optional[str] = None
    winner: Optional[str] = None
    incomplete: bool = False
    recorder: Optional[str] = None


class WICReplayParserV4:
    """
    Parser for World in Conflict .wicdemo replay files

    File Format:
    - First 19 bytes: Raw header
    - Byte 19+: zlib-compressed chunks (16KB each when decompressed)
    - First chunk: Metadata (map, server, players)
    - Remaining chunks: Game events (including score updates)
    """

    def __init__(self, filepath: str):
        self.filepath = filepath
        self.raw_data = None
        self.metadata_chunk = None  # First 16KB chunk
        self.full_data = None  # All decompressed data (lazy-loaded)
        self.decompression_gaps = []

    def read_file(self) -> bytes:
        """Read the raw replay file"""
        path = Path(self.filepath)
        if path.suffix.lower() != ".wicdemo":
            raise ValueError(f"Replay path must end in .wicdemo: {path}")
        if self.raw_data is None:
            self.raw_data = path.read_bytes()
        return self.raw_data

    def get_metadata_chunk(self) -> bytes:
        """
        Extract first 16KB metadata chunk
        Uses community-discovered method: zlib.decompress(data[19:])
        """
        if self.metadata_chunk is None:
            data = self.read_file()
            # Skip 19-byte header, decompress first chunk
            self.metadata_chunk = zlib.decompress(data[19:])
        return self.metadata_chunk

    def get_full_decompressed_data(self) -> bytes:
        """
        Decompress all chunks into one continuous byte stream
        Lazy-loaded, only called when needed for score extraction
        """
        if self.full_data is not None:
            return self.full_data

        data = self.read_file()
        all_data = b""
        offset = 19

        while offset < len(data):
            if data[offset : offset + 2] in [b"\x78\xda", b"\x78\x9c", b"\x78\x01"]:
                try:
                    dobj = zlib.decompressobj()
                    decompressed = dobj.decompress(data[offset:])
                    compressed_size = len(data[offset:]) - len(dobj.unused_data)
                    all_data += decompressed
                    offset += compressed_size
                except Exception:
                    if not self.decompression_gaps or self.decompression_gaps[
                        -1
                    ] != len(all_data):
                        self.decompression_gaps.append(len(all_data))
                    offset += 1
            else:
                offset += 1

        self.full_data = all_data
        return all_data

    def extract_map_name(self) -> str:
        """
        Extract map name from fixed offset 47
        Community-discovered: map name ALWAYS starts at byte 47
        Format: null-terminated ASCII string "maps/.../.ice"
        """
        chunk = self.get_metadata_chunk()

        # Find null terminator starting from offset 47
        map_start = 47
        map_end = chunk.find(0, map_start)

        if map_end > map_start:
            return chunk[map_start:map_end].decode("ascii", errors="ignore")

        return "Unknown"

    def extract_utf16_strings(self) -> List[Tuple[int, str]]:
        """
        Extract all UTF-16 encoded strings from metadata chunk
        Returns list of (offset, string) tuples
        """
        chunk = self.get_metadata_chunk()
        utf16_pattern = re.compile(b"((?:[\x20-\x7e]\x00){3,60})\x00\x00", re.DOTALL)

        strings = []
        for match in utf16_pattern.finditer(chunk):
            try:
                utf16_str = match.group(1).decode("utf-16le")
                if utf16_str.isprintable():
                    offset = match.start()
                    strings.append((offset, utf16_str))
            except Exception:
                pass

        return strings

    @staticmethod
    def _read_bintag_utf16_string(
        data: bytes,
        position: int,
        expected_hash: bytes,
        trim_trailing_nbsp: bool = False,
    ) -> Optional[str]:
        """Decode one structurally validated variable-length UTF-16 BinTag field."""
        if position < 0 or position + 15 > len(data):
            return None
        if data[position : position + 4] != expected_hash:
            return None
        total = struct.unpack("<I", data[position + 4 : position + 8])[0]
        repeated_total = struct.unpack("<I", data[position + 9 : position + 13])[0]
        end = position + total
        if (
            total < 15
            or data[position + 8] != 5
            or repeated_total != total
            or end > len(data)
        ):
            return None
        payload = data[position + 13 : end]
        if len(payload) < 2 or len(payload) % 2 or not payload.endswith(b"\x00\x00"):
            return None
        try:
            value = payload[:-2].decode("utf-16le", errors="strict")
        except UnicodeDecodeError:
            return None
        if trim_trailing_nbsp:
            value = value.rstrip("\xa0")
        printable = value.replace("\xa0", " ") if trim_trailing_nbsp else value
        if not value or not printable.isprintable() or "\x00" in value:
            return None
        return value

    @staticmethod
    def _read_utf16le_name(
        data: bytes, start: int, max_chars: int = 30
    ) -> Optional[str]:
        """Read a null-terminated UTF-16LE string at the given offset.

        Returns the decoded name if valid (printable, 3-30 chars), else None.
        """
        limit = min(start + max_chars * 2, len(data) - 1)
        for end in range(start, limit, 2):
            if data[end] == 0 and data[end + 1] == 0:
                if end == start:
                    return None
                try:
                    name = data[start:end].decode("utf-16le", errors="strict")
                    name = name.rstrip(
                        "\xa0"
                    )  # Strip non-breaking space padding (older replays)
                    if (
                        name.replace("\xa0", " ").isprintable()
                        and len(name) >= 3
                        and "\x00" not in name
                    ):
                        return name
                except (UnicodeDecodeError, UnicodeError):
                    pass
                return None
        return None

    def _extract_slot_names(
        self, data: bytes, start: int = 0, target_ids: Optional[set] = None
    ) -> Dict[int, str]:
        """
        Extract player slot -> name mapping using aSlot BinTag structure.

        Each aSlot entry has the player's slot ID at +13 and UTF-16LE name at +47.
        This is a structural, deterministic approach — no regex heuristics.

        Args:
            data: byte buffer to search
            start: offset to start searching from
            target_ids: if provided, only extract these slot IDs

        Returns:
            Dict mapping slot ID (0-15) to player name
        """
        slot_names: Dict[int, str] = {}
        offset = start

        while True:
            pos = data.find(_HASH_ASLOT, offset)
            if pos == -1:
                break

            if pos + 55 <= len(data) and data[pos + 4 : pos + 8] == _BINTAG_SEP:
                slot = struct.unpack("<I", data[pos + 13 : pos + 17])[0]

                if slot <= 15 and slot not in slot_names:
                    if target_ids is None or slot in target_ids:
                        # Try reading name at fixed offset +47
                        name = self._read_utf16le_name(data, pos + 47)

                        # Fallback: scan forward for name (handles rare bot entries)
                        if name is None and pos + 200 <= len(data):
                            for scan in range(
                                pos + 48, min(pos + 200, len(data) - 60), 2
                            ):
                                if (
                                    0x20 <= data[scan] <= 0xFF
                                    and data[scan + 1] == 0x00
                                ):
                                    name = self._read_utf16le_name(data, scan)
                                    if name:
                                        break

                        if name:
                            slot_names[slot] = name

            offset = pos + 1

            # Early exit if all targets found
            if target_ids and len(slot_names) >= len(target_ids):
                break

        return slot_names

    def _apply_lobby_swaps(self, slot_names: Dict[int, str]) -> None:
        """
        Detect lobby slot swaps by scanning early event stream for aSlot entries
        with names that differ from metadata. Only applies confirmed bilateral
        swaps (slot A has B's name AND slot B has A's name).

        When players swap team slots in the lobby, the metadata chunk retains the
        pre-swap name mapping, but the event stream records the corrected names.
        """
        full = self.get_full_decompressed_data()
        meta_end = len(self.get_metadata_chunk())
        # Lobby events are in the first ~1% of event data
        lobby_end = min(meta_end + 200_000, len(full))

        # Collect event stream names that differ from metadata
        event_names: Dict[int, str] = {}
        offset = meta_end
        while offset < lobby_end:
            pos = full.find(_HASH_ASLOT, offset)
            if pos == -1 or pos >= lobby_end:
                break
            if pos + 55 <= len(full) and full[pos + 4 : pos + 8] == _BINTAG_SEP:
                slot = struct.unpack("<I", full[pos + 13 : pos + 17])[0]
                if slot <= 15:
                    name = self._read_utf16le_name(full, pos + 47)
                    if name and name != slot_names.get(slot):
                        event_names[slot] = name
            offset = pos + 1

        # Only apply confirmed bilateral swaps: slot A has B's metadata name
        # and slot B has A's metadata name
        applied: set = set()
        for slot_a, event_name_a in event_names.items():
            if slot_a in applied:
                continue
            for slot_b, event_name_b in event_names.items():
                if slot_b <= slot_a or slot_b in applied:
                    continue
                meta_a = slot_names.get(slot_a)
                meta_b = slot_names.get(slot_b)
                if (
                    meta_a
                    and meta_b
                    and event_name_a == meta_b
                    and event_name_b == meta_a
                ):
                    slot_names[slot_a] = event_name_a
                    slot_names[slot_b] = event_name_b
                    applied.add(slot_a)
                    applied.add(slot_b)

    def _resolve_match_roster_names(
        self, static_names: Dict[int, str], scored_slots: set
    ) -> Dict[int, str]:
        """Resolve scored slot occupants at the first gameplay clock sample.

        Metadata is a lobby snapshot. A structurally valid PlayerLeavesGame /
        PlayerEntersGame sequence before gameplay can supersede it for scored
        slots. Unscored, unresolved, or temporarily vacant slots retain their
        static names, and later replacements do not relabel accumulated scores.
        """
        data = self.get_full_decompressed_data()
        anchor = self._find_chain_anchor(data)
        if anchor is None:
            return static_names

        active_names = static_names.copy()
        tracked_slots = set(static_names)

        cursor = anchor
        envelope_seconds = 0.0
        while True:
            envelope = self._read_envelope(data, cursor)
            if envelope is None:
                return static_names
            end, time_seconds = envelope
            if time_seconds < envelope_seconds - RECORDING_RESET_DROP_SECONDS:
                return static_names
            envelope_seconds = max(envelope_seconds, time_seconds)
            message_pos = cursor + _ENVELOPE_MESSAGE_OFFSET
            message = data[message_pos : message_pos + 4]

            if message == _HASH_SET_GAME_MODE_DATA_FLOAT:
                data_type = (
                    self._read_bintag_u32(data, message_pos + 4)
                    if data[message_pos + 4 : message_pos + 8] == _HASH_ADATA_TYPE
                    else None
                )
                remaining = (
                    self._read_bintag_float(data, message_pos + 21)
                    if data[message_pos + 21 : message_pos + 25] == _HASH_AFLOAT
                    else None
                )
                if (
                    data_type == 1
                    and remaining is not None
                    and remaining != 0.0
                    and abs(remaining) <= 86400.0
                ):
                    roster_names = static_names.copy()
                    roster_names.update(
                        {
                            slot: active_names[slot]
                            for slot in scored_slots
                            if slot in active_names
                        }
                    )
                    return self._correct_late_lobby_names(
                        roster_names,
                        scored_slots - active_names.keys(),
                        end,
                        envelope_seconds,
                    )
            elif message == _HASH_PLAYER_LEAVES_GAME:
                slot = (
                    self._read_bintag_u32(data, message_pos + 4)
                    if data[message_pos + 4 : message_pos + 8] == _HASH_ASLOT
                    else None
                )
                if slot in tracked_slots:
                    active_names.pop(slot, None)
            elif message == _HASH_PLAYER_ENTERS_GAME:
                slot = (
                    self._read_bintag_u32(data, message_pos + 4)
                    if data[message_pos + 4 : message_pos + 8] == _HASH_ASLOT
                    else None
                )
                if slot in tracked_slots:
                    unknown_pos = message_pos + 21
                    name_pos = unknown_pos + 17
                    framed = (
                        unknown_pos + 17 <= end
                        and data[unknown_pos + 4 : unknown_pos + 8] == _BINTAG_SEP
                        and data[unknown_pos + 9 : unknown_pos + 13] == _BINTAG_SEP
                    )
                    name = None
                    if framed and name_pos + 13 <= end:
                        total = struct.unpack_from("<I", data, name_pos + 4)[0]
                        if name_pos + total <= end:
                            name = self._read_bintag_utf16_string(
                                data,
                                name_pos,
                                _HASH_PLAYER_ENTRY_NAME,
                                trim_trailing_nbsp=True,
                            )
                    if name is None:
                        active_names.pop(slot, None)
                    else:
                        active_names[slot] = name

            if end <= cursor:
                return static_names
            cursor = end

    def _correct_late_lobby_names(self, names, vacant, cursor, last_seconds):
        """Correct a sole late entrant without changing existing statistics or teams."""
        if not vacant:
            return names
        data = self.get_full_decompressed_data()
        entries = {slot: [] for slot in vacant}
        active = {}
        joined = {slot: set() for slot in vacant}
        while True:
            envelope = self._read_envelope(data, cursor)
            if envelope is None:
                return names
            end, seconds = envelope
            if seconds < last_seconds - RECORDING_RESET_DROP_SECONDS:
                return names
            last_seconds = max(last_seconds, seconds)
            pos = cursor + _ENVELOPE_MESSAGE_OFFSET
            message = data[pos : pos + 4]
            if message == _HASH_TEAM_WINS:
                break
            slot = (
                self._read_bintag_u32(data, pos + 4)
                if data[pos + 4 : pos + 8] == _HASH_ASLOT
                else None
            )
            if slot in vacant:
                if message == _HASH_PLAYER_ENTERS_GAME:
                    name_pos = pos + 38
                    name = None
                    framed = (
                        data[pos + 25 : pos + 29] == _BINTAG_SEP
                        and data[pos + 30 : pos + 34] == _BINTAG_SEP
                    )
                    if framed and name_pos + 13 <= end:
                        total = struct.unpack_from("<I", data, name_pos + 4)[0]
                        if name_pos + total <= end:
                            name = self._read_bintag_utf16_string(
                                data,
                                name_pos,
                                _HASH_PLAYER_ENTRY_NAME,
                                trim_trailing_nbsp=True,
                            )
                    if name is None or active.get(slot) != name:
                        entries[slot].append(name)
                    active[slot] = name
                elif message == _HASH_PLAYER_LEAVES_GAME:
                    active.pop(slot, None)
                elif (
                    message == _HASH_PLAYER_JOINED_TEAM and active.get(slot) is not None
                ):
                    if data[pos + 21 : pos + 25] == _HASH_ATEAM:
                        joined[slot].add(self._read_bintag_u32(data, pos + 21))
            if end <= cursor:
                return names
            cursor = end
        summaries = self.extract_player_end_summaries()
        teams = self.extract_team_assignments()
        for slot, occupants in entries.items():
            team = teams.get(slot)
            if (
                len(occupants) == 1
                and occupants[0] is not None
                and any(summaries.get(slot, ())[1:5])
                and team in (1, 2, 3)
                and joined[slot] == {team}
            ):
                names[slot] = occupants[0]
        return names

    def _correct_zero_score_spectators(self, players, static_names):
        """Use explicit session-local spectator evidence only for zero-stat rows."""
        candidates = [
            p
            for p in players
            if p.team in (1, 2, 3)
            and all(
                value == 0
                for value in (
                    p.score,
                    p.score_infantry,
                    p.score_support,
                    p.score_armor,
                    p.score_air,
                    p.score_capturing,
                    p.score_fortification,
                    p.score_transportation,
                    p.score_repair,
                    p.score_bridge_laying,
                    p.score_unit_damage,
                    p.score_tactical_aid,
                    p.score_total,
                )
            )
        ]
        if not candidates:
            return
        data = self.get_full_decompressed_data()
        cursor = self._find_chain_anchor(data)
        if cursor is None:
            return
        sessions = {p.id: [[static_names.get(p.id), 0, None]] for p in candidates}
        active = {slot: rows[0] for slot, rows in sessions.items()}
        events = []
        named_entries = []
        playing = set()
        start = None
        captured_start = False
        last_seconds = 0.0
        while True:
            envelope = self._read_envelope(data, cursor)
            if envelope is None:
                return
            end, seconds = envelope
            if seconds < last_seconds - RECORDING_RESET_DROP_SECONDS:
                return
            last_seconds = max(last_seconds, seconds)
            pos = cursor + _ENVELOPE_MESSAGE_OFFSET
            message = data[pos : pos + 4]

            def field(offset, tag):
                if offset + 17 <= end and data[offset : offset + 4] == tag:
                    return self._read_bintag_u32(data, offset)
                return None

            slot = field(pos + 4, _HASH_ASLOT)
            if (
                message == _HASH_SET_GAME_MODE_DATA_FLOAT
                and field(pos + 4, _HASH_ADATA_TYPE) == 1
            ):
                remaining = (
                    self._read_bintag_float(data, pos + 21)
                    if data[pos + 21 : pos + 25] == _HASH_AFLOAT
                    else None
                )
                if (
                    remaining is not None
                    and abs(remaining) <= 86400.0
                    and start is None
                ):
                    if remaining == 0.0:
                        captured_start = True
                    else:
                        start = pos
            if message == _HASH_TEAM_WINS:
                if start is None or not captured_start:
                    return
                result_end = pos
                break
            if message == _HASH_UNIT_CREATE:
                playing.add(field(pos + 21, _HASH_APLAYER))
            elif message == _HASH_PLAYER_SET_ROLE and start is not None:
                playing.add(slot)
            if slot in sessions:
                if message == _HASH_PLAYER_LEAVES_GAME:
                    previous = active.pop(slot, None)
                    if previous is not None:
                        previous[2] = end
                elif message == _HASH_PLAYER_ENTERS_GAME:
                    name_pos = pos + 38
                    name = None
                    framed = (
                        data[pos + 25 : pos + 29] == _BINTAG_SEP
                        and data[pos + 30 : pos + 34] == _BINTAG_SEP
                    )
                    if framed and name_pos + 13 <= end:
                        total = struct.unpack_from("<I", data, name_pos + 4)[0]
                        if name_pos + total <= end:
                            name = self._read_bintag_utf16_string(
                                data,
                                name_pos,
                                _HASH_PLAYER_ENTRY_NAME,
                                trim_trailing_nbsp=True,
                            )
                    if name is not None:
                        named_entries.append((pos, slot, name))
                    previous = active.get(slot)
                    if name is None or previous is None or previous[0] != name:
                        if previous is not None:
                            previous[2] = cursor
                        current = [name, cursor, None]
                        sessions[slot].append(current)
                        active[slot] = current
                elif message in (
                    _HASH_PLAYER_JOINED_TEAM,
                    _HASH_SPECTATOR_JOINED_TEAM,
                    _HASH_PLAYER_SET_ROLE,
                ):
                    spectator = message == _HASH_SPECTATOR_JOINED_TEAM and (
                        (
                            field(pos + 21, _HASH_ATEAM) in (0, 1, 2, 3)
                            and field(pos + 38, _HASH_ASPECTATOR_LOS) == 2
                        )
                        or (
                            field(pos + 21, _HASH_ATEAM) in (1, 2, 3)
                            and field(pos + 38, _HASH_ASPECTATOR_LOS) == 1
                        )
                    )
                    events.append((pos, slot, spectator, message))
            if end <= cursor:
                return
            cursor = end
        nonzero_slots = {
            slot for _, slot, score in self._explicit_match_scores() if score != 0
        }
        for player in candidates:
            if player.id in playing:
                continue
            matching = [
                s
                for s in sessions[player.id]
                if s[0] == player.name
                and s[1] < result_end
                and (s[2] is None or s[2] > start)
            ]
            if len(matching) != 1 or matching[0][1] > start:
                continue
            session = matching[0]
            stop = min(session[2], result_end) if session[2] is not None else result_end
            spectator = False
            had_lobby_role = False
            needs_spectator = False
            joined_during_match = False
            unsuperseded_role = False
            other_lobby_role = False
            for pos, slot, state, message in events:
                if slot != player.id:
                    continue
                if message == _HASH_PLAYER_SET_ROLE and pos < session[1]:
                    other_lobby_role = True
                if not session[1] <= pos < stop:
                    continue
                if message == _HASH_PLAYER_SET_ROLE and pos < start:
                    had_lobby_role = True
                    unsuperseded_role = True
                    needs_spectator = True
                elif message == _HASH_PLAYER_JOINED_TEAM:
                    spectator = False
                    joined_during_match |= pos >= start
                    if had_lobby_role:
                        needs_spectator = True
                elif message == _HASH_SPECTATOR_JOINED_TEAM:
                    spectator = state
                    if state:
                        unsuperseded_role = False
                    if pos < start and had_lobby_role:
                        needs_spectator = not state
            explicit_inactive_session = (
                player.role is None
                and len(
                    [
                        s
                        for s in sessions[player.id]
                        if s[1] < result_end and (s[2] is None or s[2] > start)
                    ]
                )
                == 1
                and any(
                    session[1] <= pos < stop
                    and slot == player.id
                    and name == player.name
                    for pos, slot, name in named_entries
                )
                and player.id not in nonzero_slots
                and not unsuperseded_role
            )
            if spectator and (
                explicit_inactive_session
                or (
                    not needs_spectator
                    and not other_lobby_role
                    and not (had_lobby_role and joined_during_match)
                )
            ):
                player.team = 0
                player.faction = "Spectator"

    def extract_game_info(self) -> GameInfo:
        """Extract game information from metadata chunk"""
        # Map name from fixed offset
        map_name = self.extract_map_name()

        # Translate to human-readable name (fallback to raw name if not in dictionary)
        map_display_name = MAP_NAMES.get(map_name, map_name)

        # Other info from UTF-16 strings
        strings = self.extract_utf16_strings()

        metadata = self.get_metadata_chunk()
        game_name_position = metadata.find(_HASH_MY_GAME_NAME)
        server_name = (
            self._read_bintag_utf16_string(
                metadata, game_name_position, _HASH_MY_GAME_NAME
            )
            or "Unknown"
        )
        date_time = "Unknown"
        replay_name_position = metadata.find(_HASH_REPLAY_NAME)
        replay_name = self._read_bintag_utf16_string(
            metadata, replay_name_position, _HASH_REPLAY_NAME
        )

        # Regex pattern for date/time: "- YYYY-MM-DD - HH:MM"
        # Supports years 2007-2099 (game launch to future)
        date_pattern = re.compile(
            r"-?\s*(20\d{2})-(\d{2})-(\d{2})\s*-\s*(\d{2}):(\d{2})"
        )

        for offset, s in strings:
            if date_pattern.search(s):  # Date string with year-agnostic regex
                # Extract and format as "- YYYY-MM-DD - HH:MM"
                match = date_pattern.search(s)
                if match:
                    year, month, day, hour, minute = match.groups()
                    date_time = f"- {year}-{month}-{day} - {hour}:{minute}"
        # Determine game mode from map name prefix
        game_mode = self.extract_game_mode(map_display_name)

        return GameInfo(
            map_name=map_name,
            map_display_name=map_display_name,
            server_name=server_name,
            date_time=date_time,
            replay_name=replay_name,
            game_mode=game_mode,
        )

    def extract_game_mode(self, map_display_name: str) -> str:
        """
        Extract game mode from map name prefix

        Args:
            map_display_name: Map name with prefix (e.g., "do_Hometown", "as_AirBase")

        Returns:
            Game mode string: "Domination", "Assault", "Tug of War", or "Unknown"
        """
        if map_display_name.startswith("do_"):
            return "Domination"
        elif map_display_name.startswith("as_"):
            return "Assault"
        elif map_display_name.startswith("tw_"):
            return "Tug of War"
        else:
            return "Unknown"

    def _primary_messages(self):
        """Yield complete primary recording messages up to the match result."""
        data = self.get_full_decompressed_data()
        cursor = self._find_chain_anchor(data)
        last_seconds = 0.0
        while cursor is not None:
            envelope = self._read_envelope(data, cursor)
            if envelope is None:
                return
            end, seconds = envelope
            if seconds < last_seconds - RECORDING_RESET_DROP_SECONDS:
                return
            last_seconds = max(last_seconds, seconds)
            pos = cursor + _ENVELOPE_MESSAGE_OFFSET
            yield cursor, pos, end
            if data[pos : pos + 4] == _HASH_TEAM_WINS:
                return
            cursor = end

    def _explicit_match_scores(self):
        data = self.get_full_decompressed_data()
        scores = []
        for _, pos, end in self._primary_messages():
            if data[pos : pos + 4] == _HASH_TEAM_WINS:
                return scores
            if (
                data[pos : pos + 4] == bytes([41, 3, 220, 13])
                and pos + 38 <= end
                and data[pos + 4 : pos + 8] == _HASH_APOS
                and data[pos + 21 : pos + 25] == _HASH_SCORE
                and data[pos + 12] == 1
                and data[pos + 29] == 0
                and self._read_bintag_u32(data, pos + 21) is not None
            ):
                slot = self._read_bintag_u32(data, pos + 4)
                if slot is not None and slot < 16:
                    scores.append(
                        (pos, slot, struct.unpack_from("<i", data, pos + 34)[0])
                    )
        return []

    def _mark_result_departures(self, players, static_names):
        """Attach explicit departure times only to unambiguous result identities."""
        data = self.get_full_decompressed_data()
        sessions = {
            p.id: [[static_names.get(p.id), 0, None, None, False]] for p in players
        }
        active = {slot: rows[0] for slot, rows in sessions.items()}
        start = None
        result = None
        for cursor, pos, end in self._primary_messages():
            tag = data[pos : pos + 4]

            def field(at, key):
                return (
                    self._read_bintag_u32(data, at)
                    if at + 17 <= end and data[at : at + 4] == key
                    else None
                )

            if (
                tag == _HASH_SET_GAME_MODE_DATA_FLOAT
                and field(pos + 4, _HASH_ADATA_TYPE) == 1
                and start is None
            ):
                remaining = (
                    self._read_bintag_float(data, pos + 21)
                    if data[pos + 21 : pos + 25] == _HASH_AFLOAT
                    else None
                )
                if remaining is not None and abs(remaining) <= 86400:
                    if remaining != 0:
                        start = pos
            if tag == _HASH_TEAM_WINS:
                result = pos
                break
            slot = field(pos + 4, _HASH_ASLOT)
            if slot not in sessions:
                continue
            if tag == _HASH_PLAYER_LEAVES_GAME:
                previous = active.pop(slot, None)
                if previous is not None:
                    previous[2] = end
                    if start is not None:
                        previous[3] = self._read_envelope(data, cursor)[1]
            elif tag == _HASH_PLAYER_ENTERS_GAME:
                at = pos + 38
                name = None
                if (
                    at + 13 <= end
                    and data[pos + 25 : pos + 29] == _BINTAG_SEP
                    and data[pos + 30 : pos + 34] == _BINTAG_SEP
                    and at + struct.unpack_from("<I", data, at + 4)[0] <= end
                ):
                    name = self._read_bintag_utf16_string(
                        data, at, _HASH_PLAYER_ENTRY_NAME, trim_trailing_nbsp=True
                    )
                previous = active.get(slot)
                if name is None or previous is None or previous[0] != name:
                    if previous is not None:
                        previous[2] = cursor
                    previous = [name, cursor, None, None, name is not None]
                    sessions[slot].append(previous)
                    active[slot] = previous
                else:
                    previous[4] = True
        if start is None or result is None:
            return
        for player in players:
            own = [
                s
                for s in sessions[player.id]
                if s[1] < result and (s[2] is None or s[2] > start)
            ]
            if any(s[0] is None for s in own):
                continue
            matching = [s for s in own if s[0] == player.name]
            if len(matching) != 1:
                continue
            _, _, left, time, entered = matching[0]
            if entered and left is not None and left < result and time is not None:
                player.left_at_seconds = time

    def _correct_vacant_result_names(self, players, static_names):
        """Resolve a sole proven entrant in a vacant match-start result slot."""
        data = self.get_full_decompressed_data()
        sessions = {p.id: [[static_names.get(p.id), 0, None]] for p in players}
        active = {slot: rows[0] for slot, rows in sessions.items()}
        messages = list(self._primary_messages())
        start = None
        captured = False
        result = None
        units = []
        joins = []
        for cursor, pos, end in messages:
            tag = data[pos : pos + 4]

            def field(offset, key):
                at = pos + offset
                return (
                    self._read_bintag_u32(data, at)
                    if at + 17 <= end and data[at : at + 4] == key
                    else None
                )

            slot = field(4, _HASH_ASLOT)
            if (
                tag == _HASH_SET_GAME_MODE_DATA_FLOAT
                and field(4, _HASH_ADATA_TYPE) == 1
                and start is None
            ):
                remaining = (
                    self._read_bintag_float(data, pos + 21)
                    if data[pos + 21 : pos + 25] == _HASH_AFLOAT
                    else None
                )
                if remaining is not None and abs(remaining) <= 86400:
                    if remaining == 0:
                        captured = True
                    else:
                        start = pos
            if tag == _HASH_TEAM_WINS:
                result = pos
            elif tag == _HASH_UNIT_CREATE:
                units.append((pos, field(21, _HASH_APLAYER), field(38, _HASH_ATEAM)))
            elif tag == _HASH_PLAYER_JOINED_TEAM:
                joins.append((pos, slot, field(21, _HASH_ATEAM)))
            elif slot in sessions:
                if tag == _HASH_PLAYER_LEAVES_GAME:
                    old = active.pop(slot, None)
                    if old is not None:
                        old[2] = end
                elif tag == _HASH_PLAYER_ENTERS_GAME:
                    name_pos = pos + 38
                    name = None
                    if (
                        name_pos + 13 <= end
                        and data[pos + 25 : pos + 29] == _BINTAG_SEP
                        and data[pos + 30 : pos + 34] == _BINTAG_SEP
                        and name_pos + struct.unpack_from("<I", data, name_pos + 4)[0]
                        <= end
                    ):
                        name = self._read_bintag_utf16_string(
                            data,
                            name_pos,
                            _HASH_PLAYER_ENTRY_NAME,
                            trim_trailing_nbsp=True,
                        )
                    old = active.get(slot)
                    if name is None or old is None or old[0] != name:
                        if old is not None:
                            old[2] = cursor
                        current = [name, cursor, None]
                        sessions[slot].append(current)
                        active[slot] = current
        if not captured or start is None or result is None:
            return
        scores = self._explicit_match_scores()
        for player in players:
            if player.team not in (1, 2, 3):
                continue
            own = [
                s
                for s in sessions[player.id]
                if s[1] < result and (s[2] is None or s[2] > start)
            ]
            if len(own) != 1:
                continue
            name, entered, left = own[0]
            if entered == 0 or name is None or name == player.name:
                continue
            stop = min(left, result) if left is not None else result
            owned = [(pos, team) for pos, slot, team in units if slot == player.id]
            if not owned or any(
                not entered <= pos < stop or team != player.team for pos, team in owned
            ):
                continue
            if any(
                slot == player.id and pos < entered and score != 0
                for pos, slot, score in scores
            ):
                continue
            own_joins = [
                (pos, team)
                for pos, slot, team in joins
                if slot == player.id and entered <= pos < stop
            ]
            pregame = [team for pos, team in own_joins if pos < start]
            joined = {team for pos, team in own_joins if pos >= start}
            joined.update(pregame[-1:])
            if joined == {player.team}:
                player.name = name

    def _extract_scores_by_hash(self) -> Dict[int, int]:
        """
        Extract final scores per score counter using the BinTag score field hash.

        Each score entry is 55 bytes:
          Bytes  0-16:  BinTag field: hash(4) + sep(4) + flag(1) + sep(4) + SCORE(u32)
          Bytes 17-20:  03 02 b5 05
          Bytes 21-46:  other fields (float timestamp, etc.)
          Bytes 47-50:  separator (11 00 00 00)
          Bytes 51-54:  COUNTER (u32 LE) — 1-indexed, wraps: 1,2,...,15,0

        Counter maps to player slot via: slot = (counter - 1) % 16.
        Keeps last valid value per counter, but skips the corrupted final
        entry in the last score block (which can duplicate an earlier
        counter with score 0).

        Returns:
            Dict mapping counter (0-15) to final score.
        """
        data = self.get_full_decompressed_data()

        # Collect all valid score entries: (position, counter, score)
        entries: List[Tuple[int, int, int]] = []

        offset = 0
        while True:
            pos = data.find(_HASH_SCORE, offset)
            if pos == -1:
                break

            # Validate BinTag separator at +4
            if pos + 55 <= len(data) and data[pos + 4 : pos + 8] == _BINTAG_SEP:
                score = struct.unpack("<i", data[pos + 13 : pos + 17])[0]

                # Validate separator at +47 before reading counter
                if data[pos + 47 : pos + 51] == _BINTAG_SEP:
                    counter = struct.unpack("<I", data[pos + 51 : pos + 55])[0]

                    if counter <= 15:
                        entries.append((pos, counter, score))

            offset = pos + 1

        if not entries:
            return {}

        # Find the last contiguous run (entries exactly 55 bytes apart),
        # then check the final 16-entry block within it for a corrupted
        # last entry (duplicate counter with score 0)
        run_start = len(entries) - 1
        for i in range(len(entries) - 1, 0, -1):
            if entries[i][0] - entries[i - 1][0] == 55:
                run_start = i - 1
            else:
                break

        run = entries[run_start:]
        if len(run) >= 16:
            # Take the last 16 entries of the run (the actual final block)
            final_block = run[-16:]
            block_counters = {e[1] for e in final_block[:15]}
            if final_block[15][1] in block_counters:
                entries.pop()  # Remove corrupted final entry

        # Keep last value per counter
        counter_scores: Dict[int, int] = {}
        for _, counter, score in entries:
            counter_scores[counter] = score

        return counter_scores

    @staticmethod
    def _find_all(data: bytes, pattern: bytes) -> List[int]:
        """Find all occurrences of pattern in data."""
        positions = []
        start = 0
        while True:
            idx = data.find(pattern, start)
            if idx == -1:
                break
            positions.append(idx)
            start = idx + 1
        return positions

    @staticmethod
    def _read_bintag_u32(data: bytes, hash_pos: int) -> Optional[int]:
        """Read uint32 value from a BinTag int32 entry.
        Format: hash(4) + 0x11000000(4) + byte(1) + 0x11000000(4) + value(4) = 17 bytes.
        """
        if hash_pos + 17 > len(data):
            return None
        if data[hash_pos + 4 : hash_pos + 8] != _BINTAG_SEP:
            return None
        return struct.unpack("<I", data[hash_pos + 13 : hash_pos + 17])[0]

    @staticmethod
    def _read_bintag_float(data: bytes, hash_pos: int) -> Optional[float]:
        """Read float value from a BinTag float entry.
        Format: hash(4) + 0x11000000(4) + byte(1) + 0x11000000(4) + value(4) = 17 bytes.
        """
        if hash_pos + 17 > len(data):
            return None
        if data[hash_pos + 4 : hash_pos + 8] != _BINTAG_SEP:
            return None
        return struct.unpack("<f", data[hash_pos + 13 : hash_pos + 17])[0]

    @classmethod
    def _read_unique_bintag_bool(cls, data: bytes, field_hash: bytes) -> Optional[bool]:
        values = []
        for position in cls._find_all(data, field_hash):
            if (
                position + 17 <= len(data)
                and data[position + 4 : position + 8] == _BINTAG_SEP
                and data[position + 9 : position + 13] == _BINTAG_SEP
            ):
                value = cls._read_bintag_u32(data, position)
                if value is not None:
                    values.append(value)
        if len(values) != 1 or values[0] not in (0, 1):
            return None
        return bool(values[0])

    def extract_server_classification(
        self,
    ) -> Tuple[RawServerFlags, ServerClassification]:
        metadata = self.get_metadata_chunk()
        raw = RawServerFlags(
            few_player_mode_flag=self._read_unique_bintag_bool(
                metadata, _HASH_MY_FPM_MODE_FLAG
            ),
            match_mode_flag=self._read_unique_bintag_bool(
                metadata, _HASH_MY_MATCH_MODE_FLAG
            ),
            tournament_match_flag=self._read_unique_bintag_bool(
                metadata, _HASH_MY_IS_TOURNAMENT_MATCH_FLAG
            ),
            clan_match_flag=self._read_unique_bintag_bool(
                metadata, _HASH_MY_IS_CLAN_MATCH_FLAG
            ),
        )

        full = self.get_full_decompressed_data()
        player_types = {
            value
            for position in self._find_all(full, _HASH_MY_TYPE)
            if (value := self._read_bintag_u32(full, position)) is not None
            and full[position + 9 : position + 13] == _BINTAG_SEP
        }
        if 1 in player_types:
            has_bots = True
        elif player_types and player_types <= {0}:
            has_bots = False
        else:
            has_bots = None

        match_mode = None
        if raw.match_mode_flag is not None and raw.few_player_mode_flag is not None:
            match_mode = raw.match_mode_flag and not raw.few_player_mode_flag

        return raw, ServerClassification(
            few_player_mode=raw.few_player_mode_flag,
            match_mode=match_mode,
            has_bots=has_bots,
            clan_match=raw.clan_match_flag,
            tournament_match=raw.tournament_match_flag,
            ranked=None,
        )

    def extract_team_assignments(self) -> Dict[int, int]:
        """
        Extract player slot -> team/faction mapping from the event stream.

        Uses two sources (from Ghidra decompilation of wic.exe):
        1. PlayerJoinedTeam events: contain aSlot + aTeam (authoritative)
        2. UnitCreate events: contain aPlayer + aTeam (fallback for missing slots)

        Team values are faction IDs: 0=Spectator, 1=USA, 2=NATO, 3=USSR

        Returns:
            Dict mapping player slot ID to team/faction value (e.g., {1: 3, 2: 1, ...})
        """
        data = self.get_full_decompressed_data()
        slot_teams: Dict[int, int] = {}

        # Phase 1: Extract from PlayerJoinedTeam and SpectatorJoinedTeam events
        for event_pat in [_HASH_PLAYER_JOINED_TEAM, _HASH_SPECTATOR_JOINED_TEAM]:
            is_spectator_event = event_pat == _HASH_SPECTATOR_JOINED_TEAM
            for pos in self._find_all(data, event_pat):
                block = data[pos : pos + 200]
                slot_off = block.find(_HASH_ASLOT)
                team_off = block.find(_HASH_ATEAM)
                if slot_off >= 0 and team_off >= 0:
                    slot_val = self._read_bintag_u32(data, pos + slot_off)
                    team_val = self._read_bintag_u32(data, pos + team_off)
                    if slot_val is not None and team_val is not None and slot_val <= 16:
                        # SpectatorJoinedTeam aTeam carries the team being *left*, not joined.
                        # Always treat as team=0 to avoid poisoning real assignments.
                        effective_team = 0 if is_spectator_event else team_val
                        if effective_team != 0:
                            slot_teams[slot_val] = effective_team
                        elif slot_val not in slot_teams:
                            slot_teams[slot_val] = 0

        # Phase 2: Fallback via UnitCreate events for slots not yet assigned
        assigned_slots = {s for s, t in slot_teams.items() if t != 0}
        if len(assigned_slots) < 16:
            player_team_votes: Dict[int, Dict[int, int]] = {}
            for pos in self._find_all(data, _HASH_UNIT_CREATE):
                block = data[pos : pos + 300]
                player_off = block.find(_HASH_APLAYER)
                team_off = block.find(_HASH_ATEAM)
                if player_off >= 0 and team_off >= 0:
                    player_val = self._read_bintag_u32(data, pos + player_off)
                    team_val = self._read_bintag_u32(data, pos + team_off)
                    if (
                        player_val is not None
                        and team_val is not None
                        and player_val <= 16
                        and team_val != 0
                    ):
                        if player_val not in assigned_slots:
                            votes = player_team_votes.setdefault(player_val, {})
                            votes[team_val] = votes.get(team_val, 0) + 1

            for player_id, votes in player_team_votes.items():
                if player_id not in slot_teams or slot_teams[player_id] == 0:
                    best_team = max(votes, key=votes.get)
                    slot_teams[player_id] = best_team

        return slot_teams

    def _final_screen_score_table(self):
        """Slot statistics only; an intact table does not establish final identities."""
        data = self.get_full_decompressed_data()
        fields = [
            _HASH_APOS,
            _HASH_AROLE_ID,
            _HASH_TOTAL_SCORE,
            _HASH_CAPTURING_SCORE,
            _HASH_FORTIFICATION_SCORE,
            _HASH_TRANSPORTATION_SCORE,
            _HASH_REPAIR_SCORE,
            _HASH_BRIDGE_LAYING_SCORE,
            _HASH_UNIT_DAMAGE_SCORE,
            _HASH_TACTICAL_AID_SCORE,
            _HASH_SCORE_ROLE0,
            _HASH_SCORE_ROLE1,
            _HASH_SCORE_ROLE2,
            _HASH_SCORE_ROLE3,
        ]

        def field(pos, end, key, flag=None):
            if pos + 17 > end or data[pos : pos + 4] != key:
                return None
            if (
                data[pos + 4 : pos + 8] != _BINTAG_SEP
                or data[pos + 9 : pos + 13] != _BINTAG_SEP
            ):
                return None
            if flag is not None and data[pos + 8] != flag:
                return None
            return struct.unpack_from("<I", data, pos + 13)[0]

        for pos in self._find_all(data, _HASH_TEAM_WINS):
            result = self._read_envelope(data, pos - _ENVELOPE_MESSAGE_OFFSET)
            if result is not None:
                result_start = pos - _ENVELOPE_MESSAGE_OFFSET
                break
        else:
            return None
        cursor = result_start
        rows, slots = {}, set()
        while cursor >= 259:
            start = cursor - 259
            pos = start + _ENVELOPE_MESSAGE_OFFSET
            if data[pos : pos + 4] != bytes.fromhex("6f06313a"):
                break
            envelope = self._read_envelope(data, start)
            if envelope is None or envelope[0] != cursor:
                return None
            values = [
                field(pos + 4 + i * 17, cursor, key, 0 if i in (1, 2) else 1)
                for i, key in enumerate(fields)
            ]
            if any(v is None for v in values) or values[0] > 15:
                return None
            if values[0] in slots:
                return None
            slots.add(values[0])
            rows[start] = values
            cursor = start
        if any(cursor < gap < result[0] for gap in self.decompression_gaps):
            return None
        return (result_start, rows) if rows else None

    def final_screen_players(self):
        """Decode the replay's final screen; None preserves the legacy fallback."""
        data = self.get_full_decompressed_data()
        table = self._final_screen_score_table()
        if table is None:
            return None
        result_start, score_table = table
        if any(gap <= result_start for gap in self.decompression_gaps):
            return None
        states = {}
        rows = {}
        started = False
        role_names = {
            0x0E0002CB: "infantry",
            0x1ABF03DF: "support",
            0x10E00313: "armor",
            0x0AE8026E: "air",
        }

        def field(pos, end, key, flag=None):
            if pos + 17 > end or data[pos : pos + 4] != key:
                return None
            if (
                data[pos + 4 : pos + 8] != _BINTAG_SEP
                or data[pos + 9 : pos + 13] != _BINTAG_SEP
            ):
                return None
            if flag is not None and data[pos + 8] != flag:
                return None
            return struct.unpack_from("<I", data, pos + 13)[0]

        for start, pos, end in self._primary_messages():
            tag = data[pos : pos + 4]
            if tag == _HASH_TEAM_WINS:
                if (
                    start != result_start
                    or not rows
                    or any(
                        s is None or (s[4] and s[2] != 2 and slot not in rows)
                        for slot, s in states.items()
                    )
                ):
                    return None
                players = [rows[s] for s in sorted(rows)]
                if any(sum(p.team == team for p in players) > 8 for team in (1, 2, 3)):
                    return None
                return players
            if tag == bytes.fromhex("6f06313a"):
                started = True
                if start not in score_table:
                    return None
                values = score_table[start].copy()
                slot = values[0]
                state = states.get(slot)
                if state is None or not state[4] or slot in rows:
                    return None
                if state[2] == 2:
                    continue
                values[2:] = [v if v < 2**31 else v - 2**32 for v in values[2:]]
                scores = values[10:14]
                best = max((v for v in scores if v != 0), default=None)
                role = (
                    ("infantry", "support", "armor", "air")[scores.index(best)]
                    if best is not None
                    else role_names.get(values[1])
                    if values[2] > 0
                    else None
                )
                team = 0 if state[3] else state[1]
                rows[slot] = Player(
                    id=slot,
                    name=state[0],
                    team=team,
                    faction={0: "Spectator", 1: "USA", 2: "NATO", 3: "USSR"}[team],
                    score=values[2],
                    role=role,
                    score_total=values[2],
                    score_capturing=values[3],
                    score_fortification=values[4],
                    score_transportation=values[5],
                    score_repair=values[6],
                    score_bridge_laying=values[7],
                    score_unit_damage=values[8],
                    score_tactical_aid=values[9],
                    score_infantry=values[10],
                    score_support=values[11],
                    score_armor=values[12],
                    score_air=values[13],
                )
            elif tag == _HASH_PLAYER_ENTERS_GAME:
                if started:
                    return None
                slot = field(pos + 4, end, _HASH_ASLOT)
                if slot is None or slot > 15:
                    return None
                team = field(pos + 21, end, bytes.fromhex("a8013204"))
                name_pos = pos + 38
                name = self._read_bintag_utf16_string(
                    data[name_pos:end],
                    0,
                    _HASH_PLAYER_ENTRY_NAME,
                    trim_trailing_nbsp=True,
                )
                name_end = (
                    name_pos + struct.unpack_from("<I", data, name_pos + 4)[0]
                    if name_pos + 8 <= end
                    else end
                )
                kind = field(name_end + 17, end, _HASH_MY_TYPE, 1)
                states[slot] = (
                    [name, team, kind, False, True]
                    if name and team in (0, 1, 2, 3) and kind is not None
                    else None
                )
            elif tag in (
                _HASH_PLAYER_LEAVES_GAME,
                _HASH_PLAYER_JOINED_TEAM,
                _HASH_SPECTATOR_JOINED_TEAM,
            ):
                if started:
                    return None
                slot = field(pos + 4, end, _HASH_ASLOT)
                if slot is None:
                    return None
                if slot > 15:
                    continue
                if tag == _HASH_PLAYER_LEAVES_GAME:
                    states.pop(slot, None)
                else:
                    state = states.setdefault(slot, None)
                    if state is None:
                        continue
                    team = field(pos + 21, end, _HASH_ATEAM)
                    if team not in (0, 1, 2, 3):
                        return None
                    state[1] = team
                    state[3] = tag == _HASH_SPECTATOR_JOINED_TEAM
        return None

    def extract_player_end_summaries(self) -> Dict[int, Tuple]:
        """Extract per-player role and category scores from end-summary blocks."""
        data = self.get_full_decompressed_data()
        result: Dict[int, Tuple] = {}

        for pos in self._find_all(data, _HASH_APOS):
            if pos + 300 > len(data):
                continue
            player_idx = self._read_bintag_u32(data, pos)
            if player_idx is None or player_idx > 15:
                continue

            block = data[pos : pos + 300]

            def read_role_score(hash_pat: bytes) -> int:
                off = block.find(hash_pat)
                if off < 0:
                    return 0
                val = self._read_bintag_u32(data, pos + off)
                return (val if val < 2**31 else val - 2**32) if val is not None else 0

            sr0 = read_role_score(_HASH_SCORE_ROLE0)
            sr1 = read_role_score(_HASH_SCORE_ROLE1)
            sr2 = read_role_score(_HASH_SCORE_ROLE2)
            sr3 = read_role_score(_HASH_SCORE_ROLE3)

            max_score = max((s for s in (sr0, sr1, sr2, sr3) if s != 0), default=0)
            if max_score == 0:
                role = ""
            elif sr0 == max_score:
                role = "infantry"
            elif sr1 == max_score:
                role = "support"
            elif sr2 == max_score:
                role = "armor"
            else:
                role = "air"

            recorded_role = None
            if data[pos + 17 : pos + 21] == _HASH_AROLE_ID:
                recorded_role = {
                    0x0E0002CB: "infantry",
                    0x1ABF03DF: "support",
                    0x10E00313: "armor",
                    0x0AE8026E: "air",
                }.get(self._read_bintag_u32(data, pos + 17))
            result[player_idx] = (
                role,
                sr0,
                sr1,
                sr2,
                sr3,
                read_role_score(_HASH_CAPTURING_SCORE),
                read_role_score(_HASH_FORTIFICATION_SCORE),
                read_role_score(_HASH_TRANSPORTATION_SCORE),
                read_role_score(_HASH_REPAIR_SCORE),
                read_role_score(_HASH_BRIDGE_LAYING_SCORE),
                read_role_score(_HASH_UNIT_DAMAGE_SCORE),
                read_role_score(_HASH_TACTICAL_AID_SCORE),
                read_role_score(_HASH_TOTAL_SCORE),
                recorded_role,
            )

        return result

    def extract_recorder_slot(self) -> Optional[int]:
        """Extract the recorder's player slot from the POV hash in metadata."""
        meta = self.get_metadata_chunk()
        pos = meta.find(_HASH_POV_PLAYER)
        if pos < 0:
            return None
        slot = self._read_bintag_u32(meta, pos)
        if slot is not None and slot <= 15:
            return slot
        return None

    def extract_winner(self) -> Optional[int]:
        """
        Extract the winning team from the TeamWins event.

        The TeamWins event appears once per replay near the end (~98%)
        and contains the aTeam field indicating which faction won.

        Returns:
            Winning team/faction ID (1=USA, 2=NATO, 3=USSR), or None if not found.
            Returns None for team 0 (Spectator/invalid — map vote, no real winner).
        """
        data = self.get_full_decompressed_data()
        # Search from the end since TeamWins is near ~98%
        pos = data.rfind(_HASH_TEAM_WINS)
        if pos < 0:
            return None
        block = data[pos : pos + 50]
        team_off = block.find(_HASH_ATEAM)
        if team_off >= 0:
            team = self._read_bintag_u32(data, pos + team_off)
            # Team 0 = Spectator/invalid — no real winner
            if team == 0:
                return None
            return team
        return None

    def extract_domination_sample(self) -> Optional[Tuple[int, float]]:
        """
        Final well-formed aFactor sample and the offset it was read from.

        aFactor is a float (0.0-1.0) holding the domination tug-of-war bar,
        reported from the recording player's perspective. It starts at 0.5 and
        moves toward 0.0 or 1.0 as one team dominates. Scans backwards rather
        than reading only the final byte match, so one malformed or out-of-range
        record at the tail cannot suppress the result.

        Returns:
            (offset, value) with value in 0.0-1.0, or None.
        """
        data = self.get_full_decompressed_data()
        pos = data.rfind(_HASH_AFACTOR)
        while pos >= 0:
            val = self._read_bintag_float(data, pos)
            if val is not None and 0.0 <= val <= 1.0:
                return (pos, val)
            pos = data.rfind(_HASH_AFACTOR, 0, pos)
        return None

    def pov_team_at(self, offset: int) -> Optional[int]:
        """
        Team the POV player belonged to at ``offset``.

        aFactor is mirrored when the recording player changes side, so the
        anchor must be resolved at the sample being read. Returns None for a
        spectator recorder, which has no roster team to anchor to.
        """
        slot = self.extract_recorder_slot()
        if slot is None:
            return None
        data = self.get_full_decompressed_data()
        latest_pos = -1
        latest_team = 0
        for event_pat in (_HASH_PLAYER_JOINED_TEAM, _HASH_SPECTATOR_JOINED_TEAM):
            is_spectator_event = event_pat == _HASH_SPECTATOR_JOINED_TEAM
            pos = data.find(event_pat)
            while 0 <= pos < offset:
                block = data[pos : pos + 200]
                slot_off = block.find(_HASH_ASLOT)
                team_off = block.find(_HASH_ATEAM)
                if slot_off >= 0 and team_off >= 0:
                    ev_slot = self._read_bintag_u32(data, pos + slot_off)
                    ev_team = self._read_bintag_u32(data, pos + team_off)
                    if ev_slot == slot and ev_team is not None and pos > latest_pos:
                        # SpectatorJoinedTeam carries the team being left.
                        latest_pos = pos
                        latest_team = 0 if is_spectator_event else ev_team
                pos = data.find(event_pat, pos + 1)
        return latest_team or None

    def extract_clock_samples(self) -> Tuple[List[float], bool]:
        """
        Countdown-clock readings, and whether the pre-match countdown was seen.

        The clock reads zero only before it starts, so observing a zero means the
        recording was already running when the match began.
        """
        data = self.get_full_decompressed_data()
        samples: List[float] = []
        countdown_started = False
        saw_pre_match = False
        pos = data.find(_HASH_SET_GAME_MODE_DATA_FLOAT)
        while pos >= 0:
            block_type = data[pos + 4 : pos + 8]
            if block_type == _HASH_ADATA_TYPE:
                data_type = self._read_bintag_u32(data, pos + 4)
                remaining = (
                    self._read_bintag_float(data, pos + 21)
                    if data[pos + 21 : pos + 25] == _HASH_AFLOAT
                    else None
                )
                if data_type == 1 and remaining is not None and abs(remaining) <= 86400:
                    if not countdown_started and remaining == 0.0:
                        saw_pre_match = True
                    if remaining != 0.0:
                        countdown_started = True
                    if countdown_started:
                        samples.append(remaining)
            pos = data.find(_HASH_SET_GAME_MODE_DATA_FLOAT, pos + 1)
        return samples, saw_pre_match

    def _read_envelope(self, data: bytes, start: int) -> Optional[tuple]:
        """Read the envelope header at ``start`` as ``(end, time)``."""
        if start < 0 or start + _ENVELOPE_MESSAGE_OFFSET + 4 > len(data):
            return None
        if (
            data[start : start + 4] != _HASH_EVENT
            or data[start + 4 : start + 8] != _ENVELOPE_TYPE
            or data[start + 8] != _ENVELOPE_ARRAY_FLAG
        ):
            return None
        (total,) = struct.unpack_from("<I", data, start + 9)
        end = start + total
        if total < _ENVELOPE_MESSAGE_OFFSET + 4 or end > len(data):
            return None
        (seconds,) = struct.unpack_from("<f", data, start + 13)
        if seconds != seconds or seconds in (float("inf"), float("-inf")):
            return None
        return end, seconds

    def _find_chain_anchor(self, data: bytes) -> Optional[int]:
        """Find the start of the gapless ``Event`` chain.

        The metadata prefix is not envelope-structured, so the chain is located
        by requiring several envelopes to link end-to-end from the same
        candidate rather than by trusting the first header-shaped bytes.
        """
        search = 0
        while True:
            candidate = data.find(_HASH_EVENT, search)
            if candidate < 0:
                return None
            cursor = candidate
            links = 0
            while links < _CHAIN_ANCHOR_CONFIRM_LINKS and cursor < len(data):
                envelope = self._read_envelope(data, cursor)
                if envelope is None:
                    break
                cursor = envelope[0]
                links += 1
            # A chain that runs cleanly off the end of the data is confirmed by
            # exhaustion rather than by link count.
            if links == _CHAIN_ANCHOR_CONFIRM_LINKS or (
                links > 0 and cursor == len(data)
            ):
                return candidate
            search = candidate + 1

    def extract_recording_seconds(self) -> float:
        """Real length of the replay, from the ``Event`` envelope clock.

        Counts the pre-match lobby and the post-``TeamWins`` tail, and stops at
        the end-of-match summary, whose clock restarts at zero.
        """
        data = self.get_full_decompressed_data()
        anchor = self._find_chain_anchor(data)
        if anchor is None:
            return 0.0
        cursor = anchor
        seconds = 0.0
        while True:
            envelope = self._read_envelope(data, cursor)
            if envelope is None:
                return round(seconds, 3)
            end, time_seconds = envelope
            if time_seconds < seconds - RECORDING_RESET_DROP_SECONDS:
                return round(seconds, 3)
            seconds = max(seconds, time_seconds)
            if end <= cursor:
                return round(seconds, 3)
            cursor = end

    def extract_match_timing(self) -> MatchTiming:
        """Match timing, separating the recorded span from true match time."""
        samples, captured_match_start = self.extract_clock_samples()
        recording_seconds = self.extract_recording_seconds()
        if not samples:
            legacy = self.extract_game_duration()
            return MatchTiming(
                captured_match_start=captured_match_start,
                recording_seconds=recording_seconds,
                observed_gameplay_seconds=legacy,
                match_elapsed_seconds=legacy,
            )

        # Modes that run several timed rounds (Assault) restart the countdown, so
        # elapsed time accumulates per phase and the first phase's opening value
        # is the round length, not the match length.
        phase_start_elapsed = 0.0
        phase_start_remaining = samples[0]
        phase_min_remaining = samples[0]
        previous = samples[0]
        phase_count = 1
        for remaining in samples[1:]:
            if remaining - previous > CLOCK_RESET_THRESHOLD_SECONDS:
                phase_start_elapsed += max(
                    phase_start_remaining - phase_min_remaining, 0.0
                )
                phase_start_remaining = remaining
                phase_min_remaining = remaining
                phase_count += 1
            else:
                phase_min_remaining = min(phase_min_remaining, remaining)
            previous = remaining

        opening = samples[0]
        final_remaining = samples[-1]
        recorded = round(
            phase_start_elapsed + max(phase_start_remaining - phase_min_remaining, 0.0),
            3,
        )

        if captured_match_start:
            round_length = round(opening, 3)
            round_length_exact = True
        else:
            round_length = next(
                (L for L in STANDARD_ROUND_LENGTHS if L >= opening - 1.0), opening
            )
            round_length_exact = False

        # The late-join correction assumes one countdown. With several rounds the
        # missing span cannot be recovered from the final round's clock alone.
        if captured_match_start or phase_count != 1:
            match_elapsed = recorded
        else:
            # The countdown keeps ticking past zero into the post-match screen,
            # so clamp before subtracting or overtime inflates the match length.
            match_elapsed = round(
                min(max(round_length - max(final_remaining, 0.0), 0.0), round_length), 3
            )

        return MatchTiming(
            captured_match_start=captured_match_start,
            recording_seconds=recording_seconds,
            observed_gameplay_seconds=recorded,
            match_elapsed_seconds=match_elapsed,
            round_length_seconds=round_length,
            round_length_exact=round_length_exact,
            joined_at_remaining_seconds=(
                None if captured_match_start else round(opening, 3)
            ),
            final_remaining_seconds=round(final_remaining, 3),
        )

    @staticmethod
    def classify_match_ending(
        timing: MatchTiming, winner: Optional[str], bar: Optional[float]
    ) -> str:
        """Classify how the match ended from the clock and the final bar."""
        if winner is None or timing.final_remaining_seconds is None:
            return "unknown"
        # Checked first: a pinned bar is unambiguous, and across the corpus it
        # never coincides with an expired clock.
        if bar is not None and (
            bar <= DOMINATION_EXTREME_EPSILON or bar >= 1.0 - DOMINATION_EXTREME_EPSILON
        ):
            return "totalDomination"
        if timing.final_remaining_seconds <= TIMEOUT_REMAINING_SECONDS:
            return "timeout"
        return "forfeit"

    def resolve_domination_shares(
        self,
        bar: Optional[Tuple[int, float]],
        players: List[Player],
        winner: Optional[str],
    ) -> Tuple[Optional[List[DominationShare]], Optional[str]]:
        """
        Attribute the final bar split to concrete factions.

        Prefers the POV anchor, which is independent of the recorded outcome.
        Falls back to assigning the larger share to the TeamWins winner, sound
        because the winning side of the bar decides a Domination match.
        """
        if bar is None:
            return None, None
        offset, raw = bar

        factions = sorted(
            {
                p.faction
                for p in players
                if p.faction and p.faction not in ("Spectator", "Unknown")
            }
        )
        if len(factions) != 2:
            return None, None

        pov_team = self.pov_team_at(offset)
        if pov_team is not None:
            pov_faction = FACTION_NAMES.get(pov_team, "Unknown")
            other = next((f for f in factions if f != pov_faction), None)
            if other is not None and pov_faction in factions:
                return (
                    [
                        DominationShare(pov_faction, raw),
                        DominationShare(other, 1.0 - raw),
                    ],
                    "povTeam",
                )

        if winner in factions:
            losing = next(f for f in factions if f != winner)
            return (
                [
                    DominationShare(winner, max(raw, 1.0 - raw)),
                    DominationShare(losing, min(raw, 1.0 - raw)),
                ],
                "winnerInferred",
            )
        return None, None

    def extract_game_duration(self) -> Optional[float]:
        """
        Extract game duration in seconds
        Searches for duration float in end-game region

        Note: Requires full decompression (slower)
        """
        try:
            data = self.get_full_decompressed_data()

            # Search last 1% of file
            search_start = int(len(data) * 0.99)
            time_values = []

            for offset in range(search_start, len(data) - 4, 4):
                try:
                    float_val = struct.unpack("<f", data[offset : offset + 4])[0]

                    # Check if it's a reasonable match time (5-22 minutes)
                    # Excludes total recording time which includes pre-game
                    if 300 <= float_val <= 1320:
                        time_values.append(float_val)
                except Exception:
                    pass

            # Return the most common high value (final match duration)
            # Focus on values >= 1200 seconds (20 min) to exclude intermediate timestamps
            if time_values:
                high_values = [t for t in time_values if t >= 1200]
                if high_values:
                    from collections import Counter

                    counter = Counter([round(t, 1) for t in high_values])
                    return counter.most_common(1)[0][0]
                # Fallback to max if no values >= 1200
                return max(time_values)
        except Exception:
            pass

        return None

    def parse(self) -> ReplayData:
        """
        Parse the complete replay file

        Returns:
            ReplayData with game info, players (names, IDs, scores), and duration

        Raises:
            ValueError: If the replay file is corrupt (no TeamWins event)
        """
        full = self.get_full_decompressed_data()
        if full.find(_HASH_TEAM_WINS) == -1:
            raise ValueError("Corrupt replay file: no TeamWins event found")

        game_info = self.extract_game_info()
        raw_server_flags, server_classification = self.extract_server_classification()

        # 1. Extract player roster from metadata (aSlot-based)
        slot_names = self._extract_slot_names(self.get_metadata_chunk())

        # 1b. Detect lobby slot swaps (event stream has corrected names)
        self._apply_lobby_swaps(slot_names)

        # 2. Get scores and teams (single scan for both score mapping and sorted list)
        counter_scores = self._extract_scores_by_hash()
        player_score_map = {
            (counter - 1) % 16: score for counter, score in counter_scores.items()
        }
        explicit_scores = self._explicit_match_scores()
        if explicit_scores:
            player_score_map = {slot: score for _, slot, score in explicit_scores}
        scored_ids = {pid for pid, score in player_score_map.items() if score != 0}
        player_scores = sorted(
            [s for s in player_score_map.values() if s != 0], reverse=True
        )
        team_map = self.extract_team_assignments()
        end_summaries = self.extract_player_end_summaries()

        # 3. Find missing players (have scores but not in metadata)
        missing_ids = {pid for pid in scored_ids if pid not in slot_names}
        if missing_ids:
            extra = self._extract_slot_names(
                self.get_full_decompressed_data(),
                start=len(self.get_metadata_chunk()),
                target_ids=missing_ids,
            )
            slot_names.update(extra)

        # 4. Apply structurally decoded scored-slot occupants at gameplay start,
        # after the legacy metadata fallback has filled any slots it can prove.
        static_names = slot_names.copy()
        slot_names = self._resolve_match_roster_names(slot_names, scored_ids)
        missing_ids = {pid for pid in scored_ids if pid not in slot_names}

        # 5. Build Player objects
        players = []
        for slot, name in sorted(slot_names.items()):
            players.append(Player(id=slot, name=name))

        # 6. Last resort placeholder for truly missing names
        found_ids = {p.id for p in players}
        for pid in sorted(missing_ids):
            if pid not in found_ids:
                players.append(Player(id=pid, name=f"Player {pid}"))
        players.sort(key=lambda x: x.id)

        # 7. Assign scores, teams, and roles to players
        for player in players:
            if player.id in player_score_map:
                player.score = player_score_map[player.id]
            if player.id in team_map:
                player.team = team_map[player.id]
                player.faction = FACTION_NAMES.get(team_map[player.id])
            if player.id in end_summaries:
                (
                    role,
                    sr0,
                    sr1,
                    sr2,
                    sr3,
                    capturing,
                    fortification,
                    transportation,
                    repair,
                    bridge_laying,
                    unit_damage,
                    tactical_aid,
                    total,
                    recorded_role,
                ) = end_summaries[player.id]
                if role:
                    player.role = role
                elif player.score is not None and player.score > 0:
                    player.role = recorded_role
                player.score_infantry = sr0
                player.score_support = sr1
                player.score_armor = sr2
                player.score_air = sr3
                player.score_capturing = capturing
                player.score_fortification = fortification
                player.score_transportation = transportation
                player.score_repair = repair
                player.score_bridge_laying = bridge_laying
                player.score_unit_damage = unit_damage
                player.score_tactical_aid = tactical_aid
                player.score_total = total

        # player_scores already computed above from single extractScoresByHash() call

        # Match timing. For a late join this recovers true match time, which is
        # longer than the recording — see timing.observed_gameplay_seconds for the span.
        try:
            timing = self.extract_match_timing()
        except Exception:
            timing = MatchTiming()
        duration = timing.match_elapsed_seconds

        self._correct_vacant_result_names(players, static_names)

        # Extract recorder
        rec_slot = self.extract_recorder_slot()
        recorder = None
        if rec_slot is not None:
            for p in players:
                if p.id == rec_slot:
                    recorder = p.name
                    break

        # Extract winner
        winner_team = self.extract_winner()
        winner = FACTION_NAMES.get(winner_team) if winner_team else None

        # Extract domination bar percentage. Assault drives aFactor from attacker
        # progress rather than from the POV team, so its value is not a control
        # split between the two sides.
        bar = (
            self.extract_domination_sample()
            if publishes_control_split(game_info.game_mode)
            else None
        )
        winner_dom_pct = None
        loser_dom_pct = None
        if bar is not None:
            raw = bar[1]
            winner_dom_pct = max(raw, 1.0 - raw)
            loser_dom_pct = min(raw, 1.0 - raw)
        match_ending = self.classify_match_ending(
            timing, winner, bar[1] if bar else None
        )
        domination_shares, domination_anchor = self.resolve_domination_shares(
            bar, players, winner
        )

        # Detect incomplete match results:
        # 1. No valid winner (TeamWins aTeam=0 or missing)
        # 2. Players with scores but no team assignment (late-join recorder)
        # 3. Winner exists but no opposing faction visible (opponent left, etc.)
        has_unknown_scored = any(
            p.score is not None and p.score > 0 and p.faction is None for p in players
        )
        loser_missing = winner is not None and not any(
            p.faction is not None
            and p.faction not in ("Unknown", "Spectator")
            and p.faction != winner
            for p in players
        )
        incomplete = winner is None or has_unknown_scored or loser_missing

        # Preserve historical POV and match inference; correct only result rows.
        self._correct_zero_score_spectators(players, static_names)
        self._mark_result_departures(players, static_names)

        return ReplayData(
            game_info=game_info,
            raw_server_flags=raw_server_flags,
            server_classification=server_classification,
            players=players,
            player_scores=player_scores,
            duration_seconds=duration,
            timing=timing,
            match_ending=match_ending,
            winner_domination_pct=winner_dom_pct,
            loser_domination_pct=loser_dom_pct,
            domination_shares=domination_shares,
            domination_anchor=domination_anchor,
            winner=winner,
            incomplete=incomplete,
            recorder=recorder,
        )


def parse_replay(filepath: str) -> ReplayData:
    """Convenience function to parse a replay file"""
    parser = WICReplayParserV4(filepath)
    return parser.parse()


def print_replay(data):
    duration_str = (
        f"{data.duration_seconds / 60:.1f} min" if data.duration_seconds else "Unknown"
    )

    print(f"Map: {data.game_info.map_display_name} | Mode: {data.game_info.game_mode}")
    print(f"Date/Time: {data.game_info.date_time}")
    print(f"Match length: {duration_str} | Players: {len(data.players)}")
    print(f"Replay length: {data.timing.recording_seconds / 60:.1f} min")
    if not data.timing.captured_match_start and (
        data.timing.joined_at_remaining_seconds is not None
    ):
        joined = data.timing.joined_at_remaining_seconds / 60
        print(f"Recorder joined mid-match, with {joined:.1f} min left on the clock")
    if data.recorder:
        print(f"Recorded by: {data.recorder}")

    if data.incomplete:
        print("Incomplete match results")

    teams = {}
    for p in data.players:
        team_key = p.faction or "Unknown"
        teams.setdefault(team_key, []).append(p)

    # Domination bar (suppressed for incomplete results — faction labels would be misleading)
    if not data.incomplete and data.domination_shares:
        first, second = data.domination_shares
        label = MATCH_ENDING_LABELS.get(data.match_ending, data.match_ending)
        print(
            f"Domination: {first.faction} {format_share(first.pct)}% - "
            f"{second.faction} {format_share(second.pct)}% ({label})"
        )

    print()

    for team_name, team_players in sorted(teams.items()):
        team_sorted = sorted(
            team_players,
            key=lambda p: p.score if p.score is not None else -1,
            reverse=True,
        )
        team_total = sum(p.score for p in team_sorted if p.score is not None)
        if team_name == data.winner:
            result_tag = " - WINNER"
        elif data.winner and team_name not in ("Unknown", "Spectator"):
            result_tag = " - LOSER"
        else:
            result_tag = ""
        print(f"[{team_name}] Total: {team_total}{result_tag}")
        for i, p in enumerate(team_sorted, 1):
            score_str = f"{p.score:5d}" if p.score is not None else "    -"
            role_tags = {
                "infantry": "[INF] ",
                "armor": "[ARM] ",
                "air": "[AIR] ",
                "support": "[SUP] ",
            }
            role_tag = role_tags.get(p.role, "")
            print(f"  {i:2d}. {role_tag}{p.name:<28} {score_str}")
        print()


def collect_replay_files(args):
    import glob

    files = []
    i = 0
    while i < len(args):
        if args[i] == "--dir":
            if i + 1 < len(args):
                dirpath = Path(args[i + 1])
                if dirpath.is_dir():
                    files.extend(sorted(dirpath.glob("*.wicdemo")))
                else:
                    print(f"Error: '{args[i + 1]}' is not a directory", file=sys.stderr)
                i += 2
            else:
                print("Error: --dir requires a path argument", file=sys.stderr)
                i += 1
        else:
            matches = glob.glob(args[i])
            if matches:
                files.extend(
                    Path(m) for m in sorted(matches) if m.lower().endswith(".wicdemo")
                )
            else:
                p = Path(args[i])
                if p.exists():
                    files.append(p)
                else:
                    print(f"Error: file not found: {args[i]}", file=sys.stderr)
            i += 1
    return files


if __name__ == "__main__":
    import sys

    if len(sys.argv) < 2:
        print("World in Conflict Replay Parser v6.0 (Python)")
        print()
        print("Usage:")
        print(
            "  wic_replay_parser.py <file.wicdemo>              Parse a single replay"
        )
        print(
            "  wic_replay_parser.py *.wicdemo                   Parse all replays (glob)"
        )
        print("  wic_replay_parser.py game1.wicdemo game2.wicdemo Parse multiple files")
        print(
            "  wic_replay_parser.py --dir <path>                Parse all replays in directory"
        )
        sys.exit(1)

    files = collect_replay_files(sys.argv[1:])

    if not files:
        print("No .wicdemo files found.", file=sys.stderr)
        sys.exit(1)

    multiple = len(files) > 1

    for i, filepath in enumerate(files):
        if multiple:
            if i > 0:
                print()
            print(f"===== {filepath.name} =====")

        try:
            data = parse_replay(str(filepath))
            print_replay(data)
        except Exception as e:
            print(f"Error parsing {filepath.name}: {e}", file=sys.stderr)
