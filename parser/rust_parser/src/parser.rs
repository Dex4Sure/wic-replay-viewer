/// Core WiC replay parser logic.
/// Shared by CLI (main.rs) and WASM (lib.rs) entry points.
///
/// # .wicdemo File Format
///
/// `.wicdemo` files use **BinTagFormat2** serialization:
/// - 19-byte raw header (uncompressed)
/// - zlib-compressed 16KB chunks (headers: `78 da`, `78 9c`, or `78 01`)
/// - First chunk contains metadata (map path, server name, player names via aSlot entries)
/// - Remaining chunks contain a BinTag event stream (gameplay events, scores, team changes)
///
/// # BinTag Field Format
///
/// Each BinTag field entry is 17 bytes:
/// ```text
/// [hash: 4 bytes] [0x11000000: 4 bytes] [flag: 1 byte] [0x11000000: 4 bytes] [value: 4 bytes]
/// ```
/// Field names are hashed using a modified Adler-32 algorithm (mod 65521),
/// found at VMA `0xa03830` in `wic.exe`.
///
/// # Score Extraction
///
/// Complete primary-chain `SetScore` envelopes serialize their own unsigned `aPos`
/// and signed `aPlayerScore`. Keep the last observation per slot before `TeamWins`,
/// including legitimate negative values and zero resets. Never infer the slot from
/// the following message. If the primary chain does not reach a result or contains
/// no usable scores, retain the legacy counter-based summary extraction. This
/// protects recordings with an incomplete event chain but a readable final summary.
/// Zero score alone does not establish spectator or disconnected status.
///
/// # Player Name Extraction
///
/// Players are extracted structurally using the `aSlot` BinTag hash (`04 02 cc 05`)
/// in the metadata chunk. Each aSlot entry has a fixed layout: slot ID (u32) at
/// offset +13, and the player's UTF-16LE name (null-terminated) at offset +47.
/// Older replays (pre-2011) may pad names with U+00A0 (non-breaking space) before
/// the null terminator.
///
/// For replays with incomplete metadata (common in 2007-2010 era), a fallback searches
/// the concatenated decompressed stream for aSlot entries matching required player IDs,
/// including records split across a zlib chunk boundary. Match-result parsing requests
/// scored slots; timeline parsing also requests chat-sender slots.
///
/// # Team Detection
///
/// Teams are extracted by finding `PlayerJoinedTeam` events in the BinTag event stream.
/// Each event contains the player's slot ID and faction. `SpectatorJoinedTeam` events
/// are also scanned but always treated as team=0 — their `aTeam` field carries the team
/// being *left*, not joined, so using it would poison real assignments (e.g. in replays
/// with extensive lobby team-swapping). For players missing from those events,
/// `UnitCreate` events (which record the owning player and their team) are used as a
/// fallback via majority voting.
///
/// Faction IDs: 0=Spectator, 1=USA, 2=NATO, 3=USSR.
/// USA appears on American maps (Hometown, Seaside, Farmland, ...).
/// NATO appears on European maps (Canal, Mauer, Vineyard, Riviera, ...).
/// USSR appears on all maps.
///
/// # Lobby Slot Swap Detection
///
/// When players swap team slots in the pre-game lobby, the metadata chunk retains the
/// pre-swap name-to-slot mapping while the event stream records the corrected names.
/// The parser scans the first 200KB of event data for `aSlot` entries with names that
/// differ from metadata, then applies only confirmed **bilateral swaps** — slot A's event
/// stream name must match slot B's metadata name AND vice versa. This prevents false
/// positives from garbage data deeper in the file. Affects ~0.4% of replays.
///
/// Older replays can instead reuse a numeric slot before gameplay. For scored result
/// rows, a structurally decoded `PlayerEntersGame` occupant active at the first valid
/// gameplay clock sample supersedes the lobby metadata name. Later replacements do not
/// relabel accumulated results, while unscored, unresolved, vacant, and clockless slots
/// retain the established metadata behavior. Older entry names have trailing U+00A0
/// padding removed.
///
/// # Domination Bar
///
/// The `aFactor` BinTag field (hash `0x0a6f02c1`, type=float) tracks the domination
/// tug-of-war bar position:
/// - Starts at **0.5** (neutral center) at game start
/// - Moves toward **0.0** or **1.0** as one team dominates command points
/// - Updates ~once per second (800-1100+ events per game), quantised to 1/3000
/// - Absolute state, not a delta, so a recording that joins mid-match still reads
///   the true current and final values
/// - `TeamWins` freezes it; no `aFactor` record follows that event in any corpus replay
///
/// It is a **lead** meter, not a control meter: it only moves while one side holds
/// more command points, so a late comeback erases an accumulated lead and a decisive
/// match can legitimately finish near 0.5.
///
/// The value is reported **from the recording player's perspective** and is mirrored
/// when that player changes side, so the anchor is resolved at the final sample (see
/// [`WicReplayParser::pov_team_at`]). Agreement between `sign(aFactor - 0.5)` and
/// "POV team won" holds on 213 of 214 corpus replays; the exception is an Assault map.
///
/// Semantics are mode-dependent:
/// - **Domination** — continuous lead bar, quantised to 1/3000.
/// - **Tug of War** — discrete front line. Starts at 0.5 (neutral) and then snaps
///   onto a fifths grid: 0.2/0.4/0.6/0.8. It emits only on change, so a whole
///   match carries 3-30 samples rather than one per second. POV anchoring and
///   "winner holds the larger share" both hold on 9/9 corpus replays.
/// - **Assault** — works differently. The value tracks attacker progress in sixths
///   rather than a split between the two sides, and POV anchoring does not hold.
///
/// The parser therefore publishes control splits for Domination and Tug of War,
/// and none for Assault. See [`publishes_control_split`].
///
/// # Match Endings
///
/// The countdown clock and the final bar position separate the three endings cleanly:
/// - **Total domination** — bar pinned to exactly 0.0/1.0. 158 corpus replays, all
///   with time still on the clock (median 534 s), none below 2 s.
/// - **Timeout** — clock expired (< 2 s remaining); the bar froze wherever it stood.
/// - **Forfeit** — a real winner, but time left and the bar unpinned.
///
/// # Match Time vs Recording Time
///
/// The countdown reads zero only before it starts, so a recording that observed a
/// zero sample captured the match from its first second. Without one, the recorder
/// joined mid-match: the recording is shorter than the match, and true match time is
/// recovered as `round_length - remaining`. 14 of 254 corpus replays are such late
/// joins, understating duration by up to 9.9 minutes. The clock also ticks past zero
/// into the post-match screen, so it is clamped before subtracting.
///
/// # End-of-match Player Summary
///
/// Player roles and score categories are extracted from end-of-file score summary
/// blocks. Each block contains an `aPos` entry (hash `0x03c90194`, 0-based player
/// index) followed by the category values within ~300 bytes. The role fields are:
/// - `aScoreRole0` (`0x19770420`) = Infantry
/// - `aScoreRole1` (`0x19780421`) = Support
/// - `aScoreRole2` (`0x19790422`) = Armor
/// - `aScoreRole3` (`0x197a0423`) = Air
///
/// The primary role is the one with the highest score. This correctly handles players
/// who switched roles mid-game. The same block also carries the final capturing,
/// fortification, transportation, repair, bridge-laying, unit-damage, tactical-aid,
/// and total score values shown by the game's post-match summary.
///
/// # Recorder Detection
///
/// The POV (point-of-view) player hash (`0xad07934b`) appears in the metadata chunk
/// and stores the slot ID of the player who recorded the replay. The parser resolves
/// that serialized slot through the canonical roster and otherwise retains `None`.
///
/// # Corrupt File Detection
///
/// The parser checks for the presence of a `TeamWins` event (hash `0x0db80329`) in the
/// decompressed data. Valid replays contain at least one TeamWins event; a small number
/// contain two complete end-summary segments. Corrupted files that crash the game on
/// playback have zero. The check runs immediately after decompression, before parsing.
///
/// # Incomplete Match Detection
///
/// After parsing, three conditions indicate incomplete match results:
/// 1. **No valid winner** — TeamWins event has aTeam=0 (map-vote interrupted games)
/// 2. **Unknown scored players** — Players with non-zero scores but no team assignment
///    (recorder joined too late to capture team events)
/// 3. **Missing opposing faction** — Winner exists but no opposing faction visible
///    (opponent left before recorder joined)
///
/// # Known Game Bug
///
/// WiC replays have a known bug where a spectator can appear as a member of one of the
/// playing teams. The parser reads what the file contains, so these show up with a
/// team/faction and score 0. Zero score alone cannot distinguish these cases from
/// disconnected players; narrow corrections require explicit session evidence.
///
/// # Validation
///
/// Current unit, ground-truth, and aggregate corpus results are kept in `README.md`
/// rather than copied into source comments that can drift between corpus revisions.
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};

use flate2::{Decompress, FlushDecompress, Status};
#[cfg(feature = "serde")]
use serde::Serialize;

// ── Map name translation ────────────────────────────────────────────────────

/// Return the community-compatible display name with verified corrections and
/// revision paths normalized to their canonical public name. Unknown paths remain raw.
pub fn map_display_name(internal: &str) -> &str {
    match internal {
        "maps/europe4/europe4.ice" => "do_Riviera",
        "maps/ustown4/ustown4.ice" => "do_Seaside",
        "maps/russia2/russia2.ice" => "do_Powerplant",
        "maps/ustown1/ustown1.ice" => "do_Hometown",
        "maps/usfarmland1/usfarmland1.ice" => "do_Farmland",
        "maps/seattle1/seattle1.ice" => "do_SpaceNeedle",
        "maps/seattle3/seattle3.ice" => "as_Bridge",
        "maps/russia3/russia3.ice" => "do_Quarry",
        "maps/ustown3/ustown3.ice" => "do_Xmas",
        "maps/usfarmland5/usfarmland5.ice" => "do_Countryside",
        "maps/newyork1/newyork1.ice" => "do_Liberty",
        "maps/seattle4/seattle4.ice" => "do_Island",
        "maps/usdesert1/usdesert1.ice" => "do_Silo",
        "maps/europe1/europe1.ice" => "do_Ruins",
        "maps/usdesert2/usdesert2.ice" => "as_AirBase",
        "maps/usfarmland3/usfarmland3.ice" => "do_Riverbed",
        "maps/europe3/europe3.ice" => "do_Vineyard",
        "maps/do_airport/do_airport.ice" => "do_Airport",
        "maps/do_paradise/do_paradise.ice" => "do_Paradise",
        "maps/do_wake/do_wake.ice" => "do_Wake",
        "maps/europe2/europe2.ice" => "as_Hillside",
        "maps/seattle2/seattle2.ice" => "as_Dome",
        "maps/europe5/europe5.ice" => "do_Canal",
        "maps/berlin1/berlin1.ice" => "do_Mauer",
        "maps/do_apocalypse/do_apocalypse.ice" => "do_Apocalypse",
        "maps/do_studio/do_studio.ice" => "do_Studio",
        "maps/norway1/norway1.ice" => "do_Fjord",
        "maps/do_tequila/do_tequila.ice" => "do_Tequila",
        "maps/virginia/virginia.ice" => "do_Virginia",
        "maps/as_ozzault/as_ozzault.ice" => "as_Ozzault",
        "maps/russia4/russia4.ice" => "as_Typhoon",
        "maps/caspian_border_chepoint2/caspian_border_chepoint2.ice" => "do_CaspianWinter",
        "maps/caspian_border_chepoint1/caspian_border_chepoint1.ice" => "do_Caspianborder",
        "maps/do_valley/do_valley.ice" => "do_Valley",
        "maps/do_alaska/do_alaska.ice" => "do_Alaska",
        "maps/do_caspianborder/do_caspianborder.ice" => "do_Caspianborder",
        "maps/do_farmland_night/do_farmland_night.ice" => "do_Farmland_Night",
        "maps/do_hometown_night/do_hometown_night.ice" => "do_Hometown_Night",
        "maps/do_powerplant_night/do_powerplant_night.ice" => "do_Powerplant_Night",
        "maps/do_riverport/do_riverport.ice" => "do_Riverport",
        "maps/do_riviera_night/do_riviera_night.ice" => "do_Riviera_Night",
        "maps/do_seaside_night/do_seaside_night.ice" => "do_Seaside_Night",
        "maps/tw_arizona/tw_arizona.ice" => "tw_Arizona",
        "maps/tw_bocage/tw_bocage.ice" => "tw_Bocage",
        "maps/russia1/russia1.ice" => "tw_Radar",
        "maps/usfarmland4/usfarmland4.ice" => "tw_Highway",
        "maps/airport_03/airport_03.ice" => "do_Airport",
        "maps/bllack_forest/bllack_forest.ice" => "do_BlackForest",
        "maps/airport_v2/airport_v2.ice" => "do_Airport",
        "maps/usfarmland2/usfarmland2.ice" => "tw_Wasteland",
        "maps/helgoland/helgoland.ice" => "do_Helgoland",
        "maps/do_wakebeta2/do_wakebeta2.ice" => "do_Wake",
        _ => internal,
    }
}

// ── BinTag constants ────────────────────────────────────────────────────────

// Message-name hashes. Every one of these was confirmed against the client's own
// writer: walking the callers of `wic.exe:0x009240a0` enumerates all 169 messages
// the game can serialize, and all of these resolve with no mismatch. See the
// parent workspace's `findings/bintag-message-inventory-2026-08-22.md`.
const HASH_PLAYER_JOINED_TEAM: [u8; 4] = [0x4e, 0x06, 0x8e, 0x35];
const HASH_SPECTATOR_JOINED_TEAM: [u8; 4] = [0x96, 0x07, 0x2e, 0x4c];
const HASH_PLAYER_ENTERS_GAME: [u8; 4] = [0x59, 0x06, 0xd5, 0x35];
const HASH_PLAYER_LEAVES_GAME: [u8; 4] = [0x48, 0x06, 0x5b, 0x35];
const HASH_PLAYER_SET_ROLE: [u8; 4] = [0x2c, 0x05, 0xe8, 0x23];
const HASH_UNIT_CREATE: [u8; 4] = [0xf5, 0x03, 0x7e, 0x15];
const HASH_UNIT_DESTROY: [u8; 4] = [0x8b, 0x04, 0x45, 0x1a];
const HASH_BUILDING_DAMAGED: [u8; 4] = [0xd2, 0x05, 0x5e, 0x2e];
const HASH_BUILDING_SET_SLOT_STATE: [u8; 4] = [0xfe, 0x07, 0xcd, 0x52];
const HASH_CREATE_UNIT_RELATION: [u8; 4] = [0x33, 0x07, 0xcd, 0x42];
const HASH_DESTROY_UNIT_RELATIONS: [u8; 4] = [0x3c, 0x08, 0x38, 0x55];
const HASH_DESTROY_UNIT_RELATIONS_UNIT: [u8; 4] = [0x3b, 0x0a, 0x23, 0x84];
const HASH_TEAM_WINS: [u8; 4] = [0x29, 0x03, 0xb8, 0x0d];
// `SetGameModeData_Float`, with an underscore: the literal `SetGameModeDataFloat`
// hashes to `0x4eb1079c`, which appears in no replay.
const HASH_SET_GAME_MODE_DATA_FLOAT: [u8; 4] = [0xfb, 0x07, 0x91, 0x56];
const HASH_SET_COMMAND_POINT_OWNER: [u8; 4] = [0x01, 0x08, 0x4a, 0x52];
const HASH_SHOW_PLAYER_GIVE_TA_NOTIFICATION: [u8; 4] = [0x16, 0x0b, 0x48, 0x9f];
const HASH_SEND_TA_TAUNT: [u8; 4] = [0x2c, 0x04, 0x35, 0x18];
const HASH_MAKE_VOTE: [u8; 4] = [0x1d, 0x03, 0x84, 0x0d];
const HASH_PLAYER_RECEIVE_CHAT: [u8; 4] = [0xb1, 0x06, 0x18, 0x3c];
#[cfg(test)]
const HASH_PLAYER_RECEIVE_CHAT_PRIVATE: [u8; 4] = [0x8c, 0x09, 0x20, 0x76];
// Name-bearing type-5 field inside PlayerEntersGame. The field identifier string is
// absent from the shipped binary tables, so the structurally validated hash is kept
// as an evidence-backed unknown rather than assigned an invented name.
const HASH_PLAYER_ENTRY_NAME: [u8; 4] = [0xa2, 0x01, 0x1e, 0x04];
// `aFactor`, the single field of the `UpdateBalanceFactor` message (`0x49160769`).
const HASH_AFACTOR: [u8; 4] = [0xc1, 0x02, 0x6f, 0x0a];
const HASH_ASLOT: [u8; 4] = [0x04, 0x02, 0xcc, 0x05];
const HASH_ATEAM: [u8; 4] = [0xe9, 0x01, 0x98, 0x05];
const HASH_APLAYER: [u8; 4] = [0xcf, 0x02, 0xd5, 0x0a];
const HASH_ADATA_TYPE: [u8; 4] = [0x7e, 0x03, 0xd6, 0x10];
const HASH_AFLOAT: [u8; 4] = [0x58, 0x02, 0xdd, 0x07];
const HASH_APERSISTENCE_KEY: [u8; 4] = [0x10, 0x06, 0x4b, 0x30];
const HASH_AUNIT: [u8; 4] = [0x02, 0x02, 0xce, 0x05];
const HASH_ATARGET: [u8; 4] = [0xc9, 0x02, 0xc6, 0x0a];
const HASH_ATYPE: [u8; 4] = [0x04, 0x02, 0xea, 0x05];
#[cfg(test)]
const HASH_AN_IS_REPLACEMENT_UNIT_FLAG: [u8; 4] = [0x16, 0x09, 0x91, 0x6c];
const HASH_ASPAWN_SOURCE: [u8; 4] = [0xdc, 0x04, 0x1e, 0x1f];
const HASH_AKILLER: [u8; 4] = [0xc5, 0x02, 0xad, 0x0a];
const HASH_AKILLER_EXPERIENCE: [u8; 4] = [0xcd, 0x06, 0x4e, 0x3c];
const HASH_AHIT_DIRECTION_X: [u8; 4] = [0xce, 0x05, 0xf7, 0x2d];
const HASH_AHIT_DIRECTION_Y: [u8; 4] = [0xcf, 0x05, 0xf8, 0x2d];
const HASH_AHIT_DIRECTION_Z: [u8; 4] = [0xd0, 0x05, 0xf9, 0x2d];
const HASH_AN_EXPLOSION_FORCE: [u8; 4] = [0x80, 0x06, 0x35, 0x37];
const HASH_UNIT_REMOVE: [u8; 4] = [0x0f, 0x04, 0xe5, 0x15];
const HASH_PROJECTILE_HOMING_UNIT_CREATE: [u8; 4] = [0x68, 0x0a, 0x94, 0x8c];
const HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT: [u8; 4] = [0xc4, 0x0d, 0x6a, 0xf0];
const HASH_ASUPPORT_THING: [u8; 4] = [0x59, 0x05, 0x45, 0x25];
const HASH_AROLE_ID: [u8; 4] = [0xa1, 0x02, 0x9a, 0x0a];
const HASH_ANAME: [u8; 4] = [0xe3, 0x01, 0x84, 0x05];
const HASH_ABUILDING_NAME: [u8; 4] = [0x11, 0x05, 0x2f, 0x23];
const HASH_AREAL_SLOT_ID: [u8; 4] = [0x35, 0x04, 0x0f, 0x19];
const HASH_AWILL_OCCUPY_SLOT_FLAG: [u8; 4] = [0x89, 0x07, 0x05, 0x4b];
const HASH_AHEALTH: [u8; 4] = [0xb8, 0x02, 0x6f, 0x0a];
const HASH_ASTATE: [u8; 4] = [0x63, 0x02, 0x2b, 0x08];
const HASH_AFLAG: [u8; 4] = [0xdc, 0x01, 0x6f, 0x05];
const HASH_AFIRST_UNIT: [u8; 4] = [0x0a, 0x04, 0x8a, 0x15];
const HASH_ASECOND_UNIT: [u8; 4] = [0x5e, 0x04, 0x8e, 0x19];
const HASH_AFROM_SLOT: [u8; 4] = [0x98, 0x03, 0x5d, 0x11];
const HASH_ATO_SLOT: [u8; 4] = [0xc7, 0x02, 0xb3, 0x0a];
const HASH_ANUM_TA: [u8; 4] = [0x27, 0x02, 0xd6, 0x07];
const HASH_APLAYER_FROM: [u8; 4] = [0x63, 0x04, 0xca, 0x19];
const HASH_APLAYER_TAUNTED: [u8; 4] = [0xa4, 0x05, 0x9b, 0x29];
const HASH_ATA_ID: [u8; 4] = [0xa4, 0x01, 0xf3, 0x04];
const HASH_ASUPPORT_UPGRADE_LEVEL: [u8; 4] = [0x1f, 0x08, 0xba, 0x55];
const HASH_AVOTE: [u8; 4] = [0x00, 0x02, 0xdc, 0x05];
const HASH_AVALUE_FLOAT: [u8; 4] = [0x55, 0x04, 0x7d, 0x19];
const HASH_AVALUE_INT: [u8; 4] = [0x8a, 0x03, 0x59, 0x11];
const HASH_AVALUE_UINT: [u8; 4] = [0xdf, 0x03, 0x0c, 0x15];
const HASH_AMESSAGE: [u8; 4] = [0x27, 0x03, 0xea, 0x0d];
const HASH_ATEAM_CHAT: [u8; 4] = [0x69, 0x03, 0xb6, 0x10];
const HASH_SET_SCORE: [u8; 4] = [41, 3, 220, 13];
const HASH_SCORE: [u8; 4] = [0xcb, 0x04, 0xa1, 0x1e];
const HASH_APOS: [u8; 4] = [0x94, 0x01, 0xc9, 0x03];
const HASH_SCORE_ROLE0: [u8; 4] = [0x20, 0x04, 0x77, 0x19]; // aScoreRole0 = Infantry
const HASH_SCORE_ROLE1: [u8; 4] = [0x21, 0x04, 0x78, 0x19]; // aScoreRole1 = Support
const HASH_SCORE_ROLE2: [u8; 4] = [0x22, 0x04, 0x79, 0x19]; // aScoreRole2 = Armor
const HASH_SCORE_ROLE3: [u8; 4] = [0x23, 0x04, 0x7a, 0x19]; // aScoreRole3 = Air
const HASH_CAPTURING_SCORE: [u8; 4] = [0x0b, 0x06, 0x9a, 0x2f];
const HASH_FORTIFICATION_SCORE: [u8; 4] = [0xaf, 0x07, 0x1a, 0x4c];
const HASH_TRANSPORTATION_SCORE: [u8; 4] = [0x46, 0x08, 0x91, 0x56];
const HASH_REPAIR_SCORE: [u8; 4] = [0xc1, 0x04, 0x54, 0x1e];
const HASH_BRIDGE_LAYING_SCORE: [u8; 4] = [0x0f, 0x07, 0xc0, 0x41];
const HASH_UNIT_DAMAGE_SCORE: [u8; 4] = [0x3d, 0x06, 0x67, 0x34];
const HASH_TACTICAL_AID_SCORE: [u8; 4] = [0x91, 0x06, 0x82, 0x3a];
const HASH_TOTAL_SCORE: [u8; 4] = [0x62, 0x04, 0xf5, 0x19];
const HASH_POV_PLAYER: [u8; 4] = [0xad, 0x07, 0x93, 0x4b]; // Recorder's aSlot value in metadata
const HASH_MY_GAME_NAME: [u8; 4] = [0xe2, 0x03, 0x8d, 0x15]; // Server/session title
const HASH_REPLAY_NAME: [u8; 4] = [0xef, 0x03, 0x7c, 0x15]; // In-game replay title
const HASH_MY_FPM_MODE_FLAG: [u8; 4] = [0xc9, 0x04, 0x60, 0x21];
const HASH_MY_MATCH_MODE_FLAG: [u8; 4] = [0xd3, 0x05, 0x4e, 0x2f];
const HASH_MY_IS_TOURNAMENT_MATCH_FLAG: [u8; 4] = [0x37, 0x09, 0x3f, 0x70];
const HASH_MY_IS_CLAN_MATCH_FLAG: [u8; 4] = [0x88, 0x06, 0x25, 0x3b];
const HASH_MY_TYPE: [u8; 4] = [0x89, 0x02, 0xf1, 0x08];
const BINTAG_SEP: [u8; 4] = [0x11, 0x00, 0x00, 0x00];

// ── Event envelope constants ────────────────────────────────────────────────

const HASH_EVENT: [u8; 4] = [0x03, 0x02, 0xb5, 0x05];
const HASH_CHANGE_HONORS: [u8; 4] = [0xc0, 0x04, 0x82, 0x1d];
const HASH_SUPPORT_THING_USED: [u8; 4] = [0x89, 0x06, 0x12, 0x38];
const HASH_SUPPORT_THING_SPAWNED_DELAYED: [u8; 4] = [0x82, 0x0a, 0x36, 0x8f];
const HASH_SUPPORT_THING_MARKER: [u8; 4] = [0x5a, 0x07, 0x1f, 0x46];
const HASH_ANID: [u8; 4] = [0x7d, 0x01, 0xc8, 0x03];
const HASH_AN_EVENT_ID: [u8; 4] = [0x7f, 0x03, 0x8c, 0x11];
const HASH_ADELTA: [u8; 4] = [0x4c, 0x02, 0xc1, 0x07];
const HASH_APOSITION_X: [u8; 4] = [0x5d, 0x04, 0x53, 0x1a];
const HASH_APOSITION_Y: [u8; 4] = [0x5e, 0x04, 0x54, 0x1a];
const HASH_APOSITION_Z: [u8; 4] = [0x5f, 0x04, 0x55, 0x1a];
const HASH_A_SUPPORT_UPPGRADE_LEVEL: [u8; 4] = [0x8f, 0x08, 0xae, 0x5e];
const HASH_ADIRECTION_X: [u8; 4] = [0xa9, 0x04, 0x1c, 0x1e];
const HASH_ADIRECTION_Y: [u8; 4] = [0xaa, 0x04, 0x1d, 0x1e];
const HASH_ADIRECTION_Z: [u8; 4] = [0xab, 0x04, 0x1e, 0x1e];
const HASH_ATIME: [u8; 4] = [0xf1, 0x01, 0xb4, 0x05];
const HASH_ATIME_SINCE_CREATION: [u8; 4] = [0x18, 0x07, 0x09, 0x42];
const HASH_ASPECTATOR_LOS: [u8; 4] = [0x45, 0x05, 0x42, 0x24];

const ENVELOPE_TYPE: [u8; 4] = [0x15, 0x00, 0x00, 0x00];
const ENVELOPE_ARRAY_FLAG: u8 = 0x06;
/// Bytes from an envelope's start to its message-name hash.
const ENVELOPE_MESSAGE_OFFSET: usize = 17;
/// Total size of a `ChangeHonors` envelope: 21-byte header plus one `aDelta` field.
const CHANGE_HONORS_ENVELOPE_BYTES: usize = 38;

const FACTION_NAMES: [&str; 4] = ["Spectator", "USA", "NATO", "USSR"];

const TIMELINE_SCHEMA_VERSION: u32 = 18;

/// Shipped multiplayer infantry actors that are members of a squad rather than
/// independently purchased units. The replay creates and destroys these actors
/// separately from their `*_Squad_*` parent unit.
///
/// This is an explicit catalogue, not a name heuristic: the IDs are Adler-32
/// hashes of the definitions in `units/unittypes_wic.ice/.loc` from wic_ds.sdf.
const INFANTRY_SOLDIER_TYPE_IDS: &[u32] = &[
    0x0ca5_02ea, // US_Medic
    0x0e97_0333, // F1_Marine
    0x107f_0379, // US_Sniper
    0x11d8_0374, // NATO_Medic
    0x12cd_038f, // USSR_Medic
    0x15eb_03ee, // NATO_Marine
    0x163c_0403, // NATO_Sniper
    0x16fb_0409, // USSR_Marine
    0x174c_041e, // USSR_Sniper
    0x17d0_0422, // US_AntiTank
    0x180b_0435, // US_Engineer
    0x1ea1_04ac, // NATO_AntiTank
    0x1edc_04bf, // NATO_Engineer
    0x1fe7_04c7, // USSR_AntiTank
    0x2022_04da, // USSR_Engineer
    0x240f_0534, // US_AA_Infantry
    0x2c7e_05be, // NATO_AA_Infantry
    0x2e15_05d9, // USSR_AA_Infantry
    0x38a0_068b, // US_Machine_Gunner
    0x3f79_06e0, // US_Medic_w_Special
    0x42ad_0715, // NATO_Machine_Gunner
    0x4495_0730, // USSR_Machine_Gunner
    0x4a10_076a, // NATO_Medic_w_Special
    0x4c13_0785, // USSR_Medic_w_Special
    0x5b00_0845, // F1_Marine_Copy_NoScore
    0x6bd3_0900, // NATO_Marine_Copy_NoScore
    0x6e42_091b, // USSR_Marine_Copy_NoScore
];

pub fn is_infantry_soldier_type(unit_type_id: u32) -> bool {
    INFANTRY_SOLDIER_TYPE_IDS
        .binary_search(&unit_type_id)
        .is_ok()
}

const EXACT_TARGET_PROJECTILE_LOOKBACK_SECONDS: f32 = 0.5;
const EXACT_TARGET_SUPPORT_PROJECTILE_LOOKBACK_SECONDS: f32 = 3.0;
const SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS: f32 = 30.0;
const KILLER_UNIT_SENTINEL: u32 = 512;
const CLOCK_RESET_THRESHOLD_SECONDS: f32 = 5.0;
const MAX_CLOCK_SECONDS: f32 = 24.0 * 60.0 * 60.0;
const DOMINATION_SAMPLE_INTERVAL_SECONDS: f32 = 5.0;
/// Configured Domination round lengths observed in the corpus, ascending.
/// Used to recover the true round length when a recording joined mid-match and
/// therefore never observed the countdown's starting value.
const STANDARD_ROUND_LENGTHS: [f32; 4] = [600.0, 900.0, 1200.0, 1800.0];
/// Clock remaining at or below this means the countdown expired: the match was
/// decided on the timer rather than ending early.
const TIMEOUT_REMAINING_SECONDS: f32 = 2.0;
/// The domination bar is pinned to a stop, i.e. one team reached total domination.
const DOMINATION_EXTREME_EPSILON: f64 = 0.0005;
/// How close a value must land to `1 - v` to confirm a POV frame change at a
/// point where the recorder's team assignment actually changed.
const MIRROR_EPSILON: f32 = 0.01;
const MAX_CHAT_UTF16_BYTES: usize = 2_050;
const MAX_PLAYER_NAME_UTF16_BYTES: usize = 256;
const SLOT_NAME_RECORD_MAX_BYTES: usize = 47 + 30 * 2;

const MAX_CHUNK_DECOMPRESSED_BYTES: usize = 64 * 1024;
pub const MAX_REPLAY_COMPRESSED_BYTES: usize = 64 * 1024 * 1024;
const MAX_INDEXED_EVENTS: usize = 1_000_000;
const MAX_UNIT_DROP_ATTRIBUTION_COMPARISONS: usize = 1_000_000;
const MAX_ATTRIBUTION_SOURCE_EVENTS: usize = 100_000;

#[derive(Clone, Copy)]
struct DecompressionLimits {
    max_compressed_bytes: usize,
    max_decompressed_bytes: usize,
    max_zlib_chunks: usize,
    max_zlib_candidates: usize,
    max_zlib_work_bytes: usize,
}

const DECOMPRESSION_LIMITS: DecompressionLimits = DecompressionLimits {
    // Known valid corpus maxima reach 17.82 MiB compressed, 94.76 MiB / 6,065
    // chunks decompressed, and 6,873 raw zlib-header candidates. These limits
    // bound memory and repeated failed-inflate CPU work without rejecting them.
    max_compressed_bytes: MAX_REPLAY_COMPRESSED_BYTES,
    max_decompressed_bytes: 96 * 1024 * 1024,
    max_zlib_chunks: 6_144,
    max_zlib_candidates: 16_384,
    max_zlib_work_bytes: 256 * 1024 * 1024,
};

const TAG_PLAYER_JOINED_TEAM: u32 = u32::from_le_bytes(HASH_PLAYER_JOINED_TEAM);
const TAG_SPECTATOR_JOINED_TEAM: u32 = u32::from_le_bytes(HASH_SPECTATOR_JOINED_TEAM);
const TAG_PLAYER_ENTERS_GAME: u32 = u32::from_le_bytes(HASH_PLAYER_ENTERS_GAME);
const TAG_PLAYER_LEAVES_GAME: u32 = u32::from_le_bytes(HASH_PLAYER_LEAVES_GAME);
const TAG_PLAYER_SET_ROLE: u32 = u32::from_le_bytes(HASH_PLAYER_SET_ROLE);
const TAG_UNIT_CREATE: u32 = u32::from_le_bytes(HASH_UNIT_CREATE);
const TAG_UNIT_DESTROY: u32 = u32::from_le_bytes(HASH_UNIT_DESTROY);
const TAG_UNIT_REMOVE: u32 = u32::from_le_bytes(HASH_UNIT_REMOVE);
const TAG_BUILDING_DAMAGED: u32 = u32::from_le_bytes(HASH_BUILDING_DAMAGED);
const TAG_BUILDING_SET_SLOT_STATE: u32 = u32::from_le_bytes(HASH_BUILDING_SET_SLOT_STATE);
const TAG_CREATE_UNIT_RELATION: u32 = u32::from_le_bytes(HASH_CREATE_UNIT_RELATION);
const TAG_DESTROY_UNIT_RELATIONS: u32 = u32::from_le_bytes(HASH_DESTROY_UNIT_RELATIONS);
const TAG_DESTROY_UNIT_RELATIONS_UNIT: u32 = u32::from_le_bytes(HASH_DESTROY_UNIT_RELATIONS_UNIT);
const TAG_PROJECTILE_HOMING_UNIT_CREATE: u32 =
    u32::from_le_bytes(HASH_PROJECTILE_HOMING_UNIT_CREATE);
const TAG_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT: u32 =
    u32::from_le_bytes(HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT);
const TAG_TEAM_WINS: u32 = u32::from_le_bytes(HASH_TEAM_WINS);
const TAG_SET_GAME_MODE_DATA_FLOAT: u32 = u32::from_le_bytes(HASH_SET_GAME_MODE_DATA_FLOAT);
const TAG_SET_COMMAND_POINT_OWNER: u32 = u32::from_le_bytes(HASH_SET_COMMAND_POINT_OWNER);
const TAG_SHOW_PLAYER_GIVE_TA_NOTIFICATION: u32 =
    u32::from_le_bytes(HASH_SHOW_PLAYER_GIVE_TA_NOTIFICATION);
const TAG_SEND_TA_TAUNT: u32 = u32::from_le_bytes(HASH_SEND_TA_TAUNT);
const TAG_MAKE_VOTE: u32 = u32::from_le_bytes(HASH_MAKE_VOTE);
const TAG_PLAYER_RECEIVE_CHAT: u32 = u32::from_le_bytes(HASH_PLAYER_RECEIVE_CHAT);
const TAG_AFACTOR: u32 = u32::from_le_bytes(HASH_AFACTOR);
const TAG_ASLOT: u32 = u32::from_le_bytes(HASH_ASLOT);
const TAG_SCORE: u32 = u32::from_le_bytes(HASH_SCORE);
const TAG_APOS: u32 = u32::from_le_bytes(HASH_APOS);
const TAG_SUPPORT_THING_USED: u32 = u32::from_le_bytes(HASH_SUPPORT_THING_USED);
const TAG_SUPPORT_THING_SPAWNED_DELAYED: u32 =
    u32::from_le_bytes(HASH_SUPPORT_THING_SPAWNED_DELAYED);
const TAG_SUPPORT_THING_MARKER: u32 = u32::from_le_bytes(HASH_SUPPORT_THING_MARKER);

/// Four-byte BinTag occurrences needed by the match and timeline parsers.
///
/// The replay stream is byte-oriented rather than aligned, so the index keeps the
/// exact offsets produced by the previous overlapping `windows(4)` searches.
///
/// Variant names follow Rust casing, so three differ from the serialized message
/// name they index. Those carry the exact wire name; the rest match. The full
/// 169-message inventory, taken from the callers of `wic.exe:0x009240a0`, is in
/// the parent workspace's `findings/bintag-message-inventory-2026-08-22.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexedTag {
    PlayerJoinedTeam,
    SpectatorJoinedTeam,
    PlayerEntersGame,
    PlayerLeavesGame,
    PlayerSetRole,
    UnitCreate,
    UnitDestroy,
    UnitRemove,
    BuildingDamaged,
    BuildingSetSlotState,
    CreateUnitRelation,
    DestroyUnitRelations,
    DestroyUnitRelationsUnit,
    ProjectileHomingUnitCreate,
    ProjectileHomingSupportCreateUnit,
    TeamWins,
    /// Wire name `SetGameModeData_Float`, with an underscore. The countdown
    /// clock is its `aDataType == 1` records.
    SetGameModeDataFloat,
    SetCommandPointOwner,
    /// Wire name `ShowPlayerGiveTANotification`.
    ShowPlayerGiveTaNotification,
    /// Wire name `SendTATaunt`.
    SendTaTaunt,
    MakeVote,
    PlayerReceiveChat,
    AFactor,
    ASlot,
    Score,
    APos,
    SupportThingUsed,
    SupportThingSpawnedDelayed,
    SupportThingMarker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndexedOffset {
    offset: usize,
    tag: IndexedTag,
}

#[derive(Debug)]
struct EventIndex {
    offsets: Vec<IndexedOffset>,
}

impl EventIndex {
    fn new(data: &[u8]) -> Result<Self, String> {
        Self::new_with_limit(data, MAX_INDEXED_EVENTS)
    }

    fn new_with_limit(data: &[u8], max_events: usize) -> Result<Self, String> {
        let mut offsets = Vec::new();
        for (offset, bytes) in data.windows(4).enumerate() {
            let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let tag = match value {
                TAG_PLAYER_JOINED_TEAM => IndexedTag::PlayerJoinedTeam,
                TAG_SPECTATOR_JOINED_TEAM => IndexedTag::SpectatorJoinedTeam,
                TAG_PLAYER_ENTERS_GAME => IndexedTag::PlayerEntersGame,
                TAG_PLAYER_LEAVES_GAME => IndexedTag::PlayerLeavesGame,
                TAG_PLAYER_SET_ROLE => IndexedTag::PlayerSetRole,
                TAG_UNIT_CREATE => IndexedTag::UnitCreate,
                TAG_UNIT_DESTROY => IndexedTag::UnitDestroy,
                TAG_UNIT_REMOVE => IndexedTag::UnitRemove,
                TAG_BUILDING_DAMAGED => IndexedTag::BuildingDamaged,
                TAG_BUILDING_SET_SLOT_STATE => IndexedTag::BuildingSetSlotState,
                TAG_CREATE_UNIT_RELATION => IndexedTag::CreateUnitRelation,
                TAG_DESTROY_UNIT_RELATIONS => IndexedTag::DestroyUnitRelations,
                TAG_DESTROY_UNIT_RELATIONS_UNIT => IndexedTag::DestroyUnitRelationsUnit,
                TAG_PROJECTILE_HOMING_UNIT_CREATE => IndexedTag::ProjectileHomingUnitCreate,
                TAG_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT => {
                    IndexedTag::ProjectileHomingSupportCreateUnit
                }
                TAG_TEAM_WINS => IndexedTag::TeamWins,
                TAG_SET_GAME_MODE_DATA_FLOAT => IndexedTag::SetGameModeDataFloat,
                TAG_SET_COMMAND_POINT_OWNER => IndexedTag::SetCommandPointOwner,
                TAG_SHOW_PLAYER_GIVE_TA_NOTIFICATION => IndexedTag::ShowPlayerGiveTaNotification,
                TAG_SEND_TA_TAUNT => IndexedTag::SendTaTaunt,
                TAG_MAKE_VOTE => IndexedTag::MakeVote,
                TAG_PLAYER_RECEIVE_CHAT => IndexedTag::PlayerReceiveChat,
                TAG_AFACTOR => IndexedTag::AFactor,
                TAG_ASLOT => IndexedTag::ASlot,
                TAG_SCORE => IndexedTag::Score,
                TAG_APOS => IndexedTag::APos,
                TAG_SUPPORT_THING_USED => IndexedTag::SupportThingUsed,
                TAG_SUPPORT_THING_SPAWNED_DELAYED => IndexedTag::SupportThingSpawnedDelayed,
                TAG_SUPPORT_THING_MARKER => IndexedTag::SupportThingMarker,
                _ => continue,
            };
            if offsets.len() >= max_events {
                return Err(format!(
                    "Replay exceeds the maximum of {max_events} indexed events"
                ));
            }
            offsets.push(IndexedOffset { offset, tag });
        }
        Ok(Self { offsets })
    }

    fn positions(&self, tag: IndexedTag) -> impl DoubleEndedIterator<Item = usize> + Clone + '_ {
        self.offsets
            .iter()
            .filter_map(move |entry| (entry.tag == tag).then_some(entry.offset))
    }

    fn offsets(&self) -> &[IndexedOffset] {
        &self.offsets
    }
}

/// Bound the live queues and nested uniqueness maps used by exact projectile
/// and Tactical Aid attribution. In the worst case every relevant source event
/// has the same timestamp, so none can expire before the replay ends.
fn validate_attribution_source_event_budget(event_index: &EventIndex) -> Result<(), String> {
    validate_attribution_source_event_budget_with_limit(event_index, MAX_ATTRIBUTION_SOURCE_EVENTS)
}

fn validate_attribution_source_event_budget_with_limit(
    event_index: &EventIndex,
    maximum: usize,
) -> Result<(), String> {
    let mut count = 0usize;
    for indexed in event_index.offsets() {
        if matches!(
            indexed.tag,
            IndexedTag::ProjectileHomingUnitCreate
                | IndexedTag::SupportThingSpawnedDelayed
                | IndexedTag::ProjectileHomingSupportCreateUnit
        ) {
            count = count
                .checked_add(1)
                .ok_or_else(|| "Attribution source event count overflows".to_owned())?;
            if count > maximum {
                return Err(format!(
                    "Replay exceeds the maximum of {maximum} attribution source events"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod event_index_tests {
    use super::*;

    const CASES: [(IndexedTag, [u8; 4]); 29] = [
        (IndexedTag::PlayerJoinedTeam, HASH_PLAYER_JOINED_TEAM),
        (IndexedTag::SpectatorJoinedTeam, HASH_SPECTATOR_JOINED_TEAM),
        (IndexedTag::PlayerEntersGame, HASH_PLAYER_ENTERS_GAME),
        (IndexedTag::PlayerLeavesGame, HASH_PLAYER_LEAVES_GAME),
        (IndexedTag::PlayerSetRole, HASH_PLAYER_SET_ROLE),
        (IndexedTag::UnitCreate, HASH_UNIT_CREATE),
        (IndexedTag::UnitDestroy, HASH_UNIT_DESTROY),
        (IndexedTag::UnitRemove, HASH_UNIT_REMOVE),
        (IndexedTag::BuildingDamaged, HASH_BUILDING_DAMAGED),
        (
            IndexedTag::BuildingSetSlotState,
            HASH_BUILDING_SET_SLOT_STATE,
        ),
        (IndexedTag::CreateUnitRelation, HASH_CREATE_UNIT_RELATION),
        (
            IndexedTag::DestroyUnitRelations,
            HASH_DESTROY_UNIT_RELATIONS,
        ),
        (
            IndexedTag::DestroyUnitRelationsUnit,
            HASH_DESTROY_UNIT_RELATIONS_UNIT,
        ),
        (
            IndexedTag::ProjectileHomingUnitCreate,
            HASH_PROJECTILE_HOMING_UNIT_CREATE,
        ),
        (
            IndexedTag::ProjectileHomingSupportCreateUnit,
            HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT,
        ),
        (IndexedTag::TeamWins, HASH_TEAM_WINS),
        (
            IndexedTag::SetGameModeDataFloat,
            HASH_SET_GAME_MODE_DATA_FLOAT,
        ),
        (
            IndexedTag::SetCommandPointOwner,
            HASH_SET_COMMAND_POINT_OWNER,
        ),
        (
            IndexedTag::ShowPlayerGiveTaNotification,
            HASH_SHOW_PLAYER_GIVE_TA_NOTIFICATION,
        ),
        (IndexedTag::SendTaTaunt, HASH_SEND_TA_TAUNT),
        (IndexedTag::MakeVote, HASH_MAKE_VOTE),
        (IndexedTag::PlayerReceiveChat, HASH_PLAYER_RECEIVE_CHAT),
        (IndexedTag::AFactor, HASH_AFACTOR),
        (IndexedTag::ASlot, HASH_ASLOT),
        (IndexedTag::Score, HASH_SCORE),
        (IndexedTag::APos, HASH_APOS),
        (IndexedTag::SupportThingUsed, HASH_SUPPORT_THING_USED),
        (
            IndexedTag::SupportThingSpawnedDelayed,
            HASH_SUPPORT_THING_SPAWNED_DELAYED,
        ),
        (IndexedTag::SupportThingMarker, HASH_SUPPORT_THING_MARKER),
    ];

    #[test]
    fn single_pass_index_matches_overlapping_naive_searches() {
        let mut data = vec![0xff, 0x78, 0x00];
        for (_, pattern) in CASES {
            data.extend_from_slice(&pattern);
            data.push(pattern[0]);
        }
        for (_, pattern) in CASES.iter().rev() {
            data.extend_from_slice(pattern);
        }

        let index = EventIndex::new(&data).expect("bounded event index");
        for (tag, pattern) in CASES {
            let expected: Vec<usize> = find_all(&data, &pattern).collect();
            let actual: Vec<usize> = index.positions(tag).collect();
            assert_eq!(actual, expected, "index mismatch for {tag:?}");
        }
        assert!(
            index
                .offsets()
                .windows(2)
                .all(|pair| pair[0].offset <= pair[1].offset),
            "event index must retain replay byte order"
        );
    }

    #[test]
    fn rejects_more_indexed_events_than_the_budget() {
        let mut data = Vec::new();
        data.extend_from_slice(&HASH_UNIT_CREATE);
        data.extend_from_slice(&HASH_UNIT_DESTROY);
        data.extend_from_slice(&HASH_TEAM_WINS);

        let error = EventIndex::new_with_limit(&data, 2).expect_err("event index limit");
        assert!(error.contains("maximum of 2 indexed events"));
    }

    #[test]
    fn rejects_more_attribution_source_events_than_the_live_state_budget() {
        let mut data = Vec::new();
        data.extend_from_slice(&HASH_PROJECTILE_HOMING_UNIT_CREATE);
        data.extend_from_slice(&HASH_SUPPORT_THING_SPAWNED_DELAYED);
        data.extend_from_slice(&HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT);

        let index = EventIndex::new(&data).expect("bounded event index");
        let error = validate_attribution_source_event_budget_with_limit(&index, 2)
            .expect_err("attribution source event limit");
        assert!(error.contains("maximum of 2 attribution source events"));
    }
}

fn faction_name(id: u32) -> &'static str {
    FACTION_NAMES.get(id as usize).unwrap_or(&"Unknown")
}

/// Re-express a POV-relative bar curve in one consistent frame.
///
/// `aFactor` is reported from the recording player's perspective and is mirrored
/// when that player changes side, which makes the raw curve teleport across the
/// centreline mid-match.
///
/// A frame change is only considered where the recorder actually changed team,
/// and is only applied when the value there really is a mirror. Detecting it from
/// the values alone is unsound: a mode that moves a front line in discrete steps
/// makes a genuine move across the centre (0.6 -> 0.4) exactly its own mirror.
///
/// The whole curve is anchored to the frame of its **final** sample, so it agrees
/// with the end-of-match shares.
fn unmirror_bar_curve(samples: &mut [(usize, f32)], team_changes: &[usize]) {
    if samples.len() < 2 || team_changes.is_empty() {
        return;
    }
    let mut flipped = false;
    let mut flips = Vec::with_capacity(samples.len());
    flips.push(false);
    let mut team_change_index = team_changes.partition_point(|offset| *offset <= samples[0].0);
    for index in 1..samples.len() {
        let (_, previous) = samples[index - 1];
        let (current_offset, current) = samples[index];
        let changes_before = team_change_index;
        while team_changes
            .get(team_change_index)
            .is_some_and(|offset| *offset <= current_offset)
        {
            team_change_index += 1;
        }
        let changed_team = team_change_index > changes_before;
        if changed_team && (current - (1.0 - previous)).abs() < MIRROR_EPSILON {
            flipped = !flipped;
        }
        flips.push(flipped);
    }

    // Anchor to the final frame so the curve matches the reported final split.
    let final_flipped = *flips.last().unwrap_or(&false);
    for ((_, value), flip) in samples.iter_mut().zip(flips) {
        if flip != final_flipped {
            *value = 1.0 - *value;
        }
    }
}

/// Offsets at which the POV player's team assignment changed.
impl WicReplayParser {
    fn recorder_team_change_offsets(&self) -> Vec<usize> {
        let Some(slot) = self.recorder_slot else {
            return Vec::new();
        };
        let data = &self.full_data;
        let mut offsets = Vec::new();
        let mut current: Option<u32> = None;
        for entry in self.event_index.offsets() {
            let is_spectator = match entry.tag {
                IndexedTag::PlayerJoinedTeam => false,
                IndexedTag::SpectatorJoinedTeam => true,
                _ => continue,
            };
            let pos = entry.offset;
            let block = &data[pos..data.len().min(pos + 200)];
            let (Some(slot_off), Some(team_off)) = (
                find_pattern(block, &HASH_ASLOT, 0),
                find_pattern(block, &HASH_ATEAM, 0),
            ) else {
                continue;
            };
            let Some(event_slot) = read_bintag_u32(data, pos + slot_off) else {
                continue;
            };
            if event_slot != slot {
                continue;
            }
            // SpectatorJoinedTeam carries the team being left, not joined.
            let team = if is_spectator {
                Some(0)
            } else {
                read_bintag_u32(data, pos + team_off)
            };
            if team.is_some() && team != current {
                if current.is_some() {
                    offsets.push(pos);
                }
                current = team;
            }
        }
        offsets
    }
}

/// Percentage text carrying every digit the bar actually resolves.
///
/// The bar is quantised to 1/3000, i.e. steps of 0.0333%, so rounding to whole
/// percent collapses a real result like 50.07 / 49.93 into a false 50 / 50 tie.
/// Two decimals is the game's own precision; trailing zeros are dropped so a
/// clean value still reads as "58" rather than "58.00".
pub fn format_share(pct: f64) -> String {
    let text = format!("{:.2}", pct * 100.0);
    match text.contains('.') {
        true => text.trim_end_matches('0').trim_end_matches('.').to_owned(),
        false => text,
    }
}

/// Whether `aFactor` is a two-sided control split for this mode.
///
/// True for Domination (continuous lead bar) and Tug of War (discrete front
/// line). False for Assault, which works differently: its value tracks attacker
/// progress, so it does not describe a split between the two teams.
pub fn publishes_control_split(game_mode: &str) -> bool {
    matches!(game_mode, "Domination" | "Tug of War")
}

// ── Data structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Player {
    /// Recording-clock departure time, proven for this result-row identity.
    pub left_at_seconds: Option<f32>,
    pub id: u32,
    pub name: String,
    pub team: Option<u32>,
    pub faction: Option<String>,
    pub score: Option<i32>,
    pub role: Option<String>,
    pub score_infantry: Option<i32>,
    pub score_support: Option<i32>,
    pub score_armor: Option<i32>,
    pub score_air: Option<i32>,
    pub score_capturing: Option<i32>,
    pub score_fortification: Option<i32>,
    pub score_transportation: Option<i32>,
    pub score_repair: Option<i32>,
    pub score_bridge_laying: Option<i32>,
    pub score_unit_damage: Option<i32>,
    pub score_tactical_aid: Option<i32>,
    pub score_total: Option<i32>,
}

impl Player {
    fn has_zero_stats(&self) -> bool {
        [
            self.score,
            self.score_infantry,
            self.score_support,
            self.score_armor,
            self.score_air,
            self.score_capturing,
            self.score_fortification,
            self.score_transportation,
            self.score_repair,
            self.score_bridge_laying,
            self.score_unit_damage,
            self.score_tactical_aid,
            self.score_total,
        ]
        .iter()
        .all(|score| *score == Some(0))
    }
}

/// Opt-in viewer corrections; raw `ReplayData` remains unchanged.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct PlayerResultEvidence {
    pub player_id: u32,
    pub score_before_leave: Option<ScoreBeforeLeave>,
}

/// Last absolute score observed in the named session, before an explicit leave.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ScoreBeforeLeave {
    pub score: i32,
    pub observed_at_seconds: f32,
    pub left_at_seconds: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PlayerEndSummary {
    role: String,
    recorded_role: Option<String>,
    infantry: i32,
    support: i32,
    armor: i32,
    air: i32,
    capturing: i32,
    fortification: i32,
    transportation: i32,
    repair: i32,
    bridge_laying: i32,
    unit_damage: i32,
    tactical_aid: i32,
    total: i32,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct GameInfo {
    pub map_name: String,
    pub map_display_name: String,
    pub server_name: String,
    pub date_time: String,
    pub replay_name: Option<String>,
    pub game_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct RawServerFlags {
    pub few_player_mode_flag: Option<bool>,
    pub match_mode_flag: Option<bool>,
    pub tournament_match_flag: Option<bool>,
    pub clan_match_flag: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ServerClassification {
    pub few_player_mode: Option<bool>,
    pub match_mode: Option<bool>,
    pub has_bots: Option<bool>,
    pub clan_match: Option<bool>,
    pub tournament_match: Option<bool>,
    /// Ranked is advertised through server-browser state but has no confirmed
    /// replay-side signal. Keep it unknown rather than inferring from exclusions.
    pub ranked: Option<bool>,
}

/// How the match ended, recovered from the countdown clock and the final
/// domination bar position.
///
/// The two signals separate cleanly across the corpus: every replay whose bar
/// is pinned to a stop still had time on the clock (median 534 s remaining,
/// none below 2 s), while timer finishes stop with the bar wherever it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum MatchEnding {
    /// The bar reached a stop and the match was cut short.
    TotalDomination,
    /// The countdown expired; the bar froze wherever it stood.
    Timeout,
    /// A real result, but the clock still had time and the bar was not pinned —
    /// a team quit or the server dropped the match.
    Forfeit,
    /// No usable winner, or no clock to classify against.
    Unknown,
}

/// Match timing separated from recording timing.
///
/// `aFactor` and the countdown are absolute state broadcasts, so a recording
/// that joins late still observes the true remaining clock. What it cannot
/// observe is how long the match had already been running, which is why
/// `observed_gameplay_seconds` and `match_elapsed_seconds` differ for a late join.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct MatchTiming {
    /// The recording observed the pre-match countdown, so it covers the match
    /// from its first second.
    pub captured_match_start: bool,
    /// Real length of the replay file: the highest envelope timestamp in the
    /// primary event chain. Counts the pre-match lobby and the post-`TeamWins`
    /// tail, so it is the correct axis for playback and the honest answer to
    /// "how long is this replay". Corpus median exceeds
    /// `observed_gameplay_seconds` by 76 s.
    pub recording_seconds: f32,
    /// Gameplay observed between the first and last countdown sample. Derived
    /// from the game-mode clock, so it excludes lobby and post-match time and
    /// is not a wall-clock span.
    pub observed_gameplay_seconds: Option<f32>,
    /// Match time elapsed when the recording ended. Equals `observed_gameplay_seconds`
    /// when the recording captured the start.
    pub match_elapsed_seconds: Option<f32>,
    /// Configured round length. Observed directly when the recording captured
    /// the start; otherwise inferred from `STANDARD_ROUND_LENGTHS`.
    pub round_length_seconds: Option<f32>,
    /// False when `round_length_seconds` was inferred rather than observed.
    pub round_length_exact: bool,
    /// Clock remaining when the recorder joined, for a late join.
    pub joined_at_remaining_seconds: Option<f32>,
    /// Clock remaining at the final observed countdown sample.
    pub final_remaining_seconds: Option<f32>,
}

/// One faction's share of the final domination bar.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct DominationShare {
    pub faction: String,
    pub pct: f64,
}

/// Which side the raw `aFactor` value was resolved against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum DominationAnchor {
    /// Resolved from the POV player's team at the final bar sample.
    PovTeam,
    /// POV team unavailable (spectator recorder, or no roster team); the larger
    /// share was assigned to the `TeamWins` winner instead. Sound because the
    /// winning side of the bar decides a Domination match.
    WinnerInferred,
}

impl MatchEnding {
    pub fn label(self) -> &'static str {
        match self {
            MatchEnding::TotalDomination => "total domination",
            MatchEnding::Timeout => "on the timer",
            MatchEnding::Forfeit => "forfeit",
            MatchEnding::Unknown => "unknown ending",
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ReplayData {
    pub game_info: GameInfo,
    pub raw_server_flags: RawServerFlags,
    pub server_classification: ServerClassification,
    pub players: Vec<Player>,
    pub player_scores: Vec<i32>,
    /// Match time elapsed at the end of the recording. For a late join this is
    /// the true match duration, not the length of the recording — see
    /// `timing.observed_gameplay_seconds` for that.
    pub duration_seconds: Option<f32>,
    pub timing: MatchTiming,
    pub match_ending: MatchEnding,
    pub winner_domination_pct: Option<f64>,
    pub loser_domination_pct: Option<f64>,
    /// Final bar split attributed to concrete factions. Domination and Tug of
    /// War only; see [`publishes_control_split`].
    pub domination_shares: Option<Vec<DominationShare>>,
    pub domination_anchor: Option<DominationAnchor>,
    pub winner: Option<String>,
    pub incomplete: bool,
    pub recorder: Option<String>,
}

/// Versioned, replay-relative data intended for timeline visualizations.
///
/// Every timestamp is recording-elapsed time in seconds, measured from the
/// first `Event` envelope in the file. `duration_seconds` is the full length of
/// the recording on that axis, so it bounds every event, phase, and sample.
///
/// `match_duration_seconds` is the separate, countdown-derived question of how
/// long the match ran. It is smaller than `duration_seconds` whenever the
/// recording caught lobby time before the match or kept running after it, and
/// it describes only the recorded tail of a match when recording began after a
/// phase had already started.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelineData {
    pub schema_version: u32,
    /// Length of the recording: the axis every other timestamp lives on.
    pub duration_seconds: f32,
    /// Gameplay observed between the first and last countdown sample.
    pub match_duration_seconds: f32,
    pub initial_clock_seconds: Option<f32>,
    pub final_clock_seconds: Option<f32>,
    pub phases: Vec<TimelinePhase>,
    /// Bar curve in a single consistent frame, anchored to the final sample.
    pub domination_samples: Vec<TimelineValueSample>,
    /// Faction `domination_samples` is measured for, when the POV team resolves.
    /// `None` for a spectator recording: the curve is still internally consistent,
    /// just not attributable to a named side.
    pub domination_anchor_faction: Option<String>,
    pub participants: Vec<TimelineParticipant>,
    pub participant_sessions: Vec<TimelineParticipantSession>,
    /// Complete units and squad-parent objects only. Individual infantry actors
    /// are retained separately in `infantry_soldier_deaths`.
    pub events: Vec<TimelineEvent>,
    pub infantry_soldier_deaths: Vec<InfantrySoldierDeath>,
    pub pre_match_chat: Vec<PreMatchChatMessage>,
    pub post_match_chat: Vec<PostMatchChatMessage>,
    pub coverage: TimelineCoverage,
    pub recorder_tactical_aid_usage: RecorderTacticalAidUsage,
}

/// Destruction of one replay-level infantry actor within a squad.
///
/// This is not a complete unit loss. The squad parent has its own `UnitDestroy`
/// lifecycle and remains represented by `TimelineEvent::UnitDestroyed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum UnitDestructionCause {
    Unknown,
    Unit,
    TacticalAid,
}

/// Exact mechanical context for a destruction that happened through another
/// replay object. This is independent from `UnitDestructionCause`: a transport's
/// attacker can be known while the occupant was mechanically destroyed with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "type",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum UnitDestructionContext {
    BuildingCollapse { building_id: u32 },
    DestroyedWithContainer { container_unit_id: u32 },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct InfantrySoldierDeath {
    pub time_seconds: f32,
    pub unit_id: u32,
    pub unit_type_id: u32,
    pub player_id: Option<u32>,
    pub team: Option<u32>,
    pub killer_unit_id: Option<u32>,
    pub killer_player_id: Option<u32>,
    pub killer_team: Option<u32>,
    pub cause: UnitDestructionCause,
    pub destruction_context: Option<UnitDestructionContext>,
    pub tactical_aid_support_id: Option<u32>,
    pub tactical_aid_support_name: Option<String>,
}

impl InfantrySoldierDeath {
    fn referenced_player_ids(&self) -> [Option<u32>; 2] {
        [self.player_id, self.killer_player_id]
    }
}

/// Static fallback identity for a player slot referenced anywhere in the timeline.
///
/// Every player ID carried by an event, chat boundary, or recorder-scoped
/// summary has exactly one entry. A slot may be reused by multiple people, so
/// event-time consumers must prefer `TimelineData::participant_sessions`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelineParticipant {
    pub player_id: u32,
    pub player_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum TimelineParticipantIdentitySource {
    StaticMetadata,
    PlayerEnteredGame,
}

/// One time-bounded occupant of a reusable numeric player slot.
///
/// Intervals are start-inclusive and end-exclusive. Names from structurally valid
/// `PlayerEntersGame` records supersede static metadata only inside their interval;
/// gaps after `PlayerLeavesGame` remain unnamed rather than inheriting a departed
/// player's identity.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelineParticipantSession {
    pub player_id: u32,
    pub session_index: u32,
    pub player_name: Option<String>,
    pub start_seconds: f32,
    pub end_seconds: Option<f32>,
    pub identity_source: TimelineParticipantIdentitySource,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct PreMatchChatMessage {
    /// Position on the recording axis, the same axis every other timeline
    /// timestamp lives on. `seconds_before_match` remains the reading relative
    /// to the match boundary.
    pub time_seconds: f32,
    pub seconds_before_match: f32,
    pub player_id: u32,
    pub player_name: Option<String>,
    pub message: String,
    pub channel: ChatChannel,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelineCoverage {
    pub chat: &'static str,
    pub tactical_aid: &'static str,
    pub tactical_aid_markers: &'static str,
    pub tactical_aid_deployments: &'static str,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct RecorderTacticalAidUsage {
    pub player_id: Option<u32>,
    pub total_placements: u32,
    pub supports: Vec<TacticalAidUsageCount>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TacticalAidUsageCount {
    pub support_id: u32,
    pub support_name: Option<String>,
    pub placement_count: u32,
    pub observed_costs: Vec<f32>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct PostMatchChatMessage {
    /// Position on the recording axis, the same axis every other timeline
    /// timestamp lives on. `seconds_after_match` remains the reading relative
    /// to the match boundary.
    pub time_seconds: f32,
    pub seconds_after_match: f32,
    pub player_id: u32,
    pub player_name: Option<String>,
    pub message: String,
    pub channel: ChatChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum ChatChannel {
    All,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum SpectatorView {
    None,
    OneTeam,
    AllTeams,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum TacticalAidPlayerAttribution {
    /// The aid's binary-proven spawn-source-1 unit has exactly one owner under
    /// the corpus-validated support/type/time/position join.
    UnitSpawnOwnership,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelinePhase {
    pub index: u32,
    pub start_seconds: f32,
    pub end_seconds: f32,
    pub initial_clock_seconds: f32,
    pub final_clock_seconds: f32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct TimelineValueSample {
    pub time_seconds: f32,
    pub value: f32,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "type",
        rename_all = "camelCase",
        rename_all_fields = "camelCase"
    )
)]
pub enum TimelineEvent {
    PlayerEntered {
        time_seconds: f32,
        player_id: u32,
    },
    PlayerLeft {
        time_seconds: f32,
        player_id: u32,
    },
    PlayerJoinedTeam {
        time_seconds: f32,
        player_id: u32,
        team: u32,
    },
    SpectatorViewChanged {
        time_seconds: f32,
        player_id: u32,
        team: u32,
        spectator_los: u32,
        view: SpectatorView,
    },
    PlayerSetRole {
        time_seconds: f32,
        player_id: u32,
        role_id: u32,
        role: Option<String>,
    },
    CommandPointOwnerChanged {
        time_seconds: f32,
        command_point_id: u32,
        team: u32,
    },
    UnitDestroyed {
        time_seconds: f32,
        unit_id: u32,
        unit_type_id: Option<u32>,
        player_id: Option<u32>,
        team: Option<u32>,
        killer_unit_id: Option<u32>,
        killer_player_id: Option<u32>,
        killer_team: Option<u32>,
        cause: UnitDestructionCause,
        destruction_context: Option<UnitDestructionContext>,
        tactical_aid_support_id: Option<u32>,
        tactical_aid_support_name: Option<String>,
    },
    TacticalAidTransferred {
        time_seconds: f32,
        from_player_id: u32,
        to_player_id: u32,
        amount: u32,
    },
    /// A server-issued tactical-aid damage taunt with exact actor and target.
    ///
    /// The dedicated server emits this record after one player's tactical aid
    /// accumulates the support definition's taunt-score threshold against another
    /// player. It is exact damage attribution, not a unit-kill record. `ta_index`
    /// is the serialized support-manager index; `support_id` and `support_name`
    /// are populated only when that index resolves through the replay's ordered
    /// zero-position `SupportThingUsed` catalogue.
    TacticalAidDamageThreshold {
        time_seconds: f32,
        player_id: u32,
        target_player_id: u32,
        ta_index: u32,
        support_id: Option<u32>,
        support_name: Option<String>,
        upgrade_level: u32,
    },
    /// A tactical aid activated by the recording player.
    ///
    /// `support_id` is the raw BinTag hash of the support definition's name;
    /// `support_name` is populated only for top-level faction-aid definitions
    /// recovered from shipped game data.
    ///
    /// `honors_cost` is the magnitude of the paired `ChangeHonors` deduction and
    /// must not be used to infer which aid was used: it is the *marginal* price of
    /// one placement in a multi-strike, so the same `support_id` is charged
    /// different amounts across a match (a triple nuke is three events at 80, 60
    /// and 40, not one at its 180 bundle price), and unrelated aids share costs.
    /// Multi-strike selection and grouping are intentionally not reported: the
    /// replay records each placement but does not serialize the selected bundle
    /// size, and queued strikes may be held indefinitely.
    TacticalAidUsed {
        time_seconds: f32,
        support_id: u32,
        support_name: Option<String>,
        honors_cost: f32,
        position: [f32; 3],
        player_id: Option<u32>,
    },
    /// A recorder-visible-faction tactical-aid marker with its issuing player.
    ///
    /// The game names the player-slot field `aTeam`, but client/server binary
    /// tracing and an independent recorder-ledger corpus cross-check establish
    /// that its value is the issuing player slot (0-15), not a faction ID.
    /// This raw marker is not a purchase or a reconstructed multi-strike bundle.
    TacticalAidMarker {
        time_seconds: f32,
        event_id: u32,
        support_id: u32,
        support_name: Option<String>,
        position: [f32; 3],
        player_id: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        duration_seconds: f32,
    },
    /// A top-level tactical-aid deployment visible in the simulation effect stream.
    ///
    /// Unlike `TacticalAidMarker`, this event covers both factions in ordinary
    /// player and spectator recordings. It serializes the deploying faction but
    /// not the issuing player. Consumers must not invent a player identity when a
    /// corresponding player-bearing marker is unavailable.
    TacticalAidDeployed {
        time_seconds: f32,
        support_id: u32,
        support_name: String,
        position: [f32; 3],
        team: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        age_seconds: f32,
        player_id: Option<u32>,
        player_attribution: Option<TacticalAidPlayerAttribution>,
    },
    /// A chat message visible to the recording player's client.
    ChatMessage {
        time_seconds: f32,
        player_id: u32,
        player_name: Option<String>,
        message: String,
        channel: ChatChannel,
    },
    VoteStarted {
        time_seconds: f32,
        player_id: u32,
        vote_id: u32,
        float_value: f32,
        int_value: i32,
        uint_value: u32,
    },
    TeamWon {
        time_seconds: f32,
        team: u32,
        win_type: u32,
    },
}

impl TimelineEvent {
    /// Player slots serialized or evidence-backed by this event.
    ///
    /// Consumers can resolve these IDs through `TimelineData::participants`.
    pub fn referenced_player_ids(&self) -> [Option<u32>; 2] {
        match self {
            Self::PlayerEntered { player_id, .. }
            | Self::PlayerLeft { player_id, .. }
            | Self::PlayerJoinedTeam { player_id, .. }
            | Self::SpectatorViewChanged { player_id, .. }
            | Self::PlayerSetRole { player_id, .. }
            | Self::ChatMessage { player_id, .. }
            | Self::VoteStarted { player_id, .. } => [Some(*player_id), None],
            Self::UnitDestroyed {
                player_id,
                killer_player_id,
                ..
            } => [*player_id, *killer_player_id],
            Self::TacticalAidTransferred {
                from_player_id,
                to_player_id,
                ..
            } => [Some(*from_player_id), Some(*to_player_id)],
            Self::TacticalAidDamageThreshold {
                player_id,
                target_player_id,
                ..
            } => [Some(*player_id), Some(*target_player_id)],
            Self::TacticalAidUsed { player_id, .. } => [*player_id, None],
            Self::TacticalAidMarker { player_id, .. } => [Some(*player_id), None],
            Self::TacticalAidDeployed { player_id, .. } => [*player_id, None],
            Self::CommandPointOwnerChanged { .. } | Self::TeamWon { .. } => [None, None],
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct ReplayWithTimeline {
    pub replay: ReplayData,
    pub timeline: TimelineData,
}

impl fmt::Display for ReplayData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let duration_str = match self.duration_seconds {
            Some(d) => format!("{:.1} min", d / 60.0),
            None => "Unknown".to_string(),
        };

        writeln!(
            f,
            "Map: {} | Mode: {}",
            self.game_info.map_display_name, self.game_info.game_mode
        )?;
        writeln!(f, "Date/Time: {}", self.game_info.date_time)?;
        writeln!(
            f,
            "Match length: {} | Players: {}",
            duration_str,
            self.players.len()
        )?;
        writeln!(
            f,
            "Replay length: {:.1} min",
            self.timing.recording_seconds / 60.0
        )?;
        if !self.timing.captured_match_start
            && let Some(joined) = self.timing.joined_at_remaining_seconds
        {
            writeln!(
                f,
                "Recorder joined mid-match, with {:.1} min left on the clock",
                joined / 60.0
            )?;
        }
        if let Some(rec) = &self.recorder {
            writeln!(f, "Recorded by: {rec}")?;
        }

        if self.incomplete {
            writeln!(f, "Incomplete match results")?;
        }

        // Group players by team
        let mut teams: HashMap<String, Vec<&Player>> = HashMap::new();
        for p in &self.players {
            let key = p.faction.clone().unwrap_or_else(|| "Unknown".to_string());
            teams.entry(key).or_default().push(p);
        }

        // Domination bar (suppressed for incomplete results — faction labels would be misleading)
        if !self.incomplete
            && let Some(shares) = &self.domination_shares
            && shares.len() == 2
        {
            writeln!(
                f,
                "Domination: {} {}% - {} {}% ({})",
                shares[0].faction,
                format_share(shares[0].pct),
                shares[1].faction,
                format_share(shares[1].pct),
                self.match_ending.label(),
            )?;
        }
        writeln!(f)?;

        // Sort team names for consistent output
        let mut team_names: Vec<&String> = teams.keys().collect();
        team_names.sort();

        for team_name in team_names {
            let team_players = &teams[team_name];
            let mut sorted_players = team_players.clone();
            sorted_players.sort_by_key(|player| std::cmp::Reverse(player.score.unwrap_or(0)));

            let team_total: i64 = sorted_players
                .iter()
                .filter_map(|p| p.score)
                .map(i64::from)
                .sum();

            let result_tag = if Some(team_name.as_str()) == self.winner.as_deref() {
                " - WINNER"
            } else if self.winner.is_some() && team_name != "Unknown" && team_name != "Spectator" {
                " - LOSER"
            } else {
                ""
            };

            writeln!(f, "[{}] Total: {}{}", team_name, team_total, result_tag)?;
            for (i, p) in sorted_players.iter().enumerate() {
                let score_str = match p.score {
                    Some(s) => format!("{s:5}"),
                    None => "    -".to_string(),
                };
                let role_tag = match p.role.as_deref() {
                    Some("infantry") => "[INF] ",
                    Some("armor") => "[ARM] ",
                    Some("air") => "[AIR] ",
                    Some("support") => "[SUP] ",
                    _ => "",
                };
                writeln!(f, "  {:2}. {}{:<28} {}", i + 1, role_tag, p.name, score_str)?;
            }
            writeln!(f)?;
        }

        Ok(())
    }
}

// ── Byte helpers ────────────────────────────────────────────────────────────

fn find_pattern(data: &[u8], pattern: &[u8], start: usize) -> Option<usize> {
    if pattern.is_empty() || data.len() < pattern.len() + start {
        return None;
    }
    data[start..]
        .windows(pattern.len())
        .position(|w| w == pattern)
        .map(|p| p + start)
}

/// Iterator over all occurrences of `pattern` in `data`.
struct PatternIter<'a> {
    data: &'a [u8],
    pattern: &'a [u8],
    offset: usize,
}

impl<'a> PatternIter<'a> {
    fn new(data: &'a [u8], pattern: &'a [u8]) -> Self {
        Self {
            data,
            pattern,
            offset: 0,
        }
    }
}

impl Iterator for PatternIter<'_> {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        let pos = find_pattern(self.data, self.pattern, self.offset)?;
        self.offset = pos + 1;
        Some(pos)
    }
}

fn find_all<'a>(data: &'a [u8], pattern: &'a [u8]) -> PatternIter<'a> {
    PatternIter::new(data, pattern)
}

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    read_u32_le(data, offset).map(|value| value as i32)
}

fn read_f32_le(data: &[u8], offset: usize) -> Option<f32> {
    if offset + 4 > data.len() {
        return None;
    }
    Some(f32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_bintag_u32(data: &[u8], hash_pos: usize) -> Option<u32> {
    if hash_pos + 17 > data.len() {
        return None;
    }
    if data[hash_pos + 4..hash_pos + 8] != BINTAG_SEP {
        return None;
    }
    read_u32_le(data, hash_pos + 13)
}

fn read_unique_bintag_bool(data: &[u8], hash: &[u8; 4]) -> Option<bool> {
    let mut values = find_all(data, hash).filter_map(|position| {
        has_bintag_field(data, position, hash).then(|| read_u32_le(data, position + 13))?
    });
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn read_bintag_float(data: &[u8], hash_pos: usize) -> Option<f32> {
    if hash_pos + 17 > data.len() {
        return None;
    }
    if data[hash_pos + 4..hash_pos + 8] != BINTAG_SEP {
        return None;
    }
    read_f32_le(data, hash_pos + 13)
}

fn has_bintag_field(data: &[u8], hash_pos: usize, hash: &[u8; 4]) -> bool {
    hash_pos + 17 <= data.len()
        && data[hash_pos..hash_pos + 4] == *hash
        && data[hash_pos + 4..hash_pos + 8] == BINTAG_SEP
        && data[hash_pos + 9..hash_pos + 13] == BINTAG_SEP
}

fn read_expected_u32(data: &[u8], hash_pos: usize, hash: &[u8; 4]) -> Option<u32> {
    has_bintag_field(data, hash_pos, hash).then(|| read_u32_le(data, hash_pos + 13))?
}

fn read_expected_i32(data: &[u8], hash_pos: usize, hash: &[u8; 4]) -> Option<i32> {
    has_bintag_field(data, hash_pos, hash).then(|| read_i32_le(data, hash_pos + 13))?
}

fn read_expected_float(data: &[u8], hash_pos: usize, hash: &[u8; 4]) -> Option<f32> {
    has_bintag_field(data, hash_pos, hash).then(|| read_f32_le(data, hash_pos + 13))?
}

fn read_expected_utf16_string(data: &[u8], hash_pos: usize, hash: &[u8; 4]) -> Option<String> {
    if data.get(hash_pos..hash_pos.checked_add(4)?)? != hash {
        return None;
    }
    let total_u32 = read_u32_le(data, hash_pos.checked_add(4)?)?;
    let total = usize::try_from(total_u32).ok()?;
    let repeated_total = read_u32_le(data, hash_pos.checked_add(9)?)?;
    if total < 15 || data.get(hash_pos.checked_add(8)?) != Some(&5) || repeated_total != total_u32 {
        return None;
    }
    let payload_start = hash_pos.checked_add(13)?;
    let field_end = hash_pos.checked_add(total)?;
    let payload = data.get(payload_start..field_end)?;
    if payload.len() < 2 || payload.len() % 2 != 0 || !payload.ends_with(&[0, 0]) {
        return None;
    }
    let units = payload[..payload.len() - 2]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    let value = String::from_utf16(&units).ok()?;
    (!value.is_empty() && value.chars().all(|character| !character.is_control())).then_some(value)
}

/// Role identifier used by Few Player Mode, which has no roles at all.
///
/// Its presence is an exact marker for an FPM match: `adler32("FPM_ROLE")`, and no
/// ordinary match emits it. FPM prices tactical aid uniformly, since there is no
/// role to price against, and inverts the bulk discount for some aids.
const ROLE_ID_FEW_PLAYER_MODE: u32 = 0x0b23_0275;

fn role_name(role_id: u32) -> Option<&'static str> {
    match role_id {
        0x0e00_02cb => Some("infantry"),              // ARMY_ROLE
        0x1abf_03df => Some("support"),               // ENGINEER_ROLE
        0x10e0_0313 => Some("armor"),                 // ARMOR_ROLE
        0x0ae8_026e => Some("air"),                   // AIR_ROLE
        ROLE_ID_FEW_PLAYER_MODE => Some("fewPlayer"), // FPM_ROLE
        _ => None,
    }
}

fn spectator_view(team: u32, spectator_los: u32) -> SpectatorView {
    match (team, spectator_los) {
        (1..=3, 1) => SpectatorView::OneTeam,
        (_, 2) => SpectatorView::AllTeams,
        (_, 0) => SpectatorView::None,
        _ => SpectatorView::Unknown,
    }
}

/// Top-level faction-aid definition names recovered from shipped game data.
///
/// Support IDs are Adler-32 hashes of definition names. Internal child effects
/// and special-ability markers intentionally remain unnamed here so consumers can
/// use `support_name.is_some()` as the conservative player-TA presentation gate.
const TOP_LEVEL_SUPPORT_NAMES: &[(u32, &str)] = &[
    (0x1d19_047b, "GasAttack_US"),
    (0x1d3e_0477, "RadarScan_US"),
    (0x1f13_04b6, "Airstrike_US"),
    (0x1f21_04c0, "Artillery_US"),
    (0x22dc_04ed, "Light_Tank_US"),
    (0x2497_052b, "Tankbuster_US"),
    (0x26b4_0505, "GasAttack_NATO"),
    (0x26d1_0501, "RadarScan_NATO"),
    (0x2707_0520, "GasAttack_USSR"),
    (0x2724_051c, "RadarScan_USSR"),
    (0x2861_054e, "Repair_Jeep_US"),
    (0x290c_0579, "DaisyCutter_US"),
    (0x2924_0540, "Airstrike_NATO"),
    (0x2946_054a, "Artillery_NATO"),
    (0x2971_056a, "ClusterBomb_US"),
    (0x2977_055b, "Airstrike_USSR"),
    (0x2999_0565, "Artillery_USSR"),
    (0x2a64_0597, "Paratrooper_US"),
    (0x2d5b_0577, "Light_Tank_NATO"),
    (0x2dae_0592, "Light_Tank_USSR"),
    (0x2e8f_05c0, "TacticalNuke_US"),
    (0x2f92_05b5, "Tankbuster_NATO"),
    (0x2fe5_05d0, "Tankbuster_USSR"),
    (0x3015_05f3, "Napalmstrike_US"),
    (0x308a_0604, "Bunkerbuster_US"),
    (0x33a2_05d8, "Repair_Jeep_NATO"),
    (0x33f5_05f3, "Repair_Jeep_USSR"),
    (0x3476_0625, "CarpetBombing_US"),
    (0x34a3_0603, "DaisyCutter_NATO"),
    (0x34ea_05f4, "ClusterBomb_NATO"),
    (0x34f6_061e, "DaisyCutter_USSR"),
    (0x353d_060f, "ClusterBomb_USSR"),
    (0x3546_0642, "AntiAirstrike_US"),
    (0x3637_0621, "Paratrooper_NATO"),
    (0x368a_063c, "Paratrooper_USSR"),
    (0x3ab4_064a, "TacticalNuke_NATO"),
    (0x3aff_068f, "BridgeRepairer_US"),
    (0x3b07_0665, "TacticalNuke_USSR"),
    (0x3ca0_067d, "Napalmstrike_NATO"),
    (0x3cf3_0698, "Napalmstrike_USSR"),
    (0x3d37_068e, "Bunkerbuster_NATO"),
    (0x3d8a_06a9, "Bunkerbuster_USSR"),
    (0x4165_06af, "CarpetBombing_NATO"),
    (0x41b8_06ca, "CarpetBombing_USSR"),
    (0x426f_06cc, "AntiAirstrike_NATO"),
    (0x42c2_06e7, "AntiAirstrike_USSR"),
    (0x4337_071e, "HeavyAirSupport_US"),
    (0x48c2_0719, "BridgeRepairer_NATO"),
    (0x4915_0734, "BridgeRepairer_USSR"),
    (0x5218_07a8, "HeavyAirSupport_NATO"),
    (0x526b_07c3, "HeavyAirSupport_USSR"),
    (0x76be_096c, "LightArtilleryBarrage_US"),
    (0x7708_0971, "HeavyArtilleryBarrage_US"),
    (0x7adc_097e, "TacticalNuke_NATO_British"),
    (0x8a3b_09f6, "LightArtilleryBarrage_NATO"),
    (0x8a8e_0a11, "LightArtilleryBarrage_USSR"),
    (0x8a8f_09fb, "HeavyArtilleryBarrage_NATO"),
    (0x8ae2_0a16, "HeavyArtilleryBarrage_USSR"),
];

fn support_name(support_id: u32) -> Option<&'static str> {
    TOP_LEVEL_SUPPORT_NAMES
        .binary_search_by_key(&support_id, |(id, _)| *id)
        .ok()
        .map(|index| TOP_LEVEL_SUPPORT_NAMES[index].1)
}

/// Return the player-facing tactical-aid name for a support projectile.
///
/// Most projectile records carry a top-level support ID. Heavy Air Support is
/// different: its top-level definitions explicitly list five child projectile
/// definitions per faction in the shipped `maps/supportweapons.ice`. Preserve
/// the serialized child ID in output, but present its proven top-level parent.
fn tactical_aid_projectile_name(support_id: u32) -> Option<&'static str> {
    match support_id {
        // HeavyAirSupport_US children.
        0x99d2_0a52 | 0x99d3_0a53 | 0x9a0b_0a65 | 0x9a0c_0a66 | 0x9a0d_0a67 => {
            Some("HeavyAirSupport_US")
        }
        // HeavyAirSupport_NATO children.
        0xaeeb_0adc | 0xaeec_0add | 0xaf24_0aef | 0xaf25_0af0 | 0xaf26_0af1 => {
            Some("HeavyAirSupport_NATO")
        }
        // HeavyAirSupport_USSR children.
        0xafc5_0af7 | 0xafc6_0af8 | 0xaffe_0b0a | 0xafff_0b0b | 0xb000_0b0c => {
            Some("HeavyAirSupport_USSR")
        }
        _ => support_name(support_id),
    }
}

#[derive(Clone, Copy)]
struct UnitDropSpec {
    unit_type_id: u32,
    minimum_arrival_seconds: f32,
    maximum_arrival_seconds: f32,
    maximum_horizontal_distance: f32,
}

/// Unit type and conservative arrival bounds for the nine faction unit drops.
///
/// The type names come from the shipped `maps/supportweapons.ice`. Server binary
/// tracing establishes that its projectile unit-spawner creates the unit in the
/// issuing player's container with `aSpawnSource=1`. The fixed arrival bands and
/// position bounds have zero player contradictions across 52,875 independently
/// player-bearing marker deployments in the 2,880-replay validation corpus.
fn unit_drop_spec(support_id: u32) -> Option<UnitDropSpec> {
    let (unit_type_id, minimum_arrival_seconds, maximum_arrival_seconds, distance) =
        match support_id {
            // Airborne infantry: US, NATO, USSR.
            0x2a64_0597 => (0x3974_0697, 29.0, 35.0, 30.0),
            0x3637_0621 => (0x4381_0721, 29.0, 35.0, 30.0),
            0x368a_063c => (0x4569_073c, 29.0, 35.0, 30.0),
            // Airdropped transport: US, NATO, USSR.
            0x2861_054e => (0x0e98_02d3, 16.0, 22.0, 15.0),
            0x33a2_05d8 => (0x23ac_051f, 16.0, 22.0, 15.0),
            0x33f5_05f3 => (0x14f0_0340, 16.0, 22.0, 15.0),
            // Airdropped light tank: US, NATO, USSR.
            0x22dc_04ed => (0x184f_0436, 16.0, 22.0, 15.0),
            0x2d5b_0577 => (0x1079_02db, 16.0, 22.0, 15.0),
            0x2dae_0592 => (0x12b5_037d, 16.0, 22.0, 15.0),
            _ => return None,
        };
    Some(UnitDropSpec {
        unit_type_id,
        minimum_arrival_seconds,
        maximum_arrival_seconds,
        maximum_horizontal_distance: distance,
    })
}

/// Validate an `Event` envelope header whose message hash sits at `message_pos`.
///
/// Envelope layout: `[Event hash][0x15000000][0x06][total u32][time f32][message hash]`,
/// so the header begins `ENVELOPE_MESSAGE_OFFSET` bytes before the message hash.
fn event_envelope_start(data: &[u8], message_pos: usize) -> Option<usize> {
    let start = message_pos.checked_sub(ENVELOPE_MESSAGE_OFFSET)?;
    if data.len() < message_pos + 4 {
        return None;
    }
    (data[start..start + 4] == HASH_EVENT
        && data[start + 4..start + 8] == ENVELOPE_TYPE
        && data[start + 8] == ENVELOPE_ARRAY_FLAG)
        .then_some(start)
}

/// A backward step in envelope time larger than this starts a new pass.
///
/// The envelope clock is strictly monotonic inside the primary chain: a
/// 156-replay scan of `replays/main` found zero backward steps of any size
/// before the end-of-match summary, and every observed reset dropped by the
/// whole recording length (median ~900 s). One second is therefore a wide
/// margin rather than a tuned threshold.
const RECORDING_RESET_DROP_SECONDS: f32 = 1.0;

/// The primary event chain: every envelope's start offset and native timestamp.
///
/// A replay does not end at its last envelope. After `TeamWins` the recorder
/// writes a second complete pass over the match — the end-of-match summary —
/// whose envelope timestamps restart at zero and climb back to the recording
/// length. Every corpus replay contains exactly one such reset, so the chain is
/// cut there and the summary pass is left out of the timeline.
///
/// This is the parser's time base. The envelope clock starts at zero, advances
/// monotonically, and ticks 1:1 with real time, which the game-mode countdown
/// does not: the countdown starts late (corpus median 66.8 s of lobby first),
/// restarts between Assault rounds, and drifts by up to 25% per sample.
#[derive(Debug, Default)]
struct EnvelopeTimeline {
    /// Envelope start offsets, ascending.
    starts: Vec<usize>,
    /// `times[i]` is the timestamp of the envelope starting at `starts[i]`.
    times: Vec<f32>,
    /// First offset past the primary chain: the start of the end-of-match
    /// summary, or the end of the data when no reset occurs.
    end_offset: usize,
    /// Highest timestamp reached in the primary chain. This is the real length
    /// of the recording, counting pre-match lobby and the post-`TeamWins` tail.
    seconds: f32,
}

impl EnvelopeTimeline {
    /// Timestamp of the envelope containing `offset`.
    ///
    /// Records are addressed by message offset, envelope bound, or an interior
    /// field offset depending on the call site, so the lookup resolves any byte
    /// position to its enclosing envelope rather than assuming one convention.
    /// Offsets before the chain and inside the end-of-match summary clamp to the
    /// chain's first and last timestamps.
    fn time_at(&self, offset: usize) -> f32 {
        match self.starts.binary_search(&offset) {
            Ok(index) => self.times[index],
            Err(0) => 0.0,
            Err(index) => self.times[index - 1],
        }
    }
}

/// Find the start of the gapless `Event` chain.
///
/// The metadata prefix is not envelope-structured, so the chain is located by
/// requiring several envelopes to link end-to-end from the same candidate
/// rather than by trusting the first header-shaped bytes.
fn find_chain_anchor(data: &[u8]) -> Option<usize> {
    const CONFIRM_LINKS: usize = 6;
    let mut search = 0usize;
    while let Some(relative) = data
        .get(search..)?
        .windows(4)
        .position(|bytes| bytes == HASH_EVENT)
    {
        let candidate = search + relative;
        let mut cursor = candidate;
        let mut links = 0;
        while links < CONFIRM_LINKS && cursor < data.len() {
            let Some(envelope) = event_envelope(data, cursor + ENVELOPE_MESSAGE_OFFSET) else {
                break;
            };
            cursor = envelope.end;
            links += 1;
        }
        // A chain that runs cleanly off the end of the data is confirmed by
        // exhaustion rather than by link count.
        if links == CONFIRM_LINKS || (links > 0 && cursor == data.len()) {
            return Some(candidate);
        }
        search = candidate + 1;
    }
    None
}

/// Walk the envelope chain, recording each envelope's time and stopping at the
/// end-of-match summary. Decompressed input is capped at 96 MiB and every valid
/// envelope consumes at least 21 bytes with checked forward progress, bounding
/// this linear walk to at most 4,793,490 envelopes.
fn build_envelope_timeline(data: &[u8]) -> EnvelopeTimeline {
    let mut timeline = EnvelopeTimeline {
        end_offset: data.len(),
        ..EnvelopeTimeline::default()
    };
    let Some(anchor) = find_chain_anchor(data) else {
        return timeline;
    };
    let mut cursor = anchor;
    while let Some(envelope) = event_envelope(data, cursor + ENVELOPE_MESSAGE_OFFSET) {
        if envelope.raw_time_seconds < timeline.seconds - RECORDING_RESET_DROP_SECONDS {
            timeline.end_offset = envelope.start;
            return timeline;
        }
        timeline.seconds = timeline.seconds.max(envelope.raw_time_seconds);
        timeline.starts.push(envelope.start);
        timeline.times.push(envelope.raw_time_seconds);
        if envelope.end <= cursor {
            break;
        }
        cursor = envelope.end;
    }
    timeline
}

#[derive(Debug, Clone, Copy)]
struct EventEnvelope {
    start: usize,
    end: usize,
    raw_time_seconds: f32,
}

/// Validate a complete event envelope and return its byte bounds and native time.
fn event_envelope(data: &[u8], message_pos: usize) -> Option<EventEnvelope> {
    let start = event_envelope_start(data, message_pos)?;
    let total = usize::try_from(read_u32_le(data, start + 9)?).ok()?;
    if total < ENVELOPE_MESSAGE_OFFSET + 4 {
        return None;
    }
    let end = start.checked_add(total)?;
    if end > data.len() {
        return None;
    }
    let raw_time_seconds = f32::from_bits(read_u32_le(data, start + 13)?);
    raw_time_seconds.is_finite().then_some(EventEnvelope {
        start,
        end,
        raw_time_seconds,
    })
}

/// Find one structurally valid fixed-width field inside a validated event.
///
/// `UnitCreate.aSpawnSource` follows a one-byte boolean, so it is not at a normal
/// 17-byte stride. The envelope bound and repeated BinTag separators keep this a
/// structural lookup rather than an unbounded byte-pattern guess.
fn read_unique_u32_field_in_event(
    data: &[u8],
    message_pos: usize,
    expected_hash: &[u8; 4],
) -> Option<u32> {
    let envelope = event_envelope(data, message_pos)?;
    let body_start = message_pos.checked_add(4)?;
    let mut value = None;
    for relative in data.get(body_start..envelope.end)?.windows(4).enumerate() {
        let (offset, bytes) = relative;
        if bytes != expected_hash {
            continue;
        }
        let field_pos = body_start.checked_add(offset)?;
        if field_pos.checked_add(17)? > envelope.end {
            continue;
        }
        let Some(candidate) = read_expected_u32(data, field_pos, expected_hash) else {
            continue;
        };
        if value.replace(candidate).is_some() {
            return None;
        }
    }
    value
}

#[derive(Clone, Copy)]
struct UnitDropCreate {
    raw_time_seconds: f32,
    player_id: u32,
    team: u32,
    unit_type_id: u32,
    position: [f32; 3],
}

fn parse_unit_drop_create(data: &[u8], message_pos: usize) -> Option<UnitDropCreate> {
    let envelope = event_envelope(data, message_pos)?;
    read_expected_u32(data, message_pos + 4, &HASH_APERSISTENCE_KEY)?;
    let player_id = read_expected_u32(data, message_pos + 21, &HASH_APLAYER)?;
    let team = read_expected_u32(data, message_pos + 38, &HASH_ATEAM)?;
    read_expected_u32(data, message_pos + 55, &HASH_AUNIT)?;
    let unit_type_id = read_expected_u32(data, message_pos + 72, &HASH_ATYPE)?;
    let position = [
        read_expected_float(data, message_pos + 89, &HASH_APOSITION_X)?,
        read_expected_float(data, message_pos + 106, &HASH_APOSITION_Y)?,
        read_expected_float(data, message_pos + 123, &HASH_APOSITION_Z)?,
    ];
    let spawn_source = read_unique_u32_field_in_event(data, message_pos, &HASH_ASPAWN_SOURCE)?;
    if player_id > 15
        || !(1..=3).contains(&team)
        || spawn_source != 1
        || !position.iter().all(|value| value.is_finite())
    {
        return None;
    }
    Some(UnitDropCreate {
        raw_time_seconds: envelope.raw_time_seconds,
        player_id,
        team,
        unit_type_id,
        position,
    })
}

/// Reject replay structures that would make exact unit-drop ownership attribution
/// perform an attacker-controlled quadratic join. The runtime join is indexed by
/// team and unit type, so this is the upper bound on candidate comparisons before
/// its time and distance filters are applied.
fn validate_unit_drop_attribution_work(
    data: &[u8],
    event_index: &EventIndex,
) -> Result<(), String> {
    validate_unit_drop_attribution_work_with_limit(
        data,
        event_index,
        MAX_UNIT_DROP_ATTRIBUTION_COMPARISONS,
    )
}

fn validate_unit_drop_attribution_work_with_limit(
    data: &[u8],
    event_index: &EventIndex,
    max_comparisons: usize,
) -> Result<(), String> {
    let mut creates_by_key: HashMap<(u32, u32), usize> = HashMap::new();
    for unit in event_index
        .positions(IndexedTag::UnitCreate)
        .filter_map(|position| parse_unit_drop_create(data, position))
    {
        let count = creates_by_key
            .entry((unit.team, unit.unit_type_id))
            .or_default();
        *count = count
            .checked_add(1)
            .ok_or_else(|| "Unit-drop create count overflows".to_owned())?;
    }

    let mut deployments_by_key: HashMap<(u32, u32), usize> = HashMap::new();
    for position in event_index.positions(IndexedTag::SupportThingSpawnedDelayed) {
        if event_envelope(data, position).is_none() {
            continue;
        }
        let (Some(support_id), Some(team)) = (
            read_expected_u32(data, position + 4, &HASH_ANID),
            read_expected_u32(data, position + 72, &HASH_ATEAM),
        ) else {
            continue;
        };
        let Some(spec) = unit_drop_spec(support_id) else {
            continue;
        };
        if !(1..=3).contains(&team) {
            continue;
        }
        let count = deployments_by_key
            .entry((team, spec.unit_type_id))
            .or_default();
        *count = count
            .checked_add(1)
            .ok_or_else(|| "Unit-drop deployment count overflows".to_owned())?;
    }

    let mut comparisons = 0usize;
    for (key, deployment_count) in deployments_by_key {
        let create_count = creates_by_key.get(&key).copied().unwrap_or(0);
        let key_comparisons = create_count
            .checked_mul(deployment_count)
            .ok_or_else(|| "Unit-drop attribution comparison count overflows".to_owned())?;
        comparisons = comparisons
            .checked_add(key_comparisons)
            .ok_or_else(|| "Unit-drop attribution comparison count overflows".to_owned())?;
        if comparisons > max_comparisons {
            return Err(format!(
                "Replay exceeds the maximum of {max_comparisons} unit-drop attribution comparisons"
            ));
        }
    }
    Ok(())
}

/// Read one length-prefixed BinTag field inside an event envelope.
///
/// Fixed-width fields repeat `17` at offsets +4 and +9; strings and booleans use
/// the same framing with a payload-dependent total. Requiring both totals and the
/// envelope bound prevents a malformed message from consuming the next event.
fn read_event_field<'a>(
    data: &'a [u8],
    pos: usize,
    envelope_end: usize,
    expected_hash: &[u8; 4],
    expected_flag: u8,
) -> Option<(&'a [u8], usize)> {
    let header_end = pos.checked_add(13)?;
    if header_end > envelope_end || data.get(pos..pos + 4)? != expected_hash {
        return None;
    }
    let total = usize::try_from(read_u32_le(data, pos + 4)?).ok()?;
    if total < 13 || read_u32_le(data, pos + 9)? != total as u32 {
        return None;
    }
    if *data.get(pos + 8)? != expected_flag {
        return None;
    }
    let field_end = pos.checked_add(total)?;
    if field_end > envelope_end {
        return None;
    }
    Some((data.get(header_end..field_end)?, field_end))
}

fn parse_received_chat(
    data: &[u8],
    message_pos: usize,
) -> Option<(EventEnvelope, u32, String, ChatChannel)> {
    let envelope = event_envelope(data, message_pos)?;
    let (player_bytes, message_field_pos) =
        read_event_field(data, message_pos + 4, envelope.end, &HASH_APLAYER, 1)?;
    if player_bytes.len() != 4 {
        return None;
    }
    let player_id = u32::from_le_bytes(player_bytes.try_into().ok()?);
    if player_id > 15 {
        return None;
    }

    let (message_bytes, team_field_pos) =
        read_event_field(data, message_field_pos, envelope.end, &HASH_AMESSAGE, 5)?;
    if message_bytes.len() < 2
        || message_bytes.len() > MAX_CHAT_UTF16_BYTES
        || message_bytes.len() % 2 != 0
    {
        return None;
    }
    let mut utf16 = Vec::with_capacity(message_bytes.len() / 2);
    for bytes in message_bytes.as_chunks::<2>().0 {
        utf16.push(u16::from_le_bytes([bytes[0], bytes[1]]));
    }
    if utf16.pop()? != 0 || utf16.contains(&0) {
        return None;
    }
    let message = String::from_utf16(&utf16).ok()?;

    let (team_bytes, next_pos) =
        read_event_field(data, team_field_pos, envelope.end, &HASH_ATEAM_CHAT, 3)?;
    if next_pos != envelope.end || team_bytes.len() != 1 {
        return None;
    }
    let channel = match team_bytes[0] {
        0 => ChatChannel::All,
        1 => ChatChannel::Team,
        _ => return None,
    };

    Some((envelope, player_id, message, channel))
}

fn decode_bounded_utf16_string(payload: &[u8], max_bytes: usize) -> Option<String> {
    if payload.len() < 2
        || payload.len() > max_bytes
        || !payload.len().is_multiple_of(2)
        || payload[payload.len() - 2..] != [0, 0]
    {
        return None;
    }
    let mut units = Vec::with_capacity(payload.len() / 2 - 1);
    for bytes in payload[..payload.len() - 2].as_chunks::<2>().0 {
        let unit = u16::from_le_bytes([bytes[0], bytes[1]]);
        if unit == 0 {
            return None;
        }
        units.push(unit);
    }
    let value = String::from_utf16(&units).ok()?;
    let value = value.trim_end_matches('\u{a0}').to_string();
    (!value.is_empty()).then_some(value)
}

/// Decode the reusable slot and optional exact occupant name from PlayerEntersGame.
///
/// The slot remains useful when a damaged or future-format record prevents name
/// decoding: callers can then open an explicitly unknown identity interval instead
/// of leaking the previous occupant across the entry boundary.
fn parse_player_entry(
    data: &[u8],
    message_pos: usize,
) -> Option<(EventEnvelope, u32, Option<String>)> {
    let envelope = event_envelope(data, message_pos)?;
    let player_id = read_expected_u32(data, message_pos + 4, &HASH_ASLOT)?;
    if player_id > 15 {
        return None;
    }

    // The second fixed-width field is the signed entry team ("team"). Identity
    // decoding validates its framing, then reads the following type-5 name.
    let unknown_pos = message_pos.checked_add(21)?;
    if unknown_pos.checked_add(17)? > envelope.end
        || read_u32_le(data, unknown_pos + 4)? != 17
        || *data.get(unknown_pos + 8)? != 0
        || read_u32_le(data, unknown_pos + 9)? != 17
    {
        return Some((envelope, player_id, None));
    }
    let name_pos = unknown_pos + 17;
    let player_name = read_event_field(data, name_pos, envelope.end, &HASH_PLAYER_ENTRY_NAME, 5)
        .and_then(|(payload, _)| decode_bounded_utf16_string(payload, MAX_PLAYER_NAME_UTF16_BYTES));
    Some((envelope, player_id, player_name))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerIdentityTransitionKind {
    Enter,
    Leave,
}

#[derive(Debug)]
struct PlayerIdentityTransition {
    offset: usize,
    end_offset: usize,
    player_id: u32,
    player_name: Option<String>,
    kind: PlayerIdentityTransitionKind,
}

#[derive(Debug, Clone)]
struct PlayerIdentityInterval {
    player_id: u32,
    session_index: u32,
    player_name: Option<String>,
    start_offset: usize,
    end_offset: Option<usize>,
    identity_source: TimelineParticipantIdentitySource,
}

#[derive(Debug, Default)]
struct PlayerIdentityIntervals {
    intervals: Vec<PlayerIdentityInterval>,
}

impl PlayerIdentityIntervals {
    fn build(data: &[u8], event_index: &EventIndex, static_names: &HashMap<u32, String>) -> Self {
        let mut intervals = Vec::new();
        let mut active: [Option<usize>; 16] = [None; 16];
        let mut next_session_index = [0u32; 16];

        let mut static_entries: Vec<_> = static_names.iter().collect();
        static_entries.sort_by_key(|(player_id, _)| **player_id);
        for (&player_id, player_name) in static_entries {
            let interval_index = intervals.len();
            intervals.push(PlayerIdentityInterval {
                player_id,
                session_index: 0,
                player_name: Some(player_name.clone()),
                start_offset: 0,
                end_offset: None,
                identity_source: TimelineParticipantIdentitySource::StaticMetadata,
            });
            active[player_id as usize] = Some(interval_index);
            next_session_index[player_id as usize] = 1;
        }

        let mut transitions = Vec::new();
        for pos in event_index.positions(IndexedTag::PlayerEntersGame) {
            if let Some((envelope, player_id, player_name)) = parse_player_entry(data, pos) {
                transitions.push(PlayerIdentityTransition {
                    offset: envelope.start,
                    end_offset: envelope.end,
                    player_id,
                    player_name,
                    kind: PlayerIdentityTransitionKind::Enter,
                });
            }
        }
        for pos in event_index.positions(IndexedTag::PlayerLeavesGame) {
            let Some(envelope) = event_envelope(data, pos) else {
                continue;
            };
            let Some(player_id) = read_expected_u32(data, pos + 4, &HASH_ASLOT) else {
                continue;
            };
            if player_id <= 15 {
                transitions.push(PlayerIdentityTransition {
                    offset: envelope.start,
                    end_offset: envelope.end,
                    player_id,
                    player_name: None,
                    kind: PlayerIdentityTransitionKind::Leave,
                });
            }
        }
        transitions.sort_by_key(|transition| transition.offset);

        for transition in transitions {
            let slot = transition.player_id as usize;
            match transition.kind {
                PlayerIdentityTransitionKind::Enter => {
                    if let Some(active_index) = active[slot]
                        && transition.player_name.is_some()
                        && intervals[active_index].player_name == transition.player_name
                    {
                        continue;
                    }
                    if let Some(active_index) = active[slot] {
                        intervals[active_index].end_offset = Some(transition.offset);
                    }
                    let session_index = next_session_index[slot];
                    next_session_index[slot] = session_index.saturating_add(1);
                    let interval_index = intervals.len();
                    intervals.push(PlayerIdentityInterval {
                        player_id: transition.player_id,
                        session_index,
                        player_name: transition.player_name,
                        start_offset: transition.offset,
                        end_offset: None,
                        identity_source: TimelineParticipantIdentitySource::PlayerEnteredGame,
                    });
                    active[slot] = Some(interval_index);
                }
                PlayerIdentityTransitionKind::Leave => {
                    if let Some(active_index) = active[slot] {
                        intervals[active_index].end_offset = Some(transition.end_offset);
                        active[slot] = None;
                    }
                }
            }
        }
        intervals.sort_by_key(|interval| (interval.player_id, interval.session_index));
        Self { intervals }
    }

    fn name_at(&self, player_id: u32, offset: usize) -> Option<&str> {
        let player_start = self
            .intervals
            .partition_point(|interval| interval.player_id < player_id);
        let player_end = self
            .intervals
            .partition_point(|interval| interval.player_id <= player_id);
        let intervals = &self.intervals[player_start..player_end];
        let active_end = intervals.partition_point(|interval| interval.start_offset <= offset);
        intervals
            .get(active_end.checked_sub(1)?)
            .and_then(|interval| {
                interval
                    .end_offset
                    .is_none_or(|end_offset| offset < end_offset)
                    .then_some(interval.player_name.as_deref())
                    .flatten()
            })
    }

    fn public_sessions(
        &self,
        participant_ids: &HashSet<u32>,
        timeline_time: impl Fn(usize) -> f32,
    ) -> Vec<TimelineParticipantSession> {
        self.intervals
            .iter()
            .filter(|interval| participant_ids.contains(&interval.player_id))
            .map(|interval| TimelineParticipantSession {
                player_id: interval.player_id,
                session_index: interval.session_index,
                player_name: interval.player_name.clone(),
                start_seconds: round_timeline_seconds(timeline_time(interval.start_offset)),
                end_seconds: interval
                    .end_offset
                    .map(|offset| round_timeline_seconds(timeline_time(offset))),
                identity_source: interval.identity_source,
            })
            .collect()
    }
}

/// Read the `ChangeHonors` deduction that immediately precedes the envelope at
/// `envelope_start`, if one is there.
fn preceding_honors_delta(data: &[u8], envelope_start: usize) -> Option<f32> {
    let start = envelope_start.checked_sub(CHANGE_HONORS_ENVELOPE_BYTES)?;
    if data[start..start + 4] != HASH_EVENT
        || data[start + 4..start + 8] != ENVELOPE_TYPE
        || data[start + 8] != ENVELOPE_ARRAY_FLAG
        || read_u32_le(data, start + 9)? != CHANGE_HONORS_ENVELOPE_BYTES as u32
    {
        return None;
    }
    let message_pos = start + ENVELOPE_MESSAGE_OFFSET;
    if data[message_pos..message_pos + 4] != HASH_CHANGE_HONORS {
        return None;
    }
    read_expected_float(data, message_pos + 4, &HASH_ADELTA)
}

#[derive(Debug, Clone, Copy)]
struct ClockSample {
    offset: usize,
    remaining_seconds: f32,
    elapsed_seconds: f32,
    /// Which countdown run this sample belongs to. Assault restarts the clock
    /// between rounds, so phase boundaries are needed to place the sample on
    /// the recording axis.
    phase_index: u32,
}

#[derive(Debug, Clone, Copy)]
struct UnitState {
    player_id: u32,
    team: u32,
    unit_type_id: u32,
    generation_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProjectileActor {
    player_id: Option<u32>,
    team: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
struct ExactTargetProjectile {
    time_seconds: f32,
    firing_unit_id: u32,
    target_unit_id: u32,
    target_generation_offset: usize,
    actor: ProjectileActor,
}

#[derive(Debug, Clone, Copy)]
struct SupportDeployment {
    time_seconds: f32,
    support_id: u32,
    team: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TacticalAidCause {
    support_id: u32,
    team: u32,
}

#[derive(Debug, Clone, Copy)]
struct ExactTargetSupportProjectile {
    time_seconds: f32,
    target_unit_id: u32,
    target_generation_offset: usize,
    cause: TacticalAidCause,
}

type ExactTargetProjectileKey = (u32, u32, usize);
type ExactTargetSupportProjectileKey = (u32, usize);

#[derive(Default)]
struct RecentExactTargetProjectiles {
    queue: VecDeque<ExactTargetProjectile>,
    actors: HashMap<ExactTargetProjectileKey, HashMap<ProjectileActor, usize>>,
}

impl RecentExactTargetProjectiles {
    fn expire(&mut self, now: f32) {
        while self.queue.front().is_some_and(|projectile| {
            now - projectile.time_seconds > EXACT_TARGET_PROJECTILE_LOOKBACK_SECONDS
        }) {
            let projectile = self.queue.pop_front().expect("front projectile exists");
            if projectile.actor.player_id.is_none() && projectile.actor.team.is_none() {
                continue;
            }
            let key = (
                projectile.firing_unit_id,
                projectile.target_unit_id,
                projectile.target_generation_offset,
            );
            let remove_key = if let Some(actors) = self.actors.get_mut(&key) {
                if let Some(count) = actors.get_mut(&projectile.actor) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        actors.remove(&projectile.actor);
                    }
                }
                actors.is_empty()
            } else {
                false
            };
            if remove_key {
                self.actors.remove(&key);
            }
        }
    }

    fn push(&mut self, projectile: ExactTargetProjectile) {
        self.expire(projectile.time_seconds);
        if projectile.actor.player_id.is_some() || projectile.actor.team.is_some() {
            *self
                .actors
                .entry((
                    projectile.firing_unit_id,
                    projectile.target_unit_id,
                    projectile.target_generation_offset,
                ))
                .or_default()
                .entry(projectile.actor)
                .or_default() += 1;
        }
        self.queue.push_back(projectile);
    }

    fn unique_actor(&mut self, now: f32, key: ExactTargetProjectileKey) -> Option<ProjectileActor> {
        self.expire(now);
        let actors = self.actors.get(&key)?;
        (actors.len() == 1).then(|| *actors.keys().next().expect("one projectile actor"))
    }
}

#[derive(Default)]
struct RecentSupportDeployments {
    queue: VecDeque<SupportDeployment>,
    team_counts: HashMap<u32, [usize; 4]>,
}

impl RecentSupportDeployments {
    fn expire(&mut self, now: f32) {
        while self.queue.front().is_some_and(|deployment| {
            now - deployment.time_seconds > SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS
        }) {
            let deployment = self.queue.pop_front().expect("front deployment exists");
            let remove_support =
                if let Some(counts) = self.team_counts.get_mut(&deployment.support_id) {
                    let count = &mut counts[deployment.team as usize];
                    *count = count.saturating_sub(1);
                    counts.iter().all(|count| *count == 0)
                } else {
                    false
                };
            if remove_support {
                self.team_counts.remove(&deployment.support_id);
            }
        }
    }

    fn push(&mut self, deployment: SupportDeployment) {
        self.expire(deployment.time_seconds);
        self.team_counts.entry(deployment.support_id).or_default()[deployment.team as usize] += 1;
        self.queue.push_back(deployment);
    }

    fn unique_team(&mut self, now: f32, support_id: u32) -> Option<u32> {
        self.expire(now);
        let counts = self.team_counts.get(&support_id)?;
        let mut teams = (1..=3).filter(|team| counts[*team as usize] > 0);
        let team = teams.next()?;
        teams.next().is_none().then_some(team)
    }
}

#[derive(Default)]
struct RecentExactTargetSupportProjectiles {
    queue: VecDeque<ExactTargetSupportProjectile>,
    causes: HashMap<ExactTargetSupportProjectileKey, HashMap<TacticalAidCause, usize>>,
}

impl RecentExactTargetSupportProjectiles {
    fn expire(&mut self, now: f32) {
        while self.queue.front().is_some_and(|projectile| {
            now - projectile.time_seconds > EXACT_TARGET_SUPPORT_PROJECTILE_LOOKBACK_SECONDS
        }) {
            let projectile = self
                .queue
                .pop_front()
                .expect("front support projectile exists");
            let key = (
                projectile.target_unit_id,
                projectile.target_generation_offset,
            );
            let remove_key = if let Some(causes) = self.causes.get_mut(&key) {
                if let Some(count) = causes.get_mut(&projectile.cause) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        causes.remove(&projectile.cause);
                    }
                }
                causes.is_empty()
            } else {
                false
            };
            if remove_key {
                self.causes.remove(&key);
            }
        }
    }

    fn push(&mut self, projectile: ExactTargetSupportProjectile) {
        self.expire(projectile.time_seconds);
        *self
            .causes
            .entry((
                projectile.target_unit_id,
                projectile.target_generation_offset,
            ))
            .or_default()
            .entry(projectile.cause)
            .or_default() += 1;
        self.queue.push_back(projectile);
    }

    fn unique_cause(
        &mut self,
        now: f32,
        key: ExactTargetSupportProjectileKey,
    ) -> Option<TacticalAidCause> {
        self.expire(now);
        let causes = self.causes.get(&key)?;
        (causes.len() == 1).then(|| *causes.keys().next().expect("one tactical-aid cause"))
    }
}

#[derive(Debug, Clone, Copy)]
struct BuildingOccupancy {
    building_id: u32,
    unit_generation_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct ContainerRelation {
    container_unit_id: u32,
    container_generation_offset: usize,
    child_generation_offset: usize,
    removed_time_bits: Option<u32>,
    removed_offset: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct DestructionTerminal {
    offset: usize,
    raw_time_bits: u32,
    killer_unit_id: u32,
}

#[derive(Debug, Clone, Copy)]
struct DestructionContextCandidate {
    offset: usize,
    raw_time_bits: u32,
    unit_generation_offset: usize,
    killer_unit_id: u32,
    synthetic_direction: bool,
    occupancy: Option<BuildingOccupancy>,
    container_relation: Option<ContainerRelation>,
}

#[derive(Debug, Clone, Copy)]
struct DestructionContextEvidence {
    context: UnitDestructionContext,
    source_destruction_offset: Option<usize>,
}

fn synthetic_terminal_directions() -> &'static HashSet<[u32; 3]> {
    static DIRECTIONS: OnceLock<HashSet<[u32; 3]>> = OnceLock::new();
    DIRECTIONS.get_or_init(|| {
        let mut directions = HashSet::new();
        for x in -50i32..50 {
            for z in -50i32..50 {
                let norm = ((x * x + z * z) as f32).sqrt();
                let (x, z) = if norm > 0.0 {
                    let inverse = 1.0f32 / norm;
                    (inverse * x as f32, inverse * z as f32)
                } else {
                    (0.0, 0.0)
                };
                directions.insert([x.to_bits(), 0.0f32.to_bits(), z.to_bits()]);
            }
        }
        directions
    })
}

fn unit_destroy_has_synthetic_direction(data: &[u8], pos: usize) -> bool {
    let Some(envelope) = event_envelope(data, pos) else {
        return false;
    };
    let Some(end) = pos.checked_add(4 + 7 * 17) else {
        return false;
    };
    if end != envelope.end
        || read_expected_u32(data, pos + 4, &HASH_AUNIT).is_none()
        || read_expected_u32(data, pos + 21, &HASH_AKILLER).is_none()
        || read_expected_u32(data, pos + 38, &HASH_AKILLER_EXPERIENCE).is_none()
        || read_expected_float(data, pos + 106, &HASH_AN_EXPLOSION_FORCE).is_none()
    {
        return false;
    }
    let Some(direction) = [
        read_expected_float(data, pos + 55, &HASH_AHIT_DIRECTION_X),
        read_expected_float(data, pos + 72, &HASH_AHIT_DIRECTION_Y),
        read_expected_float(data, pos + 89, &HASH_AHIT_DIRECTION_Z),
    ]
    .map(|value| value.map(f32::to_bits))
    .into_iter()
    .collect::<Option<Vec<_>>>() else {
        return false;
    };
    synthetic_terminal_directions().contains(&[direction[0], direction[1], direction[2]])
}

fn remove_container_relation(
    contained_by: &mut HashMap<u32, ContainerRelation>,
    children_by_container: &mut HashMap<u32, HashSet<u32>>,
    recently_removed: &mut HashMap<u32, ContainerRelation>,
    container_unit_id: u32,
    child_unit_id: u32,
    removed_time_bits: u32,
    removed_offset: usize,
) {
    let Some(mut relation) = contained_by.get(&child_unit_id).copied() else {
        return;
    };
    if relation.container_unit_id != container_unit_id {
        return;
    }
    contained_by.remove(&child_unit_id);
    if let Some(children) = children_by_container.get_mut(&container_unit_id) {
        children.remove(&child_unit_id);
    }
    relation.removed_time_bits = Some(removed_time_bits);
    relation.removed_offset = Some(removed_offset);
    recently_removed.insert(child_unit_id, relation);
}

fn extract_destruction_contexts(
    data: &[u8],
    event_index: &EventIndex,
) -> HashMap<usize, DestructionContextEvidence> {
    const CONTAINER_RELATION_TYPE: u32 = 2;
    const DESTROYED_BUILDING_STATE: i32 = 3;

    let mut active_units = HashMap::<u32, usize>::new();
    let mut occupancy = HashMap::<u32, BuildingOccupancy>::new();
    let mut building_destructions = HashSet::<(u32, u32)>::new();
    let mut contained_by = HashMap::<u32, ContainerRelation>::new();
    let mut children_by_container = HashMap::<u32, HashSet<u32>>::new();
    let mut recently_removed = HashMap::<u32, ContainerRelation>::new();
    let mut terminals = HashMap::<usize, DestructionTerminal>::new();
    let mut candidates = Vec::<DestructionContextCandidate>::new();

    for indexed in event_index.offsets() {
        let pos = indexed.offset;
        match indexed.tag {
            IndexedTag::UnitCreate => {
                let (
                    Some(_persistence_key),
                    Some(_player_id),
                    Some(_team),
                    Some(unit_id),
                    Some(_unit_type_id),
                ) = (
                    read_expected_u32(data, pos + 4, &HASH_APERSISTENCE_KEY),
                    read_expected_u32(data, pos + 21, &HASH_APLAYER),
                    read_expected_u32(data, pos + 38, &HASH_ATEAM),
                    read_expected_u32(data, pos + 55, &HASH_AUNIT),
                    read_expected_u32(data, pos + 72, &HASH_ATYPE),
                )
                else {
                    continue;
                };
                active_units.insert(unit_id, pos);
                occupancy.remove(&unit_id);
                if let Some(relation) = contained_by.remove(&unit_id)
                    && let Some(children) =
                        children_by_container.get_mut(&relation.container_unit_id)
                {
                    children.remove(&unit_id);
                }
                recently_removed.remove(&unit_id);
                if let Some(children) = children_by_container.remove(&unit_id) {
                    for child_id in children {
                        if contained_by
                            .get(&child_id)
                            .is_some_and(|relation| relation.container_unit_id == unit_id)
                        {
                            contained_by.remove(&child_id);
                        }
                    }
                }
            }
            IndexedTag::BuildingSetSlotState => {
                let (Some(building_id), Some(_slot_id), Some(will_occupy), Some(unit_id)) = (
                    read_expected_u32(data, pos + 4, &HASH_ABUILDING_NAME),
                    read_expected_i32(data, pos + 21, &HASH_AREAL_SLOT_ID),
                    read_expected_u32(data, pos + 38, &HASH_AWILL_OCCUPY_SLOT_FLAG),
                    read_expected_u32(data, pos + 55, &HASH_AUNIT),
                ) else {
                    continue;
                };
                if will_occupy != 0 {
                    if let Some(&generation_offset) = active_units.get(&unit_id) {
                        occupancy.insert(
                            unit_id,
                            BuildingOccupancy {
                                building_id,
                                unit_generation_offset: generation_offset,
                            },
                        );
                    }
                } else if occupancy
                    .get(&unit_id)
                    .is_some_and(|state| state.building_id == building_id)
                {
                    occupancy.remove(&unit_id);
                }
            }
            IndexedTag::BuildingDamaged => {
                let Some(envelope) = event_envelope(data, pos) else {
                    continue;
                };
                let (Some(building_id), Some(_health), Some(state), Some(_flag)) = (
                    read_expected_u32(data, pos + 4, &HASH_ANAME),
                    read_expected_i32(data, pos + 21, &HASH_AHEALTH),
                    read_expected_i32(data, pos + 38, &HASH_ASTATE),
                    read_expected_u32(data, pos + 55, &HASH_AFLAG),
                ) else {
                    continue;
                };
                if state == DESTROYED_BUILDING_STATE {
                    building_destructions
                        .insert((building_id, envelope.raw_time_seconds.to_bits()));
                }
            }
            IndexedTag::CreateUnitRelation => {
                let (Some(relation_type), Some(container_unit_id), Some(child_unit_id)) = (
                    read_expected_u32(data, pos + 4, &HASH_ATYPE),
                    read_expected_u32(data, pos + 21, &HASH_AFIRST_UNIT),
                    read_expected_u32(data, pos + 38, &HASH_ASECOND_UNIT),
                ) else {
                    continue;
                };
                if relation_type != CONTAINER_RELATION_TYPE {
                    continue;
                }
                let (Some(&container_generation_offset), Some(&child_generation_offset)) = (
                    active_units.get(&container_unit_id),
                    active_units.get(&child_unit_id),
                ) else {
                    continue;
                };
                if let Some(previous) = contained_by.insert(
                    child_unit_id,
                    ContainerRelation {
                        container_unit_id,
                        container_generation_offset,
                        child_generation_offset,
                        removed_time_bits: None,
                        removed_offset: None,
                    },
                ) && let Some(children) =
                    children_by_container.get_mut(&previous.container_unit_id)
                {
                    children.remove(&child_unit_id);
                }
                children_by_container
                    .entry(container_unit_id)
                    .or_default()
                    .insert(child_unit_id);
                recently_removed.remove(&child_unit_id);
            }
            IndexedTag::DestroyUnitRelations => {
                let Some(envelope) = event_envelope(data, pos) else {
                    continue;
                };
                let (Some(relation_type), Some(container_unit_id), Some(child_unit_id)) = (
                    read_expected_u32(data, pos + 4, &HASH_ATYPE),
                    read_expected_u32(data, pos + 21, &HASH_AFIRST_UNIT),
                    read_expected_u32(data, pos + 38, &HASH_ASECOND_UNIT),
                ) else {
                    continue;
                };
                if relation_type == CONTAINER_RELATION_TYPE {
                    remove_container_relation(
                        &mut contained_by,
                        &mut children_by_container,
                        &mut recently_removed,
                        container_unit_id,
                        child_unit_id,
                        envelope.raw_time_seconds.to_bits(),
                        pos,
                    );
                }
            }
            IndexedTag::DestroyUnitRelationsUnit => {
                let (Some(envelope), Some(unit_id)) = (
                    event_envelope(data, pos),
                    read_expected_u32(data, pos + 4, &HASH_AUNIT),
                ) else {
                    continue;
                };
                let raw_time_bits = envelope.raw_time_seconds.to_bits();
                if let Some(relation) = contained_by.get(&unit_id).copied() {
                    remove_container_relation(
                        &mut contained_by,
                        &mut children_by_container,
                        &mut recently_removed,
                        relation.container_unit_id,
                        unit_id,
                        raw_time_bits,
                        pos,
                    );
                }
                let children = children_by_container
                    .get(&unit_id)
                    .cloned()
                    .unwrap_or_default();
                for child_unit_id in children {
                    remove_container_relation(
                        &mut contained_by,
                        &mut children_by_container,
                        &mut recently_removed,
                        unit_id,
                        child_unit_id,
                        raw_time_bits,
                        pos,
                    );
                }
            }
            IndexedTag::UnitRemove | IndexedTag::UnitDestroy => {
                let Some(unit_id) = read_expected_u32(data, pos + 4, &HASH_AUNIT) else {
                    continue;
                };
                let Some(generation_offset) = active_units.remove(&unit_id) else {
                    continue;
                };
                let occupancy_at_terminal = occupancy
                    .remove(&unit_id)
                    .filter(|state| state.unit_generation_offset == generation_offset);
                let active_relation = contained_by
                    .get(&unit_id)
                    .copied()
                    .filter(|relation| relation.child_generation_offset == generation_offset);
                let removed_relation = recently_removed
                    .get(&unit_id)
                    .copied()
                    .filter(|relation| relation.child_generation_offset == generation_offset);
                let relation_at_terminal = active_relation.or(removed_relation);

                if indexed.tag == IndexedTag::UnitDestroy {
                    let (Some(envelope), Some(killer_unit_id)) = (
                        event_envelope(data, pos),
                        read_expected_u32(data, pos + 21, &HASH_AKILLER),
                    ) else {
                        continue;
                    };
                    let raw_time_bits = envelope.raw_time_seconds.to_bits();
                    let terminal = DestructionTerminal {
                        offset: pos,
                        raw_time_bits,
                        killer_unit_id,
                    };
                    terminals.insert(generation_offset, terminal);
                    candidates.push(DestructionContextCandidate {
                        offset: pos,
                        raw_time_bits,
                        unit_generation_offset: generation_offset,
                        killer_unit_id,
                        synthetic_direction: unit_destroy_has_synthetic_direction(data, pos),
                        occupancy: occupancy_at_terminal,
                        container_relation: relation_at_terminal,
                    });
                }

                if let Some(relation) = active_relation {
                    remove_container_relation(
                        &mut contained_by,
                        &mut children_by_container,
                        &mut recently_removed,
                        relation.container_unit_id,
                        unit_id,
                        event_envelope(data, pos)
                            .map(|envelope| envelope.raw_time_seconds.to_bits())
                            .unwrap_or_default(),
                        pos,
                    );
                }
                let children = children_by_container
                    .get(&unit_id)
                    .cloned()
                    .unwrap_or_default();
                for child_unit_id in children {
                    remove_container_relation(
                        &mut contained_by,
                        &mut children_by_container,
                        &mut recently_removed,
                        unit_id,
                        child_unit_id,
                        event_envelope(data, pos)
                            .map(|envelope| envelope.raw_time_seconds.to_bits())
                            .unwrap_or_default(),
                        pos,
                    );
                }
                recently_removed.remove(&unit_id);
            }
            _ => {}
        }
    }

    let mut contexts = HashMap::new();
    for candidate in candidates {
        if !candidate.synthetic_direction {
            continue;
        }
        let building_context = candidate.occupancy.and_then(|state| {
            (state.unit_generation_offset == candidate.unit_generation_offset
                && building_destructions.contains(&(state.building_id, candidate.raw_time_bits)))
            .then_some(UnitDestructionContext::BuildingCollapse {
                building_id: state.building_id,
            })
        });
        let container_context = candidate.container_relation.and_then(|relation| {
            if relation.child_generation_offset != candidate.unit_generation_offset
                || relation.removed_time_bits.is_some_and(|time| {
                    time != candidate.raw_time_bits
                        || relation
                            .removed_offset
                            .is_some_and(|offset| offset > candidate.offset)
                })
            {
                return None;
            }
            let terminal = terminals.get(&relation.container_generation_offset)?;
            (terminal.raw_time_bits == candidate.raw_time_bits
                && terminal.killer_unit_id == candidate.killer_unit_id)
                .then_some((
                    UnitDestructionContext::DestroyedWithContainer {
                        container_unit_id: relation.container_unit_id,
                    },
                    terminal.offset,
                ))
        });
        let evidence = match (building_context, container_context) {
            (Some(context), None) => Some(DestructionContextEvidence {
                context,
                source_destruction_offset: None,
            }),
            (None, Some((context, source_destruction_offset))) => {
                Some(DestructionContextEvidence {
                    context,
                    source_destruction_offset: Some(source_destruction_offset),
                })
            }
            _ => None,
        };
        if let Some(evidence) = evidence {
            contexts.insert(candidate.offset, evidence);
        }
    }
    contexts
}

fn inherited_container_tactical_aid(
    destruction_offset: usize,
    contexts: &HashMap<usize, DestructionContextEvidence>,
    direct_causes: &HashMap<usize, TacticalAidCause>,
) -> Result<Option<TacticalAidCause>, ()> {
    let Some(source_offset) = contexts
        .get(&destruction_offset)
        .and_then(|evidence| evidence.source_destruction_offset)
    else {
        return Ok(None);
    };
    let Some(&source_cause) = direct_causes.get(&source_offset) else {
        return Ok(None);
    };
    match direct_causes.get(&destruction_offset) {
        Some(direct_cause) if *direct_cause != source_cause => Err(()),
        Some(_) => Ok(None),
        None => Ok(Some(source_cause)),
    }
}

struct PositionedTimelineEvent {
    offset: usize,
    event: TimelineEvent,
}

fn build_clock_timeline(raw_samples: &[(usize, f32)]) -> (Vec<ClockSample>, Vec<TimelinePhase>) {
    let mut samples = Vec::with_capacity(raw_samples.len());
    let mut phases = Vec::new();
    let Some(&(first_offset, first_remaining)) = raw_samples.first() else {
        return (samples, phases);
    };

    let mut phase_index = 0u32;
    let mut phase_start_elapsed = 0.0f32;
    let mut phase_start_remaining = first_remaining;
    let mut phase_min_remaining = first_remaining;
    let mut previous_remaining = first_remaining;

    samples.push(ClockSample {
        offset: first_offset,
        remaining_seconds: first_remaining,
        elapsed_seconds: 0.0,
        phase_index: 0,
    });

    for &(offset, remaining) in &raw_samples[1..] {
        if remaining - previous_remaining > CLOCK_RESET_THRESHOLD_SECONDS {
            let phase_duration = (phase_start_remaining - phase_min_remaining).max(0.0);
            let phase_end_elapsed = phase_start_elapsed + phase_duration;
            phases.push(TimelinePhase {
                index: phase_index,
                start_seconds: phase_start_elapsed,
                end_seconds: phase_end_elapsed,
                initial_clock_seconds: phase_start_remaining,
                final_clock_seconds: phase_min_remaining,
            });

            phase_index += 1;
            phase_start_elapsed = phase_end_elapsed;
            phase_start_remaining = remaining;
            phase_min_remaining = remaining;
        } else {
            phase_min_remaining = phase_min_remaining.min(remaining);
        }

        samples.push(ClockSample {
            offset,
            remaining_seconds: remaining,
            elapsed_seconds: phase_start_elapsed
                + (phase_start_remaining - phase_min_remaining).max(0.0),
            phase_index,
        });
        previous_remaining = remaining;
    }

    phases.push(TimelinePhase {
        index: phase_index,
        start_seconds: phase_start_elapsed,
        end_seconds: phase_start_elapsed + (phase_start_remaining - phase_min_remaining).max(0.0),
        initial_clock_seconds: phase_start_remaining,
        final_clock_seconds: phase_min_remaining,
    });

    (samples, phases)
}

fn round_timeline_seconds(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

// ── Decompression ───────────────────────────────────────────────────────────

/// Decompress all zlib chunks in a single pass.
/// Returns (metadata_chunk, all_data) where metadata is the first chunk's output.
/// Reuses a single decompressor and buffer across all chunks to avoid repeated allocation.
fn decompress_chunks(raw: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    decompress_chunks_with_limits(raw, DECOMPRESSION_LIMITS)
}

/// Decompress the bounded replay stream for consumers that need the raw Event
/// chain while sharing exactly the parser's candidate, chunk, work, and output
/// limits.
// The CLI compiles this file as its own private module, where the viewer-only
// export is intentionally unused; the library build consumes it from playback.
#[allow(dead_code)]
pub fn decompress_replay_stream(raw: &[u8]) -> Result<Vec<u8>, String> {
    decompress_chunks(raw).map(|(_, full_data)| full_data)
}

fn decompress_chunks_with_limits(
    raw: &[u8],
    limits: DecompressionLimits,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    decompress_chunks_recording_gaps(raw, limits, &mut Vec::new())
}

fn decompress_chunks_recording_gaps(
    raw: &[u8],
    limits: DecompressionLimits,
    gaps: &mut Vec<usize>,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if raw.len() < 20 {
        return Err("File too small".to_string());
    }
    if raw.len() > limits.max_compressed_bytes {
        return Err(format!(
            "Replay exceeds the {} MiB compressed size limit",
            limits.max_compressed_bytes / 1024 / 1024
        ));
    }

    let initial_capacity = raw
        .len()
        .saturating_mul(4)
        .min(limits.max_decompressed_bytes);
    let mut all_data = Vec::with_capacity(initial_capacity);
    let mut metadata: Option<Vec<u8>> = None;
    let mut offset = 19;
    let mut chunk_count = 0usize;
    let mut candidate_count = 0usize;
    let mut zlib_work_bytes = 0usize;
    let zlib_headers: [[u8; 2]; 3] = [[0x78, 0xda], [0x78, 0x9c], [0x78, 0x01]];

    let mut decompressor = Decompress::new(true);
    let mut buf = vec![0u8; MAX_CHUNK_DECOMPRESSED_BYTES];

    while offset < raw.len() {
        if raw.len() - offset >= 2 && zlib_headers.contains(&[raw[offset], raw[offset + 1]]) {
            candidate_count = candidate_count
                .checked_add(1)
                .ok_or_else(|| "Replay zlib candidate count overflow".to_string())?;
            if candidate_count > limits.max_zlib_candidates {
                return Err(format!(
                    "Replay exceeds the maximum of {} zlib candidates",
                    limits.max_zlib_candidates
                ));
            }

            decompressor.reset(true);
            let result = decompressor.decompress(&raw[offset..], &mut buf, FlushDecompress::Finish);
            let attempted = usize::try_from(decompressor.total_in())
                .map_err(|_| "Replay zlib work count overflow".to_string())?;
            zlib_work_bytes = zlib_work_bytes
                .checked_add(attempted)
                .ok_or_else(|| "Replay zlib work count overflow".to_string())?;
            if zlib_work_bytes > limits.max_zlib_work_bytes {
                return Err(format!(
                    "Replay exceeds the {} MiB zlib work limit",
                    limits.max_zlib_work_bytes / 1024 / 1024
                ));
            }

            let status = match result {
                Ok(status) => status,
                Err(_) => {
                    // Successful chunks are concatenated for legacy recovery.
                    // Preserve the splice so a coincidentally aligned event
                    // chain cannot establish final occupant continuity.
                    if gaps.last() != Some(&all_data.len()) {
                        gaps.push(all_data.len());
                    }
                    offset += 1;
                    continue;
                }
            };

            let out_len = decompressor.total_out() as usize;
            let consumed = decompressor.total_in() as usize;
            if out_len == 0 {
                offset += 1;
                continue;
            }

            // A non-terminal status means the stream either needs more than the
            // fixed 64 KiB chunk buffer or is truncated. Never accept partial data.
            if status != Status::StreamEnd {
                return Err("Replay contains an oversized or truncated zlib chunk".to_string());
            }
            if consumed == 0 {
                return Err("Replay contains a zero-length zlib chunk".to_string());
            }

            chunk_count = chunk_count
                .checked_add(1)
                .ok_or_else(|| "Replay zlib chunk count overflow".to_string())?;
            if chunk_count > limits.max_zlib_chunks {
                return Err(format!(
                    "Replay exceeds the maximum of {} zlib chunks",
                    limits.max_zlib_chunks
                ));
            }

            let next_size = all_data
                .len()
                .checked_add(out_len)
                .ok_or_else(|| "Replay decompressed size overflow".to_string())?;
            if next_size > limits.max_decompressed_bytes {
                return Err(format!(
                    "Replay exceeds the {} MiB decompressed size limit",
                    limits.max_decompressed_bytes / 1024 / 1024
                ));
            }

            let chunk_data = &buf[..out_len];
            if metadata.is_none() {
                metadata = Some(chunk_data.to_vec());
            }
            all_data.extend_from_slice(chunk_data);
            offset += consumed;
        } else {
            offset += 1;
        }
    }

    match metadata {
        Some(m) => Ok((m, all_data)),
        None => Err("No data decompressed".to_string()),
    }
}

// ── UTF-16LE name reading ───────────────────────────────────────────────────

fn read_utf16le_name(data: &[u8], start: usize, max_chars: usize) -> Option<String> {
    let limit = (start + max_chars * 2).min(data.len().saturating_sub(1));
    let mut end = start;
    while end < limit {
        if end + 1 >= data.len() {
            return None;
        }
        if data[end] == 0 && data[end + 1] == 0 {
            if end == start {
                return None;
            }
            let slice = &data[start..end];
            if !slice.len().is_multiple_of(2) {
                return None;
            }
            let chars: Vec<u16> = slice
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let name = String::from_utf16(&chars).ok()?;
            let name = name.trim_end_matches('\u{00A0}').to_string();
            if name.len() >= 3
                && name.len() <= 30
                && !name.contains('\0')
                && name.chars().all(|c| !c.is_control())
            {
                return Some(name);
            }
            return None;
        }
        end += 2;
    }
    None
}

// ── Date/time extraction ────────────────────────────────────────────────────

fn extract_date_time(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(15) {
        if bytes[i] == b'2'
            && bytes[i + 1] == b'0'
            && i + 9 < bytes.len()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4] == b'-'
            && bytes[i + 5].is_ascii_digit()
            && bytes[i + 6].is_ascii_digit()
            && bytes[i + 7] == b'-'
            && bytes[i + 8].is_ascii_digit()
            && bytes[i + 9].is_ascii_digit()
        {
            let year = &s[i..i + 4];
            let month = &s[i + 5..i + 7];
            let day = &s[i + 8..i + 10];
            if let Some(colon_pos) = s[i + 10..].find(':') {
                let abs_colon = i + 10 + colon_pos;
                if abs_colon >= 2 && abs_colon + 3 <= s.len() {
                    let hour_start = abs_colon - 2;
                    if hour_start < s.len()
                        && abs_colon + 2 < s.len()
                        && s.as_bytes()[hour_start].is_ascii_digit()
                        && s.as_bytes()[hour_start + 1].is_ascii_digit()
                        && s.as_bytes()[abs_colon + 1].is_ascii_digit()
                        && s.as_bytes()[abs_colon + 2].is_ascii_digit()
                    {
                        let hour = &s[hour_start..hour_start + 2];
                        let minute = &s[abs_colon + 1..abs_colon + 3];
                        return Some(format!("- {year}-{month}-{day} - {hour}:{minute}"));
                    }
                }
            }
        }
    }
    None
}

// ── Parser ──────────────────────────────────────────────────────────────────

pub struct WicReplayParser {
    metadata: Vec<u8>,
    full_data: Vec<u8>,
    decompression_gaps: Vec<usize>,
    event_index: EventIndex,
    clock_timeline: OnceLock<(Vec<ClockSample>, Vec<TimelinePhase>)>,
    /// Envelope time base for the primary event chain, built on first use.
    envelope_timeline: OnceLock<EnvelopeTimeline>,
    /// Whether the recording observed the pre-match countdown (`remaining == 0`)
    /// before the clock started. False means the recorder joined mid-match.
    clock_captured_start: OnceLock<bool>,
    base_slot_names: OnceLock<HashMap<u32, String>>,
    recorder_slot: Option<u32>,
}

struct ReplayWorkLimiter {
    active: Mutex<usize>,
    available: Condvar,
}

impl ReplayWorkLimiter {
    const fn new() -> Self {
        Self {
            active: Mutex::new(0),
            available: Condvar::new(),
        }
    }

    fn acquire(&self) -> ReplayWorkGuard<'_> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *active >= available_replay_workers() {
            active = self
                .available
                .wait(active)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *active += 1;
        ReplayWorkGuard { limiter: self }
    }
}

static REPLAY_WORK_LIMITER: ReplayWorkLimiter = ReplayWorkLimiter::new();

/// Process-wide permit for replay work. The ceiling follows the logical CPU
/// capacity visible to this process, so imports use the machine fully without
/// allowing independent detail or playback requests to oversubscribe it.
pub struct ReplayWorkGuard<'a> {
    limiter: &'a ReplayWorkLimiter,
}

impl Drop for ReplayWorkGuard<'_> {
    fn drop(&mut self) {
        let mut active = self
            .limiter
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *active = active.saturating_sub(1);
        self.limiter.available.notify_one();
    }
}

pub fn acquire_replay_work() -> ReplayWorkGuard<'static> {
    REPLAY_WORK_LIMITER.acquire()
}

/// Logical processor capacity available to the current process. The standard
/// library respects CPU affinity and container limits; one is the safe fallback.
pub fn available_replay_workers() -> usize {
    static WORKERS: OnceLock<usize> = OnceLock::new();
    *WORKERS
        .get_or_init(|| std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get))
}

/// Native file-path entry points accept only World in Conflict replay names.
/// Raw-byte/WASM callers have no filename and retain the same content limits.
pub fn validate_replay_path(filepath: &Path) -> Result<(), String> {
    if filepath
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wicdemo"))
    {
        return Ok(());
    }
    Err(format!(
        "Replay path must end in .wicdemo: {}",
        filepath.display()
    ))
}

/// Read a replay without allowing a regular file, special file, or concurrently
/// growing file to allocate beyond the parser's compressed-input budget.
pub fn read_replay_file(filepath: &Path) -> Result<Vec<u8>, String> {
    validate_replay_path(filepath)?;
    let file = File::open(filepath)
        .map_err(|error| format!("Cannot read {}: {error}", filepath.display()))?;
    let declared_size = file
        .metadata()
        .map_err(|error| format!("Cannot inspect {}: {error}", filepath.display()))?
        .len();
    if declared_size > MAX_REPLAY_COMPRESSED_BYTES as u64 {
        return Err(format!(
            "Replay exceeds the {} MiB compressed size limit",
            MAX_REPLAY_COMPRESSED_BYTES / 1024 / 1024
        ));
    }

    let mut raw = Vec::with_capacity(
        usize::try_from(declared_size)
            .unwrap_or(MAX_REPLAY_COMPRESSED_BYTES)
            .min(MAX_REPLAY_COMPRESSED_BYTES),
    );
    file.take((MAX_REPLAY_COMPRESSED_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|error| format!("Cannot read {}: {error}", filepath.display()))?;
    if raw.len() > MAX_REPLAY_COMPRESSED_BYTES {
        return Err(format!(
            "Replay exceeds the {} MiB compressed size limit",
            MAX_REPLAY_COMPRESSED_BYTES / 1024 / 1024
        ));
    }
    Ok(raw)
}

impl WicReplayParser {
    /// Create parser from a file path (CLI usage).
    pub fn new(filepath: &Path) -> Result<Self, String> {
        // Keep the permit local: retaining a parsed replay must not reserve a
        // worker slot. Public extraction methods reacquire it while doing work.
        let _work = acquire_replay_work();
        let raw = read_replay_file(filepath)?;
        Self::from_bytes_inner(&raw)
    }

    /// Create parser from raw bytes (WASM usage).
    // The CLI compiles this file as a private module and enters through `new`;
    // the library and WASM builds use this raw-byte entry point.
    #[allow(dead_code)]
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        // See `new`: parser values are reusable data, not active worker slots.
        let _work = acquire_replay_work();
        Self::from_bytes_inner(raw)
    }

    fn from_bytes_inner(raw: &[u8]) -> Result<Self, String> {
        let mut gaps = Vec::new();
        let (metadata, full_data) =
            decompress_chunks_recording_gaps(raw, DECOMPRESSION_LIMITS, &mut gaps)?;
        let mut parser = Self::try_from_decompressed(metadata, full_data)?;
        parser.decompression_gaps = gaps;
        if parser
            .event_index
            .positions(IndexedTag::TeamWins)
            .next()
            .is_none()
        {
            return Err("Corrupt replay file: no TeamWins event found".to_string());
        }
        if parser.envelope_timeline().starts.is_empty() {
            return Err("Corrupt replay file: no Event chain found".to_owned());
        }
        if !parser
            .envelope_timeline()
            .times
            .windows(2)
            .all(|times| times[0] <= times[1])
        {
            return Err("Corrupt replay file: non-monotonic Event chain".to_owned());
        }
        Ok(parser)
    }

    fn try_from_decompressed(metadata: Vec<u8>, full_data: Vec<u8>) -> Result<Self, String> {
        let event_index = EventIndex::new(&full_data)?;
        validate_attribution_source_event_budget(&event_index)?;
        validate_unit_drop_attribution_work(&full_data, &event_index)?;
        let timeline = build_envelope_timeline(&full_data);
        let envelope_timeline = OnceLock::new();
        envelope_timeline
            .set(timeline)
            .expect("new envelope timeline cache is empty");
        let recorder_slot = find_pattern(&metadata, &HASH_POV_PLAYER, 0)
            .and_then(|pos| read_bintag_u32(&metadata, pos))
            .filter(|slot| *slot <= 15);
        Ok(Self {
            metadata,
            full_data,
            decompression_gaps: Vec::new(),
            event_index,
            clock_timeline: OnceLock::new(),
            envelope_timeline,
            clock_captured_start: OnceLock::new(),
            base_slot_names: OnceLock::new(),
            recorder_slot,
        })
    }

    #[cfg(test)]
    fn from_decompressed(metadata: Vec<u8>, full_data: Vec<u8>) -> Self {
        Self::try_from_decompressed(metadata, full_data).expect("bounded parser fixture")
    }

    fn extract_map_name(&self) -> String {
        let chunk = &self.metadata;
        if chunk.len() <= 47 {
            return "Unknown".to_string();
        }
        let start = 47;
        if let Some(end_off) = chunk[start..].iter().position(|&b| b == 0) {
            String::from_utf8_lossy(&chunk[start..start + end_off]).to_string()
        } else {
            "Unknown".to_string()
        }
    }

    fn extract_utf16_strings(&self) -> Vec<(usize, String)> {
        let chunk = &self.metadata;
        let mut results = Vec::new();
        let mut i = 0;
        while i + 1 < chunk.len() {
            if chunk[i] >= 0x20 && chunk[i] <= 0x7e && chunk[i + 1] == 0x00 {
                let start = i;
                let mut end = i;
                while end + 1 < chunk.len()
                    && chunk[end] >= 0x20
                    && chunk[end] <= 0x7e
                    && chunk[end + 1] == 0x00
                {
                    end += 2;
                }
                let char_count = (end - start) / 2;
                if char_count >= 3
                    && end + 1 < chunk.len()
                    && chunk[end] == 0
                    && chunk[end + 1] == 0
                {
                    let chars: Vec<u16> = chunk[start..end]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    if let Ok(s) = String::from_utf16(&chars)
                        && s.chars().all(|c| !c.is_control())
                    {
                        results.push((start, s));
                    }
                }
                i = end;
            } else {
                i += 1;
            }
        }
        results
    }

    fn extract_game_info(&self) -> GameInfo {
        let map_name = self.extract_map_name();
        let display_name = map_display_name(&map_name).to_string();
        let strings = self.extract_utf16_strings();

        let server_name = find_pattern(&self.metadata, &HASH_MY_GAME_NAME, 0)
            .and_then(|position| {
                read_expected_utf16_string(&self.metadata, position, &HASH_MY_GAME_NAME)
            })
            .unwrap_or_else(|| "Unknown".to_string());
        let mut replay_names = find_all(&self.metadata, &HASH_REPLAY_NAME).filter_map(|position| {
            read_expected_utf16_string(&self.metadata, position, &HASH_REPLAY_NAME)
        });
        let replay_name = replay_names
            .next()
            .filter(|_| replay_names.next().is_none());
        let mut date_time = "Unknown".to_string();

        for (_offset, s) in &strings {
            if let Some(dt) = extract_date_time(s) {
                date_time = dt;
            }
        }

        let game_mode = if display_name.starts_with("do_") || map_name.starts_with("maps/do_") {
            "Domination"
        } else if display_name.starts_with("as_") || map_name.starts_with("maps/as_") {
            "Assault"
        } else if display_name.starts_with("tw_") || map_name.starts_with("maps/tw_") {
            "Tug of War"
        } else {
            "Unknown"
        };

        GameInfo {
            map_name,
            map_display_name: display_name,
            server_name,
            date_time,
            replay_name,
            game_mode: game_mode.to_string(),
        }
    }

    fn extract_server_classification(&self) -> (RawServerFlags, ServerClassification) {
        let raw = RawServerFlags {
            few_player_mode_flag: read_unique_bintag_bool(&self.metadata, &HASH_MY_FPM_MODE_FLAG),
            match_mode_flag: read_unique_bintag_bool(&self.metadata, &HASH_MY_MATCH_MODE_FLAG),
            tournament_match_flag: read_unique_bintag_bool(
                &self.metadata,
                &HASH_MY_IS_TOURNAMENT_MATCH_FLAG,
            ),
            clan_match_flag: read_unique_bintag_bool(&self.metadata, &HASH_MY_IS_CLAN_MATCH_FLAG),
        };

        let player_types: HashSet<u32> = find_all(&self.full_data, &HASH_MY_TYPE)
            .filter_map(|position| read_expected_u32(&self.full_data, position, &HASH_MY_TYPE))
            .collect();
        let has_bots = if player_types.contains(&1) {
            Some(true)
        } else if !player_types.is_empty() && player_types.iter().all(|value| *value == 0) {
            Some(false)
        } else {
            None
        };
        let match_mode = match (raw.match_mode_flag, raw.few_player_mode_flag) {
            (Some(match_flag), Some(fpm_flag)) => Some(match_flag && !fpm_flag),
            _ => None,
        };

        let classification = ServerClassification {
            few_player_mode: raw.few_player_mode_flag,
            match_mode,
            has_bots,
            clan_match: raw.clan_match_flag,
            tournament_match: raw.tournament_match_flag,
            ranked: None,
        };
        (raw, classification)
    }

    fn extract_slot_names_from_positions(
        &self,
        data: &[u8],
        positions: impl IntoIterator<Item = usize>,
        target_ids: Option<&HashSet<u32>>,
    ) -> HashMap<u32, String> {
        let mut slot_names: HashMap<u32, String> = HashMap::new();

        for pos in positions {
            if pos + 55 <= data.len()
                && data[pos + 4..pos + 8] == BINTAG_SEP
                && let Some(slot) = read_u32_le(data, pos + 13)
                && slot <= 15
                && !slot_names.contains_key(&slot)
            {
                let should_extract = target_ids.map(|ids| ids.contains(&slot)).unwrap_or(true);

                if should_extract {
                    let mut name = read_utf16le_name(data, pos + 47, 30);

                    if name.is_none() && pos + 200 <= data.len() {
                        let mut scan = pos + 48;
                        let scan_end = (pos + 200).min(data.len().saturating_sub(60));
                        while scan < scan_end {
                            if scan + 1 < data.len() && data[scan] >= 0x20 && data[scan + 1] == 0x00
                            {
                                name = read_utf16le_name(data, scan, 30);
                                if name.is_some() {
                                    break;
                                }
                            }
                            scan += 2;
                        }
                    }

                    if let Some(n) = name {
                        slot_names.insert(slot, n);
                    }
                }
            }

            if let Some(ids) = target_ids
                && slot_names.len() >= ids.len()
            {
                break;
            }
        }

        slot_names
    }

    /// Detect lobby slot swaps by scanning early event stream for aSlot entries
    /// with names that differ from metadata. Only applies confirmed bilateral swaps.
    fn apply_lobby_swaps(&self, slot_names: &mut HashMap<u32, String>) {
        let meta_end = self.metadata.len();
        let lobby_end = (meta_end + 200_000).min(self.full_data.len());
        let data = &self.full_data;

        // Collect event stream names that differ from metadata
        let mut event_names: HashMap<u32, String> = HashMap::new();
        for pos in self.event_index.positions(IndexedTag::ASlot) {
            if pos < meta_end {
                continue;
            }
            if pos >= lobby_end {
                break;
            }
            if pos + 55 <= data.len()
                && data[pos + 4..pos + 8] == BINTAG_SEP
                && let Some(slot) = read_u32_le(data, pos + 13)
                && slot <= 15
                && let Some(name) = read_utf16le_name(data, pos + 47, 30)
                && slot_names.get(&slot).map(|n| n != &name).unwrap_or(false)
            {
                event_names.insert(slot, name);
            }
        }

        // Only apply confirmed bilateral swaps
        let mut applied: HashSet<u32> = HashSet::new();
        let slots: Vec<u32> = event_names.keys().copied().collect();
        for &slot_a in &slots {
            if applied.contains(&slot_a) {
                continue;
            }
            for &slot_b in &slots {
                if slot_b <= slot_a || applied.contains(&slot_b) {
                    continue;
                }
                let meta_a = slot_names.get(&slot_a);
                let meta_b = slot_names.get(&slot_b);
                let event_a = event_names.get(&slot_a);
                let event_b = event_names.get(&slot_b);
                if let (Some(ma), Some(mb), Some(ea), Some(eb)) = (meta_a, meta_b, event_a, event_b)
                    && ea == mb
                    && eb == ma
                {
                    slot_names.insert(slot_a, ea.clone());
                    slot_names.insert(slot_b, eb.clone());
                    applied.insert(slot_a);
                    applied.insert(slot_b);
                }
            }
        }
    }

    /// Resolve canonical player names for the requested slots.
    ///
    /// Metadata supplies the normal roster, confirmed bilateral lobby swaps
    /// correct stale pre-swap slot assignments, and the concatenated stream
    /// supplies a fallback for requested slots missing from incomplete older
    /// metadata or split across the first decompressed chunk boundary.
    fn extract_resolved_slot_names(&self, required_ids: &HashSet<u32>) -> HashMap<u32, String> {
        let mut slot_names = self
            .base_slot_names
            .get_or_init(|| {
                let mut names = self.extract_slot_names_from_positions(
                    &self.metadata,
                    find_all(&self.metadata, &HASH_ASLOT),
                    None,
                );
                self.apply_lobby_swaps(&mut names);
                names
            })
            .clone();

        let missing_ids: HashSet<u32> = required_ids
            .iter()
            .filter(|player_id| !slot_names.contains_key(player_id))
            .copied()
            .collect();
        if !missing_ids.is_empty() {
            let fallback_start = self
                .metadata
                .len()
                .saturating_sub(SLOT_NAME_RECORD_MAX_BYTES);
            let extra = self.extract_slot_names_from_positions(
                &self.full_data,
                self.event_index
                    .positions(IndexedTag::ASlot)
                    .skip_while(|pos| *pos < fallback_start),
                Some(&missing_ids),
            );
            slot_names.extend(extra);
        }

        slot_names
    }

    /// Resolve the occupants of scored slots when gameplay begins.
    ///
    /// Static metadata is a lobby snapshot and can be stale after a player leaves
    /// and another player enters the same numeric slot before the match. The first
    /// valid countdown sample is the existing gameplay boundary, so a structurally
    /// decoded occupant already active there supersedes the metadata name. Unscored,
    /// unresolved, or temporarily vacant slots retain the established metadata
    /// behavior, and later replacements do not relabel accumulated score rows.
    fn extract_match_roster_names(&self, required_ids: &HashSet<u32>) -> HashMap<u32, String> {
        let mut roster_names = self.extract_resolved_slot_names(required_ids);
        let Some(match_start_offset) = self
            .extract_clock_timeline()
            .0
            .first()
            .map(|sample| sample.offset)
        else {
            return roster_names;
        };
        let identity_intervals =
            PlayerIdentityIntervals::build(&self.full_data, &self.event_index, &roster_names);

        for &player_id in required_ids {
            if let Some(name) = identity_intervals.name_at(player_id, match_start_offset) {
                roster_names.insert(player_id, name.to_string());
            }
        }
        self.correct_late_lobby_names(
            &mut roster_names,
            required_ids,
            &identity_intervals,
            match_start_offset,
        );
        roster_names
    }

    /// Correct only a vacant lobby slot with one later gameplay entrant whose
    /// recorded team agrees with the existing scored/role-bearing result row.
    fn correct_late_lobby_names(
        &self,
        names: &mut HashMap<u32, String>,
        scored: &HashSet<u32>,
        identities: &PlayerIdentityIntervals,
        start: usize,
    ) {
        let vacant: Vec<_> = scored
            .iter()
            .copied()
            .filter(|slot| {
                !identities.intervals.iter().any(|s| {
                    s.player_id == *slot
                        && s.start_offset <= start
                        && s.end_offset.is_none_or(|end| end > start)
                })
            })
            .collect();
        if vacant.is_empty() {
            return;
        }
        let Some(end) = self
            .event_index
            .positions(IndexedTag::TeamWins)
            .find(|pos| *pos > start && *pos < self.envelope_timeline().end_offset)
        else {
            return;
        };
        let summaries = self.extract_player_end_summaries();
        let teams = self.extract_team_assignments();
        for slot in vacant {
            // A negative role score still proves activity in that role. Do not
            // make identity resolution depend on which role wins signed ranking.
            if !summaries.get(&slot).is_some_and(|s| {
                [s.infantry, s.support, s.armor, s.air]
                    .iter()
                    .any(|score| *score != 0)
            }) {
                continue;
            }
            let Some(&team) = teams.get(&slot).filter(|team| (1..=3).contains(*team)) else {
                continue;
            };
            let mut sessions = identities.intervals.iter().filter(|s| {
                s.player_id == slot
                    && s.start_offset < end
                    && s.end_offset.is_none_or(|stop| stop > start)
            });
            let Some(session) = sessions.next() else {
                continue;
            };
            if sessions.next().is_some() || session.start_offset <= start {
                continue;
            }
            let Some(name) = &session.player_name else {
                continue;
            };
            let stop = session.end_offset.unwrap_or(end).min(end);
            let joined: HashSet<_> = self
                .event_index
                .positions(IndexedTag::PlayerJoinedTeam)
                .filter(|pos| *pos >= session.start_offset && *pos < stop)
                .filter(|pos| {
                    read_expected_u32(&self.full_data, *pos + 4, &HASH_ASLOT) == Some(slot)
                })
                .filter_map(|pos| read_expected_u32(&self.full_data, pos + 21, &HASH_ATEAM))
                .collect();
            if joined == HashSet::from([team]) {
                names.insert(slot, name.clone());
            }
        }
    }

    /// Mark only a uniquely identified session that explicitly left before the
    /// primary result. Reconnects, unknown occupants, and implicit replacement
    /// boundaries do not establish a departure for a result row.
    fn mark_result_departures(&self, players: &mut [Player]) {
        let Some(start) = self.extract_clock_timeline().0.first().map(|s| s.offset) else {
            return;
        };
        let timeline = self.envelope_timeline();
        let index = EventIndex {
            offsets: self
                .event_index
                .offsets()
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .offset
                        .checked_sub(ENVELOPE_MESSAGE_OFFSET)
                        .is_some_and(|offset| timeline.starts.binary_search(&offset).is_ok())
                })
                .collect(),
        };
        let Some(end) = index
            .positions(IndexedTag::TeamWins)
            .find(|pos| *pos > start)
        else {
            return;
        };
        let data = &self.full_data;
        let identities = PlayerIdentityIntervals::build(
            data,
            &index,
            &self.extract_resolved_slot_names(&HashSet::new()),
        );
        for player in players {
            let sessions: Vec<_> = identities
                .intervals
                .iter()
                .filter(|s| {
                    s.player_id == player.id
                        && s.start_offset < end
                        && s.end_offset.is_none_or(|stop| stop > start)
                })
                .collect();
            if sessions.iter().any(|s| s.player_name.is_none()) {
                continue;
            }
            let matching: Vec<_> = sessions
                .iter()
                .filter(|s| s.player_name.as_deref() == Some(&player.name))
                .collect();
            if matching.len() != 1 {
                continue;
            }
            let session = matching[0];
            let Some(left) = session.end_offset.filter(|left| *left < end) else {
                continue;
            };
            let entered = index.positions(IndexedTag::PlayerEntersGame).any(|pos| {
                pos >= session.start_offset
                    && pos < left
                    && parse_player_entry(data, pos).is_some_and(|(_, slot, name)| {
                        slot == player.id && name.as_deref() == Some(&player.name)
                    })
            });
            if !entered {
                continue;
            }
            player.left_at_seconds = index
                .positions(IndexedTag::PlayerLeavesGame)
                .filter(|pos| {
                    *pos >= start
                        && *pos >= session.start_offset
                        && *pos < left
                        && read_expected_u32(data, *pos + 4, &HASH_ASLOT) == Some(player.id)
                })
                .find_map(|pos| {
                    event_envelope(data, pos)
                        .filter(|envelope| envelope.end == left)
                        .map(|envelope| envelope.raw_time_seconds)
                });
        }
    }

    /// A lobby name can survive a replacement before gameplay or a sole late
    /// entrant into a vacant match-start slot. Require a full start, one explicit session, and owned units
    /// on the existing result team; never rename a slot shared during gameplay.
    fn correct_vacant_result_names(&self, players: &mut [Player]) {
        if !self.captured_match_start() {
            return;
        }
        let Some(start) = self.extract_clock_timeline().0.first().map(|s| s.offset) else {
            return;
        };
        let timeline = self.envelope_timeline();
        let index = EventIndex {
            offsets: self
                .event_index
                .offsets()
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .offset
                        .checked_sub(ENVELOPE_MESSAGE_OFFSET)
                        .is_some_and(|offset| timeline.starts.binary_search(&offset).is_ok())
                })
                .collect(),
        };
        let Some(end) = index.positions(IndexedTag::TeamWins).find(|p| *p > start) else {
            return;
        };
        let data = &self.full_data;
        let identities = PlayerIdentityIntervals::build(
            data,
            &index,
            &self.extract_resolved_slot_names(&HashSet::new()),
        );
        let scores = self.explicit_match_scores();
        for player in players {
            let Some(team) = player.team.filter(|t| (1..=3).contains(t)) else {
                continue;
            };
            let sessions: Vec<_> = identities
                .intervals
                .iter()
                .filter(|s| {
                    s.player_id == player.id
                        && s.start_offset < end
                        && s.end_offset.is_none_or(|stop| stop > start)
                })
                .collect();
            if sessions.len() != 1 {
                continue;
            }
            let session = sessions[0];
            if session.identity_source != TimelineParticipantIdentitySource::PlayerEnteredGame {
                continue;
            }
            let Some(name) = &session.player_name else {
                continue;
            };
            if name == &player.name {
                continue;
            }
            let stop = session.end_offset.unwrap_or(end).min(end);
            let units: Vec<_> = index
                .positions(IndexedTag::UnitCreate)
                .filter(|pos| {
                    *pos < end
                        && read_expected_u32(data, *pos + 21, &HASH_APLAYER) == Some(player.id)
                })
                .collect();
            if units.is_empty()
                || units.iter().any(|pos| {
                    *pos < session.start_offset
                        || *pos >= stop
                        || read_expected_u32(data, *pos + 38, &HASH_ATEAM) != Some(team)
                })
            {
                continue;
            }
            if scores.iter().any(|(pos, slot, score)| {
                *slot == player.id && *pos < session.start_offset && *score != 0
            }) {
                continue;
            }
            let joins: Vec<_> = index
                .positions(IndexedTag::PlayerJoinedTeam)
                .filter(|pos| {
                    *pos >= session.start_offset
                        && *pos < stop
                        && read_expected_u32(data, *pos + 4, &HASH_ASLOT) == Some(player.id)
                })
                .filter_map(|pos| {
                    read_expected_u32(data, pos + 21, &HASH_ATEAM).map(|team| (pos, team))
                })
                .collect();
            let joined: HashSet<_> = joins
                .iter()
                .filter(|(pos, _)| *pos >= start)
                .chain(joins.iter().rev().find(|(pos, _)| *pos < start))
                .map(|(_, team)| *team)
                .collect();
            if joined == HashSet::from([team]) {
                player.name = name.clone();
            }
        }
    }

    /// Repair only zero-stat result rows with explicit spectator evidence for
    /// their own match-start identity. Keep the general team resolver unchanged.
    fn correct_zero_score_spectators(&self, players: &mut [Player]) {
        let candidates: Vec<_> = players
            .iter_mut()
            .filter(|p| p.team.is_some_and(|team| (1..=3).contains(&team)) && p.has_zero_stats())
            .collect();
        if candidates.is_empty() {
            return;
        }
        if !self.captured_match_start() {
            return;
        }
        let Some(start) = self.extract_clock_timeline().0.first().map(|s| s.offset) else {
            return;
        };
        let timeline = self.envelope_timeline();
        let index = EventIndex {
            offsets: self
                .event_index
                .offsets()
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .offset
                        .checked_sub(ENVELOPE_MESSAGE_OFFSET)
                        .is_some_and(|offset| timeline.starts.binary_search(&offset).is_ok())
                })
                .collect(),
        };
        if !index
            .positions(IndexedTag::SetGameModeDataFloat)
            .any(|pos| pos == start)
        {
            return;
        }
        let Some(end) = index
            .positions(IndexedTag::TeamWins)
            .find(|pos| *pos > start)
        else {
            return;
        };
        let data = &self.full_data;
        let identities = PlayerIdentityIntervals::build(
            data,
            &index,
            &self.extract_resolved_slot_names(&HashSet::new()),
        );
        // Units at any time and gameplay role selections block the whole slot.
        // Lobby roles require a later, pre-game spectator state in this session.
        let playing_slots: HashSet<_> = index
            .offsets()
            .iter()
            .filter(|entry| entry.offset < end)
            .filter_map(|entry| match entry.tag {
                IndexedTag::PlayerSetRole if entry.offset >= start => {
                    read_expected_u32(data, entry.offset + 4, &HASH_ASLOT)
                }
                IndexedTag::UnitCreate => read_expected_u32(data, entry.offset + 21, &HASH_APLAYER),
                _ => None,
            })
            .collect();
        let nonzero_slots: HashSet<_> = self
            .explicit_match_scores()
            .iter()
            .filter(|(_, _, score)| *score != 0)
            .map(|(_, slot, _)| *slot)
            .collect();
        for player in candidates {
            if playing_slots.contains(&player.id) {
                continue;
            }
            let mut sessions = identities.intervals.iter().filter(|s| {
                s.player_id == player.id
                    && s.player_name.as_deref() == Some(player.name.as_str())
                    && s.start_offset < end
                    && s.end_offset.is_none_or(|stop| stop > start)
            });
            let Some(session) = sessions.next() else {
                continue;
            };
            if sessions.next().is_some() || session.start_offset > start {
                continue;
            }
            let stop = session.end_offset.unwrap_or(end).min(end);
            let mut spectator = false;
            let mut lobby_role_needs_spectator = false;
            let mut had_lobby_role = false;
            let mut joined_during_match = false;
            let mut unsuperseded_role = false;
            for entry in index
                .offsets()
                .iter()
                .filter(|entry| entry.offset >= session.start_offset && entry.offset < stop)
            {
                let pos = entry.offset;
                if read_expected_u32(data, pos + 4, &HASH_ASLOT) != Some(player.id) {
                    continue;
                }
                match entry.tag {
                    IndexedTag::PlayerSetRole if pos < start => {
                        had_lobby_role = true;
                        unsuperseded_role = true;
                        lobby_role_needs_spectator = true;
                    }
                    IndexedTag::PlayerJoinedTeam => {
                        spectator = false;
                        if pos >= start {
                            joined_during_match = true;
                        }
                        if had_lobby_role {
                            lobby_role_needs_spectator = true;
                        }
                    }
                    IndexedTag::SpectatorJoinedTeam => {
                        spectator = matches!(
                            (
                                read_expected_u32(data, pos + 21, &HASH_ATEAM),
                                read_expected_u32(data, pos + 38, &HASH_ASPECTATOR_LOS)
                            ),
                            (Some(0..=3), Some(2)) | (Some(1..=3), Some(1))
                        );
                        if spectator {
                            unsuperseded_role = false;
                        }
                        if pos < start && had_lobby_role {
                            lobby_role_needs_spectator = !spectator;
                        }
                    }
                    _ => {}
                }
            }
            // Roles belonging to another occupant remain a blocker, too.
            let other_lobby_role = index.positions(IndexedTag::PlayerSetRole).any(|pos| {
                pos < start
                    && pos < end
                    && read_expected_u32(data, pos + 4, &HASH_ASLOT) == Some(player.id)
                    && pos < session.start_offset
            });
            let explicit_inactive_session = player.role.is_none()
                && identities
                    .intervals
                    .iter()
                    .filter(|s| {
                        s.player_id == player.id
                            && s.start_offset < end
                            && s.end_offset.is_none_or(|stop| stop > start)
                    })
                    .count()
                    == 1
                && index.positions(IndexedTag::PlayerEntersGame).any(|pos| {
                    pos >= session.start_offset
                        && pos < stop
                        && parse_player_entry(data, pos).is_some_and(|(_, slot, name)| {
                            slot == player.id && name.as_deref() == Some(&player.name)
                        })
                })
                && !nonzero_slots.contains(&player.id)
                && !unsuperseded_role;
            if spectator
                && (explicit_inactive_session
                    || (!lobby_role_needs_spectator
                        && !other_lobby_role
                        && !(had_lobby_role && joined_during_match)))
            {
                player.team = Some(0);
                player.faction = Some("Spectator".to_string());
            }
        }
    }

    /// Recover only a missing score before departure, supported by a single named
    /// gameplay session. Do not change roles, ordinary results, teams, or identities.
    pub fn player_result_evidence(&self, players: &[Player]) -> Vec<PlayerResultEvidence> {
        let candidates: Vec<_> = players
            .iter()
            .filter(|p| {
                p.has_zero_stats()
                    && p.role.is_none()
                    && p.team.is_some_and(|t| (1..=3).contains(&t))
            })
            .collect();
        if candidates.is_empty() || !self.captured_match_start() {
            return Vec::new();
        }
        let Some(start) = self.extract_clock_timeline().0.first().map(|s| s.offset) else {
            return Vec::new();
        };
        let timeline = self.envelope_timeline();
        let index = EventIndex {
            offsets: self
                .event_index
                .offsets()
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .offset
                        .checked_sub(ENVELOPE_MESSAGE_OFFSET)
                        .is_some_and(|offset| timeline.starts.binary_search(&offset).is_ok())
                })
                .collect(),
        };
        if !index
            .positions(IndexedTag::SetGameModeDataFloat)
            .any(|pos| pos == start)
        {
            return Vec::new();
        }
        let Some(end) = index
            .positions(IndexedTag::TeamWins)
            .find(|pos| *pos > start)
        else {
            return Vec::new();
        };
        let data = &self.full_data;
        let identities = PlayerIdentityIntervals::build(
            data,
            &index,
            &self.extract_resolved_slot_names(&HashSet::new()),
        );
        // SetScore fields: unsigned aPos, signed aPlayerScore. Use complete
        // primary-chain messages, not loose field matches or slot-wide maxima.
        let scores: Vec<_> = self
            .event_index
            .positions(IndexedTag::APos)
            .filter_map(|field| {
                let pos = field.checked_sub(4)?;
                if data.get(pos..pos + 4)? != HASH_SET_SCORE {
                    return None;
                }
                let envelope = event_envelope(data, pos)?;
                if timeline.starts.binary_search(&envelope.start).is_err()
                    || pos < start
                    || pos >= end
                    || pos + 38 > envelope.end
                    || data[pos + 12] != 1
                    || data[pos + 29] != 0
                {
                    return None;
                }
                Some((
                    pos,
                    envelope.raw_time_seconds,
                    read_expected_u32(data, pos + 4, &HASH_APOS)?,
                    read_expected_i32(data, pos + 21, &HASH_SCORE)?,
                ))
            })
            .collect();
        let mut evidence = Vec::new();
        for player in candidates {
            let sessions: Vec<_> = identities
                .intervals
                .iter()
                .filter(|s| {
                    s.player_id == player.id
                        && s.start_offset < end
                        && s.end_offset.is_none_or(|stop| stop > start)
                })
                .collect();
            let matching: Vec<_> = sessions
                .iter()
                .copied()
                .filter(|s| s.player_name.as_deref() == Some(&player.name))
                .collect();
            if matching.len() != 1 || sessions.iter().any(|s| s.player_name.is_none()) {
                continue;
            }
            let session = matching[0];
            let stop = session.end_offset.unwrap_or(end).min(end);
            let entered = index.positions(IndexedTag::PlayerEntersGame).any(|pos| {
                pos >= session.start_offset
                    && pos < stop
                    && parse_player_entry(data, pos).is_some_and(|(_, slot, name)| {
                        slot == player.id && name.as_deref() == Some(&player.name)
                    })
            });
            if !entered {
                continue;
            }
            let units: Vec<_> = index
                .positions(IndexedTag::UnitCreate)
                .filter(|pos| {
                    *pos >= start
                        && *pos < end
                        && read_expected_u32(data, *pos + 21, &HASH_APLAYER) == Some(player.id)
                })
                .collect();
            if units.is_empty() {
                continue;
            }
            if units.iter().any(|pos| {
                *pos < session.start_offset
                    || *pos >= stop
                    || read_expected_u32(data, *pos + 38, &HASH_ATEAM) != player.team
            }) {
                continue;
            }
            // Any role activity by another gameplay occupant makes attribution
            // ambiguous, even if that occupant's final counters are also zero.
            if index.positions(IndexedTag::PlayerSetRole).any(|pos| {
                pos >= start
                    && pos < end
                    && (pos < session.start_offset || pos >= stop)
                    && read_expected_u32(data, pos + 4, &HASH_ASLOT) == Some(player.id)
            }) {
                continue;
            }
            let mut score_before_leave = None;
            if let Some(left) = session.end_offset.filter(|left| *left < end) {
                let departure = index
                    .positions(IndexedTag::PlayerLeavesGame)
                    .filter(|pos| {
                        *pos >= start
                            && *pos < left
                            && read_expected_u32(data, *pos + 4, &HASH_ASLOT) == Some(player.id)
                    })
                    .find_map(|pos| {
                        event_envelope(data, pos)
                            .filter(|e| e.end == left)
                            .map(|e| (pos, e.raw_time_seconds))
                    });
                if let Some((leave_pos, leave_time)) = departure {
                    let own: Vec<_> = scores
                        .iter()
                        .filter(|(pos, _, id, _)| {
                            *id == player.id && *pos >= session.start_offset && *pos < leave_pos
                        })
                        .collect();
                    let next_entry = index
                        .positions(IndexedTag::PlayerEntersGame)
                        .find(|pos| {
                            *pos >= left
                                && *pos < end
                                && read_expected_u32(data, *pos + 4, &HASH_ASLOT) == Some(player.id)
                        })
                        .unwrap_or(end);
                    let reset = scores.iter().any(|(pos, _, id, score)| {
                        *id == player.id && *pos >= left && *pos < next_entry && *score == 0
                    });
                    let other_score = scores.iter().any(|(pos, _, id, score)| {
                        *id == player.id
                            && *score != 0
                            && (*pos < session.start_offset || *pos >= leave_pos)
                    });
                    if reset
                        && !other_score
                        && let Some((_, observed, _, score)) = own.last()
                        && *score > 0
                    {
                        score_before_leave = Some(ScoreBeforeLeave {
                            score: *score,
                            observed_at_seconds: *observed,
                            left_at_seconds: leave_time,
                        });
                    }
                }
            }
            if score_before_leave.is_some() {
                evidence.push(PlayerResultEvidence {
                    player_id: player.id,
                    score_before_leave,
                });
            }
        }
        evidence.sort_by_key(|e| e.player_id);
        evidence
    }

    /// Proven abandoned lobby duplicates for viewer roster presentation only.
    /// This does not remove or rewrite any row returned by `parse()`.
    pub fn abandoned_lobby_duplicate_slots(&self, players: &[Player]) -> Vec<u32> {
        let candidates: Vec<_> = players
            .iter()
            .filter(|p| {
                p.has_zero_stats()
                    && p.role.is_none()
                    && p.team.is_some_and(|t| (1..=3).contains(&t))
                    && players.iter().any(|q| {
                        q.id != p.id
                            && q.name == p.name
                            && q.team == p.team
                            && q.score.is_some_and(|score| score > 0)
                            && q.role.is_some()
                    })
            })
            .collect();
        if candidates.is_empty() || !self.captured_match_start() {
            return Vec::new();
        }
        let Some(start) = self.extract_clock_timeline().0.first().map(|s| s.offset) else {
            return Vec::new();
        };
        let timeline = self.envelope_timeline();
        let index = EventIndex {
            offsets: self
                .event_index
                .offsets()
                .iter()
                .copied()
                .filter(|entry| {
                    entry
                        .offset
                        .checked_sub(ENVELOPE_MESSAGE_OFFSET)
                        .is_some_and(|offset| timeline.starts.binary_search(&offset).is_ok())
                })
                .collect(),
        };
        if !index
            .positions(IndexedTag::SetGameModeDataFloat)
            .any(|pos| pos == start)
        {
            return Vec::new();
        }
        let Some(end) = index
            .positions(IndexedTag::TeamWins)
            .find(|pos| *pos > start)
        else {
            return Vec::new();
        };
        let data = &self.full_data;
        let identities = PlayerIdentityIntervals::build(
            data,
            &index,
            &self.extract_resolved_slot_names(&HashSet::new()),
        );
        let mut hidden = Vec::new();
        for player in candidates {
            let sessions: Vec<_> = identities
                .intervals
                .iter()
                .filter(|s| s.player_id == player.id)
                .collect();
            if sessions.is_empty()
                || sessions.iter().any(|s| {
                    s.player_name.as_deref() != Some(&player.name)
                        || !s.end_offset.is_some_and(|stop| stop < start)
                })
            {
                continue;
            }
            // Require actual framed entry and departure; inferred absence is insufficient.
            let entered = index.positions(IndexedTag::PlayerEntersGame).any(|pos| {
                parse_player_entry(data, pos).is_some_and(|(_, slot, name)| {
                    slot == player.id && name.as_deref() == Some(&player.name)
                })
            });
            let departed = index.positions(IndexedTag::PlayerLeavesGame).any(|pos| {
                pos < start
                    && read_expected_u32(data, pos + 4, &HASH_ASLOT) == Some(player.id)
                    && event_envelope(data, pos)
                        .is_some_and(|e| Some(e.end) == sessions.last().unwrap().end_offset)
            });
            let activity = index.offsets().iter().any(|entry| match entry.tag {
                IndexedTag::PlayerSetRole => {
                    read_expected_u32(data, entry.offset + 4, &HASH_ASLOT) == Some(player.id)
                }
                IndexedTag::UnitCreate => {
                    read_expected_u32(data, entry.offset + 21, &HASH_APLAYER) == Some(player.id)
                }
                _ => false,
            });
            if !entered || !departed || activity {
                continue;
            }
            let mut matches = players.iter().filter(|q| {
                q.id != player.id
                    && q.name == player.name
                    && q.team == player.team
                    && q.score.is_some_and(|score| score > 0)
                    && q.role.is_some()
            });
            let Some(active) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                continue;
            }
            let active_sessions: Vec<_> = identities
                .intervals
                .iter()
                .filter(|s| {
                    s.player_id == active.id
                        && s.start_offset < end
                        && s.end_offset.is_none_or(|stop| stop > start)
                })
                .collect();
            if active_sessions.len() != 1 {
                continue;
            }
            let session = active_sessions[0];
            if session.start_offset > start
                || session.player_name.as_deref() != Some(&active.name)
                || session.end_offset.is_some_and(|stop| stop < end)
            {
                continue;
            }
            let active_entry = index.positions(IndexedTag::PlayerEntersGame).any(|pos| {
                pos < start
                    && parse_player_entry(data, pos).is_some_and(|(_, slot, name)| {
                        slot == active.id && name.as_deref() == Some(&active.name)
                    })
            });
            let active_units = index.positions(IndexedTag::UnitCreate).any(|pos| {
                pos >= start
                    && pos < end
                    && read_expected_u32(data, pos + 21, &HASH_APLAYER) == Some(active.id)
            });
            if active_entry && active_units {
                hidden.push(player.id);
            }
        }
        hidden.sort_unstable();
        hidden
    }

    /// Complete primary-chain SetScore messages identify their own slot. Stop at
    /// the result so later lobby resets cannot replace match scores.
    fn explicit_match_scores(&self) -> Vec<(usize, u32, i32)> {
        let timeline = self.envelope_timeline();
        let mut scores = Vec::new();
        for &offset in &timeline.starts {
            let pos = offset + ENVELOPE_MESSAGE_OFFSET;
            let data = &self.full_data;
            if data.get(pos..pos + 4) == Some(&HASH_TEAM_WINS) {
                return scores;
            }
            if data.get(pos..pos + 4) != Some(&HASH_SET_SCORE) {
                continue;
            }
            let Some(envelope) = event_envelope(data, pos) else {
                continue;
            };
            if pos + 38 <= envelope.end
                && data[pos + 12] == 1
                && data[pos + 29] == 0
                && let Some(slot) = read_expected_u32(data, pos + 4, &HASH_APOS).filter(|s| *s < 16)
                && let Some(score) = read_expected_i32(data, pos + 21, &HASH_SCORE)
            {
                scores.push((pos, slot, score));
            }
        }
        // Incomplete primary chains must retain the established summary.
        Vec::new()
    }

    fn extract_scores_by_hash(&self) -> HashMap<u32, i32> {
        let data = &self.full_data;
        let mut entries: Vec<(usize, u32, i32)> = Vec::new();

        for pos in self.event_index.positions(IndexedTag::Score) {
            if pos + 55 <= data.len()
                && data[pos + 4..pos + 8] == BINTAG_SEP
                && let Some(score) = read_i32_le(data, pos + 13)
                && data[pos + 47..pos + 51] == BINTAG_SEP
                && let Some(counter) = read_u32_le(data, pos + 51)
                && counter <= 15
            {
                entries.push((pos, counter, score));
            }
        }

        if entries.is_empty() {
            return HashMap::new();
        }

        // Find last contiguous run (entries exactly 55 bytes apart)
        let mut run_start = entries.len() - 1;
        for i in (1..entries.len()).rev() {
            if entries[i].0 - entries[i - 1].0 == 55 {
                run_start = i - 1;
            } else {
                break;
            }
        }

        let run = &entries[run_start..];
        if run.len() >= 16 {
            let final_block = &run[run.len() - 16..];
            let block_counters: HashSet<u32> = final_block[..15].iter().map(|e| e.1).collect();
            if block_counters.contains(&final_block[15].1) {
                entries.pop();
            }
        }

        let mut counter_scores: HashMap<u32, i32> = HashMap::new();
        for &(_, counter, score) in &entries {
            counter_scores.insert(counter, score);
        }

        counter_scores
    }

    fn extract_team_assignments(&self) -> HashMap<u32, u32> {
        let data = &self.full_data;
        let mut slot_teams: HashMap<u32, u32> = HashMap::new();

        for (event_tag, is_spectator_event) in [
            (IndexedTag::PlayerJoinedTeam, false),
            (IndexedTag::SpectatorJoinedTeam, true),
        ] {
            for pos in self.event_index.positions(event_tag) {
                if pos + 200 > data.len() {
                    continue;
                }
                let block = &data[pos..pos + 200];
                let slot_off = find_pattern(block, &HASH_ASLOT, 0);
                let team_off = find_pattern(block, &HASH_ATEAM, 0);
                if let (Some(so), Some(to)) = (slot_off, team_off)
                    && let (Some(sv), Some(tv)) = (
                        read_bintag_u32(data, pos + so),
                        read_bintag_u32(data, pos + to),
                    )
                    && sv <= 16
                {
                    // SpectatorJoinedTeam aTeam carries the team being *left*, not joined.
                    // Always treat as team=0 to avoid poisoning real assignments.
                    let effective_team = if is_spectator_event { 0 } else { tv };
                    if effective_team != 0 {
                        slot_teams.insert(sv, effective_team);
                    } else {
                        slot_teams.entry(sv).or_insert(0);
                    }
                }
            }
        }

        let assigned: HashSet<u32> = slot_teams
            .iter()
            .filter(|&(_, &t)| t != 0)
            .map(|(&s, _)| s)
            .collect();

        if assigned.len() < 16 {
            let mut votes: HashMap<u32, HashMap<u32, u32>> = HashMap::new();
            for pos in self.event_index.positions(IndexedTag::UnitCreate) {
                if pos + 300 > data.len() {
                    continue;
                }
                let block = &data[pos..pos + 300];
                let player_off = find_pattern(block, &HASH_APLAYER, 0);
                let team_off = find_pattern(block, &HASH_ATEAM, 0);
                if let (Some(po), Some(to)) = (player_off, team_off)
                    && let (Some(pv), Some(tv)) = (
                        read_bintag_u32(data, pos + po),
                        read_bintag_u32(data, pos + to),
                    )
                    && pv <= 16
                    && tv != 0
                    && !assigned.contains(&pv)
                {
                    *votes.entry(pv).or_default().entry(tv).or_insert(0) += 1;
                }
            }

            for (player_id, team_votes) in &votes {
                if (!slot_teams.contains_key(player_id) || slot_teams[player_id] == 0)
                    && let Some((&best_team, _)) = team_votes.iter().max_by_key(|&(_, &v)| v)
                {
                    slot_teams.insert(*player_id, best_team);
                }
            }
        }

        slot_teams
    }

    /// Decode the contiguous score table before the first framed result, even
    /// when damaged earlier data prevents primary-chain traversal. These are
    /// slot statistics only: they do not establish names or final membership.
    fn final_screen_score_table(&self) -> Option<(usize, HashMap<usize, [i32; 14]>)> {
        let data = &self.full_data;
        let result = self
            .event_index
            .positions(IndexedTag::TeamWins)
            .find_map(|pos| event_envelope(data, pos))?;
        let mut cursor = result.start;
        let mut rows = HashMap::new();
        let mut slots = HashSet::new();
        while let Some(start) = cursor.checked_sub(259) {
            let pos = start + ENVELOPE_MESSAGE_OFFSET;
            if data.get(pos..pos + 4)? != [0x6f, 0x06, 0x31, 0x3a] {
                break;
            }
            let envelope = event_envelope(data, pos)?;
            if envelope.end != cursor {
                return None;
            }
            let fields = [
                HASH_APOS,
                HASH_AROLE_ID,
                HASH_TOTAL_SCORE,
                HASH_CAPTURING_SCORE,
                HASH_FORTIFICATION_SCORE,
                HASH_TRANSPORTATION_SCORE,
                HASH_REPAIR_SCORE,
                HASH_BRIDGE_LAYING_SCORE,
                HASH_UNIT_DAMAGE_SCORE,
                HASH_TACTICAL_AID_SCORE,
                HASH_SCORE_ROLE0,
                HASH_SCORE_ROLE1,
                HASH_SCORE_ROLE2,
                HASH_SCORE_ROLE3,
            ];
            if envelope.end != pos + 4 + fields.len() * 17 {
                return None;
            }
            let mut values = [0i32; 14];
            for (i, hash) in fields.iter().enumerate() {
                let field_pos = pos + 4 + i * 17;
                let flag = if i == 1 || i == 2 { 0 } else { 1 };
                let (bytes, _) = read_event_field(data, field_pos, envelope.end, hash, flag)?;
                values[i] = i32::from_le_bytes(bytes.try_into().ok()?);
            }
            let slot = u32::try_from(values[0]).ok().filter(|s| *s < 16)?;
            if !slots.insert(slot) {
                return None;
            }
            rows.insert(start, values);
            cursor = start;
        }
        if self
            .decompression_gaps
            .iter()
            .any(|gap| cursor < *gap && *gap < result.end)
        {
            return None;
        }
        (!rows.is_empty()).then_some((result.start, rows))
    }

    /// Replay end-screen rows from a complete primary SetScoreAtGameEnd table.
    /// Unlike the historical roster, this uses the named occupant still present
    /// at the result and the screen's own total, including zero and signed totals.
    /// Missing or ambiguous source data leaves the existing result path available.
    pub fn final_screen_players(&self) -> Option<Vec<Player>> {
        let _work = acquire_replay_work();
        #[derive(Clone)]
        struct Occupant {
            name: String,
            team: u32,
            kind: u32,
            spectator: bool,
            active: bool,
        }
        let data = &self.full_data;
        let (result_start, score_table) = self.final_screen_score_table()?;
        if self
            .decompression_gaps
            .iter()
            .any(|gap| *gap <= result_start)
        {
            return None;
        }
        let mut occupants: HashMap<u32, Option<Occupant>> = HashMap::new();
        let mut rows = HashMap::new();
        let mut summary_started = false;
        for &start in &self.envelope_timeline().starts {
            let pos = start + ENVELOPE_MESSAGE_OFFSET;
            let envelope = event_envelope(data, pos)?;
            let tag = data.get(pos..pos + 4)?;
            if tag == HASH_TEAM_WINS {
                if start != result_start
                    || rows.is_empty()
                    || occupants.iter().any(|(slot, state)| {
                        state
                            .as_ref()
                            .is_none_or(|s| s.active && s.kind != 2 && !rows.contains_key(slot))
                    })
                {
                    return None;
                }
                let mut players: Vec<Player> = rows.into_values().collect();
                players.sort_by_key(|p| p.id);
                // An unexplained overflow means this is not a complete two-side
                // snapshot. Keep the fallback rather than silently guessing rows.
                if (1..=3).any(|team| players.iter().filter(|p| p.team == Some(team)).count() > 8) {
                    return None;
                }
                return Some(players);
            }
            if tag == [0x6f, 0x06, 0x31, 0x3a] {
                // SetScoreAtGameEnd
                summary_started = true;
                let values = score_table.get(&start)?;
                let slot = values[0] as u32;
                let state = occupants.get(&slot)?.as_ref()?;
                if !state.active || rows.contains_key(&slot) {
                    return None;
                }
                if state.kind == 2 {
                    continue;
                }
                let team = if state.spectator { 0 } else { state.team };
                let role_scores = &values[10..14];
                let best = role_scores.iter().copied().filter(|v| *v != 0).max();
                let role = best
                    .and_then(|v| role_scores.iter().position(|s| *s == v))
                    .map(|i| ["infantry", "support", "armor", "air"][i].to_string())
                    .or_else(|| {
                        (values[2] > 0)
                            .then(|| role_name(values[1] as u32))
                            .flatten()
                            .filter(|r| *r != "fewPlayer")
                            .map(str::to_string)
                    });
                rows.insert(
                    slot,
                    Player {
                        id: slot,
                        name: state.name.clone(),
                        team: Some(team),
                        faction: Some(faction_name(team).into()),
                        score: Some(values[2]),
                        role,
                        score_total: Some(values[2]),
                        score_capturing: Some(values[3]),
                        score_fortification: Some(values[4]),
                        score_transportation: Some(values[5]),
                        score_repair: Some(values[6]),
                        score_bridge_laying: Some(values[7]),
                        score_unit_damage: Some(values[8]),
                        score_tactical_aid: Some(values[9]),
                        score_infantry: Some(values[10]),
                        score_support: Some(values[11]),
                        score_armor: Some(values[12]),
                        score_air: Some(values[13]),
                        left_at_seconds: None,
                    },
                );
            } else if tag == HASH_PLAYER_ENTERS_GAME {
                if summary_started {
                    return None;
                }
                let (_, slot, name) = parse_player_entry(data, pos)?;
                let state = (|| {
                    let name = name?;
                    let team = read_expected_u32(data, pos + 21, &[0xa8, 0x01, 0x32, 0x04])?;
                    if team > 3 {
                        return None;
                    }
                    let (_, name_end) =
                        read_event_field(data, pos + 38, envelope.end, &HASH_PLAYER_ENTRY_NAME, 5)?;
                    let (kind, _) =
                        read_event_field(data, name_end + 17, envelope.end, &HASH_MY_TYPE, 1)?;
                    let kind = u32::from_le_bytes(kind.try_into().ok()?);
                    Some(Occupant {
                        name,
                        team,
                        kind,
                        spectator: false,
                        active: true,
                    })
                })();
                occupants.insert(slot, state);
            } else if tag == HASH_PLAYER_LEAVES_GAME
                || tag == HASH_PLAYER_JOINED_TEAM
                || tag == HASH_SPECTATOR_JOINED_TEAM
            {
                if summary_started {
                    return None;
                }
                let slot = read_expected_u32(data, pos + 4, &HASH_ASLOT)?;
                if slot > 15 {
                    continue;
                }
                if tag == HASH_PLAYER_LEAVES_GAME {
                    occupants.remove(&slot);
                } else {
                    let Some(state) = occupants.entry(slot).or_insert(None).as_mut() else {
                        continue;
                    };
                    state.team = read_expected_u32(data, pos + 21, &HASH_ATEAM)?;
                    if state.team > 3 {
                        return None;
                    }
                    state.spectator = tag == HASH_SPECTATOR_JOINED_TEAM;
                }
            }
        }
        None
    }

    fn extract_player_end_summaries(&self) -> HashMap<u32, PlayerEndSummary> {
        let data = &self.full_data;
        let mut result = HashMap::new();

        for pos in self.event_index.positions(IndexedTag::APos) {
            if pos + 300 > data.len() {
                continue;
            }
            let Some(player_idx) = read_bintag_u32(data, pos) else {
                continue;
            };
            if player_idx > 15 {
                continue;
            }

            let block = &data[pos..pos + 300];

            let read_score = |hash: &[u8; 4]| {
                find_pattern(block, hash, 0)
                    .and_then(|off| read_bintag_u32(data, pos + off))
                    .map(|value| value as i32)
                    .unwrap_or(0)
            };
            let sr0 = read_score(&HASH_SCORE_ROLE0);
            let sr1 = read_score(&HASH_SCORE_ROLE1);
            let sr2 = read_score(&HASH_SCORE_ROLE2);
            let sr3 = read_score(&HASH_SCORE_ROLE3);

            // Primary role = highest score role (ties: infantry > support > armor > air)
            // Zero alone does not show that a role was played. Rank recorded
            // nonzero role scores, including penalties, by their signed value.
            let max_score = [sr0, sr1, sr2, sr3]
                .into_iter()
                .filter(|score| *score != 0)
                .max()
                .unwrap_or(0);
            let role = if max_score == 0 {
                String::new()
            } else if sr0 == max_score {
                "infantry".to_string()
            } else if sr1 == max_score {
                "support".to_string()
            } else if sr2 == max_score {
                "armor".to_string()
            } else {
                "air".to_string()
            };

            result.insert(
                player_idx,
                PlayerEndSummary {
                    role,
                    recorded_role: read_expected_u32(data, pos + 17, &HASH_AROLE_ID)
                        .and_then(role_name)
                        .filter(|role| *role != "fewPlayer")
                        .map(str::to_string),
                    infantry: sr0,
                    support: sr1,
                    armor: sr2,
                    air: sr3,
                    capturing: read_score(&HASH_CAPTURING_SCORE),
                    fortification: read_score(&HASH_FORTIFICATION_SCORE),
                    transportation: read_score(&HASH_TRANSPORTATION_SCORE),
                    repair: read_score(&HASH_REPAIR_SCORE),
                    bridge_laying: read_score(&HASH_BRIDGE_LAYING_SCORE),
                    unit_damage: read_score(&HASH_UNIT_DAMAGE_SCORE),
                    tactical_aid: read_score(&HASH_TACTICAL_AID_SCORE),
                    total: read_score(&HASH_TOTAL_SCORE),
                },
            );
        }

        result
    }

    fn extract_winner(&self) -> Option<u32> {
        let pos = self
            .event_index
            .positions(IndexedTag::TeamWins)
            .next_back()?;
        if pos + 50 > self.full_data.len() {
            return None;
        }
        let block = &self.full_data[pos..pos + 50];
        let team_off = find_pattern(block, &HASH_ATEAM, 0)?;
        let team = read_bintag_u32(&self.full_data, pos + team_off)?;
        // Team 0 = Spectator/invalid — no real winner (map vote, corrupt data, etc.)
        if team == 0 { None } else { Some(team) }
    }

    fn extract_recorder_slot(&self) -> Option<u32> {
        self.recorder_slot
    }

    /// Last well-formed `aFactor` sample and the offset it was read from.
    ///
    /// Scans backwards rather than reading only the final byte match, so one
    /// malformed or out-of-range record at the tail cannot suppress the result.
    fn extract_domination_sample(&self) -> Option<(usize, f64)> {
        self.event_index
            .positions(IndexedTag::AFactor)
            .rev()
            .find_map(|pos| {
                let value = read_bintag_float(&self.full_data, pos)? as f64;
                (0.0..=1.0).contains(&value).then_some((pos, value))
            })
    }

    /// Team the POV player belonged to at `offset`.
    ///
    /// `aFactor` is reported from the recording player's perspective and is
    /// mirrored when that player changes side, so the anchor must be resolved at
    /// the sample being read rather than once for the whole replay.
    ///
    /// Returns `None` for a spectator recorder, where the bar has no roster team
    /// to anchor to.
    fn pov_team_at(&self, offset: usize) -> Option<u32> {
        let slot = self.recorder_slot?;
        let data = &self.full_data;
        self.event_index
            .offsets()
            .iter()
            .take_while(|entry| entry.offset < offset)
            .filter_map(|entry| {
                let is_spectator = match entry.tag {
                    IndexedTag::PlayerJoinedTeam => false,
                    IndexedTag::SpectatorJoinedTeam => true,
                    _ => return None,
                };
                let pos = entry.offset;
                // Clamp rather than skip: a join event close to the end of the
                // stream still carries its slot and team fields.
                let block = &data[pos..data.len().min(pos + 200)];
                let slot_off = find_pattern(block, &HASH_ASLOT, 0)?;
                let team_off = find_pattern(block, &HASH_ATEAM, 0)?;
                if read_bintag_u32(data, pos + slot_off)? != slot {
                    return None;
                }
                // SpectatorJoinedTeam carries the team being left, not joined.
                let team = if is_spectator {
                    0
                } else {
                    read_bintag_u32(data, pos + team_off)?
                };
                Some(team)
            })
            .last()
            .filter(|team| *team != 0)
    }

    /// Envelope time base, cached across the match and timeline parsers.
    fn envelope_timeline(&self) -> &EnvelopeTimeline {
        self.envelope_timeline
            .get_or_init(|| build_envelope_timeline(&self.full_data))
    }

    fn extract_clock_timeline(&self) -> &(Vec<ClockSample>, Vec<TimelinePhase>) {
        self.clock_timeline
            .get_or_init(|| self.build_clock_timeline_from_index())
    }

    fn build_clock_timeline_from_index(&self) -> (Vec<ClockSample>, Vec<TimelinePhase>) {
        let mut raw_samples = Vec::new();
        let mut countdown_started = false;
        let mut saw_pre_match = false;
        for pos in self.event_index.positions(IndexedTag::SetGameModeDataFloat) {
            let Some(data_type) = read_expected_u32(&self.full_data, pos + 4, &HASH_ADATA_TYPE)
            else {
                continue;
            };
            let Some(remaining) = read_expected_float(&self.full_data, pos + 21, &HASH_AFLOAT)
            else {
                continue;
            };
            if data_type != 1 || !remaining.is_finite() || remaining.abs() > MAX_CLOCK_SECONDS {
                continue;
            }

            if !countdown_started && remaining == 0.0 {
                // The countdown reads zero only before it starts. Seeing it means
                // the recording was already running when the match began.
                saw_pre_match = true;
            }
            countdown_started |= remaining != 0.0;
            if countdown_started {
                raw_samples.push((pos, remaining));
            }
        }

        let _ = self.clock_captured_start.set(saw_pre_match);
        build_clock_timeline(&raw_samples)
    }

    /// Whether the recording observed the pre-match countdown.
    fn captured_match_start(&self) -> bool {
        // Populated as a side effect of building the timeline.
        self.extract_clock_timeline();
        *self.clock_captured_start.get().unwrap_or(&false)
    }

    /// Match timing, separating the recorded span from true match time.
    fn extract_match_timing(&self) -> MatchTiming {
        let captured_match_start = self.captured_match_start();
        let (samples, phases) = self.extract_clock_timeline();
        let observed_gameplay_seconds = self.extract_game_duration();
        let recording_seconds = round_timeline_seconds(self.envelope_timeline().seconds);
        let Some(first) = samples.first() else {
            return MatchTiming {
                captured_match_start,
                recording_seconds,
                observed_gameplay_seconds,
                match_elapsed_seconds: observed_gameplay_seconds,
                round_length_seconds: None,
                round_length_exact: false,
                joined_at_remaining_seconds: None,
                final_remaining_seconds: None,
            };
        };

        let final_remaining = samples.last().map(|sample| sample.remaining_seconds);
        // Modes that run several timed rounds (Assault) restart the countdown, so
        // the first phase's opening value is the round length, not the match length.
        let observed_round_max = phases
            .first()
            .map(|phase| phase.initial_clock_seconds)
            .unwrap_or(first.remaining_seconds);

        // With the start captured, that opening value is the configured round
        // length. Without it, it is only a lower bound, so fall back to the
        // shortest standard round that could contain it.
        let (round_length_seconds, round_length_exact) = if captured_match_start {
            (Some(round_timeline_seconds(observed_round_max)), true)
        } else {
            let inferred = STANDARD_ROUND_LENGTHS
                .iter()
                .copied()
                .find(|length| *length >= observed_round_max - 1.0)
                .unwrap_or(observed_round_max);
            (Some(inferred), false)
        };

        // The late-join correction assumes one countdown. With several rounds the
        // missing span cannot be recovered from the final round's clock alone.
        let recoverable = !captured_match_start && phases.len() == 1;
        let match_elapsed_seconds = if recoverable {
            match (round_length_seconds, final_remaining) {
                // The countdown keeps ticking past zero into the post-match
                // screen, so clamp before subtracting or overtime inflates the
                // match length beyond the round.
                (Some(length), Some(remaining)) => Some(round_timeline_seconds(
                    (length - remaining.max(0.0)).clamp(0.0, length),
                )),
                _ => observed_gameplay_seconds,
            }
        } else {
            observed_gameplay_seconds
        };

        MatchTiming {
            captured_match_start,
            recording_seconds,
            observed_gameplay_seconds,
            match_elapsed_seconds,
            round_length_seconds,
            round_length_exact,
            joined_at_remaining_seconds: (!captured_match_start)
                .then(|| round_timeline_seconds(first.remaining_seconds)),
            final_remaining_seconds: final_remaining.map(round_timeline_seconds),
        }
    }

    /// Attribute the final bar split to concrete factions.
    ///
    /// Prefers the POV anchor, which is independent of the recorded outcome.
    /// Falls back to assigning the larger share to the `TeamWins` winner, which
    /// is sound because the winning side of the bar decides a Domination match —
    /// verified against the POV anchor on 136/136 corpus replays that ended by
    /// total domination or on the timer.
    fn resolve_domination_shares(
        &self,
        bar: Option<(usize, f64)>,
        players: &[Player],
        winner: Option<&str>,
    ) -> (Option<Vec<DominationShare>>, Option<DominationAnchor>) {
        let Some((offset, raw)) = bar else {
            return (None, None);
        };

        let mut factions: Vec<String> = players
            .iter()
            .filter_map(|player| player.faction.clone())
            .filter(|faction| faction != "Spectator" && faction != "Unknown")
            .collect();
        factions.sort();
        factions.dedup();
        if factions.len() != 2 {
            return (None, None);
        }

        let share = |faction: &str, pct: f64| DominationShare {
            faction: faction.to_owned(),
            pct,
        };

        if let Some(pov_team) = self.pov_team_at(offset) {
            let pov_faction = faction_name(pov_team);
            if let Some(other) = factions.iter().find(|faction| *faction != pov_faction) {
                return (
                    Some(vec![share(pov_faction, raw), share(other, 1.0 - raw)]),
                    Some(DominationAnchor::PovTeam),
                );
            }
        }

        match winner.and_then(|name| factions.iter().find(|faction| *faction == name)) {
            Some(winning) => {
                let losing = factions
                    .iter()
                    .find(|faction| *faction != winning)
                    .expect("two distinct factions");
                (
                    Some(vec![
                        share(winning, raw.max(1.0 - raw)),
                        share(losing, raw.min(1.0 - raw)),
                    ]),
                    Some(DominationAnchor::WinnerInferred),
                )
            }
            None => (None, None),
        }
    }

    /// Classify how the match ended from the clock and the final bar position.
    fn classify_match_ending(
        &self,
        timing: &MatchTiming,
        winner: Option<u32>,
        bar: Option<f64>,
    ) -> MatchEnding {
        if winner.is_none() {
            return MatchEnding::Unknown;
        }
        let Some(remaining) = timing.final_remaining_seconds else {
            return MatchEnding::Unknown;
        };
        // Checked first: a pinned bar is unambiguous, and across the corpus it
        // never coincides with an expired clock.
        if let Some(value) = bar
            && (value <= DOMINATION_EXTREME_EPSILON || value >= 1.0 - DOMINATION_EXTREME_EPSILON)
        {
            return MatchEnding::TotalDomination;
        }
        if remaining <= TIMEOUT_REMAINING_SECONDS {
            MatchEnding::Timeout
        } else {
            MatchEnding::Forfeit
        }
    }

    fn extract_legacy_game_duration(&self) -> Option<f32> {
        let data = &self.full_data;
        if data.len() < 100 {
            return None;
        }
        let search_start = (data.len() as f64 * 0.99) as usize;
        let mut time_values: Vec<f32> = Vec::new();

        let mut offset = search_start;
        while offset + 4 <= data.len() {
            if let Some(val) = read_f32_le(data, offset)
                && (300.0..=1320.0).contains(&val)
            {
                time_values.push(val);
            }
            offset += 4;
        }

        if time_values.is_empty() {
            return None;
        }

        let high_values: Vec<f32> = time_values
            .iter()
            .copied()
            .filter(|&t| t >= 1200.0)
            .collect();
        if !high_values.is_empty() {
            let mut counts: HashMap<i32, u32> = HashMap::new();
            for &t in &high_values {
                *counts.entry((t * 10.0).round() as i32).or_insert(0) += 1;
            }
            let (&best_key, _) = counts.iter().max_by_key(|&(_, &v)| v)?;
            return Some(best_key as f32 / 10.0);
        }

        time_values.iter().copied().reduce(f32::max)
    }

    /// Gameplay observed in the recording, falling back to the legacy end-of-file
    /// scan when a replay carries no countdown. Use `extract_match_timing` for
    /// true match time, which differs when the recorder joined mid-match.
    fn extract_game_duration(&self) -> Option<f32> {
        let (samples, _) = self.extract_clock_timeline();
        samples
            .last()
            .map(|sample| sample.elapsed_seconds)
            .filter(|duration| *duration > 0.0)
            .map(round_timeline_seconds)
            .or_else(|| self.extract_legacy_game_duration())
    }

    /// Extract a compact, replay-relative timeline without changing `parse()`.
    ///
    /// Every `time_seconds` is recording-elapsed time, read from the `Event`
    /// envelope the record sits in. Zero is the first envelope in the file, so
    /// the axis covers the pre-match lobby and the post-`TeamWins` tail and
    /// runs to `TimelineData::duration_seconds`.
    ///
    /// Schema versions through 11 instead interpolated event positions between
    /// countdown samples, which pinned zero to the first countdown record and
    /// collapsed all lobby activity onto it. The countdown still supplies phase
    /// boundaries and clock readings, which is all it is sound for: it starts
    /// late, restarts between Assault rounds, and does not tick at real time.
    ///
    /// UnitCreate records are retained only as internal attribution state and
    /// are not emitted.
    pub fn parse_timeline(&self) -> TimelineData {
        let _work = acquire_replay_work();
        let data = &self.full_data;
        let (clock_samples, phase_template) = self.extract_clock_timeline();
        let mut phases = phase_template.clone();

        let gameplay_start_offset = clock_samples
            .first()
            .map(|sample| sample.offset)
            .unwrap_or(0);
        let match_start = clock_samples
            .first()
            .and_then(|sample| event_envelope(data, sample.offset));
        let envelope_timeline = self.envelope_timeline();
        // Past this the file repeats the whole match as the end-of-match
        // summary, with its own envelope clock restarting at zero.
        let recording_start_offset = envelope_timeline.starts.first().copied().unwrap_or(0);
        let recording_end_offset = envelope_timeline.end_offset;
        let timeline_time = |offset| round_timeline_seconds(envelope_timeline.time_at(offset));

        // Phase bounds were accumulated in countdown-elapsed seconds. Re-anchor
        // them onto the recording axis using the offsets of the samples that
        // opened and closed each countdown run, so a phase's span lines up with
        // the events inside it.
        let mut phase_bounds = vec![(None, None); phases.len()];
        for sample in clock_samples {
            let Some(bounds) = usize::try_from(sample.phase_index)
                .ok()
                .and_then(|index| phase_bounds.get_mut(index))
            else {
                continue;
            };
            bounds.0.get_or_insert(sample.offset);
            bounds.1 = Some(sample.offset);
        }
        for phase in &mut phases {
            let (first, last) = usize::try_from(phase.index)
                .ok()
                .and_then(|index| phase_bounds.get(index))
                .copied()
                .unwrap_or_default();
            let last = last.or(first);
            phase.start_seconds = first.map(timeline_time).unwrap_or(0.0);
            phase.end_seconds = last.map(timeline_time).unwrap_or(phase.start_seconds);
            phase.initial_clock_seconds = round_timeline_seconds(phase.initial_clock_seconds);
            phase.final_clock_seconds = round_timeline_seconds(phase.final_clock_seconds);
        }
        let all_player_slots: HashSet<u32> = (0..=15).collect();
        let static_participant_names = self.extract_resolved_slot_names(&all_player_slots);
        let identity_intervals =
            PlayerIdentityIntervals::build(data, &self.event_index, &static_participant_names);
        let mut positioned_events = Vec::new();
        let mut pre_match_chat = Vec::new();
        let mut post_match_chat = Vec::new();
        let match_end = self
            .event_index
            .positions(IndexedTag::TeamWins)
            .filter_map(|pos| {
                let envelope = event_envelope(data, pos)?;
                read_expected_u32(data, pos + 4, &HASH_ATEAM)?;
                read_expected_u32(data, pos + 21, &HASH_ATYPE)?;
                Some(envelope)
            })
            .next_back();

        for pos in self.event_index.positions(IndexedTag::PlayerEntersGame) {
            if pos < gameplay_start_offset {
                continue;
            }
            if let Some(player_id) = read_expected_u32(data, pos + 4, &HASH_ASLOT)
                && player_id <= 15
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::PlayerEntered {
                        time_seconds: timeline_time(pos),
                        player_id,
                    },
                });
            }
        }

        for pos in self.event_index.positions(IndexedTag::PlayerLeavesGame) {
            if pos < gameplay_start_offset {
                continue;
            }
            if let Some(player_id) = read_expected_u32(data, pos + 4, &HASH_ASLOT)
                && player_id <= 15
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::PlayerLeft {
                        time_seconds: timeline_time(pos),
                        player_id,
                    },
                });
            }
        }

        for pos in self.event_index.positions(IndexedTag::PlayerJoinedTeam) {
            if pos < gameplay_start_offset {
                continue;
            }
            if let (Some(player_id), Some(team)) = (
                read_expected_u32(data, pos + 4, &HASH_ASLOT),
                read_expected_u32(data, pos + 21, &HASH_ATEAM),
            ) && player_id <= 15
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::PlayerJoinedTeam {
                        time_seconds: timeline_time(pos),
                        player_id,
                        team,
                    },
                });
            }
        }

        // `SpectatorJoinedTeam` also records camera visibility. Preserve the last
        // pre-game state per player at t=0 so a recorder who entered spectator
        // mode before the gameplay clock still has a usable initial state.
        // Team joins and session boundaries supersede earlier lobby settings.
        let mut initial_spectator_states: HashMap<u32, (usize, u32, u32)> = HashMap::new();
        for entry in self.event_index.offsets() {
            let pos = entry.offset;
            if pos < gameplay_start_offset
                && matches!(
                    entry.tag,
                    IndexedTag::PlayerJoinedTeam
                        | IndexedTag::PlayerLeavesGame
                        | IndexedTag::PlayerEntersGame
                )
                && let Some(player_id) = read_expected_u32(data, pos + 4, &HASH_ASLOT)
            {
                initial_spectator_states.remove(&player_id);
            }
            if entry.tag != IndexedTag::SpectatorJoinedTeam {
                continue;
            }
            let (Some(player_id), Some(team), Some(spectator_los)) = (
                read_expected_u32(data, pos + 4, &HASH_ASLOT),
                read_expected_u32(data, pos + 21, &HASH_ATEAM),
                read_expected_u32(data, pos + 38, &HASH_ASPECTATOR_LOS),
            ) else {
                continue;
            };
            if player_id > 15 {
                continue;
            }
            if pos < gameplay_start_offset {
                initial_spectator_states.insert(player_id, (pos, team, spectator_los));
                continue;
            }
            positioned_events.push(PositionedTimelineEvent {
                offset: pos,
                event: TimelineEvent::SpectatorViewChanged {
                    time_seconds: timeline_time(pos),
                    player_id,
                    team,
                    spectator_los,
                    view: spectator_view(team, spectator_los),
                },
            });
        }
        for (player_id, (offset, team, spectator_los)) in initial_spectator_states {
            positioned_events.push(PositionedTimelineEvent {
                offset,
                event: TimelineEvent::SpectatorViewChanged {
                    time_seconds: 0.0,
                    player_id,
                    team,
                    spectator_los,
                    view: spectator_view(team, spectator_los),
                },
            });
        }

        for pos in self.event_index.positions(IndexedTag::PlayerSetRole) {
            if pos < gameplay_start_offset {
                continue;
            }
            if let (Some(player_id), Some(role_id)) = (
                read_expected_u32(data, pos + 4, &HASH_ASLOT),
                read_expected_u32(data, pos + 21, &HASH_AROLE_ID),
            ) && player_id <= 15
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::PlayerSetRole {
                        time_seconds: timeline_time(pos),
                        player_id,
                        role_id,
                        role: role_name(role_id).map(str::to_string),
                    },
                });
            }
        }

        let mut command_point_owners = HashMap::new();
        for pos in self.event_index.positions(IndexedTag::SetCommandPointOwner) {
            if pos < gameplay_start_offset {
                continue;
            }
            if let (Some(command_point_id), Some(team)) = (
                read_expected_u32(data, pos + 4, &HASH_ANAME),
                read_expected_u32(data, pos + 21, &HASH_ATEAM),
            ) && command_point_owners.get(&command_point_id) != Some(&team)
            {
                command_point_owners.insert(command_point_id, team);
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::CommandPointOwnerChanged {
                        time_seconds: timeline_time(pos),
                        command_point_id,
                        team,
                    },
                });
            }
        }

        let destruction_contexts = extract_destruction_contexts(data, &self.event_index);

        enum UnitLifecycle {
            Create,
            Remove,
            HomingProjectile,
            SupportDeployment,
            HomingSupportProjectile,
            Destroy,
        }
        let mut units: HashMap<u32, UnitState> = HashMap::new();
        let mut exact_target_projectiles = RecentExactTargetProjectiles::default();
        let mut support_deployments = RecentSupportDeployments::default();
        let mut exact_target_support_projectiles = RecentExactTargetSupportProjectiles::default();
        let mut exact_ta_causes_by_destruction_offset = HashMap::new();
        let mut positioned_infantry_soldier_deaths = Vec::new();
        for indexed in self.event_index.offsets() {
            if !envelope_timeline.starts.is_empty() {
                if indexed.offset < recording_start_offset {
                    continue;
                }
                if indexed.offset >= recording_end_offset {
                    break;
                }
                let Some(envelope_start) = event_envelope_start(data, indexed.offset) else {
                    continue;
                };
                if envelope_timeline
                    .starts
                    .binary_search(&envelope_start)
                    .is_err()
                {
                    continue;
                }
            }
            let lifecycle_event = match indexed.tag {
                IndexedTag::UnitCreate => UnitLifecycle::Create,
                IndexedTag::UnitRemove => UnitLifecycle::Remove,
                IndexedTag::ProjectileHomingUnitCreate => UnitLifecycle::HomingProjectile,
                IndexedTag::SupportThingSpawnedDelayed => UnitLifecycle::SupportDeployment,
                IndexedTag::ProjectileHomingSupportCreateUnit => {
                    UnitLifecycle::HomingSupportProjectile
                }
                IndexedTag::UnitDestroy => UnitLifecycle::Destroy,
                _ => continue,
            };
            let pos = indexed.offset;
            match lifecycle_event {
                UnitLifecycle::Create => {
                    let Some(_persistence_key) =
                        read_expected_u32(data, pos + 4, &HASH_APERSISTENCE_KEY)
                    else {
                        continue;
                    };
                    let (Some(player_id), Some(team), Some(unit_id), Some(unit_type_id)) = (
                        read_expected_u32(data, pos + 21, &HASH_APLAYER),
                        read_expected_u32(data, pos + 38, &HASH_ATEAM),
                        read_expected_u32(data, pos + 55, &HASH_AUNIT),
                        read_expected_u32(data, pos + 72, &HASH_ATYPE),
                    ) else {
                        continue;
                    };
                    units.insert(
                        unit_id,
                        UnitState {
                            player_id,
                            team,
                            unit_type_id,
                            generation_offset: pos,
                        },
                    );
                }
                UnitLifecycle::Remove => {
                    if let Some(unit_id) = read_expected_u32(data, pos + 4, &HASH_AUNIT) {
                        units.remove(&unit_id);
                    }
                }
                UnitLifecycle::HomingProjectile => {
                    let Some(envelope) = event_envelope(data, pos) else {
                        continue;
                    };
                    let (Some(target_unit_id), Some(firing_unit_id)) = (
                        read_expected_u32(data, pos + 157, &HASH_ATARGET),
                        read_expected_u32(data, pos + 174, &HASH_AUNIT),
                    ) else {
                        continue;
                    };
                    let (Some(target), Some(firing_unit)) = (
                        units.get(&target_unit_id).copied(),
                        units.get(&firing_unit_id).copied(),
                    ) else {
                        continue;
                    };
                    exact_target_projectiles.push(ExactTargetProjectile {
                        time_seconds: envelope.raw_time_seconds,
                        firing_unit_id,
                        target_unit_id,
                        target_generation_offset: target.generation_offset,
                        actor: ProjectileActor {
                            player_id: (firing_unit.player_id <= 15)
                                .then_some(firing_unit.player_id),
                            team: (firing_unit.team != 0).then_some(firing_unit.team),
                        },
                    });
                }
                UnitLifecycle::SupportDeployment => {
                    let Some(envelope) = event_envelope(data, pos) else {
                        continue;
                    };
                    let (Some(support_id), Some(team)) = (
                        read_expected_u32(data, pos + 4, &HASH_ANID),
                        read_expected_u32(data, pos + 72, &HASH_ATEAM),
                    ) else {
                        continue;
                    };
                    if tactical_aid_projectile_name(support_id).is_none()
                        || !(1..=3).contains(&team)
                    {
                        continue;
                    }
                    support_deployments.push(SupportDeployment {
                        time_seconds: envelope.raw_time_seconds,
                        support_id,
                        team,
                    });
                }
                UnitLifecycle::HomingSupportProjectile => {
                    let Some(envelope) = event_envelope(data, pos) else {
                        continue;
                    };
                    let (Some(support_id), Some(target_unit_id)) = (
                        read_expected_u32(data, pos + 21, &HASH_ASUPPORT_THING),
                        read_expected_u32(data, pos + 157, &HASH_AUNIT),
                    ) else {
                        continue;
                    };
                    let Some(target) = units.get(&target_unit_id).copied() else {
                        continue;
                    };
                    let Some(team) =
                        support_deployments.unique_team(envelope.raw_time_seconds, support_id)
                    else {
                        continue;
                    };
                    exact_target_support_projectiles.push(ExactTargetSupportProjectile {
                        time_seconds: envelope.raw_time_seconds,
                        target_unit_id,
                        target_generation_offset: target.generation_offset,
                        cause: TacticalAidCause { support_id, team },
                    });
                }
                UnitLifecycle::Destroy => {
                    let (Some(unit_id), Some(killer_unit_id), Some(_killer_experience)) = (
                        read_expected_u32(data, pos + 4, &HASH_AUNIT),
                        read_expected_u32(data, pos + 21, &HASH_AKILLER),
                        read_expected_u32(data, pos + 38, &HASH_AKILLER_EXPERIENCE),
                    ) else {
                        continue;
                    };
                    let victim = units.get(&unit_id).copied();
                    let killer = units.get(&killer_unit_id).copied();
                    let recovered_actor =
                        if killer.is_none() && killer_unit_id != KILLER_UNIT_SENTINEL {
                            event_envelope(data, pos).and_then(|envelope| {
                                victim.and_then(|victim| {
                                    exact_target_projectiles.unique_actor(
                                        envelope.raw_time_seconds,
                                        (killer_unit_id, unit_id, victim.generation_offset),
                                    )
                                })
                            })
                        } else {
                            None
                        };
                    let tactical_aid_cause = if killer_unit_id == KILLER_UNIT_SENTINEL {
                        event_envelope(data, pos).and_then(|envelope| {
                            victim.and_then(|victim| {
                                exact_target_support_projectiles.unique_cause(
                                    envelope.raw_time_seconds,
                                    (unit_id, victim.generation_offset),
                                )
                            })
                        })
                    } else {
                        None
                    };
                    if let Some(cause) = tactical_aid_cause {
                        exact_ta_causes_by_destruction_offset.insert(pos, cause);
                    }
                    let destruction_context = destruction_contexts
                        .get(&pos)
                        .map(|evidence| evidence.context);
                    units.remove(&unit_id);

                    if pos >= gameplay_start_offset && pos < recording_end_offset {
                        let player_id = victim
                            .filter(|unit| unit.player_id <= 15)
                            .map(|unit| unit.player_id);
                        let team = victim.filter(|unit| unit.team != 0).map(|unit| unit.team);
                        let killer_player_id = killer
                            .filter(|unit| unit.player_id <= 15)
                            .map(|unit| unit.player_id)
                            .or_else(|| recovered_actor.and_then(|actor| actor.player_id));
                        let killer_team = killer
                            .filter(|unit| unit.team != 0)
                            .map(|unit| unit.team)
                            .or_else(|| recovered_actor.and_then(|actor| actor.team))
                            .or_else(|| tactical_aid_cause.map(|cause| cause.team));
                        let cause = if tactical_aid_cause.is_some() {
                            UnitDestructionCause::TacticalAid
                        } else if killer_unit_id != KILLER_UNIT_SENTINEL {
                            UnitDestructionCause::Unit
                        } else {
                            UnitDestructionCause::Unknown
                        };
                        let tactical_aid_support_id =
                            tactical_aid_cause.map(|cause| cause.support_id);
                        let tactical_aid_support_name = tactical_aid_support_id
                            .and_then(tactical_aid_projectile_name)
                            .map(str::to_string);

                        if let Some(unit_type_id) = victim
                            .map(|unit| unit.unit_type_id)
                            .filter(|unit_type_id| is_infantry_soldier_type(*unit_type_id))
                        {
                            positioned_infantry_soldier_deaths.push((
                                pos,
                                InfantrySoldierDeath {
                                    time_seconds: timeline_time(pos),
                                    unit_id,
                                    unit_type_id,
                                    player_id,
                                    team,
                                    killer_unit_id: Some(killer_unit_id),
                                    killer_player_id,
                                    killer_team,
                                    cause,
                                    destruction_context,
                                    tactical_aid_support_id,
                                    tactical_aid_support_name,
                                },
                            ));
                            continue;
                        }

                        positioned_events.push(PositionedTimelineEvent {
                            offset: pos,
                            event: TimelineEvent::UnitDestroyed {
                                time_seconds: timeline_time(pos),
                                unit_id,
                                unit_type_id: victim.map(|unit| unit.unit_type_id),
                                player_id,
                                team,
                                killer_unit_id: Some(killer_unit_id),
                                killer_player_id,
                                killer_team,
                                cause,
                                destruction_context,
                                tactical_aid_support_id,
                                tactical_aid_support_name,
                            },
                        });
                    }
                }
            }
        }

        for positioned in &mut positioned_events {
            let TimelineEvent::UnitDestroyed {
                killer_unit_id,
                killer_team,
                cause,
                destruction_context,
                tactical_aid_support_id,
                tactical_aid_support_name,
                ..
            } = &mut positioned.event
            else {
                continue;
            };
            match inherited_container_tactical_aid(
                positioned.offset,
                &destruction_contexts,
                &exact_ta_causes_by_destruction_offset,
            ) {
                Ok(Some(inherited)) if *killer_unit_id == Some(KILLER_UNIT_SENTINEL) => {
                    *killer_team = Some(inherited.team);
                    *cause = UnitDestructionCause::TacticalAid;
                    *tactical_aid_support_id = Some(inherited.support_id);
                    *tactical_aid_support_name =
                        tactical_aid_projectile_name(inherited.support_id).map(str::to_string);
                }
                Err(()) => *destruction_context = None,
                _ => {}
            }
        }
        for (offset, death) in &mut positioned_infantry_soldier_deaths {
            match inherited_container_tactical_aid(
                *offset,
                &destruction_contexts,
                &exact_ta_causes_by_destruction_offset,
            ) {
                Ok(Some(inherited)) if death.killer_unit_id == Some(KILLER_UNIT_SENTINEL) => {
                    death.killer_team = Some(inherited.team);
                    death.cause = UnitDestructionCause::TacticalAid;
                    death.tactical_aid_support_id = Some(inherited.support_id);
                    death.tactical_aid_support_name =
                        tactical_aid_projectile_name(inherited.support_id).map(str::to_string);
                }
                Err(()) => death.destruction_context = None,
                _ => {}
            }
        }

        for pos in self
            .event_index
            .positions(IndexedTag::ShowPlayerGiveTaNotification)
        {
            if pos < gameplay_start_offset {
                continue;
            }
            if let (Some(from_player_id), Some(to_player_id), Some(amount)) = (
                read_expected_u32(data, pos + 4, &HASH_AFROM_SLOT),
                read_expected_u32(data, pos + 21, &HASH_ATO_SLOT),
                read_expected_u32(data, pos + 38, &HASH_ANUM_TA),
            ) && from_player_id <= 15
                && to_player_id <= 15
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::TacticalAidTransferred {
                        time_seconds: timeline_time(pos),
                        from_player_id,
                        to_player_id,
                        amount,
                    },
                });
            }
        }

        // `SendTATaunt` carries exact tactical-aid actor and target slots, but its
        // `aTAId` is an index into the support manager rather than the support
        // definition hash used by other replay records. The replay serializes that
        // manager order as zero-position `SupportThingUsed` catalogue rows. Keep
        // the raw index even when an older/custom replay has no resolvable row.
        // Priced world-origin uses are excluded from the catalogue.
        let tactical_aid_catalogue: Vec<u32> = self
            .event_index
            .positions(IndexedTag::SupportThingUsed)
            .filter_map(|pos| {
                let envelope_start = event_envelope_start(data, pos)?;
                let support_id = read_expected_u32(data, pos + 4, &HASH_ANID)?;
                let position = [
                    read_expected_float(data, pos + 21, &HASH_APOSITION_X)?,
                    read_expected_float(data, pos + 38, &HASH_APOSITION_Y)?,
                    read_expected_float(data, pos + 55, &HASH_APOSITION_Z)?,
                ];
                if position != [0.0, 0.0, 0.0]
                    || preceding_honors_delta(data, envelope_start)
                        .is_some_and(|delta| delta.is_finite() && delta < 0.0)
                {
                    return None;
                }
                Some(support_id)
            })
            .collect();

        for pos in self.event_index.positions(IndexedTag::SendTaTaunt) {
            let Some(envelope_start) = event_envelope_start(data, pos) else {
                continue;
            };
            if envelope_start < gameplay_start_offset {
                continue;
            }
            let (Some(player_id), Some(target_player_id), Some(ta_index), Some(upgrade_level)) = (
                read_expected_u32(data, pos + 4, &HASH_APLAYER_FROM),
                read_expected_u32(data, pos + 21, &HASH_APLAYER_TAUNTED),
                read_expected_u32(data, pos + 38, &HASH_ATA_ID),
                read_expected_u32(data, pos + 55, &HASH_ASUPPORT_UPGRADE_LEVEL),
            ) else {
                continue;
            };
            if player_id > 15 || target_player_id > 15 {
                continue;
            }
            let support_id = usize::try_from(ta_index)
                .ok()
                .and_then(|index| tactical_aid_catalogue.get(index).copied());
            positioned_events.push(PositionedTimelineEvent {
                offset: envelope_start,
                event: TimelineEvent::TacticalAidDamageThreshold {
                    time_seconds: timeline_time(envelope_start),
                    player_id,
                    target_player_id,
                    ta_index,
                    support_id,
                    support_name: support_id.and_then(support_name).map(str::to_string),
                    upgrade_level,
                },
            });
        }

        // Tactical aid activations. `SupportThingUsed` serializes only `anId` and
        // `aPosition` (wic.exe 0x00b830c0), so the acting player is never in the
        // record. The paired `ChangeHonors` deduction is the recording player's
        // own honors ledger, which is what makes recorder attribution and exact
        // cost possible. Other players do not appear in this purchase ledger;
        // their top-level deployments remain visible through effect records below.
        // A burst of zero-position `SupportThingUsed` records is emitted at match
        // start to enumerate the available supports; those are not activations
        // because they have no paired negative honors deduction. Position alone is
        // not used to classify the record, so an activation at the world origin is
        // retained.
        let recorder_slot = self.extract_recorder_slot();
        let mut tactical_aid_costs: HashMap<u32, Vec<f32>> = HashMap::new();
        for pos in self.event_index.positions(IndexedTag::SupportThingUsed) {
            let Some(envelope_start) = event_envelope_start(data, pos) else {
                continue;
            };
            if envelope_start < gameplay_start_offset {
                continue;
            }
            let (Some(support_id), Some(x), Some(y), Some(z)) = (
                read_expected_u32(data, pos + 4, &HASH_ANID),
                read_expected_float(data, pos + 21, &HASH_APOSITION_X),
                read_expected_float(data, pos + 38, &HASH_APOSITION_Y),
                read_expected_float(data, pos + 55, &HASH_APOSITION_Z),
            ) else {
                continue;
            };
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                continue;
            }
            let Some(delta) = preceding_honors_delta(data, envelope_start) else {
                continue;
            };
            if !delta.is_finite() || delta >= 0.0 {
                continue;
            }

            tactical_aid_costs
                .entry(support_id)
                .or_default()
                .push(-delta);

            positioned_events.push(PositionedTimelineEvent {
                offset: envelope_start,
                event: TimelineEvent::TacticalAidUsed {
                    time_seconds: timeline_time(envelope_start),
                    support_id,
                    support_name: support_name(support_id).map(str::to_string),
                    honors_cost: -delta,
                    position: [x, y, z],
                    player_id: recorder_slot,
                },
            });
        }

        let mut unit_drop_creates: HashMap<(u32, u32), Vec<UnitDropCreate>> = HashMap::new();
        for unit in self
            .event_index
            .positions(IndexedTag::UnitCreate)
            .filter_map(|pos| parse_unit_drop_create(data, pos))
        {
            unit_drop_creates
                .entry((unit.team, unit.unit_type_id))
                .or_default()
                .push(unit);
        }
        // Top-level simulation deployments. `SupportThingSpawnedDelayed`
        // serializes support type, faction and position for both sides, including
        // while the recorder is spectating one team. It carries no player slot.
        // For the nine unit-drop definitions only, a separately serialized
        // spawn-source-1 UnitCreate can supply the player through a binary-proven,
        // corpus-validated ownership join. Ambiguous and unmatched drops remain
        // player-null. Child effects and unit abilities retain their raw records.
        for pos in self
            .event_index
            .positions(IndexedTag::SupportThingSpawnedDelayed)
        {
            let Some(envelope_start) = event_envelope_start(data, pos) else {
                continue;
            };
            if envelope_start < gameplay_start_offset {
                continue;
            }
            let (
                Some(support_id),
                Some(x),
                Some(y),
                Some(z),
                Some(team),
                Some(upgrade_level),
                Some(direction_x),
                Some(direction_y),
                Some(direction_z),
                Some(age_seconds),
            ) = (
                read_expected_u32(data, pos + 4, &HASH_ANID),
                read_expected_float(data, pos + 21, &HASH_APOSITION_X),
                read_expected_float(data, pos + 38, &HASH_APOSITION_Y),
                read_expected_float(data, pos + 55, &HASH_APOSITION_Z),
                read_expected_u32(data, pos + 72, &HASH_ATEAM),
                read_expected_u32(data, pos + 89, &HASH_A_SUPPORT_UPPGRADE_LEVEL),
                read_expected_float(data, pos + 106, &HASH_ADIRECTION_X),
                read_expected_float(data, pos + 123, &HASH_ADIRECTION_Y),
                read_expected_float(data, pos + 140, &HASH_ADIRECTION_Z),
                read_expected_float(data, pos + 157, &HASH_ATIME_SINCE_CREATION),
            )
            else {
                continue;
            };
            let Some(support_name) = support_name(support_id) else {
                continue;
            };
            if !(1..=3).contains(&team)
                || ![x, y, z, direction_x, direction_y, direction_z, age_seconds]
                    .iter()
                    .all(|value| value.is_finite())
                || age_seconds < 0.0
            {
                continue;
            }

            let player_id = unit_drop_spec(support_id).and_then(|spec| {
                let envelope = event_envelope(data, pos)?;
                let deployment_raw_time = envelope.raw_time_seconds - age_seconds;
                let units = unit_drop_creates.get(&(team, spec.unit_type_id))?;
                let mut players = HashSet::new();
                for unit in units {
                    let arrival_seconds = unit.raw_time_seconds - deployment_raw_time;
                    let horizontal_distance =
                        ((unit.position[0] - x).powi(2) + (unit.position[2] - z).powi(2)).sqrt();
                    if (spec.minimum_arrival_seconds..=spec.maximum_arrival_seconds)
                        .contains(&arrival_seconds)
                        && horizontal_distance <= spec.maximum_horizontal_distance
                    {
                        players.insert(unit.player_id);
                    }
                }
                (players.len() == 1).then(|| *players.iter().next().expect("one player"))
            });

            positioned_events.push(PositionedTimelineEvent {
                offset: envelope_start,
                event: TimelineEvent::TacticalAidDeployed {
                    time_seconds: timeline_time(envelope_start),
                    support_id,
                    support_name: support_name.to_string(),
                    position: [x, y, z],
                    team,
                    upgrade_level,
                    direction: [direction_x, direction_y, direction_z],
                    age_seconds,
                    player_id,
                    player_attribution: player_id
                        .map(|_| TacticalAidPlayerAttribution::UnitSpawnOwnership),
                },
            });
        }

        // Single-faction tactical-aid markers. `wic.exe:0x00b86e70` names the
        // sixth payload field `aTeam`, but the server support-barrage path keeps
        // this value as the issuing connection's player-slot byte alongside the
        // marker event ID. Across the 2,880-replay validation corpus, all 199,451
        // values are in 0-15 and all 42,340 independently matched recorder
        // purchases agree with the serialized slot (zero mismatches).
        for pos in self.event_index.positions(IndexedTag::SupportThingMarker) {
            let Some(envelope_start) = event_envelope_start(data, pos) else {
                continue;
            };
            if envelope_start < gameplay_start_offset {
                continue;
            }
            let (
                Some(event_id),
                Some(support_id),
                Some(x),
                Some(y),
                Some(z),
                Some(player_id),
                Some(upgrade_level),
                Some(direction_x),
                Some(direction_y),
                Some(direction_z),
                Some(duration_seconds),
            ) = (
                read_expected_u32(data, pos + 4, &HASH_AN_EVENT_ID),
                read_expected_u32(data, pos + 21, &HASH_ANID),
                read_expected_float(data, pos + 38, &HASH_APOSITION_X),
                read_expected_float(data, pos + 55, &HASH_APOSITION_Y),
                read_expected_float(data, pos + 72, &HASH_APOSITION_Z),
                read_expected_u32(data, pos + 89, &HASH_ATEAM),
                read_expected_u32(data, pos + 106, &HASH_A_SUPPORT_UPPGRADE_LEVEL),
                read_expected_float(data, pos + 123, &HASH_ADIRECTION_X),
                read_expected_float(data, pos + 140, &HASH_ADIRECTION_Y),
                read_expected_float(data, pos + 157, &HASH_ADIRECTION_Z),
                read_expected_float(data, pos + 174, &HASH_ATIME),
            )
            else {
                continue;
            };
            if player_id > 15
                || ![
                    x,
                    y,
                    z,
                    direction_x,
                    direction_y,
                    direction_z,
                    duration_seconds,
                ]
                .iter()
                .all(|value| value.is_finite())
                || duration_seconds < 0.0
            {
                continue;
            }

            positioned_events.push(PositionedTimelineEvent {
                offset: envelope_start,
                event: TimelineEvent::TacticalAidMarker {
                    time_seconds: timeline_time(envelope_start),
                    event_id,
                    support_id,
                    support_name: support_name(support_id).map(str::to_string),
                    position: [x, y, z],
                    player_id,
                    upgrade_level,
                    direction: [direction_x, direction_y, direction_z],
                    duration_seconds,
                },
            });
        }

        // `PlayerReceiveChat` is the chat stream visible to the recorder's
        // client: global chat from everyone and team chat from the recorder's
        // team. The variable-length `aMessage` field is UTF-16LE and is parsed
        // only within a structurally valid event envelope. Sender names are
        // resolved through the active slot-occupant interval at the exact envelope
        // offset. The serialized player ID remains authoritative when a name cannot
        // be recovered.
        let received_chat: Vec<(EventEnvelope, u32, String, ChatChannel)> = self
            .event_index
            .positions(IndexedTag::PlayerReceiveChat)
            .filter_map(|pos| parse_received_chat(data, pos))
            .collect();
        for (envelope, player_id, message, channel) in received_chat {
            if envelope.start < gameplay_start_offset {
                pre_match_chat.push(PreMatchChatMessage {
                    time_seconds: timeline_time(envelope.start),
                    seconds_before_match: round_timeline_seconds(
                        match_start
                            .map(|start| {
                                (start.raw_time_seconds - envelope.raw_time_seconds).max(0.0)
                            })
                            .unwrap_or(0.0),
                    ),
                    player_id,
                    player_name: identity_intervals
                        .name_at(player_id, envelope.start)
                        .map(str::to_owned),
                    message,
                    channel,
                });
            } else if let Some(match_end) = match_end
                && envelope.start > match_end.start
            {
                post_match_chat.push(PostMatchChatMessage {
                    time_seconds: timeline_time(envelope.start),
                    seconds_after_match: round_timeline_seconds(
                        (envelope.raw_time_seconds - match_end.raw_time_seconds).max(0.0),
                    ),
                    player_id,
                    player_name: identity_intervals
                        .name_at(player_id, envelope.start)
                        .map(str::to_owned),
                    message,
                    channel,
                });
            } else {
                positioned_events.push(PositionedTimelineEvent {
                    offset: envelope.start,
                    event: TimelineEvent::ChatMessage {
                        time_seconds: timeline_time(envelope.start),
                        player_id,
                        player_name: identity_intervals
                            .name_at(player_id, envelope.start)
                            .map(str::to_owned),
                        message,
                        channel,
                    },
                });
            }
        }

        for pos in self.event_index.positions(IndexedTag::MakeVote) {
            if let (
                Some(player_id),
                Some(vote_id),
                Some(float_value),
                Some(int_value),
                Some(uint_value),
            ) = (
                read_expected_u32(data, pos + 4, &HASH_ASLOT),
                read_expected_u32(data, pos + 21, &HASH_AVOTE),
                read_expected_float(data, pos + 38, &HASH_AVALUE_FLOAT),
                read_expected_i32(data, pos + 55, &HASH_AVALUE_INT),
                read_expected_u32(data, pos + 72, &HASH_AVALUE_UINT),
            ) && player_id <= 15
                && float_value.is_finite()
            {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::VoteStarted {
                        time_seconds: timeline_time(pos),
                        player_id,
                        vote_id,
                        float_value,
                        int_value,
                        uint_value,
                    },
                });
            }
        }

        for pos in self.event_index.positions(IndexedTag::TeamWins) {
            if let (Some(team), Some(win_type)) = (
                read_expected_u32(data, pos + 4, &HASH_ATEAM),
                read_expected_u32(data, pos + 21, &HASH_ATYPE),
            ) {
                positioned_events.push(PositionedTimelineEvent {
                    offset: pos,
                    event: TimelineEvent::TeamWon {
                        time_seconds: timeline_time(pos),
                        team,
                        win_type,
                    },
                });
            }
        }

        // The end-of-match summary repeats every record with its own clock, so
        // anything at or past the chain cut would land a duplicate on the
        // recording axis.
        positioned_events.retain(|event| event.offset < recording_end_offset);
        positioned_events.sort_by_key(|event| event.offset);
        pre_match_chat.sort_by(|left, right| {
            right
                .seconds_before_match
                .total_cmp(&left.seconds_before_match)
        });
        post_match_chat.sort_by(|left, right| {
            left.seconds_after_match
                .total_cmp(&right.seconds_after_match)
        });
        let events: Vec<TimelineEvent> = positioned_events
            .into_iter()
            .map(|positioned| positioned.event)
            .collect();
        let infantry_soldier_deaths: Vec<InfantrySoldierDeath> = positioned_infantry_soldier_deaths
            .into_iter()
            .map(|(_, death)| death)
            .collect();

        let mut participant_ids = HashSet::new();
        for event in &events {
            participant_ids.extend(event.referenced_player_ids().into_iter().flatten());
        }
        for death in &infantry_soldier_deaths {
            participant_ids.extend(death.referenced_player_ids().into_iter().flatten());
        }
        participant_ids.extend(pre_match_chat.iter().map(|message| message.player_id));
        participant_ids.extend(post_match_chat.iter().map(|message| message.player_id));
        participant_ids.extend(recorder_slot);

        let mut participants: Vec<TimelineParticipant> = participant_ids
            .iter()
            .copied()
            .map(|player_id| TimelineParticipant {
                player_id,
                player_name: static_participant_names.get(&player_id).cloned(),
            })
            .collect();
        participants.sort_by_key(|participant| participant.player_id);
        let participant_sessions =
            identity_intervals.public_sessions(&participant_ids, timeline_time);

        let mut tactical_aid_supports: Vec<TacticalAidUsageCount> = tactical_aid_costs
            .into_iter()
            .map(|(support_id, observed_costs)| TacticalAidUsageCount {
                support_id,
                support_name: support_name(support_id).map(str::to_string),
                placement_count: observed_costs.len() as u32,
                observed_costs,
            })
            .collect();
        tactical_aid_supports.sort_by_key(|usage| usage.support_id);
        let total_placements = tactical_aid_supports.iter().fold(0u32, |total, usage| {
            total.saturating_add(usage.placement_count)
        });

        let mut factor_samples: Vec<(usize, f32)> = self
            .event_index
            .positions(IndexedTag::AFactor)
            .filter_map(|pos| {
                if pos >= recording_end_offset {
                    return None;
                }
                let value = read_bintag_float(data, pos)?;
                ((0.0..=1.0).contains(&value) && value.is_finite()).then_some((pos, value))
            })
            .collect();

        // Assault's value tracks attacker progress rather than a two-sided split,
        // so mirroring it would be meaningless.
        let bar_game_mode = self.extract_game_info().game_mode;
        let domination_anchor_faction = if publishes_control_split(&bar_game_mode) {
            unmirror_bar_curve(&mut factor_samples, &self.recorder_team_change_offsets());
            factor_samples
                .last()
                .and_then(|(offset, _)| self.pov_team_at(*offset))
                .map(|team| faction_name(team).to_owned())
        } else {
            None
        };
        let mut domination_samples: Vec<TimelineValueSample> = Vec::new();
        for &(offset, value) in &factor_samples {
            let time_seconds = timeline_time(offset);
            let should_sample = domination_samples.last().is_none_or(|last| {
                time_seconds - last.time_seconds >= DOMINATION_SAMPLE_INTERVAL_SECONDS
            });
            if should_sample {
                domination_samples.push(TimelineValueSample {
                    time_seconds,
                    value,
                });
            }
        }
        if let Some(&(offset, value)) = factor_samples.last() {
            let final_sample = TimelineValueSample {
                time_seconds: timeline_time(offset),
                value,
            };
            if let Some(last) = domination_samples.last_mut()
                && last.time_seconds == final_sample.time_seconds
            {
                *last = final_sample;
            } else {
                domination_samples.push(final_sample);
            }
        }

        TimelineData {
            schema_version: TIMELINE_SCHEMA_VERSION,
            domination_anchor_faction,
            duration_seconds: round_timeline_seconds(envelope_timeline.seconds),
            match_duration_seconds: round_timeline_seconds(
                clock_samples
                    .last()
                    .map(|sample| sample.elapsed_seconds)
                    .unwrap_or(0.0),
            ),
            initial_clock_seconds: clock_samples
                .first()
                .map(|sample| round_timeline_seconds(sample.remaining_seconds)),
            final_clock_seconds: clock_samples
                .last()
                .map(|sample| round_timeline_seconds(sample.remaining_seconds)),
            phases,
            domination_samples,
            participants,
            participant_sessions,
            events,
            infantry_soldier_deaths,
            pre_match_chat,
            post_match_chat,
            coverage: TimelineCoverage {
                chat: "visibleToRecorder",
                tactical_aid: "recorderOnly",
                tactical_aid_markers: "visibleFactionWithPlayer",
                tactical_aid_deployments: "bothFactionsWithValidatedUnitDropPlayers",
            },
            recorder_tactical_aid_usage: RecorderTacticalAidUsage {
                player_id: recorder_slot,
                total_placements,
                supports: tactical_aid_supports,
            },
        }
    }

    pub fn parse_with_timeline(&self) -> ReplayWithTimeline {
        ReplayWithTimeline {
            replay: self.parse(),
            timeline: self.parse_timeline(),
        }
    }

    pub fn parse(&self) -> ReplayData {
        let _work = acquire_replay_work();
        let game_info = self.extract_game_info();
        let (raw_server_flags, server_classification) = self.extract_server_classification();

        // 1. Get scores and teams (single scan)
        let counter_scores = self.extract_scores_by_hash();
        let mut player_score_map: HashMap<u32, i32> = HashMap::new();
        for (&counter, &score) in &counter_scores {
            let slot = ((counter as i32 - 1).rem_euclid(16)) as u32;
            player_score_map.insert(slot, score);
        }
        let explicit_scores = self.explicit_match_scores();
        if !explicit_scores.is_empty() {
            player_score_map = explicit_scores
                .into_iter()
                .map(|(_, slot, score)| (slot, score))
                .collect();
        }
        let mut player_scores: Vec<i32> = player_score_map
            .values()
            .copied()
            .filter(|&s| s != 0)
            .collect();
        player_scores.sort_unstable_by(|a, b| b.cmp(a));

        let team_map = self.extract_team_assignments();
        let end_summaries = self.extract_player_end_summaries();

        // 2. Resolve the scored roster at gameplay start. Static metadata and
        // confirmed lobby swaps provide the baseline; a structurally decoded
        // pre-match replacement corrects a reused slot before scores and
        // end-summary roles are attached. Later replacements remain separate.
        let scored_ids: HashSet<u32> = player_score_map
            .iter()
            .filter(|&(_, &score)| score != 0)
            .map(|(&pid, _)| pid)
            .collect();
        let slot_names = self.extract_match_roster_names(&scored_ids);
        let missing_ids: HashSet<u32> = scored_ids
            .iter()
            .filter(|player_id| !slot_names.contains_key(player_id))
            .copied()
            .collect();

        // 3. Build Player objects
        let mut players: Vec<Player> = slot_names
            .iter()
            .map(|(&slot, name)| Player {
                id: slot,
                name: name.clone(),
                team: None,
                faction: None,
                score: None,
                role: None,
                score_infantry: None,
                score_support: None,
                score_armor: None,
                score_air: None,
                score_capturing: None,
                score_fortification: None,
                score_transportation: None,
                score_repair: None,
                score_bridge_laying: None,
                score_unit_damage: None,
                score_tactical_aid: None,
                score_total: None,
                left_at_seconds: None,
            })
            .collect();

        // 4. Last resort placeholder for truly missing names
        let found_ids: HashSet<u32> = players.iter().map(|p| p.id).collect();
        for &pid in &missing_ids {
            if !found_ids.contains(&pid) {
                players.push(Player {
                    id: pid,
                    name: format!("Player {pid}"),
                    team: None,
                    faction: None,
                    score: None,
                    role: None,
                    score_infantry: None,
                    score_support: None,
                    score_armor: None,
                    score_air: None,
                    score_capturing: None,
                    score_fortification: None,
                    score_transportation: None,
                    score_repair: None,
                    score_bridge_laying: None,
                    score_unit_damage: None,
                    score_tactical_aid: None,
                    score_total: None,
                    left_at_seconds: None,
                });
            }
        }
        players.sort_by_key(|p| p.id);

        // 5. Assign scores, teams, and roles
        for player in &mut players {
            if let Some(&score) = player_score_map.get(&player.id) {
                player.score = Some(score);
            }
            if let Some(&team) = team_map.get(&player.id) {
                player.team = Some(team);
                player.faction = Some(faction_name(team).to_string());
            }
            if let Some(summary) = end_summaries.get(&player.id) {
                if !summary.role.is_empty() {
                    player.role = Some(summary.role.clone());
                } else if player.score.is_some_and(|score| score > 0) {
                    // The same result block records a role even when its four
                    // per-role totals are zero. Keep scored primary roles first.
                    player.role = summary.recorded_role.clone();
                }
                player.score_infantry = Some(summary.infantry);
                player.score_support = Some(summary.support);
                player.score_armor = Some(summary.armor);
                player.score_air = Some(summary.air);
                player.score_capturing = Some(summary.capturing);
                player.score_fortification = Some(summary.fortification);
                player.score_transportation = Some(summary.transportation);
                player.score_repair = Some(summary.repair);
                player.score_bridge_laying = Some(summary.bridge_laying);
                player.score_unit_damage = Some(summary.unit_damage);
                player.score_tactical_aid = Some(summary.tactical_aid);
                player.score_total = Some(summary.total);
            }
        }

        self.correct_vacant_result_names(&mut players);

        let recorder = self
            .extract_recorder_slot()
            .and_then(|slot| players.iter().find(|p| p.id == slot))
            .map(|p| p.name.clone());

        let timing = self.extract_match_timing();
        let duration = timing.match_elapsed_seconds;
        let winner_team = self.extract_winner();
        let winner = winner_team.map(|t| faction_name(t).to_string());

        // Assault drives `aFactor` from attacker progress rather than from the
        // POV team, so its value is not a control split — see the module docs.
        let bar = publishes_control_split(&game_info.game_mode)
            .then(|| self.extract_domination_sample())
            .flatten();
        let match_ending =
            self.classify_match_ending(&timing, winner_team, bar.map(|(_, value)| value));

        let (winner_dom_pct, loser_dom_pct) = match bar {
            Some((_, raw)) => (Some(raw.max(1.0 - raw)), Some(raw.min(1.0 - raw))),
            None => (None, None),
        };

        let (domination_shares, domination_anchor) =
            self.resolve_domination_shares(bar, &players, winner.as_deref());

        // Detect incomplete match results:
        // 1. No valid winner (TeamWins aTeam=0 or missing)
        // 2. Players with scores but no team assignment (late-join recorder)
        // 3. Winner exists but no opposing faction visible (opponent left, etc.)
        let has_unknown_scored = players
            .iter()
            .any(|p| p.score.is_some_and(|s| s > 0) && p.faction.is_none());
        let loser_missing = winner.is_some() && {
            let winner_ref = winner.as_deref();
            !players.iter().any(|p| {
                p.faction
                    .as_deref()
                    .is_some_and(|f| f != "Unknown" && f != "Spectator" && Some(f) != winner_ref)
            })
        };
        let incomplete = winner.is_none() || has_unknown_scored || loser_missing;

        // Result-roster correction must not alter historical POV or match inference.
        self.correct_zero_score_spectators(&mut players);
        self.mark_result_departures(&mut players);

        ReplayData {
            game_info,
            raw_server_flags,
            server_classification,
            players,
            player_scores,
            duration_seconds: duration,
            timing,
            match_ending,
            winner_domination_pct: winner_dom_pct,
            loser_domination_pct: loser_dom_pct,
            domination_shares,
            domination_anchor,
            winner,
            incomplete,
            recorder,
        }
    }
}

#[cfg(test)]
mod decompression_tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn limits(
        max_compressed_bytes: usize,
        max_decompressed_bytes: usize,
        max_zlib_chunks: usize,
        max_zlib_candidates: usize,
        max_zlib_work_bytes: usize,
    ) -> DecompressionLimits {
        DecompressionLimits {
            max_compressed_bytes,
            max_decompressed_bytes,
            max_zlib_chunks,
            max_zlib_candidates,
            max_zlib_work_bytes,
        }
    }

    fn replay_with_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut raw = vec![0u8; 19];
        for chunk in chunks {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(chunk).expect("compress fixture");
            raw.extend(encoder.finish().expect("finish fixture"));
        }
        raw
    }

    #[test]
    fn decompresses_complete_chunks_within_limits() {
        let raw = replay_with_chunks(&[vec![1; 32], vec![2; 48]]);
        let (metadata, all_data) =
            decompress_chunks_with_limits(&raw, limits(raw.len(), 80, 2, 2, raw.len()))
                .expect("valid replay chunks");

        assert_eq!(metadata, vec![1; 32]);
        assert_eq!(all_data.len(), 80);
    }

    #[test]
    fn rejects_aggregate_decompressed_output_over_limit() {
        let raw = replay_with_chunks(&[vec![1; 40], vec![2; 40]]);
        let error = decompress_chunks_with_limits(&raw, limits(raw.len(), 79, 2, 2, raw.len()))
            .expect_err("size limit");

        assert!(error.contains("decompressed size limit"));
    }

    #[test]
    fn rejects_too_many_zlib_chunks() {
        let raw = replay_with_chunks(&[vec![1; 8], vec![2; 8], vec![3; 8]]);
        let error = decompress_chunks_with_limits(&raw, limits(raw.len(), 24, 2, 3, raw.len()))
            .expect_err("chunk limit");

        assert!(error.contains("maximum of 2 zlib chunks"));
    }

    #[test]
    fn rejects_a_chunk_larger_than_the_chunk_buffer() {
        let raw = replay_with_chunks(&[vec![0; MAX_CHUNK_DECOMPRESSED_BYTES + 1]]);
        let error = decompress_chunks_with_limits(
            &raw,
            limits(
                raw.len(),
                DECOMPRESSION_LIMITS.max_decompressed_bytes,
                1,
                1,
                raw.len(),
            ),
        )
        .expect_err("chunk");

        assert!(error.contains("oversized or truncated zlib chunk"));
    }

    #[test]
    fn rejects_compressed_input_over_limit() {
        let raw = replay_with_chunks(&[vec![1; 8]]);
        let error = decompress_chunks_with_limits(&raw, limits(raw.len() - 1, 8, 1, 1, raw.len()))
            .expect_err("compressed size limit");

        assert!(error.contains("compressed size limit"));
    }

    #[test]
    fn rejects_oversized_file_before_reading_it_into_memory() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "wic-replay-size-limit-{}-{unique}.wicdemo",
            std::process::id()
        ));
        let file = File::create(&path).expect("create sparse replay fixture");
        file.set_len((MAX_REPLAY_COMPRESSED_BYTES + 1) as u64)
            .expect("size sparse replay fixture");
        drop(file);

        let error = read_replay_file(&path).expect_err("compressed file size limit");
        std::fs::remove_file(&path).expect("remove sparse replay fixture");
        assert!(error.contains("compressed size limit"));
    }

    #[test]
    fn native_paths_require_the_wicdemo_extension_case_insensitively() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "wic-replay-extension-{}-{unique}",
            std::process::id()
        ));
        for suffix in ["WICDEMO", "wicdemo.wicdemo"] {
            let path = base.with_extension(suffix);
            std::fs::write(&path, [0u8; 20]).expect("write replay extension fixture");
            assert_eq!(
                read_replay_file(&path).expect("accepted replay path").len(),
                20
            );
            std::fs::remove_file(path).expect("remove replay extension fixture");
        }

        for path in [
            base.with_extension("txt"),
            base.with_extension("wicdemo.txt"),
            base.clone(),
            base.with_file_name(".wicdemo"),
        ] {
            let error = read_replay_file(&path).expect_err("wrong replay extension");
            assert!(error.contains("must end in .wicdemo"), "{error}");
        }
    }

    #[test]
    fn replay_worker_limit_uses_all_detected_logical_processors() {
        let detected = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        assert_eq!(available_replay_workers(), detected);
    }

    #[test]
    fn rejects_excessive_failed_zlib_candidates() {
        let mut raw = vec![0u8; 19];
        raw.extend_from_slice(&[0x78, 0xda, 0xff, 0x78, 0xda, 0xff, 0x78, 0xda, 0xff]);
        let error = decompress_chunks_with_limits(&raw, limits(raw.len(), 64, 1, 2, raw.len()))
            .expect_err("candidate limit");

        assert!(error.contains("maximum of 2 zlib candidates"));
    }

    #[test]
    fn rejects_excessive_cumulative_zlib_work() {
        let raw = replay_with_chunks(&[vec![1; 32]]);
        let error = decompress_chunks_with_limits(&raw, limits(raw.len(), 32, 1, 1, 1))
            .expect_err("zlib work limit");

        assert!(error.contains("zlib work limit"));
    }
}

#[cfg(test)]
mod timeline_tests {
    use super::*;

    fn push_field(data: &mut Vec<u8>, hash: &[u8; 4], flag: u8, payload: &[u8]) {
        let total = u32::try_from(13 + payload.len()).expect("test field fits in u32");
        data.extend_from_slice(hash);
        data.extend_from_slice(&total.to_le_bytes());
        data.push(flag);
        data.extend_from_slice(&total.to_le_bytes());
        data.extend_from_slice(payload);
    }

    fn push_u32_field(data: &mut Vec<u8>, hash: &[u8; 4], value: u32) {
        data.extend_from_slice(hash);
        data.extend_from_slice(&BINTAG_SEP);
        data.push(0);
        data.extend_from_slice(&BINTAG_SEP);
        data.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i32_field(data: &mut Vec<u8>, hash: &[u8; 4], value: i32) {
        push_u32_field(data, hash, value as u32);
    }

    fn push_float_field(data: &mut Vec<u8>, hash: &[u8; 4], value: f32) {
        push_u32_field(data, hash, value.to_bits());
    }

    fn push_clock(data: &mut Vec<u8>, remaining: f32) {
        data.extend_from_slice(&HASH_SET_GAME_MODE_DATA_FLOAT);
        push_u32_field(data, &HASH_ADATA_TYPE, 1);
        push_float_field(data, &HASH_AFLOAT, remaining);
    }

    #[test]
    fn classifies_orthogonal_server_properties_without_guessing_ranked() {
        let mut metadata = Vec::new();
        push_u32_field(&mut metadata, &HASH_MY_FPM_MODE_FLAG, 1);
        push_u32_field(&mut metadata, &HASH_MY_MATCH_MODE_FLAG, 1);
        push_u32_field(&mut metadata, &HASH_MY_IS_TOURNAMENT_MATCH_FLAG, 0);
        push_u32_field(&mut metadata, &HASH_MY_IS_CLAN_MATCH_FLAG, 0);
        let mut full_data = metadata.clone();
        push_u32_field(&mut full_data, &HASH_MY_TYPE, 1);

        let parser = WicReplayParser::from_decompressed(metadata, full_data);
        let (raw, classification) = parser.extract_server_classification();

        assert_eq!(raw.few_player_mode_flag, Some(true));
        assert_eq!(raw.match_mode_flag, Some(true));
        assert_eq!(classification.few_player_mode, Some(true));
        assert_eq!(classification.match_mode, Some(false));
        assert_eq!(classification.has_bots, Some(true));
        assert_eq!(classification.ranked, None);
    }

    #[test]
    fn classifies_plain_match_mode_and_preserves_unknown_inputs() {
        let mut metadata = Vec::new();
        push_u32_field(&mut metadata, &HASH_MY_FPM_MODE_FLAG, 0);
        push_u32_field(&mut metadata, &HASH_MY_MATCH_MODE_FLAG, 1);
        push_u32_field(&mut metadata, &HASH_MY_IS_TOURNAMENT_MATCH_FLAG, 0);
        push_u32_field(&mut metadata, &HASH_MY_IS_CLAN_MATCH_FLAG, 1);
        let mut full_data = metadata.clone();
        push_u32_field(&mut full_data, &HASH_MY_TYPE, 0);

        let parser = WicReplayParser::from_decompressed(metadata, full_data);
        let (_, classification) = parser.extract_server_classification();
        assert_eq!(classification.match_mode, Some(true));
        assert_eq!(classification.has_bots, Some(false));
        assert_eq!(classification.clan_match, Some(true));

        let parser = WicReplayParser::from_decompressed(Vec::new(), Vec::new());
        let (_, classification) = parser.extract_server_classification();
        assert_eq!(classification.few_player_mode, None);
        assert_eq!(classification.match_mode, None);
        assert_eq!(classification.has_bots, None);
    }

    #[test]
    fn countdown_jitter_does_not_inflate_elapsed_time() {
        let raw = [(10, 1200.0), (20, 1199.0), (30, 1199.2), (40, 1198.0)];
        let (samples, phases) = build_clock_timeline(&raw);

        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.elapsed_seconds)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 1.0, 2.0]
        );
        assert_eq!(
            phases,
            vec![TimelinePhase {
                index: 0,
                start_seconds: 0.0,
                end_seconds: 2.0,
                initial_clock_seconds: 1200.0,
                final_clock_seconds: 1198.0,
            }]
        );
    }

    #[test]
    fn positive_clock_reset_starts_a_new_phase() {
        let raw = [(10, 600.0), (20, 1.0), (30, 600.1), (40, 599.1)];
        let (samples, phases) = build_clock_timeline(&raw);

        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].start_seconds, 0.0);
        assert_eq!(phases[0].end_seconds, 599.0);
        assert_eq!(phases[1].start_seconds, 599.0);
        assert!((phases[1].end_seconds - 600.0).abs() < 0.001);
        assert!((samples.last().expect("last clock").elapsed_seconds - 600.0).abs() < 0.001);
    }

    #[test]
    fn event_time_comes_from_the_enclosing_envelope() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 10.5, 1200.0);
        let second = data.len();
        push_clock_envelope(&mut data, 11.5, 1199.0);
        let end = data.len();

        let timeline = build_envelope_timeline(&data);

        assert_eq!(timeline.starts, vec![0, second]);
        assert_eq!(timeline.seconds, 11.5);
        assert_eq!(timeline.end_offset, data.len());
        // Any byte inside an envelope resolves to that envelope's timestamp,
        // whichever convention the call site addresses records by.
        assert_eq!(timeline.time_at(0), 10.5);
        assert_eq!(timeline.time_at(ENVELOPE_MESSAGE_OFFSET), 10.5);
        assert_eq!(timeline.time_at(second - 1), 10.5);
        assert_eq!(timeline.time_at(second), 11.5);
        assert_eq!(timeline.time_at(end), 11.5);
    }

    #[test]
    fn the_end_of_match_summary_cuts_the_chain() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 10.0, 1200.0);
        push_clock_envelope(&mut data, 900.0, 300.0);
        let summary = data.len();
        // The summary pass restarts the envelope clock at zero and repeats the
        // whole match.
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_clock_envelope(&mut data, 900.0, 300.0);

        let timeline = build_envelope_timeline(&data);

        assert_eq!(timeline.end_offset, summary);
        assert_eq!(timeline.seconds, 900.0);
        assert_eq!(timeline.starts.len(), 2);
    }

    #[test]
    fn ignores_pre_match_zero_clock_but_keeps_terminal_zero() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 0.0);
        push_clock(&mut full_data, 600.0);
        push_clock(&mut full_data, 0.0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);

        let timeline = parser.parse_timeline();

        assert_eq!(timeline.match_duration_seconds, 600.0);
        assert_eq!(timeline.initial_clock_seconds, Some(600.0));
        assert_eq!(timeline.final_clock_seconds, Some(0.0));
        assert_eq!(timeline.phases.len(), 1);
    }

    #[test]
    fn supports_replays_recorded_during_overtime() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 0.0);
        push_clock(&mut full_data, -6900.0);
        push_clock(&mut full_data, -6901.0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);

        let timeline = parser.parse_timeline();

        assert_eq!(timeline.match_duration_seconds, 1.0);
        assert_eq!(timeline.initial_clock_seconds, Some(-6900.0));
        assert_eq!(timeline.final_clock_seconds, Some(-6901.0));
    }

    /// Wrap a message body in an `Event` envelope, as the replay stream does.
    fn push_envelope(data: &mut Vec<u8>, message: &[u8; 4], time: f32, body: &[u8]) {
        let total = (ENVELOPE_MESSAGE_OFFSET + 4 + body.len()) as u32;
        data.extend_from_slice(&HASH_EVENT);
        data.extend_from_slice(&ENVELOPE_TYPE);
        data.push(ENVELOPE_ARRAY_FLAG);
        data.extend_from_slice(&total.to_le_bytes());
        data.extend_from_slice(&time.to_bits().to_le_bytes());
        data.extend_from_slice(message);
        data.extend_from_slice(body);
    }

    #[test]
    fn accepts_more_than_one_million_bounded_event_envelopes() {
        const ENVELOPE_COUNT: usize = 1_000_001;
        let mut data = Vec::with_capacity(ENVELOPE_COUNT * (ENVELOPE_MESSAGE_OFFSET + 4));
        for _ in 0..ENVELOPE_COUNT {
            push_envelope(&mut data, &HASH_TEAM_WINS, 0.0, &[]);
        }

        let timeline = build_envelope_timeline(&data);
        assert_eq!(timeline.starts.len(), ENVELOPE_COUNT);
        assert_eq!(timeline.end_offset, data.len());
    }

    fn push_clock_envelope(data: &mut Vec<u8>, time: f32, remaining: f32) {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ADATA_TYPE, 1);
        push_float_field(&mut body, &HASH_AFLOAT, remaining);
        push_envelope(data, &HASH_SET_GAME_MODE_DATA_FLOAT, time, &body);
    }

    fn change_honors_body(delta: f32) -> Vec<u8> {
        let mut body = Vec::new();
        push_float_field(&mut body, &HASH_ADELTA, delta);
        body
    }

    fn support_used_body(support_id: u32, position: [f32; 3]) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ANID, support_id);
        push_float_field(&mut body, &HASH_APOSITION_X, position[0]);
        push_float_field(&mut body, &HASH_APOSITION_Y, position[1]);
        push_float_field(&mut body, &HASH_APOSITION_Z, position[2]);
        body
    }

    fn ta_taunt_body(
        player_id: u32,
        target_player_id: u32,
        ta_index: u32,
        upgrade_level: u32,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_APLAYER_FROM, player_id);
        push_u32_field(&mut body, &HASH_APLAYER_TAUNTED, target_player_id);
        push_u32_field(&mut body, &HASH_ATA_ID, ta_index);
        push_u32_field(&mut body, &HASH_ASUPPORT_UPGRADE_LEVEL, upgrade_level);
        body
    }

    fn simple_unit_create_body(player_id: u32, team: u32, unit_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_APERSISTENCE_KEY, unit_id);
        push_u32_field(&mut body, &HASH_APLAYER, player_id);
        push_u32_field(&mut body, &HASH_ATEAM, team);
        push_u32_field(&mut body, &HASH_AUNIT, unit_id);
        push_u32_field(&mut body, &HASH_ATYPE, 0x1000 + unit_id);
        body
    }

    fn homing_unit_projectile_body(firing_unit_id: u32, target_unit_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ANID, 1);
        for hash in [
            HASH_APOSITION_X,
            HASH_APOSITION_Y,
            HASH_APOSITION_Z,
            HASH_ADIRECTION_X,
            HASH_ADIRECTION_Y,
            HASH_ADIRECTION_Z,
        ] {
            push_float_field(&mut body, &hash, 0.0);
        }
        push_u32_field(&mut body, &HASH_ASLOT, 0);
        push_float_field(&mut body, &HASH_ATIME_SINCE_CREATION, 0.0);
        push_u32_field(&mut body, &HASH_ATARGET, target_unit_id);
        push_u32_field(&mut body, &HASH_AUNIT, firing_unit_id);
        body
    }

    fn homing_support_projectile_body(support_id: u32, target_unit_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ANID, 1);
        push_u32_field(&mut body, &HASH_ASUPPORT_THING, support_id);
        for hash in [
            HASH_APOSITION_X,
            HASH_APOSITION_Y,
            HASH_APOSITION_Z,
            HASH_ADIRECTION_X,
            HASH_ADIRECTION_Y,
            HASH_ADIRECTION_Z,
        ] {
            push_float_field(&mut body, &hash, 0.0);
        }
        push_float_field(&mut body, &HASH_AFACTOR, 0.0);
        push_u32_field(&mut body, &HASH_AUNIT, target_unit_id);
        push_u32_field(&mut body, &HASH_ASUPPORT_UPGRADE_LEVEL, 0);
        body
    }

    fn unit_destroy_body(unit_id: u32, killer_unit_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_AUNIT, unit_id);
        push_u32_field(&mut body, &HASH_AKILLER, killer_unit_id);
        push_u32_field(&mut body, &HASH_AKILLER_EXPERIENCE, 0);
        body
    }

    fn unit_destroy_body_with_direction(
        unit_id: u32,
        killer_unit_id: u32,
        direction: [f32; 3],
    ) -> Vec<u8> {
        let mut body = unit_destroy_body(unit_id, killer_unit_id);
        push_float_field(&mut body, &HASH_AHIT_DIRECTION_X, direction[0]);
        push_float_field(&mut body, &HASH_AHIT_DIRECTION_Y, direction[1]);
        push_float_field(&mut body, &HASH_AHIT_DIRECTION_Z, direction[2]);
        push_float_field(&mut body, &HASH_AN_EXPLOSION_FORCE, 0.0);
        body
    }

    fn building_slot_state_body(
        building_id: u32,
        slot_id: i32,
        will_occupy: bool,
        unit_id: u32,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ABUILDING_NAME, building_id);
        push_i32_field(&mut body, &HASH_AREAL_SLOT_ID, slot_id);
        push_u32_field(
            &mut body,
            &HASH_AWILL_OCCUPY_SLOT_FLAG,
            u32::from(will_occupy),
        );
        push_u32_field(&mut body, &HASH_AUNIT, unit_id);
        body
    }

    fn building_damaged_body(building_id: u32, health: i32, state: i32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ANAME, building_id);
        push_i32_field(&mut body, &HASH_AHEALTH, health);
        push_i32_field(&mut body, &HASH_ASTATE, state);
        push_u32_field(&mut body, &HASH_AFLAG, 1);
        body
    }

    fn unit_relation_body(relation_type: u32, first_unit: u32, second_unit: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ATYPE, relation_type);
        push_u32_field(&mut body, &HASH_AFIRST_UNIT, first_unit);
        push_u32_field(&mut body, &HASH_ASECOND_UNIT, second_unit);
        body
    }

    fn unit_remove_body(unit_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_AUNIT, unit_id);
        body
    }

    fn support_marker_body(
        event_id: u32,
        support_id: u32,
        position: [f32; 3],
        player_id: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        duration_seconds: f32,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_AN_EVENT_ID, event_id);
        push_u32_field(&mut body, &HASH_ANID, support_id);
        push_float_field(&mut body, &HASH_APOSITION_X, position[0]);
        push_float_field(&mut body, &HASH_APOSITION_Y, position[1]);
        push_float_field(&mut body, &HASH_APOSITION_Z, position[2]);
        push_u32_field(&mut body, &HASH_ATEAM, player_id);
        push_u32_field(&mut body, &HASH_A_SUPPORT_UPPGRADE_LEVEL, upgrade_level);
        push_float_field(&mut body, &HASH_ADIRECTION_X, direction[0]);
        push_float_field(&mut body, &HASH_ADIRECTION_Y, direction[1]);
        push_float_field(&mut body, &HASH_ADIRECTION_Z, direction[2]);
        push_float_field(&mut body, &HASH_ATIME, duration_seconds);
        body
    }

    fn support_spawned_delayed_body(
        support_id: u32,
        position: [f32; 3],
        team: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        age_seconds: f32,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ANID, support_id);
        push_float_field(&mut body, &HASH_APOSITION_X, position[0]);
        push_float_field(&mut body, &HASH_APOSITION_Y, position[1]);
        push_float_field(&mut body, &HASH_APOSITION_Z, position[2]);
        push_u32_field(&mut body, &HASH_ATEAM, team);
        push_u32_field(&mut body, &HASH_A_SUPPORT_UPPGRADE_LEVEL, upgrade_level);
        push_float_field(&mut body, &HASH_ADIRECTION_X, direction[0]);
        push_float_field(&mut body, &HASH_ADIRECTION_Y, direction[1]);
        push_float_field(&mut body, &HASH_ADIRECTION_Z, direction[2]);
        push_float_field(&mut body, &HASH_ATIME_SINCE_CREATION, age_seconds);
        body
    }

    fn unit_create_body(
        player_id: u32,
        team: u32,
        unit_id: u32,
        unit_type_id: u32,
        position: [f32; 3],
        spawn_source: u32,
    ) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_APERSISTENCE_KEY, 1);
        push_u32_field(&mut body, &HASH_APLAYER, player_id);
        push_u32_field(&mut body, &HASH_ATEAM, team);
        push_u32_field(&mut body, &HASH_AUNIT, unit_id);
        push_u32_field(&mut body, &HASH_ATYPE, unit_type_id);
        push_float_field(&mut body, &HASH_APOSITION_X, position[0]);
        push_float_field(&mut body, &HASH_APOSITION_Y, position[1]);
        push_float_field(&mut body, &HASH_APOSITION_Z, position[2]);
        // The real field immediately before aSpawnSource is a one-byte boolean;
        // retaining it here guards the non-17-byte-stride lookup.
        push_field(&mut body, &HASH_AN_IS_REPLACEMENT_UNIT_FLAG, 3, &[0]);
        push_u32_field(&mut body, &HASH_ASPAWN_SOURCE, spawn_source);
        body
    }

    #[test]
    fn rejects_unit_drop_attribution_work_over_the_budget() {
        const SUPPORT_ID: u32 = 0x2a64_0597;
        const UNIT_TYPE_ID: u32 = 0x3974_0697;
        let mut data = Vec::new();
        for index in 0..2 {
            push_envelope(
                &mut data,
                &HASH_UNIT_CREATE,
                30.0 + index as f32,
                &unit_create_body(index, 1, 100 + index, UNIT_TYPE_ID, [0.0; 3], 1),
            );
        }
        for index in 0..2 {
            push_envelope(
                &mut data,
                &HASH_SUPPORT_THING_SPAWNED_DELAYED,
                10.0 + index as f32,
                &support_spawned_delayed_body(SUPPORT_ID, [0.0; 3], 1, 0, [0.0, 0.0, 1.0], 0.0),
            );
        }

        let index = EventIndex::new(&data).expect("event index");
        let error = validate_unit_drop_attribution_work_with_limit(&data, &index, 3)
            .expect_err("unit-drop comparison limit");
        assert!(error.contains("maximum of 3 unit-drop attribution comparisons"));
    }

    fn spectator_joined_team_body(player_id: u32, team: u32, spectator_los: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ASLOT, player_id);
        push_u32_field(&mut body, &HASH_ATEAM, team);
        push_u32_field(&mut body, &HASH_ASPECTATOR_LOS, spectator_los);
        body
    }

    fn chat_body(player_id: u32, message: &str, channel: ChatChannel) -> Vec<u8> {
        let mut body = Vec::new();
        push_field(&mut body, &HASH_APLAYER, 1, &player_id.to_le_bytes());
        let mut encoded = Vec::new();
        for unit in message.encode_utf16().chain(std::iter::once(0)) {
            encoded.extend_from_slice(&unit.to_le_bytes());
        }
        push_field(&mut body, &HASH_AMESSAGE, 5, &encoded);
        match channel {
            ChatChannel::All => push_field(&mut body, &HASH_ATEAM_CHAT, 3, &[0]),
            ChatChannel::Team => push_field(&mut body, &HASH_ATEAM_CHAT, 3, &[1]),
        }
        body
    }

    fn bot_response_body(player_id: u32, message: &str) -> Vec<u8> {
        let mut body = Vec::new();
        push_field(&mut body, &HASH_APLAYER, 1, &player_id.to_le_bytes());
        let mut encoded = Vec::new();
        for unit in message.encode_utf16().chain(std::iter::once(0)) {
            encoded.extend_from_slice(&unit.to_le_bytes());
        }
        push_field(&mut body, &HASH_AMESSAGE, 5, &encoded);
        body
    }

    fn push_tactical_aid_use(data: &mut Vec<u8>, cost: f32, support_id: u32, position: [f32; 3]) {
        push_tactical_aid_use_at(data, 0.0, cost, support_id, position);
    }

    fn push_tactical_aid_use_at(
        data: &mut Vec<u8>,
        seconds: f32,
        cost: f32,
        support_id: u32,
        position: [f32; 3],
    ) {
        push_envelope(
            data,
            &HASH_CHANGE_HONORS,
            seconds,
            &change_honors_body(-cost),
        );
        push_envelope(
            data,
            &HASH_SUPPORT_THING_USED,
            seconds,
            &support_used_body(support_id, position),
        );
    }

    fn tactical_aid_events_for(metadata: Vec<u8>, full_data: Vec<u8>) -> Vec<TimelineEvent> {
        let parser = WicReplayParser::from_decompressed(metadata, full_data);
        parser
            .parse_timeline()
            .events
            .into_iter()
            .filter(|event| matches!(event, TimelineEvent::TacticalAidUsed { .. }))
            .collect()
    }

    /// Metadata chunk carrying just the point-of-view slot.
    fn metadata_with_recorder(slot: u32) -> Vec<u8> {
        let mut metadata = Vec::new();
        push_u32_field(&mut metadata, &HASH_POV_PLAYER, slot);
        metadata
    }

    fn metadata_with_player_names(players: &[(u32, &str)]) -> Vec<u8> {
        let mut metadata = Vec::new();
        for &(slot, name) in players {
            push_u32_field(&mut metadata, &HASH_ASLOT, slot);
            metadata.extend_from_slice(&[0; 30]);
            for unit in name.encode_utf16().chain(std::iter::once(0)) {
                metadata.extend_from_slice(&unit.to_le_bytes());
            }
        }
        metadata
    }

    fn player_entry_body(player_id: u32, player_name: &str) -> Vec<u8> {
        const HASH_UNKNOWN_ENTRY_ATTRIBUTE: [u8; 4] = [0xa8, 0x01, 0x32, 0x04];
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ASLOT, player_id);
        push_u32_field(&mut body, &HASH_UNKNOWN_ENTRY_ATTRIBUTE, 0);
        let mut encoded = player_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        encoded.extend_from_slice(&[0, 0]);
        push_field(&mut body, &HASH_PLAYER_ENTRY_NAME, 5, &encoded);
        body
    }

    fn player_leave_body(player_id: u32) -> Vec<u8> {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ASLOT, player_id);
        body
    }

    #[test]
    fn resolves_reused_slot_through_time_bounded_player_sessions() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.1,
            &player_entry_body(1, "Original"),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_LEAVES_GAME,
            0.2,
            &player_leave_body(1),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.3,
            &player_entry_body(1, "Replacement\u{a0}\u{a0}"),
        );
        push_clock_envelope(&mut full_data, 1.0, 100.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            2.0,
            &chat_body(1, "replacement speaking", ChatChannel::All),
        );
        push_clock_envelope(&mut full_data, 3.0, 99.0);

        let timeline = WicReplayParser::from_decompressed(metadata, full_data).parse_timeline();

        assert_eq!(
            timeline.participants,
            vec![TimelineParticipant {
                player_id: 1,
                player_name: Some("Original".to_string()),
            }]
        );
        assert_eq!(timeline.participant_sessions.len(), 2);
        assert_eq!(
            timeline.participant_sessions[0].player_name.as_deref(),
            Some("Original")
        );
        // The first occupant's session closes at the replacement's entry
        // envelope. Under the countdown axis this clamped to zero.
        assert_eq!(timeline.participant_sessions[0].end_seconds, Some(0.3));
        assert_eq!(
            timeline.participant_sessions[1].player_name.as_deref(),
            Some("Replacement")
        );
        let chat = timeline.events.iter().find_map(|event| match event {
            TimelineEvent::ChatMessage { player_name, .. } => player_name.as_deref(),
            _ => None,
        });
        assert_eq!(chat, Some("Replacement"));
    }

    #[test]
    fn repeated_unresolved_entries_create_distinct_unknown_sessions() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        let mut unresolved_body = Vec::new();
        push_u32_field(&mut unresolved_body, &HASH_ASLOT, 1);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.1,
            &unresolved_body,
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.2,
            &unresolved_body,
        );
        push_clock_envelope(&mut full_data, 1.0, 100.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            1.1,
            &chat_body(1, "unknown speaking", ChatChannel::All),
        );

        let timeline = WicReplayParser::from_decompressed(metadata, full_data).parse_timeline();
        assert_eq!(timeline.participant_sessions.len(), 3);
        assert_eq!(timeline.participant_sessions[1].player_name, None);
        assert_eq!(timeline.participant_sessions[1].end_seconds, Some(0.2));
        assert_eq!(timeline.participant_sessions[2].player_name, None);
    }

    #[test]
    fn out_of_bounds_field_candidate_does_not_discard_an_earlier_valid_value() {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ASPAWN_SOURCE, 7);
        body.extend_from_slice(&HASH_ASPAWN_SOURCE);
        let mut data = Vec::new();
        push_envelope(&mut data, &HASH_UNIT_CREATE, 0.0, &body);
        data.extend_from_slice(&BINTAG_SEP);
        data.push(0);
        data.extend_from_slice(&BINTAG_SEP);
        data.extend_from_slice(&99_u32.to_le_bytes());

        assert_eq!(
            read_unique_u32_field_in_event(&data, ENVELOPE_MESSAGE_OFFSET, &HASH_ASPAWN_SOURCE),
            Some(7)
        );
    }

    fn match_roster_name(metadata: Vec<u8>, full_data: Vec<u8>) -> Option<String> {
        let parser = WicReplayParser::from_decompressed(metadata, full_data);
        parser
            .extract_match_roster_names(&HashSet::from([1]))
            .remove(&1)
    }

    #[test]
    fn match_roster_uses_replacement_who_entered_before_gameplay() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_LEAVES_GAME,
            0.2,
            &player_leave_body(1),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.3,
            &player_entry_body(1, "Replacement\u{a0}\u{a0}"),
        );
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_envelope(&mut full_data, &HASH_TEAM_WINS, 2.0, &[]);

        assert_eq!(
            match_roster_name(metadata, full_data).as_deref(),
            Some("Replacement")
        );
    }

    #[test]
    fn match_roster_does_not_apply_replacement_after_gameplay_started() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_LEAVES_GAME,
            2.0,
            &player_leave_body(1),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            3.0,
            &player_entry_body(1, "Replacement"),
        );
        push_clock_envelope(&mut full_data, 4.0, 1199.0);
        push_envelope(&mut full_data, &HASH_TEAM_WINS, 5.0, &[]);

        assert_eq!(
            match_roster_name(metadata, full_data).as_deref(),
            Some("Original")
        );
    }

    #[test]
    fn match_roster_preserves_metadata_when_scored_slot_is_vacant_at_gameplay_start() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_LEAVES_GAME,
            0.2,
            &player_leave_body(1),
        );
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_envelope(&mut full_data, &HASH_TEAM_WINS, 2.0, &[]);

        assert_eq!(
            match_roster_name(metadata, full_data).as_deref(),
            Some("Original")
        );
    }

    #[test]
    fn match_roster_preserves_static_name_without_a_gameplay_clock() {
        let metadata = metadata_with_player_names(&[(1, "Original")]);
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_LEAVES_GAME,
            0.2,
            &player_leave_body(1),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            0.3,
            &player_entry_body(1, "Replacement"),
        );

        assert_eq!(
            match_roster_name(metadata, full_data).as_deref(),
            Some("Original")
        );
    }

    #[test]
    fn late_lobby_name_correction_preserves_unrelated_roster_cases() {
        for case in [
            "sole entrant",
            "negative role",
            "active at start",
            "two entrants",
            "missing role",
            "postmatch team mismatch",
            "missing result",
            "unscored",
            "postmatch entrant",
        ] {
            let metadata = metadata_with_player_names(&[(1, "Original")]);
            let mut data = Vec::new();
            push_envelope(
                &mut data,
                &HASH_PLAYER_LEAVES_GAME,
                if case == "active at start" { 1.5 } else { 0.2 },
                &player_leave_body(1),
            );
            // Keep the envelope chain in chronological order for the active control.
            if case == "active at start" {
                data.clear();
                push_clock_envelope(&mut data, 1.0, 1200.0);
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    1.5,
                    &player_leave_body(1),
                );
            } else {
                push_clock_envelope(&mut data, 1.0, 1200.0);
            }
            if case == "postmatch entrant" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 1.8, &[]);
            }
            push_envelope(
                &mut data,
                &HASH_PLAYER_ENTERS_GAME,
                2.0,
                &player_entry_body(1, "Replacement"),
            );
            push_joined_team(&mut data, 2.1, 1, 3);
            if case == "two entrants" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    2.5,
                    &player_entry_body(1, "Second"),
                );
            }
            if case != "missing result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 3.0, &[]);
            }
            if case == "postmatch team mismatch" {
                push_joined_team(&mut data, 3.5, 1, 2);
            }
            let summary = data.len();
            push_u32_field(&mut data, &HASH_APOS, 1);
            if case != "missing role" {
                push_i32_field(
                    &mut data,
                    &HASH_SCORE_ROLE0,
                    if case == "negative role" { -12 } else { 247 },
                );
            }
            data.resize(summary + 300, 0);
            let parser = WicReplayParser::from_decompressed(metadata, data);
            let scored = if case == "unscored" {
                HashSet::new()
            } else {
                HashSet::from([1])
            };
            if case == "negative role" {
                assert_eq!(parser.extract_player_end_summaries()[&1].role, "infantry");
            }
            let names = parser.extract_match_roster_names(&scored);
            assert_eq!(
                names[&1],
                if matches!(case, "sole entrant" | "negative role") {
                    "Replacement"
                } else {
                    "Original"
                },
                "{case}"
            );
        }
    }

    #[test]
    fn result_evidence_requires_session_activity_and_preserves_final_results() {
        for case in [
            "departed",
            "departed last role",
            "departed unknown role",
            "departed FPM role",
            "departed malformed role",
            "departed truncated role",
            "departed lobby role",
            "departed role before named entry",
            "departed post-result role",
            "decreased score",
            "zero last score",
            "negative last score",
            "no reset",
            "reset after result",
            "positive after leave",
            "no departure",
            "unframed departure",
            "implicit departure",
            "spectator replacement",
            "playing replacement",
            "other occupant role",
            "returned",
            "unknown occupant",
            "no units",
            "wrong unit team",
            "no entry",
            "late recording",
            "missing result",
            "positive final score",
            "missing stats",
            "summary stats",
            "existing role",
            "unknown team",
            "bad score flag",
            "bad slot flag",
            "role",
            "different summary role",
            "unknown summary role",
            "role after units",
            "switched role",
            "no selection",
        ] {
            let role_case = [
                "role",
                "different summary role",
                "unknown summary role",
                "role after units",
                "switched role",
                "no selection",
            ]
            .contains(&case);
            let mut data = Vec::new();
            if case != "late recording" {
                push_clock_envelope(&mut data, 0.0, 0.0);
            }
            if case == "departed role before named entry" {
                let mut body = player_leave_body(3);
                push_u32_field(&mut body, &HASH_AROLE_ID, 0x1abf_03df);
                push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 0.05, &body);
            }
            if case != "no entry" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.1,
                    &player_entry_body(3, "Hunter"),
                );
            }
            push_joined_team(&mut data, 0.2, 3, 3);
            if case == "departed lobby role" {
                let mut body = player_leave_body(3);
                push_u32_field(&mut body, &HASH_AROLE_ID, 0x1abf_03df);
                push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 0.3, &body);
            }
            push_clock_envelope(&mut data, 1.0, 1200.0);
            push_clock_envelope(&mut data, 1.1, 1199.9);
            push_clock_envelope(&mut data, 1.2, 1199.8);
            let role = |data: &mut Vec<u8>, time: f32, id: u32| {
                let mut body = player_leave_body(3);
                push_u32_field(&mut body, &HASH_AROLE_ID, id);
                push_envelope(data, &HASH_PLAYER_SET_ROLE, time, &body);
            };
            if role_case && !["role after units", "no selection"].contains(&case) {
                role(&mut data, 2.0, 0x0ae8_026e);
            }
            if case != "no units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    3.0,
                    &simple_unit_create_body(3, if case == "wrong unit team" { 1 } else { 3 }, 99),
                );
            }
            if case.starts_with("departed ")
                && !["departed lobby role", "departed role before named entry"].contains(&case)
            {
                role(&mut data, 3.1, 0x0ae8_026e);
                let id = match case {
                    "departed unknown role" => 42,
                    "departed FPM role" => ROLE_ID_FEW_PLAYER_MODE,
                    _ => 0x10e0_0313,
                };
                if ["departed malformed role", "departed truncated role"].contains(&case) {
                    let mut body = player_leave_body(3);
                    push_u32_field(&mut body, &HASH_AROLE_ID, id);
                    if case == "departed malformed role" {
                        body[17] ^= 1;
                    } else {
                        body.truncate(21);
                    }
                    push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 3.2, &body);
                } else {
                    role(&mut data, 3.2, id);
                }
            }
            if case == "role after units" {
                role(&mut data, 3.1, 0x0ae8_026e);
            }
            if case == "switched role" {
                role(&mut data, 3.1, 0x10e0_0313);
            }
            let score = |data: &mut Vec<u8>, time: f32, value: i32| {
                let mut body = Vec::new();
                push_u32_field(&mut body, &HASH_APOS, 3);
                body[8] = if case == "bad slot flag" { 0 } else { 1 };
                push_i32_field(&mut body, &HASH_SCORE, value);
                if case == "bad score flag" {
                    body[25] = 1;
                }
                push_envelope(data, &HASH_SET_SCORE, time, &body);
            };
            if !role_case {
                score(&mut data, 3.5, 200);
                score(
                    &mut data,
                    4.0,
                    match case {
                        "zero last score" => 0,
                        "negative last score" => -5,
                        _ => 150,
                    },
                );
                if !["no departure", "unframed departure", "implicit departure"].contains(&case) {
                    push_envelope(
                        &mut data,
                        &HASH_PLAYER_LEAVES_GAME,
                        5.0,
                        &player_leave_body(3),
                    );
                }
                if !["no reset", "reset after result"].contains(&case) {
                    score(
                        &mut data,
                        5.1,
                        if case == "positive after leave" {
                            10
                        } else {
                            0
                        },
                    );
                }
                if [
                    "implicit departure",
                    "spectator replacement",
                    "playing replacement",
                    "other occupant role",
                    "returned",
                    "unknown occupant",
                ]
                .contains(&case)
                {
                    let mut body = player_entry_body(
                        3,
                        if case == "returned" {
                            "Hunter"
                        } else {
                            "Other"
                        },
                    );
                    if case == "unknown occupant" {
                        body.truncate(34);
                    }
                    push_envelope(&mut data, &HASH_PLAYER_ENTERS_GAME, 5.2, &body);
                    if case == "playing replacement" {
                        push_envelope(
                            &mut data,
                            &HASH_UNIT_CREATE,
                            5.3,
                            &simple_unit_create_body(3, 3, 100),
                        );
                    }
                    if case == "other occupant role" {
                        role(&mut data, 5.3, 0x0ae8_026e);
                    }
                }
            }
            if case != "missing result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 6.0, &[]);
            }
            if case == "departed post-result role" {
                role(&mut data, 6.1, 0x1abf_03df);
            }
            if case == "reset after result" {
                score(&mut data, 6.1, 0);
            }
            if case == "unframed departure" {
                data.extend_from_slice(&HASH_PLAYER_LEAVES_GAME);
                data.extend_from_slice(&player_leave_body(3));
            }
            if role_case {
                let offset = data.len();
                push_u32_field(&mut data, &HASH_APOS, 3);
                push_u32_field(
                    &mut data,
                    &HASH_AROLE_ID,
                    match case {
                        "different summary role" => 0x10e0_0313,
                        "unknown summary role" => 42,
                        _ => 0x0ae8_026e,
                    },
                );
                data.resize(offset + 300, 0);
            }
            let parser = WicReplayParser::from_decompressed(
                metadata_with_player_names(&[(3, "Hunter")]),
                data,
            );
            let zero = Player {
                id: 3,
                name: "Hunter".to_owned(),
                team: Some(3),
                faction: Some("USSR".to_owned()),
                role: None,
                score: Some(0),
                score_infantry: Some(0),
                score_support: Some(0),
                score_armor: Some(0),
                score_air: Some(0),
                score_capturing: Some(0),
                score_fortification: Some(0),
                score_transportation: Some(0),
                score_repair: Some(0),
                score_bridge_laying: Some(0),
                score_unit_damage: Some(0),
                score_tactical_aid: Some(0),
                score_total: Some(0),
                left_at_seconds: None,
            };
            let mut player = zero;
            match case {
                "positive final score" => player.score = Some(100),
                "missing stats" => player.score_total = None,
                "summary stats" => player.score_air = Some(1),
                "existing role" => player.role = Some("air".to_owned()),
                "unknown team" => player.team = None,
                _ => {}
            }
            let before = serde_json::to_string(&player).unwrap();
            let result = parser.player_result_evidence(std::slice::from_ref(&player));
            if case.starts_with("departed")
                || ["decreased score", "spectator replacement"].contains(&case)
            {
                assert_eq!(result.len(), 1, "{case}");
                assert_eq!(
                    result[0].score_before_leave.as_ref().unwrap().score,
                    150,
                    "last observation, not max: {case}"
                );
            } else {
                assert!(result.is_empty(), "{case}: {result:?}");
            }
            assert_eq!(
                serde_json::to_string(&player).unwrap(),
                before,
                "raw results: {case}"
            );
        }
    }

    #[test]
    fn abandoned_lobby_duplicates_require_departure_and_active_match_evidence() {
        for case in [
            "duplicate",
            "positive old score",
            "missing old score",
            "missing summary",
            "old role",
            "old units",
            "no departure",
            "late departure",
            "reoccupied",
            "different name",
            "different team",
            "no active units",
            "no active entry",
            "no old entry",
            "late recording",
            "active left",
            "missing result",
            "ambiguous active",
            "unframed departure",
        ] {
            let mut data = Vec::new();
            if case != "late recording" {
                push_clock_envelope(&mut data, 0.0, 0.0);
            }
            if case != "no old entry" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.1,
                    &player_entry_body(3, "Hunter"),
                );
            }
            push_joined_team(&mut data, 0.2, 3, 3);
            if case == "old role" {
                let mut body = player_leave_body(3);
                push_u32_field(&mut body, &HASH_AROLE_ID, 0x10e0_0313);
                push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 0.3, &body);
            }
            if case == "old units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    0.3,
                    &simple_unit_create_body(3, 3, 1),
                );
            }
            if !["no departure", "late departure", "unframed departure"].contains(&case) {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    0.4,
                    &player_leave_body(3),
                );
            }
            if case != "no active entry" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.5,
                    &player_entry_body(8, "Hunter"),
                );
            }
            push_joined_team(&mut data, 0.6, 8, 3);
            push_clock_envelope(&mut data, 1.0, 1200.0);
            push_clock_envelope(&mut data, 1.1, 1199.9);
            push_clock_envelope(&mut data, 1.2, 1199.8);
            if case != "no active units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    1.3,
                    &simple_unit_create_body(8, 3, 2),
                );
            }
            if case == "reoccupied" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    1.4,
                    &player_entry_body(3, "Other"),
                );
            }
            if case == "late departure" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    1.5,
                    &player_leave_body(3),
                );
            }
            if case == "active left" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    1.5,
                    &player_leave_body(8),
                );
            }
            if case != "missing result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 2.0, &[]);
            }
            if case == "unframed departure" {
                data.extend_from_slice(&HASH_PLAYER_LEAVES_GAME);
                data.extend_from_slice(&player_leave_body(3));
            }
            let parser = WicReplayParser::from_decompressed(
                metadata_with_player_names(&[(3, "Hunter"), (8, "Hunter")]),
                data,
            );
            let zero = Player {
                id: 3,
                name: "Hunter".to_owned(),
                team: Some(3),
                faction: Some("USSR".to_owned()),
                role: None,
                score: Some(0),
                score_infantry: Some(0),
                score_support: Some(0),
                score_armor: Some(0),
                score_air: Some(0),
                score_capturing: Some(0),
                score_fortification: Some(0),
                score_transportation: Some(0),
                score_repair: Some(0),
                score_bridge_laying: Some(0),
                score_unit_damage: Some(0),
                score_tactical_aid: Some(0),
                score_total: Some(0),
                left_at_seconds: None,
            };
            let active = Player {
                id: 8,
                score: Some(1024),
                role: Some("air".to_owned()),
                ..zero.clone()
            };
            let mut players = vec![zero, active];
            match case {
                "positive old score" => players[0].score = Some(1),
                "missing old score" => players[0].score = None,
                "missing summary" => players[0].score_total = None,
                "different name" => players[1].name = "Other".to_owned(),
                "different team" => players[1].team = Some(2),
                "ambiguous active" => players.push(Player {
                    id: 9,
                    ..players[1].clone()
                }),
                _ => {}
            }
            let before = format!("{players:?}");
            assert_eq!(
                parser.abandoned_lobby_duplicate_slots(&players),
                if case == "duplicate" { vec![3] } else { vec![] },
                "{case}"
            );
            assert_eq!(format!("{players:?}"), before, "raw rows preserved: {case}");
        }
    }

    #[test]
    fn final_screen_uses_present_occupants_and_its_own_score_table() {
        fn team_join_body(slot: u32, team: u32) -> Vec<u8> {
            let mut body = Vec::new();
            push_u32_field(&mut body, &HASH_ASLOT, slot);
            push_u32_field(&mut body, &HASH_ATEAM, team);
            body
        }
        fn entry(data: &mut Vec<u8>, slot: u32, name: &str, kind: u32) {
            let mut body = player_entry_body(slot, name);
            push_u32_field(&mut body, &[0, 0, 0, 0], 1); // readyFlag
            push_field(&mut body, &HASH_MY_TYPE, 1, &kind.to_le_bytes());
            push_envelope(data, &HASH_PLAYER_ENTERS_GAME, 1.0, &body);
        }
        fn summary(data: &mut Vec<u8>, slot: u32, total: i32) {
            let fields = [
                HASH_APOS,
                HASH_AROLE_ID,
                HASH_TOTAL_SCORE,
                HASH_CAPTURING_SCORE,
                HASH_FORTIFICATION_SCORE,
                HASH_TRANSPORTATION_SCORE,
                HASH_REPAIR_SCORE,
                HASH_BRIDGE_LAYING_SCORE,
                HASH_UNIT_DAMAGE_SCORE,
                HASH_TACTICAL_AID_SCORE,
                HASH_SCORE_ROLE0,
                HASH_SCORE_ROLE1,
                HASH_SCORE_ROLE2,
                HASH_SCORE_ROLE3,
            ];
            let mut body = Vec::new();
            for (i, hash) in fields.iter().enumerate() {
                let v = if i == 0 {
                    slot as i32
                } else if i == 1 {
                    0x0ae8026e
                } else if i == 2 || i == 13 {
                    total
                } else {
                    0
                };
                push_field(
                    &mut body,
                    hash,
                    if i == 1 || i == 2 { 0 } else { 1 },
                    &v.to_le_bytes(),
                );
            }
            push_envelope(data, &[0x6f, 0x06, 0x31, 0x3a], 2.0, &body);
        }
        for case in [
            "normal",
            "missing result",
            "missing table",
            "missing entry",
            "missing row",
            "duplicate row",
            "malformed",
            "spectator",
            "unknown",
            "replacement",
            "bad name",
            "overflow",
            "omitted replacement name",
            "compressed gap",
            "damaged table",
            "post result gap",
        ] {
            let mut data = Vec::new();
            if case != "missing entry" {
                entry(
                    &mut data,
                    0,
                    if case == "bad name" { "" } else { "Original" },
                    0,
                );
            }
            if case == "replacement" || case == "omitted replacement name" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    1.0,
                    &player_leave_body(0),
                );
                if case == "omitted replacement name" {
                    let mut body = Vec::new();
                    push_u32_field(&mut body, &HASH_ASLOT, 0);
                    push_u32_field(&mut body, &[0xa8, 0x01, 0x32, 0x04], 3);
                    push_u32_field(&mut body, &[0; 4], 1);
                    push_u32_field(&mut body, &HASH_MY_TYPE, 0);
                    push_envelope(&mut data, &HASH_PLAYER_ENTERS_GAME, 1.0, &body);
                } else {
                    entry(&mut data, 0, "Replacement", 0);
                }
            }
            entry(&mut data, 1, "Departed", 0);
            push_envelope(
                &mut data,
                &HASH_PLAYER_LEAVES_GAME,
                1.0,
                &player_leave_body(1),
            );
            if case == "missing row" {
                entry(&mut data, 2, "Missing", 0);
            }
            if case != "unknown" {
                push_envelope(
                    &mut data,
                    if case == "spectator" {
                        &HASH_SPECTATOR_JOINED_TEAM
                    } else {
                        &HASH_PLAYER_JOINED_TEAM
                    },
                    1.0,
                    &team_join_body(0, 3),
                );
            }
            if case == "overflow" {
                for slot in 2..10 {
                    entry(&mut data, slot, "Other", 0);
                    push_envelope(
                        &mut data,
                        &HASH_PLAYER_JOINED_TEAM,
                        1.0,
                        &team_join_body(slot, 3),
                    );
                }
            }
            let table_start = data.len();
            if case != "missing table" {
                summary(&mut data, 0, -12);
            }
            if case == "duplicate row" {
                summary(&mut data, 0, 9);
            }
            if case == "overflow" {
                for slot in 2..10 {
                    summary(&mut data, slot, 1);
                }
            }
            if case == "malformed" {
                let len = data.len();
                data[len - 17] = 0;
            }
            if case != "missing result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 2.0, &[]);
            }
            let result_end = data.len();
            // Later activity must not overwrite the match's screen.
            entry(&mut data, 0, "After result", 0);
            data[result_end + 13..result_end + 17].copy_from_slice(&3.0f32.to_le_bytes());
            let p = if matches!(case, "compressed gap" | "damaged table" | "post result gap") {
                use flate2::{Compression, write::ZlibEncoder};
                use std::io::Write;
                let compress = |bytes: &[u8]| {
                    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                    encoder.write_all(bytes).unwrap();
                    encoder.finish().unwrap()
                };
                // Removing a whole damaged chunk leaves a perfectly aligned
                // chain. It must still not establish occupant continuity.
                let split = match case {
                    "damaged table" => table_start + 100,
                    "post result gap" => result_end,
                    _ => table_start,
                };
                let mut missing = Vec::new();
                entry(&mut missing, 0, "Unseen replacement", 0);
                let mut damaged = compress(&missing);
                *damaged.last_mut().unwrap() ^= 1;
                let mut raw = vec![0; 19];
                raw.extend(compress(&data[..split]));
                raw.extend(damaged);
                raw.extend(compress(&data[split..]));
                let p = WicReplayParser::from_bytes(&raw).expect("recoverable replay");
                assert_eq!(p.full_data, data);
                assert_eq!(p.decompression_gaps, vec![split]);
                assert_eq!(
                    p.final_screen_score_table().is_some(),
                    case != "damaged table"
                );
                p
            } else {
                WicReplayParser::from_decompressed(Vec::new(), data)
            };
            if case == "omitted replacement name" {
                assert!(p.final_screen_score_table().is_some());
            }
            let rows = p.final_screen_players();
            if [
                "normal",
                "spectator",
                "unknown",
                "replacement",
                "post result gap",
            ]
            .contains(&case)
            {
                let rows = rows.unwrap_or_else(|| panic!("{case}"));
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0].name,
                    if case == "replacement" {
                        "Replacement"
                    } else {
                        "Original"
                    }
                );
                assert_eq!(rows[0].score, Some(-12));
                assert_eq!(rows[0].score_total, Some(-12));
                assert_eq!(rows[0].role.as_deref(), Some("air"));
                assert_eq!(
                    rows[0].team,
                    Some(if case == "spectator" || case == "unknown" {
                        0
                    } else {
                        3
                    })
                );
            } else {
                assert!(rows.is_none(), "{case}");
            }
        }
    }

    #[test]
    fn departure_markers_require_explicit_session_local_leaves() {
        for case in [
            "left",
            "active",
            "reconnected",
            "replacement",
            "implicit replacement",
            "unknown replacement",
            "post result",
            "no entry",
            "late capture",
            "no result",
        ] {
            let mut data = Vec::new();
            if case != "late capture" {
                push_clock_envelope(&mut data, 0.0, 0.0);
            }
            if case != "no entry" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.1,
                    &player_entry_body(1, "Original"),
                );
            }
            push_clock_envelope(&mut data, 1.0, 1200.0);
            push_clock_envelope(&mut data, 1.1, 1199.9);
            push_clock_envelope(&mut data, 1.2, 1199.8);
            if !["active", "post result", "implicit replacement"].contains(&case) {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    2.0,
                    &player_leave_body(1),
                );
            }
            if [
                "reconnected",
                "replacement",
                "implicit replacement",
                "unknown replacement",
            ]
            .contains(&case)
            {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    3.0,
                    &player_entry_body(
                        1,
                        if case == "reconnected" {
                            "Original"
                        } else if case == "unknown replacement" {
                            ""
                        } else {
                            "Other"
                        },
                    ),
                );
            }
            if case != "no result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 4.0, &[]);
            }
            if case == "post result" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    5.0,
                    &player_leave_body(1),
                );
            }
            let parser = WicReplayParser::from_decompressed(
                metadata_with_player_names(&[(1, "Original")]),
                data,
            );
            let result = parser.parse();
            assert_eq!(
                result.players[0].left_at_seconds,
                if ["left", "replacement", "late capture"].contains(&case) {
                    Some(2.0)
                } else {
                    None
                },
                "{case}"
            );
        }
    }

    #[test]
    fn explicit_scores_keep_last_signed_value_in_own_slot_before_result() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 0.0);
        for (time, slot, score) in [
            (1.0, 15, 809i32),
            (2.0, 0, 12),
            (3.0, 15, 810),
            (4.0, 4, -11),
            (5.0, 0, 0),
        ] {
            let mut body = Vec::new();
            push_u32_field(&mut body, &HASH_APOS, slot);
            body[8] = 1;
            push_u32_field(&mut body, &HASH_SCORE, score as u32);
            push_envelope(&mut data, &HASH_SET_SCORE, time, &body);
        }
        push_envelope(&mut data, &HASH_TEAM_WINS, 6.0, &[]);
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_APOS, 15);
        body[8] = 1;
        push_u32_field(&mut body, &HASH_SCORE, 0);
        push_envelope(&mut data, &HASH_SET_SCORE, 7.0, &body);
        let parser = WicReplayParser::from_decompressed(
            metadata_with_player_names(&[(0, "A"), (4, "B"), (15, "C")]),
            data,
        );
        let scores: HashMap<_, _> = parser
            .explicit_match_scores()
            .into_iter()
            .map(|(_, slot, score)| (slot, score))
            .collect();
        assert_eq!(scores, HashMap::from([(0, 0), (4, -11), (15, 810)]));
        let result = parser.parse();
        assert_eq!(
            result.players.iter().find(|p| p.id == 15).unwrap().score,
            Some(810)
        );
        assert_eq!(result.player_scores, vec![810, -11]);
    }

    #[test]
    fn vacant_names_require_one_proven_gameplay_occupant() {
        for case in [
            "late",
            "pregame",
            "shared",
            "no units",
            "wrong team",
            "late capture",
            "no result",
            "old units",
        ] {
            let mut data = Vec::new();
            if case != "late capture" {
                push_clock_envelope(&mut data, 0.0, 0.0);
            }
            if case == "old units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    0.1,
                    &simple_unit_create_body(2, 3, 10),
                );
            }
            if case != "shared" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    0.2,
                    &player_leave_body(2),
                );
            }
            if case == "pregame" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.3,
                    &player_entry_body(2, "New"),
                );
            }
            push_clock_envelope(&mut data, 1.0, 1200.0);
            push_clock_envelope(&mut data, 1.1, 1199.9);
            push_clock_envelope(&mut data, 1.2, 1199.8);
            if case != "pregame" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    2.0,
                    &player_entry_body(2, "New"),
                );
            }
            push_joined_team(&mut data, 3.0, 2, 3);
            if case != "no units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    4.0,
                    &simple_unit_create_body(2, if case == "wrong team" { 1 } else { 3 }, 11),
                );
            }
            if case != "no result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 5.0, &[]);
            }
            let parser =
                WicReplayParser::from_decompressed(metadata_with_player_names(&[(2, "Old")]), data);
            let mut players = parser.parse().players;
            players[0].team = Some(3);
            players[0].name = "Old".to_owned();
            players[0].score = Some(0);
            parser.correct_vacant_result_names(&mut players);
            assert_eq!(
                players[0].name,
                if ["late", "pregame"].contains(&case) {
                    "New"
                } else {
                    "Old"
                },
                "{case}"
            );
            assert_eq!(players[0].score, Some(0));
            assert_eq!(players[0].role, None);
        }
    }

    #[test]
    fn zero_score_spectator_correction_requires_session_evidence() {
        for case in [
            "spectator",
            "one team spectator",
            "invalid spectator view",
            "lobby role explicit late spectator",
            "lobby role spectator",
            "lobby role late spectator",
            "lobby role rejoin",
            "lobby role after spectator",
            "lobby role match role",
            "lobby units spectator",
            "replacement",
            "positive score",
            "summary score",
            "units",
            "role selection",
            "still playing team",
            "postmatch spectator",
            "missing spectator",
            "missing result",
            "late recording",
            "missing score",
            "missing summary",
            "reconnect",
            "unframed spectator",
            "unknown team",
        ] {
            let mut data = Vec::new();
            if case != "late recording" {
                push_clock_envelope(&mut data, 0.0, 0.0);
            }
            push_joined_team(
                &mut data,
                0.1,
                1,
                if case == "unknown team" { 42 } else { 3 },
            );
            if case == "lobby role explicit late spectator" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    0.15,
                    &player_entry_body(1, "Original"),
                );
            }
            if case.starts_with("lobby") {
                if case == "lobby role after spectator" {
                    push_envelope(
                        &mut data,
                        &HASH_SPECTATOR_JOINED_TEAM,
                        0.2,
                        &spectator_joined_team_body(
                            1,
                            if case == "one team spectator" { 3 } else { 0 },
                            if case == "one team spectator" {
                                1
                            } else if case == "invalid spectator view" {
                                99
                            } else {
                                2
                            },
                        ),
                    );
                }
                let mut body = player_leave_body(1);
                push_u32_field(&mut body, &HASH_AROLE_ID, 0x10e0_0313);
                push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 0.3, &body);
                if case == "lobby units spectator" {
                    push_envelope(
                        &mut data,
                        &HASH_UNIT_CREATE,
                        0.4,
                        &simple_unit_create_body(1, 3, 123),
                    );
                }
                if ![
                    "lobby role late spectator",
                    "lobby role explicit late spectator",
                    "lobby role after spectator",
                ]
                .contains(&case)
                {
                    push_envelope(
                        &mut data,
                        &HASH_SPECTATOR_JOINED_TEAM,
                        0.5,
                        &spectator_joined_team_body(
                            1,
                            if case == "one team spectator" { 3 } else { 0 },
                            if case == "one team spectator" {
                                1
                            } else if case == "invalid spectator view" {
                                99
                            } else {
                                2
                            },
                        ),
                    );
                }
            }
            push_clock_envelope(&mut data, 1.0, 1200.0);
            if case == "lobby role rejoin" {
                push_joined_team(&mut data, 1.05, 1, 3);
            }
            // Extra clock observations also establish a full envelope chain.
            push_clock_envelope(&mut data, 1.1, 1199.9);
            push_clock_envelope(&mut data, 1.2, 1199.8);
            if case == "units" {
                push_envelope(
                    &mut data,
                    &HASH_UNIT_CREATE,
                    1.3,
                    &simple_unit_create_body(1, 3, 123),
                );
            }
            if case == "role selection" || case == "lobby role match role" {
                let mut body = player_leave_body(1);
                push_u32_field(&mut body, &HASH_AROLE_ID, 0x10e0_0313);
                push_envelope(&mut data, &HASH_PLAYER_SET_ROLE, 1.3, &body);
            }
            if ![
                "missing spectator",
                "postmatch spectator",
                "unframed spectator",
            ]
            .contains(&case)
            {
                push_envelope(
                    &mut data,
                    &HASH_SPECTATOR_JOINED_TEAM,
                    2.0,
                    &spectator_joined_team_body(
                        1,
                        if case == "one team spectator" { 3 } else { 0 },
                        if case == "one team spectator" {
                            1
                        } else if case == "invalid spectator view" {
                            99
                        } else {
                            2
                        },
                    ),
                );
            }
            if case == "replacement" || case == "reconnect" {
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_LEAVES_GAME,
                    2.1,
                    &player_leave_body(1),
                );
                push_envelope(
                    &mut data,
                    &HASH_PLAYER_ENTERS_GAME,
                    2.2,
                    &player_entry_body(
                        1,
                        if case == "replacement" {
                            "Other"
                        } else {
                            "Original"
                        },
                    ),
                );
                push_joined_team(&mut data, 2.3, 1, 3);
                push_envelope(
                    &mut data,
                    &HASH_SPECTATOR_JOINED_TEAM,
                    2.4,
                    &spectator_joined_team_body(
                        1,
                        if case == "one team spectator" { 3 } else { 0 },
                        if case == "one team spectator" {
                            1
                        } else if case == "invalid spectator view" {
                            99
                        } else {
                            2
                        },
                    ),
                );
            }
            if case == "still playing team" {
                push_joined_team(&mut data, 2.5, 1, 3);
            }
            if case != "missing result" {
                push_envelope(&mut data, &HASH_TEAM_WINS, 3.0, &[]);
            }
            if case == "postmatch spectator" {
                push_envelope(
                    &mut data,
                    &HASH_SPECTATOR_JOINED_TEAM,
                    3.1,
                    &spectator_joined_team_body(
                        1,
                        if case == "one team spectator" { 3 } else { 0 },
                        if case == "one team spectator" {
                            1
                        } else if case == "invalid spectator view" {
                            99
                        } else {
                            2
                        },
                    ),
                );
            }
            if case == "unframed spectator" {
                data.extend_from_slice(&HASH_SPECTATOR_JOINED_TEAM);
                push_u32_field(&mut data, &HASH_ASLOT, 1);
                push_u32_field(&mut data, &HASH_ATEAM, 0);
                push_u32_field(&mut data, &HASH_ASPECTATOR_LOS, 2);
            }
            let score = if case == "positive score" { 100 } else { 0 };
            if case != "missing score" {
                let offset = data.len();
                push_u32_field(&mut data, &HASH_SCORE, score);
                data.resize(offset + 47, 0);
                data.extend_from_slice(&BINTAG_SEP);
                data.extend_from_slice(&2u32.to_le_bytes());
            }
            if case != "missing summary" {
                let offset = data.len();
                push_u32_field(&mut data, &HASH_APOS, 1);
                // A role ID alone can persist even in a spectator's summary.
                push_u32_field(&mut data, &HASH_AROLE_ID, 0x10e0_0313);
                push_u32_field(
                    &mut data,
                    &HASH_SCORE_ROLE0,
                    if case == "summary score" { 1 } else { 0 },
                );
                data.resize(offset + 300, 0);
            }
            let parser = WicReplayParser::from_decompressed(
                metadata_with_player_names(&[(1, "Original")]),
                data,
            );
            let replay = parser.parse();
            let player = replay.players.iter().find(|p| p.id == 1).unwrap();
            assert_eq!(player.name, "Original", "{case}");
            assert_eq!(
                player.score,
                if case == "missing score" {
                    None
                } else {
                    Some(score as i32)
                },
                "{case}"
            );
            assert_eq!(
                player.team,
                Some(match case {
                    "spectator"
                    | "replacement"
                    | "lobby role spectator"
                    | "one team spectator"
                    | "lobby role explicit late spectator" => 0,
                    "unknown team" => 42,
                    _ => 3,
                }),
                "{case}"
            );
        }
    }

    #[test]
    fn uses_recorded_summary_role_only_for_scored_players_without_role_totals() {
        for (role_id, role_score, score, expected) in [
            (0x1abf_03df, 0, 930, Some("support")),
            (0x1abf_03df, 12, 930, Some("infantry")),
            (0x1abf_03df, 0, 0, None),
            (ROLE_ID_FEW_PLAYER_MODE, 0, 930, None),
            (42, 0, 930, None),
        ] {
            let mut data = Vec::new();
            push_u32_field(&mut data, &HASH_SCORE, score);
            data.resize(47, 0);
            data.extend_from_slice(&BINTAG_SEP);
            data.extend_from_slice(&9u32.to_le_bytes()); // counter 9 = slot 8
            push_u32_field(&mut data, &HASH_APOS, 8);
            push_u32_field(&mut data, &HASH_AROLE_ID, role_id);
            push_u32_field(&mut data, &HASH_SCORE_ROLE0, role_score);
            data.resize(355, 0);
            let parser = WicReplayParser::from_decompressed(
                metadata_with_player_names(&[(8, "Small Island")]),
                data,
            );
            let replay = parser.parse();
            let player = replay.players.iter().find(|p| p.id == 8).unwrap();
            assert_eq!(player.role.as_deref(), expected);
            assert_eq!(player.score, Some(score as i32));
            assert_eq!(player.score_infantry, Some(role_score as i32));
            assert_eq!(player.score_support, Some(0));
        }
    }

    #[test]
    fn match_roster_preserves_unscored_metadata_players() {
        let metadata = metadata_with_player_names(&[(1, "Scored"), (2, "Spectator")]);
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_ENTERS_GAME,
            2.0,
            &player_entry_body(2, "Replacement"),
        );
        push_envelope(&mut full_data, &HASH_TEAM_WINS, 3.0, &[]);
        let parser = WicReplayParser::from_decompressed(metadata, full_data);

        assert_eq!(
            parser.extract_match_roster_names(&HashSet::from([1])),
            HashMap::from([(1, "Scored".to_string()), (2, "Spectator".to_string())])
        );
    }

    #[test]
    fn extracts_server_name_from_the_structural_game_name_field() {
        let mut metadata = vec![0; 47];
        metadata.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        let mut encoded = "Shit Happens 2 1"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        encoded.extend_from_slice(&[0, 0]);
        push_field(&mut metadata, &HASH_MY_GAME_NAME, 5, &encoded);
        let parser = WicReplayParser::from_decompressed(metadata, Vec::new());

        assert_eq!(parser.extract_game_info().server_name, "Shit Happens 2 1");
    }

    #[test]
    fn extracts_arbitrary_replay_name_from_its_structural_field() {
        let mut metadata = vec![0; 47];
        metadata.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        let mut encoded = "Riviera comeback"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        encoded.extend_from_slice(&[0, 0]);
        push_field(&mut metadata, &HASH_REPLAY_NAME, 5, &encoded);
        let parser = WicReplayParser::from_decompressed(metadata, Vec::new());

        assert_eq!(
            parser.extract_game_info().replay_name.as_deref(),
            Some("Riviera comeback")
        );
    }

    #[test]
    fn preserves_missing_or_ambiguous_replay_names_as_unknown() {
        let mut metadata = vec![0; 47];
        metadata.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        let parser = WicReplayParser::from_decompressed(metadata.clone(), Vec::new());
        assert_eq!(parser.extract_game_info().replay_name, None);

        for value in ["first", "second"] {
            let mut encoded = value
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            encoded.extend_from_slice(&[0, 0]);
            push_field(&mut metadata, &HASH_REPLAY_NAME, 5, &encoded);
        }
        let parser = WicReplayParser::from_decompressed(metadata, Vec::new());
        assert_eq!(parser.extract_game_info().replay_name, None);
    }

    #[test]
    fn rejects_malformed_structural_server_name_fields() {
        let mut metadata = vec![0; 47];
        metadata.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        push_field(&mut metadata, &HASH_MY_GAME_NAME, 5, &[b'A', 0]);
        let parser = WicReplayParser::from_decompressed(metadata, Vec::new());

        assert_eq!(parser.extract_game_info().server_name, "Unknown");
    }

    fn tactical_aid_events(full_data: Vec<u8>) -> Vec<TimelineEvent> {
        tactical_aid_events_for(Vec::new(), full_data)
    }

    #[test]
    fn keeps_pre_match_chat_separate_and_excludes_bot_responses() {
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            5.0,
            &chat_body(2, "ready?", ChatChannel::All),
        );
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            20.0,
            &chat_body(3, "team ready 👍", ChatChannel::Team),
        );
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT_PRIVATE,
            21.0,
            &bot_response_body(4, "I will attack and hold area."),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);

        let mut win_body = Vec::new();
        push_u32_field(&mut win_body, &HASH_ATEAM, 1);
        push_u32_field(&mut win_body, &HASH_ATYPE, 0);
        push_envelope(&mut full_data, &HASH_TEAM_WINS, 40.0, &win_body);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            44.5,
            &chat_body(5, "gg", ChatChannel::All),
        );

        let parser = WicReplayParser::from_decompressed(
            metadata_with_player_names(&[(2, "Alpha"), (3, "Bravo"), (5, "Charlie")]),
            full_data,
        );
        let timeline = parser.parse_timeline();
        let chat: Vec<&TimelineEvent> = timeline
            .events
            .iter()
            .filter(|event| matches!(event, TimelineEvent::ChatMessage { .. }))
            .collect();

        assert_eq!(timeline.schema_version, 18);
        assert_eq!(
            timeline.participants,
            vec![
                TimelineParticipant {
                    player_id: 2,
                    player_name: Some("Alpha".to_string()),
                },
                TimelineParticipant {
                    player_id: 3,
                    player_name: Some("Bravo".to_string()),
                },
                TimelineParticipant {
                    player_id: 5,
                    player_name: Some("Charlie".to_string()),
                },
            ]
        );
        assert_eq!(timeline.coverage.chat, "visibleToRecorder");
        assert_eq!(timeline.pre_match_chat.len(), 1);
        // Both readings of the same message: its own position on the recording
        // axis, and its distance from the match boundary. The first clock sample
        // opens the match at 10.0, so the message envelope at 5.0 is 5.0 before it.
        assert_eq!(timeline.pre_match_chat[0].time_seconds, 5.0);
        assert_eq!(timeline.pre_match_chat[0].seconds_before_match, 5.0);
        assert_eq!(
            timeline.pre_match_chat[0].player_name.as_deref(),
            Some("Alpha")
        );
        assert_eq!(timeline.pre_match_chat[0].message, "ready?");
        assert_eq!(chat.len(), 1);
        assert!(matches!(
            chat[0],
            TimelineEvent::ChatMessage {
                player_id: 3,
                player_name: Some(player_name),
                message,
                channel: ChatChannel::Team,
                ..
            } if player_name == "Bravo" && message == "team ready 👍"
        ));
        assert_eq!(timeline.post_match_chat.len(), 1);
        // `TeamWins` lands at 40.0, so the message envelope at 44.5 is 4.5 after it.
        assert_eq!(timeline.post_match_chat[0].time_seconds, 44.5);
        assert_eq!(timeline.post_match_chat[0].seconds_after_match, 4.5);
        assert_eq!(
            timeline.post_match_chat[0].player_name.as_deref(),
            Some("Charlie")
        );
        assert_eq!(timeline.post_match_chat[0].message, "gg");
        assert_eq!(timeline.post_match_chat[0].channel, ChatChannel::All);
    }

    #[test]
    fn retains_chat_player_id_when_name_is_unresolved() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            20.0,
            &chat_body(7, "hello", ChatChannel::All),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);

        let timeline = parser.parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::ChatMessage {
                player_id: 7,
                player_name: None,
                message,
                ..
            } if message == "hello"
        )));
    }

    #[test]
    fn resolves_chat_name_split_across_metadata_chunk_boundary() {
        let mut full_data = metadata_with_player_names(&[(4, "[NTOW]chris98")]);
        let metadata = full_data[..50].to_vec();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_PLAYER_RECEIVE_CHAT,
            20.0,
            &chat_body(4, "me told him", ChatChannel::All),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let parser = WicReplayParser::from_decompressed(metadata, full_data);

        let timeline = parser.parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::ChatMessage {
                player_id: 4,
                player_name: Some(player_name),
                message,
                ..
            } if player_name == "[NTOW]chris98" && message == "me told him"
        )));
    }

    #[test]
    fn rejects_malformed_chat_fields_without_consuming_another_event() {
        let mut body = chat_body(3, "hello", ChatChannel::All);
        // The repeated size in the variable-length aMessage field must agree.
        body[17 + 9..17 + 13].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut data = Vec::new();
        push_envelope(&mut data, &HASH_PLAYER_RECEIVE_CHAT, 1.0, &body);

        assert!(parse_received_chat(&data, ENVELOPE_MESSAGE_OFFSET).is_none());
    }

    #[test]
    fn rejects_invalid_utf16_chat() {
        let mut body = Vec::new();
        push_field(&mut body, &HASH_APLAYER, 1, &3u32.to_le_bytes());
        push_field(&mut body, &HASH_AMESSAGE, 5, &[0x00, 0xd8, 0x00, 0x00]);
        push_field(&mut body, &HASH_ATEAM_CHAT, 3, &[0]);
        let mut data = Vec::new();
        push_envelope(&mut data, &HASH_PLAYER_RECEIVE_CHAT, 1.0, &body);

        assert!(parse_received_chat(&data, ENVELOPE_MESSAGE_OFFSET).is_none());
    }

    #[test]
    fn rejects_oversized_chat_and_invalid_player_slots() {
        let mut data = Vec::new();
        push_envelope(
            &mut data,
            &HASH_PLAYER_RECEIVE_CHAT,
            1.0,
            &chat_body(3, &"a".repeat(MAX_CHAT_UTF16_BYTES / 2), ChatChannel::All),
        );
        assert!(parse_received_chat(&data, ENVELOPE_MESSAGE_OFFSET).is_none());

        data.clear();
        push_envelope(
            &mut data,
            &HASH_PLAYER_RECEIVE_CHAT,
            1.0,
            &chat_body(16, "hello", ChatChannel::All),
        );
        assert!(parse_received_chat(&data, ENVELOPE_MESSAGE_OFFSET).is_none());
    }

    #[test]
    fn summarizes_recorder_tactical_aid_as_observed_placements() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 80.0, 0x3b07_0665, [1.0, 2.0, 3.0]);
        push_tactical_aid_use(&mut full_data, 60.0, 0x3b07_0665, [4.0, 5.0, 6.0]);
        push_tactical_aid_use(&mut full_data, 6.0, 0x2fe5_05d0, [7.0, 8.0, 9.0]);
        push_clock(&mut full_data, 1199.0);
        let parser = WicReplayParser::from_decompressed(metadata_with_recorder(7), full_data);

        let timeline = parser.parse_timeline();
        let summary = timeline.recorder_tactical_aid_usage;
        let nuke = summary
            .supports
            .iter()
            .find(|usage| usage.support_id == 0x3b07_0665)
            .expect("nuke summary");

        assert_eq!(timeline.coverage.tactical_aid, "recorderOnly");
        assert_eq!(summary.player_id, Some(7));
        assert_eq!(summary.total_placements, 3);
        assert_eq!(summary.supports.len(), 2);
        assert_eq!(nuke.support_name.as_deref(), Some("TacticalNuke_USSR"));
        assert_eq!(nuke.placement_count, 2);
        assert_eq!(nuke.observed_costs, vec![80.0, 60.0]);
    }

    #[test]
    fn extracts_single_faction_tactical_aid_marker_with_player_identity() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_MARKER,
            20.0,
            &support_marker_body(
                42,
                0x3b07_0665,
                [446.4, 32.6, 453.0],
                7,
                2,
                [0.0, 0.0, 1.0],
                20.0,
            ),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let parser = WicReplayParser::from_decompressed(
            metadata_with_player_names(&[(7, "Marker User")]),
            full_data,
        );

        let timeline = parser.parse_timeline();

        assert_eq!(
            timeline.coverage.tactical_aid_markers,
            "visibleFactionWithPlayer"
        );
        assert_eq!(
            timeline.participants,
            vec![TimelineParticipant {
                player_id: 7,
                player_name: Some("Marker User".to_string()),
            }]
        );
        let [
            TimelineEvent::TacticalAidMarker {
                time_seconds,
                event_id,
                support_id,
                support_name,
                position,
                player_id,
                upgrade_level,
                direction,
                duration_seconds,
            },
        ] = timeline.events.as_slice()
        else {
            panic!("expected one tactical-aid marker: {:?}", timeline.events);
        };
        // Recording-elapsed time, straight off the marker's own envelope.
        assert_eq!(*time_seconds, 20.0);
        assert_eq!(*event_id, 42);
        assert_eq!(*support_id, 0x3b07_0665);
        assert_eq!(support_name.as_deref(), Some("TacticalNuke_USSR"));
        assert_eq!(*position, [446.4, 32.6, 453.0]);
        assert_eq!(*player_id, 7);
        assert_eq!(*upgrade_level, 2);
        assert_eq!(*direction, [0.0, 0.0, 1.0]);
        assert_eq!(*duration_seconds, 20.0);
    }

    fn push_bar(data: &mut Vec<u8>, time: f32, value: f32) {
        let mut body = Vec::new();
        push_float_field(&mut body, &HASH_AFACTOR, value);
        push_envelope(data, &HASH_PLAYER_RECEIVE_CHAT, time, &body);
    }

    fn push_joined_team(data: &mut Vec<u8>, time: f32, slot: u32, team: u32) {
        let mut body = Vec::new();
        push_u32_field(&mut body, &HASH_ASLOT, slot);
        push_u32_field(&mut body, &HASH_ATEAM, team);
        body.resize(body.len().max(60), 0);
        push_envelope(data, &HASH_PLAYER_JOINED_TEAM, time, &body);
    }

    #[test]
    fn treats_a_recording_that_saw_the_pre_match_countdown_as_complete() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 0.0, 0.0);
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_clock_envelope(&mut full_data, 2.0, 400.0);
        let timing =
            WicReplayParser::from_decompressed(Vec::new(), full_data).extract_match_timing();

        assert!(timing.captured_match_start);
        assert!(timing.round_length_exact);
        assert_eq!(timing.round_length_seconds, Some(1200.0));
        assert_eq!(timing.joined_at_remaining_seconds, None);
        assert_eq!(
            timing.match_elapsed_seconds,
            timing.observed_gameplay_seconds
        );
    }

    #[test]
    fn recovers_match_time_for_a_recording_that_joined_mid_match() {
        // No pre-match sample: the recorder arrived with 926.6 s on the clock.
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 0.0, 926.6);
        push_clock_envelope(&mut full_data, 1.0, 500.0);
        push_clock_envelope(&mut full_data, 2.0, 0.95);
        let timing =
            WicReplayParser::from_decompressed(Vec::new(), full_data).extract_match_timing();

        assert!(!timing.captured_match_start);
        assert!(!timing.round_length_exact);
        // 926.6 fits only the 1200 s round, not the 900 s one.
        assert_eq!(timing.round_length_seconds, Some(1200.0));
        assert_eq!(timing.joined_at_remaining_seconds, Some(926.6));
        // The recording covers 925.65 s, but the match had run 1199.05 s.
        assert_eq!(timing.observed_gameplay_seconds, Some(925.65));
        assert_eq!(timing.match_elapsed_seconds, Some(1199.05));
    }

    #[test]
    fn clamps_overtime_so_match_time_never_exceeds_the_round() {
        // The countdown keeps ticking past zero into the post-match screen.
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 0.0, 8.33);
        push_clock_envelope(&mut full_data, 1.0, 0.99);
        push_clock_envelope(&mut full_data, 2.0, -64.1);
        let timing =
            WicReplayParser::from_decompressed(Vec::new(), full_data).extract_match_timing();

        assert_eq!(timing.round_length_seconds, Some(600.0));
        assert_eq!(timing.final_remaining_seconds, Some(-64.1));
        assert_eq!(timing.match_elapsed_seconds, Some(600.0));
    }

    #[test]
    fn classifies_endings_from_the_clock_and_the_final_bar() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 0.0, 0.0);
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_clock_envelope(&mut full_data, 2.0, 300.0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);
        let timing = parser.extract_match_timing();

        // Bar pinned to a stop with time left: the match was cut short.
        assert_eq!(
            parser.classify_match_ending(&timing, Some(1), Some(1.0)),
            MatchEnding::TotalDomination
        );
        // Real winner, time left, bar not pinned: someone quit.
        assert_eq!(
            parser.classify_match_ending(&timing, Some(1), Some(0.62)),
            MatchEnding::Forfeit
        );
        // No winner is never classified.
        assert_eq!(
            parser.classify_match_ending(&timing, None, Some(1.0)),
            MatchEnding::Unknown
        );
    }

    #[test]
    fn classifies_an_expired_clock_as_a_timer_finish() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 0.0, 0.0);
        push_clock_envelope(&mut full_data, 1.0, 1200.0);
        push_clock_envelope(&mut full_data, 2.0, 0.95);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);
        let timing = parser.extract_match_timing();

        assert_eq!(
            parser.classify_match_ending(&timing, Some(1), Some(0.5023)),
            MatchEnding::Timeout
        );
    }

    #[test]
    fn falls_back_past_a_malformed_trailing_bar_record() {
        let mut full_data = Vec::new();
        push_bar(&mut full_data, 1.0, 0.62);
        // Out of range: must not suppress the result.
        push_bar(&mut full_data, 2.0, 7.5);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);

        let value = parser
            .extract_domination_sample()
            .map(|(_, value)| value)
            .expect("valid sample before the malformed one");
        assert!((value - 0.62).abs() < 1e-6);
    }

    #[test]
    fn anchors_the_bar_to_the_pov_team_at_the_final_sample() {
        // Slot 7 records, starts on USSR (3), then switches to USA (1). The bar
        // is mirrored at the switch, so the anchor must be read at the last sample.
        let mut full_data = Vec::new();
        push_joined_team(&mut full_data, 1.0, 7, 3);
        push_bar(&mut full_data, 2.0, 0.70);
        push_joined_team(&mut full_data, 3.0, 7, 1);
        push_bar(&mut full_data, 4.0, 0.30);
        let parser = WicReplayParser::from_decompressed(metadata_with_recorder(7), full_data);

        let sample = parser.extract_domination_sample().expect("bar sample");
        assert_eq!(parser.pov_team_at(sample.0), Some(1));

        let players = vec![
            Player {
                id: 7,
                name: "Rec".to_string(),
                team: Some(1),
                faction: Some("USA".to_string()),
                score: Some(10),
                role: None,
                score_infantry: None,
                score_support: None,
                score_armor: None,
                score_air: None,
                score_capturing: None,
                score_fortification: None,
                score_transportation: None,
                score_repair: None,
                score_bridge_laying: None,
                score_unit_damage: None,
                score_tactical_aid: None,
                score_total: None,
                left_at_seconds: None,
            },
            Player {
                id: 8,
                name: "Foe".to_string(),
                team: Some(3),
                faction: Some("USSR".to_string()),
                score: Some(20),
                role: None,
                score_infantry: None,
                score_support: None,
                score_armor: None,
                score_air: None,
                score_capturing: None,
                score_fortification: None,
                score_transportation: None,
                score_repair: None,
                score_bridge_laying: None,
                score_unit_damage: None,
                score_tactical_aid: None,
                score_total: None,
                left_at_seconds: None,
            },
        ];
        let (shares, anchor) =
            parser.resolve_domination_shares(Some(sample), &players, Some("USSR"));
        let shares = shares.expect("shares");
        assert_eq!(anchor, Some(DominationAnchor::PovTeam));
        // 0.30 belongs to the POV team (USA), not to the winner.
        assert_eq!(shares[0].faction, "USA");
        assert!((shares[0].pct - 0.30).abs() < 1e-6);
        assert!((shares[1].pct - 0.70).abs() < 1e-6);
    }

    #[test]
    fn unmirrors_a_bar_curve_where_the_pov_team_changed() {
        // demo25: the bar drifts up at +0.001/s, the recorder joins a team at
        // offset 35, and the frame mirrors while the bar keeps its direction.
        let mut samples = vec![
            (10, 0.6983),
            (20, 0.6993),
            (30, 0.7003),
            (40, 0.2987),
            (50, 0.2977),
            (60, 0.2967),
        ];
        unmirror_bar_curve(&mut samples, &[35]);
        let values: Vec<f32> = samples.iter().map(|(_, value)| *value).collect();

        // Anchored to the final frame, so the tail is untouched...
        assert!((values[5] - 0.2967).abs() < 1e-6);
        assert!((values[3] - 0.2987).abs() < 1e-6);
        // ...and the head is mirrored into it, leaving one monotonic curve.
        assert!((values[0] - 0.3017).abs() < 1e-4);
        assert!(
            values.windows(2).all(|pair| pair[0] > pair[1]),
            "curve must be monotonic once the frame change is removed: {values:?}"
        );
    }

    #[test]
    fn leaves_a_discrete_front_line_move_alone() {
        // A Tug of War step across the centre is exactly its own mirror. Without a
        // team change at that offset it must not be read as a frame flip.
        let original = vec![(10, 0.4), (20, 0.4), (30, 0.6), (40, 0.6), (50, 0.8)];
        let mut samples = original.clone();
        unmirror_bar_curve(&mut samples, &[]);
        assert_eq!(samples, original);

        // Even with a team change elsewhere in the stream, an unrelated step stands.
        let mut samples = original.clone();
        unmirror_bar_curve(&mut samples, &[45]);
        assert_eq!(samples, original);
    }

    #[test]
    fn leaves_an_unflipped_curve_untouched() {
        let original = vec![(1, 0.5), (2, 0.52), (3, 0.54), (4, 0.9), (5, 1.0)];
        let mut samples = original.clone();
        unmirror_bar_curve(&mut samples, &[3]);
        assert_eq!(samples, original);
    }

    #[test]
    fn keeps_every_digit_the_bar_resolves() {
        // demo335 finished USA 50.07 / USSR 49.93. Rounding to whole percent
        // collapsed that into a false 50 / 50 tie.
        assert_eq!(format_share(0.500_666_677_951_812_7), "50.07");
        assert_eq!(format_share(0.499_333_322_048_187_26), "49.93");
        // Clean values stay clean.
        assert_eq!(format_share(1.0), "100");
        assert_eq!(format_share(0.0), "0");
        assert_eq!(format_share(0.58), "58");
    }

    #[test]
    fn falls_back_to_the_winner_when_the_recorder_is_a_spectator() {
        let mut full_data = Vec::new();
        push_bar(&mut full_data, 1.0, 0.4993);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);
        let sample = parser.extract_domination_sample().expect("bar sample");

        let players = vec![
            Player {
                id: 1,
                name: "A".to_string(),
                team: Some(1),
                faction: Some("USA".to_string()),
                score: Some(10),
                role: None,
                score_infantry: None,
                score_support: None,
                score_armor: None,
                score_air: None,
                score_capturing: None,
                score_fortification: None,
                score_transportation: None,
                score_repair: None,
                score_bridge_laying: None,
                score_unit_damage: None,
                score_tactical_aid: None,
                score_total: None,
                left_at_seconds: None,
            },
            Player {
                id: 2,
                name: "B".to_string(),
                team: Some(3),
                faction: Some("USSR".to_string()),
                score: Some(20),
                role: None,
                score_infantry: None,
                score_support: None,
                score_armor: None,
                score_air: None,
                score_capturing: None,
                score_fortification: None,
                score_transportation: None,
                score_repair: None,
                score_bridge_laying: None,
                score_unit_damage: None,
                score_tactical_aid: None,
                score_total: None,
                left_at_seconds: None,
            },
        ];
        let (shares, anchor) =
            parser.resolve_domination_shares(Some(sample), &players, Some("USA"));
        let shares = shares.expect("shares");
        assert_eq!(anchor, Some(DominationAnchor::WinnerInferred));
        assert_eq!(shares[0].faction, "USA");
        assert!(shares[0].pct > shares[1].pct);
        assert!((shares[0].pct + shares[1].pct - 1.0).abs() < 1e-9);
    }

    #[test]
    fn extracts_both_faction_tactical_aid_deployment_without_player_identity() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            20.0,
            &support_spawned_delayed_body(
                0x3b07_0665,
                [446.4, 32.6, 453.0],
                3,
                2,
                [0.0, 0.0, 1.0],
                0.25,
            ),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();

        assert_eq!(
            timeline.coverage.tactical_aid_deployments,
            "bothFactionsWithValidatedUnitDropPlayers"
        );
        let [
            TimelineEvent::TacticalAidDeployed {
                support_id,
                support_name,
                position,
                team,
                upgrade_level,
                direction,
                age_seconds,
                player_id,
                player_attribution,
                ..
            },
        ] = timeline.events.as_slice()
        else {
            panic!(
                "expected one tactical-aid deployment: {:?}",
                timeline.events
            );
        };
        assert_eq!(*support_id, 0x3b07_0665);
        assert_eq!(support_name, "TacticalNuke_USSR");
        assert_eq!(*position, [446.4, 32.6, 453.0]);
        assert_eq!(*team, 3);
        assert_eq!(*upgrade_level, 2);
        assert_eq!(*direction, [0.0, 0.0, 1.0]);
        assert_eq!(*age_seconds, 0.25);
        assert_eq!(*player_id, None);
        assert_eq!(*player_attribution, None);
    }

    #[test]
    fn attributes_a_unit_drop_to_its_unique_spawn_owner() {
        const SUPPORT_ID: u32 = 0x22dc_04ed;
        const UNIT_TYPE_ID: u32 = 0x184f_0436;
        let position = [446.4, 32.6, 453.0];
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 90.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            115.0,
            &support_spawned_delayed_body(SUPPORT_ID, position, 1, 1, [0.0, 0.0, 1.0], 15.0),
        );
        push_envelope(
            &mut full_data,
            &HASH_UNIT_CREATE,
            119.0,
            &unit_create_body(7, 1, 42, UNIT_TYPE_ID, position, 1),
        );
        push_clock_envelope(&mut full_data, 130.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();
        let deployment = timeline.events.iter().find_map(|event| match event {
            TimelineEvent::TacticalAidDeployed {
                player_id,
                player_attribution,
                ..
            } => Some((*player_id, *player_attribution)),
            _ => None,
        });
        assert_eq!(
            deployment,
            Some((
                Some(7),
                Some(TacticalAidPlayerAttribution::UnitSpawnOwnership)
            ))
        );
        assert!(timeline.participants.iter().any(|item| item.player_id == 7));
    }

    #[test]
    fn leaves_a_unit_drop_unknown_when_two_spawn_owners_are_compatible() {
        const SUPPORT_ID: u32 = 0x22dc_04ed;
        const UNIT_TYPE_ID: u32 = 0x184f_0436;
        let position = [446.4, 32.6, 453.0];
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 90.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            115.0,
            &support_spawned_delayed_body(SUPPORT_ID, position, 1, 1, [0.0, 0.0, 1.0], 15.0),
        );
        for (unit_id, player_id) in [(42, 7), (43, 8)] {
            push_envelope(
                &mut full_data,
                &HASH_UNIT_CREATE,
                119.0,
                &unit_create_body(player_id, 1, unit_id, UNIT_TYPE_ID, position, 1),
            );
        }
        push_clock_envelope(&mut full_data, 130.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::TacticalAidDeployed {
                player_id: None,
                player_attribution: None,
                ..
            }
        )));
    }

    #[test]
    fn leaves_a_unit_drop_unknown_when_spawn_evidence_is_outside_the_contract() {
        const SUPPORT_ID: u32 = 0x22dc_04ed;
        const UNIT_TYPE_ID: u32 = 0x184f_0436;
        let position = [446.4, 32.6, 453.0];
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 90.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            115.0,
            &support_spawned_delayed_body(SUPPORT_ID, position, 1, 1, [0.0, 0.0, 1.0], 15.0),
        );
        for (unit_id, raw_time, unit_position, spawn_source) in [
            (42, 119.0, position, 0),
            (43, 119.0, [position[0] + 20.0, position[1], position[2]], 1),
            (44, 123.0, position, 1),
        ] {
            push_envelope(
                &mut full_data,
                &HASH_UNIT_CREATE,
                raw_time,
                &unit_create_body(7, 1, unit_id, UNIT_TYPE_ID, unit_position, spawn_source),
            );
        }
        push_clock_envelope(&mut full_data, 130.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::TacticalAidDeployed {
                player_id: None,
                player_attribution: None,
                ..
            }
        )));
    }

    #[test]
    fn preserves_last_pregame_spectator_view_and_later_changes() {
        let mut full_data = Vec::new();
        push_envelope(
            &mut full_data,
            &HASH_SPECTATOR_JOINED_TEAM,
            1.0,
            &spectator_joined_team_body(14, 1, 1),
        );
        push_envelope(
            &mut full_data,
            &HASH_SPECTATOR_JOINED_TEAM,
            2.0,
            &spectator_joined_team_body(14, 0, 2),
        );
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SPECTATOR_JOINED_TEAM,
            20.0,
            &spectator_joined_team_body(14, 3, 1),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();

        assert!(matches!(
            timeline.events.first(),
            Some(TimelineEvent::SpectatorViewChanged {
                time_seconds: 0.0,
                player_id: 14,
                team: 0,
                spectator_los: 2,
                view: SpectatorView::AllTeams,
            })
        ));
        assert!(matches!(
            timeline.events.get(1),
            Some(TimelineEvent::SpectatorViewChanged {
                player_id: 14,
                team: 3,
                spectator_los: 1,
                view: SpectatorView::OneTeam,
                ..
            })
        ));
    }

    #[test]
    fn lobby_team_join_clears_spectator_state_but_later_spectating_survives() {
        for return_to_spectator in [false, true] {
            let mut data = Vec::new();
            push_envelope(
                &mut data,
                &HASH_SPECTATOR_JOINED_TEAM,
                1.0,
                &spectator_joined_team_body(7, 0, 2),
            );
            push_joined_team(&mut data, 2.0, 7, 3);
            if return_to_spectator {
                push_envelope(
                    &mut data,
                    &HASH_SPECTATOR_JOINED_TEAM,
                    3.0,
                    &spectator_joined_team_body(7, 3, 1),
                );
            }
            push_clock_envelope(&mut data, 10.0, 1200.0);
            push_clock_envelope(&mut data, 11.0, 1199.0);
            let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
            let views: Vec<_> = timeline
                .events
                .iter()
                .filter(|event| matches!(event, TimelineEvent::SpectatorViewChanged { .. }))
                .collect();
            assert_eq!(views.len(), usize::from(return_to_spectator));
            if return_to_spectator {
                assert!(matches!(
                    views[0],
                    TimelineEvent::SpectatorViewChanged {
                        player_id: 7,
                        view: SpectatorView::OneTeam,
                        ..
                    }
                ));
            }
        }
    }

    #[test]
    fn lobby_session_boundaries_clear_only_that_slots_spectator_state() {
        for boundary in [HASH_PLAYER_LEAVES_GAME, HASH_PLAYER_ENTERS_GAME] {
            let mut data = Vec::new();
            for slot in [7, 14] {
                push_envelope(
                    &mut data,
                    &HASH_SPECTATOR_JOINED_TEAM,
                    1.0,
                    &spectator_joined_team_body(slot, 0, 2),
                );
            }
            let body = if boundary == HASH_PLAYER_ENTERS_GAME {
                player_entry_body(7, "Replacement")
            } else {
                player_leave_body(7)
            };
            push_envelope(&mut data, &boundary, 2.0, &body);
            push_clock_envelope(&mut data, 10.0, 1200.0);
            push_clock_envelope(&mut data, 11.0, 1199.0);
            let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
            let views: Vec<_> = timeline
                .events
                .iter()
                .filter(|event| matches!(event, TimelineEvent::SpectatorViewChanged { .. }))
                .collect();
            assert_eq!(views.len(), 1);
            assert!(matches!(
                views[0],
                TimelineEvent::SpectatorViewChanged {
                    player_id: 14,
                    view: SpectatorView::AllTeams,
                    ..
                }
            ));
        }
    }

    #[test]
    fn rejects_tactical_aid_marker_with_invalid_player_slot() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_MARKER,
            0.0,
            &support_marker_body(
                42,
                0x3b07_0665,
                [1.0, 2.0, 3.0],
                16,
                0,
                [0.0, 0.0, 1.0],
                20.0,
            ),
        );
        push_clock(&mut full_data, 1199.0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);

        assert!(
            !parser
                .parse_timeline()
                .events
                .iter()
                .any(|event| matches!(event, TimelineEvent::TacticalAidMarker { .. }))
        );
    }

    #[test]
    fn extracts_exact_tactical_aid_damage_threshold_with_catalogue_name() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        for support_id in [0x3b07_0665, 0x2f92_05b5] {
            push_envelope(
                &mut full_data,
                &HASH_SUPPORT_THING_USED,
                10.0,
                &support_used_body(support_id, [0.0, 0.0, 0.0]),
            );
        }
        push_envelope(
            &mut full_data,
            &HASH_SEND_TA_TAUNT,
            20.0,
            &ta_taunt_body(11, 1, 1, 2),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let parser = WicReplayParser::from_decompressed(
            metadata_with_player_names(&[(1, "Target"), (11, "Actor")]),
            full_data,
        );

        let timeline = parser.parse_timeline();
        assert_eq!(
            timeline.participants,
            vec![
                TimelineParticipant {
                    player_id: 1,
                    player_name: Some("Target".to_string()),
                },
                TimelineParticipant {
                    player_id: 11,
                    player_name: Some("Actor".to_string()),
                },
            ]
        );
        assert!(matches!(
            timeline.events.as_slice(),
            [TimelineEvent::TacticalAidDamageThreshold {
                time_seconds,
                player_id: 11,
                target_player_id: 1,
                ta_index: 1,
                support_id: Some(0x2f92_05b5),
                support_name: Some(name),
                upgrade_level: 2,
            }] if *time_seconds == 20.0 && name == "Tankbuster_NATO"
        ));
    }

    #[test]
    fn retains_tactical_aid_damage_threshold_when_catalogue_is_unavailable() {
        let mut full_data = Vec::new();
        push_clock_envelope(&mut full_data, 10.0, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SEND_TA_TAUNT,
            20.0,
            &ta_taunt_body(3, 7, 59, 0),
        );
        push_clock_envelope(&mut full_data, 30.0, 1199.0);
        let timeline = WicReplayParser::from_decompressed(Vec::new(), full_data).parse_timeline();

        assert!(matches!(
            timeline.events.as_slice(),
            [TimelineEvent::TacticalAidDamageThreshold {
                player_id: 3,
                target_player_id: 7,
                ta_index: 59,
                support_id: None,
                support_name: None,
                upgrade_level: 0,
                ..
            }]
        ));
    }

    #[test]
    fn extracts_tactical_aid_use_with_its_paired_honors_cost() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 80.0, 0x3b07_0665, [446.4, 32.6, 453.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events(full_data);

        assert_eq!(events.len(), 1);
        let TimelineEvent::TacticalAidUsed {
            support_id,
            ref support_name,
            honors_cost,
            position,
            player_id,
            ..
        } = events[0]
        else {
            panic!("expected a tactical aid use");
        };
        assert_eq!(support_id, 0x3b07_0665);
        assert_eq!(support_name.as_deref(), Some("TacticalNuke_USSR"));
        assert_eq!(honors_cost, 80.0);
        assert_eq!(position, [446.4, 32.6, 453.0]);
        // No metadata chunk in the fixture, so the recorder slot is unknown.
        assert_eq!(player_id, None);
    }

    #[test]
    fn ignores_the_zero_position_support_catalogue_emitted_at_match_start() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        for support_id in [0x3b07_0665, 0x2e8f_05c0, 0x7adc_097e] {
            push_envelope(
                &mut full_data,
                &HASH_SUPPORT_THING_USED,
                0.0,
                &support_used_body(support_id, [0.0, 0.0, 0.0]),
            );
        }
        push_clock(&mut full_data, 1199.0);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn retains_a_priced_activation_at_the_world_origin() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 80.0, 0x3b07_0665, [0.0, 0.0, 0.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events(full_data);
        assert_eq!(events.len(), 1);
        let TimelineEvent::TacticalAidUsed { position, .. } = events[0] else {
            panic!("expected a tactical aid use");
        };
        assert_eq!(position, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn attributes_tactical_aid_to_the_recording_player() {
        const RECORDER: u32 = 8;
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 45.0, 0x41b8_06ca, [1.0, 2.0, 3.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events_for(metadata_with_recorder(RECORDER), full_data);
        assert_eq!(events.len(), 1);
        let TimelineEvent::TacticalAidUsed { player_id, .. } = events[0] else {
            panic!("expected a tactical aid use");
        };
        assert_eq!(player_id, Some(RECORDER));
    }

    #[test]
    fn ignores_a_support_use_without_a_preceding_honors_deduction() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_SUPPORT_THING_USED,
            0.0,
            &support_used_body(0x3b07_0665, [1.0, 2.0, 3.0]),
        );
        push_clock(&mut full_data, 1199.0);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn ignores_a_positive_honors_change_before_a_support_record() {
        // A tactical aid gift credits the recorder; it must not read as a purchase.
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, -5.0, 0x3b07_0665, [1.0, 2.0, 3.0]);
        push_clock(&mut full_data, 1199.0);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn ignores_a_support_record_that_is_not_inside_an_event_envelope() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_CHANGE_HONORS,
            0.0,
            &change_honors_body(-80.0),
        );
        full_data.extend_from_slice(&HASH_SUPPORT_THING_USED);
        full_data.extend_from_slice(&support_used_body(0x3b07_0665, [1.0, 2.0, 3.0]));
        push_clock(&mut full_data, 1199.0);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn ignores_a_support_record_truncated_mid_position() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_envelope(
            &mut full_data,
            &HASH_CHANGE_HONORS,
            0.0,
            &change_honors_body(-80.0),
        );
        let mut body = support_used_body(0x3b07_0665, [1.0, 2.0, 3.0]);
        body.truncate(body.len() - 9);
        push_envelope(&mut full_data, &HASH_SUPPORT_THING_USED, 0.0, &body);
        push_clock(&mut full_data, 1199.0);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn tolerates_a_support_record_at_the_very_end_of_the_stream() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_clock(&mut full_data, 1199.0);
        push_envelope(
            &mut full_data,
            &HASH_CHANGE_HONORS,
            0.0,
            &change_honors_body(-80.0),
        );
        full_data.extend_from_slice(&HASH_SUPPORT_THING_USED);

        assert!(tactical_aid_events(full_data).is_empty());
    }

    #[test]
    fn labels_the_few_player_mode_role() {
        // FPM has no roles, so its sentinel role is an exact marker for the mode.
        assert_eq!(role_name(ROLE_ID_FEW_PLAYER_MODE), Some("fewPlayer"));
        assert_eq!(ROLE_ID_FEW_PLAYER_MODE, 0x0b23_0275);
    }

    #[test]
    fn emits_each_observed_placement_without_reconstructing_a_bundle() {
        // This may be the recorded tail of a triple nuke whose 80-cost opener is
        // absent. Preserve the two raw placements without presenting them as a
        // complete double beginning inside the captured interval.
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 60.0, 0x3b07_0665, [4.0, 5.0, 6.0]);
        push_tactical_aid_use(&mut full_data, 40.0, 0x3b07_0665, [7.0, 8.0, 9.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events(full_data);

        assert_eq!(events.len(), 2);
        let costs: Vec<f32> = events
            .iter()
            .map(|event| match event {
                TimelineEvent::TacticalAidUsed { honors_cost, .. } => *honors_cost,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(costs, vec![60.0, 40.0]);
    }

    #[test]
    fn equal_cost_placements_remain_raw_across_arbitrary_gaps() {
        const LIGHT_ARTILLERY: u32 = 0x8a8e_0a11;
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use_at(&mut full_data, 100.0, 5.0, LIGHT_ARTILLERY, [1.0, 2.0, 3.0]);
        push_tactical_aid_use_at(&mut full_data, 900.0, 5.0, LIGHT_ARTILLERY, [4.0, 5.0, 6.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events(full_data);
        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| matches!(
            event,
            TimelineEvent::TacticalAidUsed {
                support_id: LIGHT_ARTILLERY,
                honors_cost: 5.0,
                ..
            }
        )));
    }

    #[test]
    fn leaves_internal_child_effect_ids_unnamed() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);
        push_tactical_aid_use(&mut full_data, 45.0, 0xd6e4_0c66, [1.0, 2.0, 3.0]);
        push_clock(&mut full_data, 1199.0);

        let events = tactical_aid_events(full_data);

        assert_eq!(events.len(), 1);
        let TimelineEvent::TacticalAidUsed {
            ref support_name, ..
        } = events[0]
        else {
            panic!("expected a tactical aid use");
        };
        assert_eq!(support_name.as_deref(), None);
    }

    #[test]
    fn top_level_support_names_round_trip_to_their_adler32_ids() {
        fn adler32(value: &str) -> u32 {
            const MOD_ADLER: u32 = 65_521;
            let (mut a, mut b) = (1_u32, 0_u32);
            for byte in value.bytes() {
                a = (a + u32::from(byte)) % MOD_ADLER;
                b = (b + a) % MOD_ADLER;
            }
            (b << 16) | a
        }

        assert_eq!(TOP_LEVEL_SUPPORT_NAMES.len(), 58);
        for &(support_id, support_name) in TOP_LEVEL_SUPPORT_NAMES {
            assert_eq!(adler32(support_name), support_id, "{support_name}");
            assert_eq!(super::support_name(support_id), Some(support_name));
        }
        assert!(
            TOP_LEVEL_SUPPORT_NAMES
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
    }

    #[test]
    fn infantry_soldier_catalogue_round_trips_to_shipped_definition_names() {
        fn adler32(value: &str) -> u32 {
            const MOD_ADLER: u32 = 65_521;
            let (mut a, mut b) = (1_u32, 0_u32);
            for byte in value.bytes() {
                a = (a + u32::from(byte)) % MOD_ADLER;
                b = (b + a) % MOD_ADLER;
            }
            (b << 16) | a
        }

        let names = [
            "US_Medic",
            "F1_Marine",
            "US_Sniper",
            "NATO_Medic",
            "USSR_Medic",
            "NATO_Marine",
            "NATO_Sniper",
            "USSR_Marine",
            "USSR_Sniper",
            "US_AntiTank",
            "US_Engineer",
            "NATO_AntiTank",
            "NATO_Engineer",
            "USSR_AntiTank",
            "USSR_Engineer",
            "US_AA_Infantry",
            "NATO_AA_Infantry",
            "USSR_AA_Infantry",
            "US_Machine_Gunner",
            "US_Medic_w_Special",
            "NATO_Machine_Gunner",
            "USSR_Machine_Gunner",
            "NATO_Medic_w_Special",
            "USSR_Medic_w_Special",
            "F1_Marine_Copy_NoScore",
            "NATO_Marine_Copy_NoScore",
            "USSR_Marine_Copy_NoScore",
        ];

        assert_eq!(names.len(), INFANTRY_SOLDIER_TYPE_IDS.len());
        for (&unit_type_id, name) in INFANTRY_SOLDIER_TYPE_IDS.iter().zip(names) {
            assert_eq!(adler32(name), unit_type_id, "{name}");
        }
        assert!(
            INFANTRY_SOLDIER_TYPE_IDS
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
    }

    #[test]
    fn extracts_supported_events_without_emitting_unit_creates() {
        let mut full_data = Vec::new();
        push_clock(&mut full_data, 1200.0);

        full_data.extend_from_slice(&HASH_PLAYER_JOINED_TEAM);
        push_u32_field(&mut full_data, &HASH_ASLOT, 3);
        push_u32_field(&mut full_data, &HASH_ATEAM, 2);

        full_data.extend_from_slice(&HASH_UNIT_CREATE);
        push_u32_field(&mut full_data, &HASH_APERSISTENCE_KEY, 10);
        push_u32_field(&mut full_data, &HASH_APLAYER, 3);
        push_u32_field(&mut full_data, &HASH_ATEAM, 2);
        push_u32_field(&mut full_data, &HASH_AUNIT, 42);
        push_u32_field(&mut full_data, &HASH_ATYPE, 0x1234_5678);

        full_data.extend_from_slice(&HASH_UNIT_DESTROY);
        push_u32_field(&mut full_data, &HASH_AUNIT, 42);
        push_u32_field(&mut full_data, &HASH_AKILLER, 512);
        push_u32_field(&mut full_data, &HASH_AKILLER_EXPERIENCE, 0);

        full_data.extend_from_slice(&HASH_TEAM_WINS);
        push_u32_field(&mut full_data, &HASH_ATEAM, 2);
        push_u32_field(&mut full_data, &HASH_ATYPE, 1);
        push_clock(&mut full_data, 1199.0);

        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);
        let timeline = parser.parse_timeline();

        assert_eq!(timeline.schema_version, 18);
        assert_eq!(timeline.match_duration_seconds, 1.0);
        assert_eq!(
            timeline.participants,
            vec![TimelineParticipant {
                player_id: 3,
                player_name: None,
            }]
        );
        assert_eq!(timeline.events.len(), 3);
        assert!(matches!(
            timeline.events[0],
            TimelineEvent::PlayerJoinedTeam {
                player_id: 3,
                team: 2,
                ..
            }
        ));
        assert!(matches!(
            timeline.events[1],
            TimelineEvent::UnitDestroyed {
                unit_id: 42,
                unit_type_id: Some(0x1234_5678),
                player_id: Some(3),
                team: Some(2),
                killer_unit_id: Some(512),
                killer_player_id: None,
                ..
            }
        ));
        assert!(matches!(
            timeline.events[2],
            TimelineEvent::TeamWon { team: 2, .. }
        ));
        assert!(timeline.infantry_soldier_deaths.is_empty());
    }

    #[test]
    fn separates_infantry_soldier_deaths_from_complete_squad_destructions() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &unit_create_body(3, 1, 41, 0x0e97_0333, [0.0; 3], 0),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &unit_create_body(3, 1, 42, 0x39a8_06b0, [0.0; 3], 0),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.2,
            &unit_destroy_body(41, KILLER_UNIT_SENTINEL),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.2,
            &unit_destroy_body(42, KILLER_UNIT_SENTINEL),
        );
        push_clock_envelope(&mut data, 0.3, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        let destroyed: Vec<_> = timeline
            .events
            .iter()
            .filter_map(|event| match event {
                TimelineEvent::UnitDestroyed {
                    unit_id,
                    unit_type_id,
                    ..
                } => Some((*unit_id, *unit_type_id)),
                _ => None,
            })
            .collect();

        assert_eq!(destroyed, vec![(42, Some(0x39a8_06b0))]);
        assert_eq!(timeline.infantry_soldier_deaths.len(), 1);
        assert_eq!(timeline.infantry_soldier_deaths[0].unit_id, 41);
        assert_eq!(
            timeline.infantry_soldier_deaths[0].unit_type_id,
            0x0e97_0333
        );
    }

    #[test]
    fn classifies_an_exact_building_collapse_even_when_its_record_is_serialized_afterward() {
        const BUILDING_ID: u32 = 0x2321_0510;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &unit_create_body(3, 1, 41, 0x39a8_06b0, [0.0; 3], 0),
        );
        push_envelope(
            &mut data,
            &HASH_BUILDING_SET_SLOT_STATE,
            0.2,
            &building_slot_state_body(BUILDING_ID, 2, true, 41),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.3,
            &unit_destroy_body_with_direction(41, KILLER_UNIT_SENTINEL, [0.6, 0.0, 0.8]),
        );
        push_envelope(
            &mut data,
            &HASH_BUILDING_DAMAGED,
            0.3,
            &building_damaged_body(BUILDING_ID, -100, 3),
        );
        push_clock_envelope(&mut data, 0.4, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 41,
                cause: UnitDestructionCause::Unknown,
                destruction_context: Some(UnitDestructionContext::BuildingCollapse {
                    building_id: BUILDING_ID,
                }),
                ..
            }
        )));
    }

    #[test]
    fn building_collapse_context_rejects_vacated_reused_and_non_synthetic_lifecycles() {
        const BUILDING_ID: u32 = 0x2321_0510;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);

        for unit_id in [41, 42, 43] {
            push_envelope(
                &mut data,
                &HASH_UNIT_CREATE,
                0.1,
                &unit_create_body(3, 1, unit_id, 0x39a8_06b0, [0.0; 3], 0),
            );
            push_envelope(
                &mut data,
                &HASH_BUILDING_SET_SLOT_STATE,
                0.2,
                &building_slot_state_body(BUILDING_ID, unit_id as i32, true, unit_id),
            );
        }
        push_envelope(
            &mut data,
            &HASH_BUILDING_SET_SLOT_STATE,
            0.25,
            &building_slot_state_body(BUILDING_ID, 41, false, 41),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.25,
            &unit_create_body(3, 1, 42, 0x39a8_06b0, [0.0; 3], 0),
        );
        for (unit_id, direction) in [
            (41, [0.6, 0.0, 0.8]),
            (42, [0.6, 0.0, 0.8]),
            (43, [1.0, 1.0, 0.0]),
        ] {
            push_envelope(
                &mut data,
                &HASH_UNIT_DESTROY,
                0.3,
                &unit_destroy_body_with_direction(unit_id, KILLER_UNIT_SENTINEL, direction),
            );
        }
        push_envelope(
            &mut data,
            &HASH_BUILDING_DAMAGED,
            0.3,
            &building_damaged_body(BUILDING_ID, -100, 3),
        );
        push_clock_envelope(&mut data, 0.4, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().all(|event| !matches!(
            event,
            TimelineEvent::UnitDestroyed {
                destruction_context: Some(_),
                ..
            }
        )));
    }

    #[test]
    fn inherits_an_exact_tactical_aid_cause_through_container_destruction() {
        const HEAVY_AIR_SUPPORT_US: u32 = 0x4337_071e;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            0.1,
            &support_spawned_delayed_body(HEAVY_AIR_SUPPORT_US, [0.0; 3], 1, 0, [0.0; 3], 0.0),
        );
        for unit_id in [50, 51] {
            push_envelope(
                &mut data,
                &HASH_UNIT_CREATE,
                0.2,
                &unit_create_body(7, 2, unit_id, 0x1000 + unit_id, [0.0; 3], 0),
            );
        }
        push_envelope(
            &mut data,
            &HASH_CREATE_UNIT_RELATION,
            0.3,
            &unit_relation_body(2, 50, 51),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT,
            0.4,
            &homing_support_projectile_body(HEAVY_AIR_SUPPORT_US, 50),
        );
        push_envelope(
            &mut data,
            &HASH_DESTROY_UNIT_RELATIONS,
            2.9,
            &unit_relation_body(2, 50, 51),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            2.9,
            &unit_destroy_body_with_direction(51, KILLER_UNIT_SENTINEL, [0.6, 0.0, 0.8]),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            2.9,
            &unit_destroy_body_with_direction(50, KILLER_UNIT_SENTINEL, [0.4, 0.1, 0.2]),
        );
        push_clock_envelope(&mut data, 3.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 51,
                killer_team: Some(1),
                cause: UnitDestructionCause::TacticalAid,
                destruction_context: Some(UnitDestructionContext::DestroyedWithContainer {
                    container_unit_id: 50,
                }),
                tactical_aid_support_id: Some(HEAVY_AIR_SUPPORT_US),
                ..
            }
        )));
    }

    #[test]
    fn container_context_rejects_a_different_parent_killer() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        for unit_id in [50, 51] {
            push_envelope(
                &mut data,
                &HASH_UNIT_CREATE,
                0.1,
                &unit_create_body(7, 2, unit_id, 0x1000 + unit_id, [0.0; 3], 0),
            );
        }
        push_envelope(
            &mut data,
            &HASH_CREATE_UNIT_RELATION,
            0.2,
            &unit_relation_body(2, 50, 51),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.3,
            &unit_destroy_body_with_direction(51, KILLER_UNIT_SENTINEL, [0.6, 0.0, 0.8]),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.3,
            &unit_destroy_body_with_direction(50, 41, [0.4, 0.1, 0.2]),
        );
        push_clock_envelope(&mut data, 0.4, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 51,
                cause: UnitDestructionCause::Unknown,
                destruction_context: None,
                ..
            }
        )));
    }

    #[test]
    fn attributes_a_sentinel_death_to_an_exact_target_support_projectile_team() {
        const HEAVY_AIR_SUPPORT_US: u32 = 0x4337_071e;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            0.1,
            &support_spawned_delayed_body(HEAVY_AIR_SUPPORT_US, [0.0; 3], 1, 0, [0.0; 3], 0.0),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.2,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT,
            0.3,
            &homing_support_projectile_body(HEAVY_AIR_SUPPORT_US, 95),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            2.9,
            &unit_destroy_body(95, KILLER_UNIT_SENTINEL),
        );
        push_clock_envelope(&mut data, 3.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 95,
                killer_unit_id: Some(KILLER_UNIT_SENTINEL),
                killer_player_id: None,
                killer_team: Some(1),
                cause: UnitDestructionCause::TacticalAid,
                tactical_aid_support_id: Some(HEAVY_AIR_SUPPORT_US),
                tactical_aid_support_name: Some(name),
                ..
            } if name == "HeavyAirSupport_US"
        )));
    }

    #[test]
    fn maps_a_heavy_air_child_projectile_to_its_top_level_tactical_aid() {
        const HEAVY_AIR_SUPPORT_US_AT_1: u32 = 0x9a0b_0a65;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_SUPPORT_THING_SPAWNED_DELAYED,
            0.1,
            &support_spawned_delayed_body(HEAVY_AIR_SUPPORT_US_AT_1, [0.0; 3], 1, 0, [0.0; 3], 0.0),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.2,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT,
            0.3,
            &homing_support_projectile_body(HEAVY_AIR_SUPPORT_US_AT_1, 95),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            2.9,
            &unit_destroy_body(95, KILLER_UNIT_SENTINEL),
        );
        push_clock_envelope(&mut data, 3.0, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 95,
                killer_team: Some(1),
                cause: UnitDestructionCause::TacticalAid,
                tactical_aid_support_id: Some(HEAVY_AIR_SUPPORT_US_AT_1),
                tactical_aid_support_name: Some(name),
                ..
            } if name == "HeavyAirSupport_US"
        )));
    }

    #[test]
    fn tactical_aid_death_attribution_rejects_conflicting_deployment_teams() {
        const NAPALM_USSR: u32 = 0x3cf3_0698;
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        for (time, team) in [(0.1, 1), (0.2, 3)] {
            push_envelope(
                &mut data,
                &HASH_SUPPORT_THING_SPAWNED_DELAYED,
                time,
                &support_spawned_delayed_body(NAPALM_USSR, [0.0; 3], team, 0, [0.0; 3], 0.0),
            );
        }
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.3,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_SUPPORT_CREATE_UNIT,
            0.4,
            &homing_support_projectile_body(NAPALM_USSR, 95),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.5,
            &unit_destroy_body(95, KILLER_UNIT_SENTINEL),
        );
        push_clock_envelope(&mut data, 0.6, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 95,
                killer_team: None,
                cause: UnitDestructionCause::Unknown,
                tactical_aid_support_id: None,
                tactical_aid_support_name: None,
                ..
            }
        )));
    }

    #[test]
    fn recovers_a_removed_killer_only_from_an_exact_target_homing_projectile() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &simple_unit_create_body(3, 1, 41),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            0.2,
            &homing_unit_projectile_body(41, 95),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 0.3, &unit_remove_body(41));
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.5,
            &unit_destroy_body(95, 41),
        );
        push_clock_envelope(&mut data, 0.6, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        assert!(timeline.events.iter().any(|event| matches!(
            event,
            TimelineEvent::UnitDestroyed {
                unit_id: 95,
                killer_unit_id: Some(41),
                killer_player_id: Some(3),
                killer_team: Some(1),
                ..
            }
        )));
    }

    #[test]
    fn exact_target_recovery_abstains_across_target_reuse_or_actor_ambiguity() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &simple_unit_create_body(3, 1, 41),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.1,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            0.2,
            &homing_unit_projectile_body(41, 95),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 0.25, &unit_remove_body(95));
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.3,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 0.31, &unit_remove_body(41));
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.4,
            &unit_destroy_body(95, 41),
        );

        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            1.0,
            &simple_unit_create_body(3, 1, 42),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            1.0,
            &simple_unit_create_body(7, 2, 96),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            1.1,
            &homing_unit_projectile_body(42, 96),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 1.2, &unit_remove_body(42));
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            1.25,
            &simple_unit_create_body(4, 1, 42),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            1.3,
            &homing_unit_projectile_body(42, 96),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 1.35, &unit_remove_body(42));
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            1.4,
            &unit_destroy_body(96, 42),
        );
        push_clock_envelope(&mut data, 1.5, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        let unresolved: Vec<_> = timeline
            .events
            .iter()
            .filter_map(|event| match event {
                TimelineEvent::UnitDestroyed {
                    killer_player_id,
                    killer_team,
                    ..
                } => Some((*killer_player_id, *killer_team)),
                _ => None,
            })
            .collect();
        assert_eq!(unresolved, vec![(None, None), (None, None)]);
    }

    #[test]
    fn exact_target_recovery_rejects_the_sentinel_and_out_of_window_projectiles() {
        let mut data = Vec::new();
        push_clock_envelope(&mut data, 0.0, 1200.0);
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.05,
            &simple_unit_create_body(3, 1, KILLER_UNIT_SENTINEL),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.05,
            &simple_unit_create_body(7, 2, 95),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            0.1,
            &homing_unit_projectile_body(KILLER_UNIT_SENTINEL, 95),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_REMOVE,
            0.2,
            &unit_remove_body(KILLER_UNIT_SENTINEL),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            0.3,
            &unit_destroy_body(95, KILLER_UNIT_SENTINEL),
        );

        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.4,
            &simple_unit_create_body(3, 1, 41),
        );
        push_envelope(
            &mut data,
            &HASH_UNIT_CREATE,
            0.4,
            &simple_unit_create_body(7, 2, 96),
        );
        push_envelope(
            &mut data,
            &HASH_PROJECTILE_HOMING_UNIT_CREATE,
            0.5,
            &homing_unit_projectile_body(41, 96),
        );
        push_envelope(&mut data, &HASH_UNIT_REMOVE, 0.6, &unit_remove_body(41));
        push_envelope(
            &mut data,
            &HASH_UNIT_DESTROY,
            1.001,
            &unit_destroy_body(96, 41),
        );
        push_clock_envelope(&mut data, 1.1, 1199.0);

        let timeline = WicReplayParser::from_decompressed(Vec::new(), data).parse_timeline();
        let unresolved: Vec<_> = timeline
            .events
            .iter()
            .filter_map(|event| match event {
                TimelineEvent::UnitDestroyed {
                    killer_player_id,
                    killer_team,
                    ..
                } => Some((*killer_player_id, *killer_team)),
                _ => None,
            })
            .collect();
        assert_eq!(unresolved, vec![(None, None), (None, None)]);
    }

    #[test]
    fn translates_community_map_paths_with_verified_corrections() {
        let expected = [
            ("maps/tw_arizona/tw_arizona.ice", "tw_Arizona"),
            ("maps/tw_bocage/tw_bocage.ice", "tw_Bocage"),
            ("maps/russia1/russia1.ice", "tw_Radar"),
            ("maps/do_wake/do_wake.ice", "do_Wake"),
            ("maps/usfarmland4/usfarmland4.ice", "tw_Highway"),
            ("maps/usfarmland2/usfarmland2.ice", "tw_Wasteland"),
            ("maps/airport_03/airport_03.ice", "do_Airport"),
            ("maps/airport_v2/airport_v2.ice", "do_Airport"),
            ("maps/bllack_forest/bllack_forest.ice", "do_BlackForest"),
            ("maps/helgoland/helgoland.ice", "do_Helgoland"),
            ("maps/do_wakebeta2/do_wakebeta2.ice", "do_Wake"),
        ];

        for (path, display_name) in expected {
            assert_eq!(map_display_name(path), display_name);
        }
        assert_eq!(
            map_display_name("maps/custom/unknown.ice"),
            "maps/custom/unknown.ice"
        );
    }
}

#[cfg(all(test, feature = "serde"))]
mod serialization_contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replay_data_json_contract_is_stable() {
        let replay = ReplayData {
            game_info: GameInfo {
                map_name: "maps/ustown1/ustown1.ice".to_string(),
                map_display_name: "do_Hometown".to_string(),
                server_name: "Example Server".to_string(),
                date_time: "- 2026-08-16 - 12:00".to_string(),
                replay_name: Some("Riviera comeback".to_string()),
                game_mode: "Domination".to_string(),
            },
            raw_server_flags: RawServerFlags {
                few_player_mode_flag: Some(false),
                match_mode_flag: Some(true),
                tournament_match_flag: Some(false),
                clan_match_flag: Some(true),
            },
            server_classification: ServerClassification {
                few_player_mode: Some(false),
                match_mode: Some(true),
                has_bots: Some(false),
                clan_match: Some(true),
                tournament_match: Some(false),
                ranked: None,
            },
            players: vec![Player {
                id: 1,
                name: "Alpha".to_string(),
                team: Some(1),
                faction: Some("USA".to_string()),
                score: Some(100),
                role: Some("air".to_string()),
                score_infantry: Some(0),
                score_support: Some(0),
                score_armor: Some(0),
                score_air: Some(99),
                score_capturing: Some(12),
                score_fortification: Some(23),
                score_transportation: Some(34),
                score_repair: Some(45),
                score_bridge_laying: Some(56),
                score_unit_damage: Some(67),
                score_tactical_aid: Some(78),
                score_total: Some(100),
                left_at_seconds: None,
            }],
            player_scores: vec![100],
            duration_seconds: Some(90.5),
            timing: MatchTiming {
                captured_match_start: false,
                recording_seconds: 128.25,
                observed_gameplay_seconds: Some(60.5),
                match_elapsed_seconds: Some(90.5),
                round_length_seconds: Some(120.0),
                round_length_exact: false,
                joined_at_remaining_seconds: Some(59.5),
                final_remaining_seconds: Some(29.5),
            },
            match_ending: MatchEnding::Timeout,
            winner_domination_pct: Some(0.75),
            loser_domination_pct: Some(0.25),
            domination_shares: Some(vec![
                DominationShare {
                    faction: "USA".to_string(),
                    pct: 0.75,
                },
                DominationShare {
                    faction: "USSR".to_string(),
                    pct: 0.25,
                },
            ]),
            domination_anchor: Some(DominationAnchor::PovTeam),
            winner: Some("USA".to_string()),
            incomplete: false,
            recorder: Some("Alpha".to_string()),
        };

        assert_eq!(
            serde_json::to_value(replay).expect("serialize ReplayData"),
            json!({
                "gameInfo": {
                    "mapName": "maps/ustown1/ustown1.ice",
                    "mapDisplayName": "do_Hometown",
                    "serverName": "Example Server",
                    "dateTime": "- 2026-08-16 - 12:00",
                    "replayName": "Riviera comeback",
                    "gameMode": "Domination"
                },
                "rawServerFlags": {
                    "fewPlayerModeFlag": false,
                    "matchModeFlag": true,
                    "tournamentMatchFlag": false,
                    "clanMatchFlag": true
                },
                "serverClassification": {
                    "fewPlayerMode": false,
                    "matchMode": true,
                    "hasBots": false,
                    "clanMatch": true,
                    "tournamentMatch": false,
                    "ranked": null
                },
                "players": [{
                    "id": 1,
                    "name": "Alpha",
                    "team": 1,
                    "faction": "USA",
                    "score": 100,
                    "role": "air",
                    "scoreInfantry": 0,
                    "scoreSupport": 0,
                    "scoreArmor": 0,
                    "scoreAir": 99,
                    "scoreCapturing": 12,
                    "scoreFortification": 23,
                    "scoreTransportation": 34,
                    "scoreRepair": 45,
                    "scoreBridgeLaying": 56,
                    "scoreUnitDamage": 67,
                    "scoreTacticalAid": 78,
                    "scoreTotal": 100,
                        "leftAtSeconds": null
                }],
                "playerScores": [100],
                "durationSeconds": 90.5,
                "timing": {
                    "capturedMatchStart": false,
                    "recordingSeconds": 128.25,
                    "observedGameplaySeconds": 60.5,
                    "matchElapsedSeconds": 90.5,
                    "roundLengthSeconds": 120.0,
                    "roundLengthExact": false,
                    "joinedAtRemainingSeconds": 59.5,
                    "finalRemainingSeconds": 29.5
                },
                "matchEnding": "timeout",
                "winnerDominationPct": 0.75,
                "loserDominationPct": 0.25,
                "dominationShares": [
                    { "faction": "USA", "pct": 0.75 },
                    { "faction": "USSR", "pct": 0.25 }
                ],
                "dominationAnchor": "povTeam",
                "winner": "USA",
                "incomplete": false,
                "recorder": "Alpha"
            })
        );
    }

    #[test]
    fn preserves_signed_scores_and_ranks_roles_by_signed_value() {
        fn push_i32_field(data: &mut Vec<u8>, hash: &[u8; 4], value: i32) {
            data.extend_from_slice(hash);
            data.extend_from_slice(&BINTAG_SEP);
            data.push(0);
            data.extend_from_slice(&BINTAG_SEP);
            data.extend_from_slice(&value.to_le_bytes());
        }
        let mut data = Vec::new();
        // Final counter-score records are 55 bytes; counter 5 identifies slot 4.
        push_i32_field(&mut data, &HASH_SCORE, -11);
        data.resize(47, 0);
        data.extend_from_slice(&BINTAG_SEP);
        data.extend_from_slice(&5u32.to_le_bytes());
        push_i32_field(&mut data, &HASH_APOS, 4);
        push_i32_field(&mut data, &HASH_SCORE_ROLE0, 10);
        push_i32_field(&mut data, &HASH_SCORE_ROLE3, -12);
        push_i32_field(&mut data, &HASH_TOTAL_SCORE, -12);
        data.resize(400, 0);
        let parser = WicReplayParser::from_decompressed(Vec::new(), data);
        assert_eq!(parser.extract_scores_by_hash().get(&5), Some(&-11));
        let summaries = parser.extract_player_end_summaries();
        let summary = &summaries[&4];
        assert_eq!(summary.air, -12);
        assert_eq!(summary.total, -12);
        assert_eq!(summary.role, "infantry");
    }

    #[test]
    fn extracts_end_of_match_player_score_categories() {
        let mut full_data = Vec::new();
        let push_score = |data: &mut Vec<u8>, hash: &[u8; 4], value: u32| {
            data.extend_from_slice(hash);
            data.extend_from_slice(&BINTAG_SEP);
            data.push(0);
            data.extend_from_slice(&BINTAG_SEP);
            data.extend_from_slice(&value.to_le_bytes());
        };
        push_score(&mut full_data, &HASH_APOS, 4);
        push_score(&mut full_data, &HASH_SCORE_ROLE0, 10);
        push_score(&mut full_data, &HASH_SCORE_ROLE1, 20);
        push_score(&mut full_data, &HASH_SCORE_ROLE2, 30);
        push_score(&mut full_data, &HASH_SCORE_ROLE3, 40);
        push_score(&mut full_data, &HASH_CAPTURING_SCORE, 51);
        push_score(&mut full_data, &HASH_FORTIFICATION_SCORE, 52);
        push_score(&mut full_data, &HASH_TRANSPORTATION_SCORE, 53);
        push_score(&mut full_data, &HASH_REPAIR_SCORE, 54);
        push_score(&mut full_data, &HASH_BRIDGE_LAYING_SCORE, 55);
        push_score(&mut full_data, &HASH_UNIT_DAMAGE_SCORE, 56);
        push_score(&mut full_data, &HASH_TACTICAL_AID_SCORE, 57);
        push_score(&mut full_data, &HASH_TOTAL_SCORE, 58);
        full_data.resize(300, 0);

        let parser = WicReplayParser::from_decompressed(Vec::new(), full_data);
        let summaries = parser.extract_player_end_summaries();
        assert_eq!(
            summaries.get(&4),
            Some(&PlayerEndSummary {
                role: "air".to_string(),
                recorded_role: None,
                infantry: 10,
                support: 20,
                armor: 30,
                air: 40,
                capturing: 51,
                fortification: 52,
                transportation: 53,
                repair: 54,
                bridge_laying: 55,
                unit_damage: 56,
                tactical_aid: 57,
                total: 58,
            })
        );
    }

    #[test]
    fn timeline_v18_json_contract_covers_every_event_variant() {
        let timeline = TimelineData {
            schema_version: TIMELINE_SCHEMA_VERSION,
            domination_anchor_faction: Some("USA".to_string()),
            duration_seconds: 128.25,
            match_duration_seconds: 90.0,
            initial_clock_seconds: Some(120.0),
            final_clock_seconds: Some(30.0),
            phases: vec![TimelinePhase {
                index: 0,
                start_seconds: 0.0,
                end_seconds: 90.0,
                initial_clock_seconds: 120.0,
                final_clock_seconds: 30.0,
            }],
            domination_samples: vec![TimelineValueSample {
                time_seconds: 5.0,
                value: 0.5,
            }],
            participants: vec![
                TimelineParticipant {
                    player_id: 1,
                    player_name: Some("Alpha".to_string()),
                },
                TimelineParticipant {
                    player_id: 2,
                    player_name: None,
                },
            ],
            participant_sessions: vec![
                TimelineParticipantSession {
                    player_id: 1,
                    session_index: 0,
                    player_name: Some("Alpha".to_string()),
                    start_seconds: 0.0,
                    end_seconds: Some(45.0),
                    identity_source: TimelineParticipantIdentitySource::StaticMetadata,
                },
                TimelineParticipantSession {
                    player_id: 1,
                    session_index: 1,
                    player_name: Some("Replacement".to_string()),
                    start_seconds: 45.0,
                    end_seconds: None,
                    identity_source: TimelineParticipantIdentitySource::PlayerEnteredGame,
                },
            ],
            events: vec![
                TimelineEvent::PlayerEntered {
                    time_seconds: 1.0,
                    player_id: 1,
                },
                TimelineEvent::PlayerLeft {
                    time_seconds: 2.0,
                    player_id: 2,
                },
                TimelineEvent::PlayerJoinedTeam {
                    time_seconds: 3.0,
                    player_id: 1,
                    team: 1,
                },
                TimelineEvent::SpectatorViewChanged {
                    time_seconds: 4.0,
                    player_id: 2,
                    team: 0,
                    spectator_los: 2,
                    view: SpectatorView::AllTeams,
                },
                TimelineEvent::PlayerSetRole {
                    time_seconds: 5.0,
                    player_id: 1,
                    role_id: 42,
                    role: Some("air".to_string()),
                },
                TimelineEvent::CommandPointOwnerChanged {
                    time_seconds: 6.0,
                    command_point_id: 100,
                    team: 1,
                },
                TimelineEvent::UnitDestroyed {
                    time_seconds: 7.0,
                    unit_id: 200,
                    unit_type_id: Some(300),
                    player_id: Some(1),
                    team: Some(1),
                    killer_unit_id: Some(201),
                    killer_player_id: Some(2),
                    killer_team: None,
                    cause: UnitDestructionCause::Unit,
                    destruction_context: Some(UnitDestructionContext::BuildingCollapse {
                        building_id: 900,
                    }),
                    tactical_aid_support_id: None,
                    tactical_aid_support_name: None,
                },
                TimelineEvent::TacticalAidTransferred {
                    time_seconds: 8.0,
                    from_player_id: 1,
                    to_player_id: 2,
                    amount: 10,
                },
                TimelineEvent::TacticalAidDamageThreshold {
                    time_seconds: 8.5,
                    player_id: 1,
                    target_player_id: 2,
                    ta_index: 60,
                    support_id: Some(402),
                    support_name: Some("Tankbuster_NATO".to_string()),
                    upgrade_level: 1,
                },
                TimelineEvent::TacticalAidUsed {
                    time_seconds: 9.0,
                    support_id: 400,
                    support_name: None,
                    honors_cost: 20.0,
                    position: [1.0, 2.0, 3.0],
                    player_id: Some(1),
                },
                TimelineEvent::TacticalAidMarker {
                    time_seconds: 9.5,
                    event_id: 600,
                    support_id: 401,
                    support_name: Some("HeavyAirSupport_US".to_string()),
                    position: [4.0, 5.0, 6.0],
                    player_id: 2,
                    upgrade_level: 1,
                    direction: [0.0, 0.0, 1.0],
                    duration_seconds: 15.0,
                },
                TimelineEvent::TacticalAidDeployed {
                    time_seconds: 9.75,
                    support_id: 401,
                    support_name: "HeavyAirSupport_US".to_string(),
                    position: [4.0, 5.0, 6.0],
                    team: 1,
                    upgrade_level: 1,
                    direction: [0.0, 0.0, 1.0],
                    age_seconds: 0.25,
                    player_id: Some(2),
                    player_attribution: Some(TacticalAidPlayerAttribution::UnitSpawnOwnership),
                },
                TimelineEvent::ChatMessage {
                    time_seconds: 10.0,
                    player_id: 2,
                    player_name: None,
                    message: "hello".to_string(),
                    channel: ChatChannel::Team,
                },
                TimelineEvent::VoteStarted {
                    time_seconds: 11.0,
                    player_id: 1,
                    vote_id: 500,
                    float_value: 1.5,
                    int_value: -2,
                    uint_value: 3,
                },
                TimelineEvent::TeamWon {
                    time_seconds: 12.0,
                    team: 1,
                    win_type: 2,
                },
            ],
            infantry_soldier_deaths: vec![InfantrySoldierDeath {
                time_seconds: 7.0,
                unit_id: 202,
                unit_type_id: 0x0e97_0333,
                player_id: Some(1),
                team: Some(1),
                killer_unit_id: Some(201),
                killer_player_id: Some(2),
                killer_team: None,
                cause: UnitDestructionCause::Unit,
                destruction_context: Some(UnitDestructionContext::DestroyedWithContainer {
                    container_unit_id: 203,
                }),
                tactical_aid_support_id: None,
                tactical_aid_support_name: None,
            }],
            pre_match_chat: vec![PreMatchChatMessage {
                time_seconds: 0.0,
                seconds_before_match: 5.0,
                player_id: 1,
                player_name: Some("Alpha".to_string()),
                message: "ready".to_string(),
                channel: ChatChannel::All,
            }],
            post_match_chat: vec![PostMatchChatMessage {
                time_seconds: 14.0,
                seconds_after_match: 2.0,
                player_id: 2,
                player_name: None,
                message: "gg".to_string(),
                channel: ChatChannel::All,
            }],
            coverage: TimelineCoverage {
                chat: "visibleToRecorder",
                tactical_aid: "recorderOnly",
                tactical_aid_markers: "visibleFactionWithPlayer",
                tactical_aid_deployments: "bothFactionsWithValidatedUnitDropPlayers",
            },
            recorder_tactical_aid_usage: RecorderTacticalAidUsage {
                player_id: Some(1),
                total_placements: 2,
                supports: vec![TacticalAidUsageCount {
                    support_id: 400,
                    support_name: None,
                    placement_count: 2,
                    observed_costs: vec![20.0, 10.0],
                }],
            },
        };

        assert_eq!(TIMELINE_SCHEMA_VERSION, 18);
        assert_eq!(
            serde_json::to_value(timeline).expect("serialize TimelineData"),
            json!({
                "schemaVersion": 18,
                "durationSeconds": 128.25,
                "matchDurationSeconds": 90.0,
                "initialClockSeconds": 120.0,
                "finalClockSeconds": 30.0,
                "phases": [{
                    "index": 0,
                    "startSeconds": 0.0,
                    "endSeconds": 90.0,
                    "initialClockSeconds": 120.0,
                    "finalClockSeconds": 30.0
                }],
                "dominationSamples": [{"timeSeconds": 5.0, "value": 0.5}],
                "dominationAnchorFaction": "USA",
                "participants": [
                    {"playerId": 1, "playerName": "Alpha"},
                    {"playerId": 2, "playerName": null}
                ],
                "participantSessions": [
                    {
                        "playerId": 1,
                        "sessionIndex": 0,
                        "playerName": "Alpha",
                        "startSeconds": 0.0,
                        "endSeconds": 45.0,
                        "identitySource": "staticMetadata"
                    },
                    {
                        "playerId": 1,
                        "sessionIndex": 1,
                        "playerName": "Replacement",
                        "startSeconds": 45.0,
                        "endSeconds": null,
                        "identitySource": "playerEnteredGame"
                    }
                ],
                "events": [
                    {"type": "playerEntered", "timeSeconds": 1.0, "playerId": 1},
                    {"type": "playerLeft", "timeSeconds": 2.0, "playerId": 2},
                    {"type": "playerJoinedTeam", "timeSeconds": 3.0, "playerId": 1, "team": 1},
                    {"type": "spectatorViewChanged", "timeSeconds": 4.0, "playerId": 2, "team": 0, "spectatorLos": 2, "view": "allTeams"},
                    {"type": "playerSetRole", "timeSeconds": 5.0, "playerId": 1, "roleId": 42, "role": "air"},
                    {"type": "commandPointOwnerChanged", "timeSeconds": 6.0, "commandPointId": 100, "team": 1},
                    {
                        "type": "unitDestroyed",
                        "timeSeconds": 7.0,
                        "unitId": 200,
                        "unitTypeId": 300,
                        "playerId": 1,
                        "team": 1,
                        "killerUnitId": 201,
                        "killerPlayerId": 2,
                        "killerTeam": null,
                        "cause": "unit",
                        "destructionContext": {
                            "type": "buildingCollapse",
                            "buildingId": 900
                        },
                        "tacticalAidSupportId": null,
                        "tacticalAidSupportName": null
                    },
                    {"type": "tacticalAidTransferred", "timeSeconds": 8.0, "fromPlayerId": 1, "toPlayerId": 2, "amount": 10},
                    {
                        "type": "tacticalAidDamageThreshold",
                        "timeSeconds": 8.5,
                        "playerId": 1,
                        "targetPlayerId": 2,
                        "taIndex": 60,
                        "supportId": 402,
                        "supportName": "Tankbuster_NATO",
                        "upgradeLevel": 1
                    },
                    {
                        "type": "tacticalAidUsed",
                        "timeSeconds": 9.0,
                        "supportId": 400,
                        "supportName": null,
                        "honorsCost": 20.0,
                        "position": [1.0, 2.0, 3.0],
                        "playerId": 1
                    },
                    {
                        "type": "tacticalAidMarker",
                        "timeSeconds": 9.5,
                        "eventId": 600,
                        "supportId": 401,
                        "supportName": "HeavyAirSupport_US",
                        "position": [4.0, 5.0, 6.0],
                        "playerId": 2,
                        "upgradeLevel": 1,
                        "direction": [0.0, 0.0, 1.0],
                        "durationSeconds": 15.0
                    },
                    {
                        "type": "tacticalAidDeployed",
                        "timeSeconds": 9.75,
                        "supportId": 401,
                        "supportName": "HeavyAirSupport_US",
                        "position": [4.0, 5.0, 6.0],
                        "team": 1,
                        "upgradeLevel": 1,
                        "direction": [0.0, 0.0, 1.0],
                        "ageSeconds": 0.25,
                        "playerId": 2,
                        "playerAttribution": "unitSpawnOwnership"
                    },
                    {
                        "type": "chatMessage",
                        "timeSeconds": 10.0,
                        "playerId": 2,
                        "playerName": null,
                        "message": "hello",
                        "channel": "team"
                    },
                    {
                        "type": "voteStarted",
                        "timeSeconds": 11.0,
                        "playerId": 1,
                        "voteId": 500,
                        "floatValue": 1.5,
                        "intValue": -2,
                        "uintValue": 3
                    },
                    {"type": "teamWon", "timeSeconds": 12.0, "team": 1, "winType": 2}
                ],
                "infantrySoldierDeaths": [{
                    "timeSeconds": 7.0,
                    "unitId": 202,
                    "unitTypeId": 244777779,
                    "playerId": 1,
                    "team": 1,
                    "killerUnitId": 201,
                    "killerPlayerId": 2,
                    "killerTeam": null,
                    "cause": "unit",
                    "destructionContext": {
                        "type": "destroyedWithContainer",
                        "containerUnitId": 203
                    },
                    "tacticalAidSupportId": null,
                    "tacticalAidSupportName": null
                }],
                "preMatchChat": [{
                    "timeSeconds": 0.0,
                    "secondsBeforeMatch": 5.0,
                    "playerId": 1,
                    "playerName": "Alpha",
                    "message": "ready",
                    "channel": "all"
                }],
                "postMatchChat": [{
                    "timeSeconds": 14.0,
                    "secondsAfterMatch": 2.0,
                    "playerId": 2,
                    "playerName": null,
                    "message": "gg",
                    "channel": "all"
                }],
                "coverage": {
                    "chat": "visibleToRecorder",
                    "tacticalAid": "recorderOnly",
                    "tacticalAidMarkers": "visibleFactionWithPlayer",
                    "tacticalAidDeployments": "bothFactionsWithValidatedUnitDropPlayers"
                },
                "recorderTacticalAidUsage": {
                    "playerId": 1,
                    "totalPlacements": 2,
                    "supports": [{
                        "supportId": 400,
                        "supportName": null,
                        "placementCount": 2,
                        "observedCosts": [20.0, 10.0]
                    }]
                }
            })
        );
    }
}
