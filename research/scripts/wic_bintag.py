#!/usr/bin/env python3
"""Reusable BinTagFormat2 reader for World in Conflict `.wicdemo` replays.

This module exists so replay-format research is reproducible from the research
workspace instead of from throwaway probes. It is deliberately read-only: it
never writes to `local/replays/` or `local/binaries/`.

Container
---------
A `.wicdemo` file is a 19-byte raw header followed by a sequence of independent
zlib streams. Concatenating every inflated stream yields the decompressed image
used throughout this module.

Event envelope
--------------
Past a short metadata prefix, the decompressed image is a *gapless chain* of
`Event` envelopes. Measured on the corpus, the chain accounts for ~100% of the
bytes after its anchor, so envelope walking is exact rather than heuristic::

    offset  size  meaning
    +0      4     0x05b50203  Adler-32 of "Event"
    +4      4     0x15000000  envelope type marker
    +8      1     0x06        BinTag flag 6 (array/blob)
    +9      4     total envelope size in bytes, header included
    +13     4     float32 gameplay timestamp in seconds
    +17     4     Adler-32 of the message name
    +21     ...   message body

The next envelope starts at ``offset + total``.

Field records
-------------
A message body is a run of 17-byte fields::

    +0  4  Adler-32 of the field name
    +4  4  0x11000000
    +8  1  type flag: 0=int32, 1=uint32, 2=float32, 3=bool, 6=array
    +9  4  0x11000000
    +13 4  value

Some messages end in a trailing blob that is not field-structured; the reader
returns it verbatim rather than guessing.

Names
-----
Both message names and field names are hashed with stock Adler-32 over the ASCII
identifier, so `zlib.adler32` reproduces them exactly. `NameTable` recovers the
inverse mapping by hashing identifier strings found in the shipped binaries.

`KNOWN_NAMES` holds the exhaustive message list rather than a sample: walking the
callers of `wic.exe:0x009240a0` enumerates every message the game can serialize.
An unresolved message hash therefore means the record is not written by the client
at all, which is itself a useful result.
"""

from __future__ import annotations

import pathlib
import re
import struct
import zlib
from dataclasses import dataclass
from typing import Iterator, Sequence

ZLIB_HEADERS = (b"\x78\xda", b"\x78\x9c", b"\x78\x01")
RAW_HEADER_BYTES = 19

ENVELOPE_HASH = 0x05B50203  # adler32(b"Event")
ENVELOPE_TAG = struct.pack("<I", ENVELOPE_HASH)
ENVELOPE_TYPE = b"\x15\x00\x00\x00"
ENVELOPE_FLAG = 0x06
ENVELOPE_HEADER_BYTES = 21

FIELD_SEP = b"\x11\x00\x00\x00"
FIELD_BYTES = 17

TYPE_INT32 = 0
TYPE_UINT32 = 1
TYPE_FLOAT32 = 2
TYPE_BOOL = 3
TYPE_ARRAY = 6


def name_hash(name: str | bytes) -> int:
    """Return the BinTag hash of an identifier."""
    if isinstance(name, str):
        name = name.encode("ascii")
    return zlib.adler32(name)


# Message and field names confirmed against the shipped binaries.
#
# The message list is exhaustive rather than a sample: walking the callers of the
# message-begin function `wic.exe:0x009240a0` enumerates every message the game
# can serialize into a replay. Regenerate it with
# `research/scripts/bintag_message_inventory.py`; see
# `research/findings/bintag-message-inventory-2026-08-22.md`.
#
# Note `SetGameModeData_Float` carries an underscore. The literal
# `SetGameModeDataFloat` hashes to a value that appears in no replay.
KNOWN_NAMES: dict[int, str] = {
    name_hash(n): n
    for n in (
        "Event",
        # Every message wic.exe can serialize, from the callers of 0x009240a0.
        "AbortCNMH",
        "AddAIUnitObjective",
        "AddCNMH_Zone",
        "AddCommandPoint",
        "AddOffensiveFortificationPoint",
        "AddPerimeterPoint",
        "AddScriptEvent",
        "AddSubObjectiveMarker",
        "AntiProjectile",
        "BlinkUnit",
        "BlowerBlew",
        "BotModeChanged",
        "BroadcastLongMove",
        "BuildingCreate",
        "BuildingDamaged",
        "BuildingSetSlotState",
        "CameraOrientation",
        "CameraPosition",
        "ChangeHonors",
        "ClearOrderQueue",
        "ContainerCantLoad",
        "ContainerCantUnload",
        "ContainerCantUnloadAll",
        "ContainerWillLoad",
        "ContainerWillUnloadAllQueued",
        "CreateBuildingRelation",
        "CreateCloud",
        "CreateGenericModelFromScript",
        "CreateUnitRelation",
        "DeleteGenericModel",
        "DeployableCreate",
        "DeployableDamaged",
        "DestroyAllProjectiles",
        "DestroyBuildingRelations",
        "DestroyBuildingRelations_Unit",
        "DestroyUnitRelations",
        "DestroyUnitRelations_Unit",
        "EnableSpecialAbility",
        "HideMessageBox",
        "HostShuttingDown",
        "MakeVote",
        "MapSignal",
        "MatchStarting",
        "MoverCantFollow",
        "MoverCantMove",
        "MoverHalted",
        "MoverIsFollowing",
        "MoverIsMoving",
        "MoverQueued",
        "NukeGroundEffectBeforeJoin",
        "OffensiveFortificationSetOwner",
        "PlayerEntersGame",
        "PlayerError",
        "PlayerJoinedTeam",
        "PlayerLeavesGame",
        "PlayerReceiveChat",
        "PlayerReceiveChatPrivate",
        "PlayerReceiveSystemMessage",
        "PlayerSetRole",
        "ProjectileBallisticCreate",
        "ProjectileBallisticSupportCreate",
        "ProjectileHomingSupportCreate_Position",
        "ProjectileHomingSupportCreate_Unit",
        "ProjectileHomingTargetCreate",
        "ProjectileHomingUnitCreate",
        "ProjectileStraightCreate",
        "ProjectileStraightSupportCreate",
        "PropDamaged",
        "PurgeMessageBoxQueue",
        "RemoveAllSpawners",
        "RemoveAllWarfilters",
        "RemoveSubObjectiveMarker",
        "RepairablePropDamaged",
        "RepairerCantRepair",
        "RepairerWillRepair",
        "RepairerWillRepairQueued",
        "RequestAccepted",
        "RequestSent",
        "RequestStartingUnits",
        "ResetSpecialAbility",
        "ResidentCantEnterBuilding",
        "ResidentWillEnterBuilding",
        "ResidentWillEnterBuildingQueued",
        "RestoreClientStateForUnit",
        "ResumeDispand",
        "ResupplyBegin",
        "ResupplyCanceled",
        "ResupplyFinished",
        "SaveClientStateForUnit",
        "SendMatchReadyCheck",
        "SendTATaunt",
        "SetCommandPointActive",
        "SetCommandPointCapturable",
        "SetCommandPointOwner",
        "SetCoreSystemState",
        "SetExperienceLevel",
        "SetGameModeData_Float",
        "SetGameModeData_Int",
        "SetMaxAP",
        "SetObjective",
        "SetPauseState",
        "SetPerimeterPointOwner",
        "SetPlayerLANName",
        "SetRoundInfo",
        "SetScore",
        "SetScoreAtGameEnd",
        "SetShooterAmmoType",
        "SetShooterNumRounds",
        "SetState_Unit",
        "SetUnitPrisoner",
        "SetWeatherEffect",
        "ShooterAcquiredTarget",
        "ShooterCantAttack_Pos",
        "ShooterCantAttack_Unit",
        "ShooterCantAttack_UnitList",
        "ShooterIsAttacking_Pos",
        "ShooterIsAttacking_Unit",
        "ShooterIsAttacking_UnitList",
        "ShooterQueuedAttack",
        "ShowMessageBox",
        "ShowPlayerGiveTANotification",
        "ShowSystemMessage",
        "ShowTimer",
        "ShutdownImminentWarning",
        "SpawnExplosion",
        "SpawnExplosionWithCrater",
        "SpawnerAvailable",
        "SpawnerCantSetPayload",
        "SpawnerDeployed",
        "SpawnerSetPayload",
        "SpawnerSetPosition",
        "SpawnerSpawning",
        "SpawnerSpawningDone",
        "SpecialAbilityPending",
        "SpecialAbilityStartProcessing",
        "SpectatorJoinedTeam",
        "SpeedTreeStateChange",
        "StartCNMH_Arrival",
        "StartCNMH_Delivery",
        "StartCNMH_Timer",
        "StartGameLogic",
        "StartGameTime",
        "SupportThingFeedback",
        "SupportThingMarker",
        "SupportThingMarkerStopped",
        "SupportThingNotUsed",
        "SupportThingReady",
        "SupportThingSpawnedDelayed",
        "SupportThingUsed",
        "SwitchShooter",
        "TeamWins",
        "UnitCantStop",
        "UnitCreate",
        "UnitDestroy",
        "UnitFrame",
        "UnitHealth",
        "UnitRemove",
        "UnitSetFireBehaviour",
        "UnitSetOwner",
        "UnitSetTeam",
        "UnitStopped",
        "UnitWillLoadQueued",
        "UpdateBalanceFactor",
        "UpdateCNMH_BuildPercentage",
        "UpdateLOS",
        "UpdateObjective",
        "UpdateTickSpeed",
        "Vote",
        "VoteEnded",
        # Field names, confirmed against the shipped binaries.
        "aCreator",
        "aDataType",
        "aDelta",
        "aDirection",
        "aDirection.x",
        "aDirection.y",
        "aDirection.z",
        "aEventId",
        "aFactor",
        "aFlag",
        "aFloat",
        "aForestDestroyRadius",
        "aFromSlot",
        "aHeading",
        "aHealth",
        "aMessage",
        "aMover",
        "aName",
        "aNumTa",
        "aPlayer",
        "aPlayerFrom",
        "aPlayerScore",
        "aPlayerTaunted",
        "aPos",
        "aPosition.x",
        "aPosition.y",
        "aPosition.z",
        "aProjectile",
        "aRadius",
        "aRequestType",
        "aShooter",
        "aSlot",
        "aSource.x",
        "aSource.y",
        "aSource.z",
        "aSpectatorLos",
        "aState",
        "aStrength",
        "aSupportThing",
        "aSupportUpgradeLevel",
        "aSupportUppgradeLevel",
        "aTAId",
        "aTeam",
        "aTeamChat",
        "aTime",
        "aTimeSinceCreation",
        "aTimeToLive",
        "aToSlot",
        "aTrackFactor",
        "aType",
        "aUnit",
        "aVector.x",
        "aVector.y",
        "aVector.z",
        "anExplosionForce",
        "anId",
        "anOptionalTacAidName",
        "anOptionalUnitId",
        "anUpgradeLevel",
    )
}


def decompress(path: str | pathlib.Path) -> bytes:
    """Inflate every zlib stream in a `.wicdemo` file and concatenate the output."""
    raw = pathlib.Path(path).read_bytes()
    out = bytearray()
    offset = RAW_HEADER_BYTES
    while offset < len(raw):
        if raw[offset : offset + 2] not in ZLIB_HEADERS:
            offset += 1
            continue
        try:
            obj = zlib.decompressobj()
            pieces = []
            feed = offset
            while feed < len(raw) and not obj.eof:
                part = raw[feed : feed + 65536]
                pieces.append(obj.decompress(part))
                used = len(part) - len(obj.unused_data)
                feed += used
                if used == 0:
                    break
            chunk = b"".join(pieces)
            consumed = feed - offset
        except zlib.error:
            offset += 1
            continue
        if not chunk or consumed <= 0:
            offset += 1
            continue
        out.extend(chunk)
        offset += consumed
    return bytes(out)


@dataclass(frozen=True)
class Field:
    hash: int
    flag: int
    raw: bytes

    @property
    def u32(self) -> int:
        return struct.unpack("<I", self.raw)[0]

    @property
    def i32(self) -> int:
        return struct.unpack("<i", self.raw)[0]

    @property
    def f32(self) -> float:
        return struct.unpack("<f", self.raw)[0]

    def name(self, table: "NameTable | None" = None) -> str:
        if self.hash in KNOWN_NAMES:
            return KNOWN_NAMES[self.hash]
        if table is not None:
            resolved = table.lookup(self.hash)
            if resolved:
                return resolved
        return f"{self.hash:08x}"


@dataclass(frozen=True)
class Envelope:
    offset: int
    total: int
    time: float
    message: int
    body_start: int
    body_end: int

    def message_name(self, table: "NameTable | None" = None) -> str:
        if self.message in KNOWN_NAMES:
            return KNOWN_NAMES[self.message]
        if table is not None:
            resolved = table.lookup(self.message)
            if resolved:
                return resolved
        return f"{self.message:08x}"


def read_envelope(data: bytes, offset: int) -> Envelope | None:
    """Parse an envelope header at `offset`, or return None if it is not one."""
    if offset < 0 or offset + ENVELOPE_HEADER_BYTES > len(data):
        return None
    if (
        data[offset : offset + 4] != ENVELOPE_TAG
        or data[offset + 4 : offset + 8] != ENVELOPE_TYPE
        or data[offset + 8] != ENVELOPE_FLAG
    ):
        return None
    total = struct.unpack_from("<I", data, offset + 9)[0]
    if total < ENVELOPE_HEADER_BYTES or offset + total > len(data):
        return None
    return Envelope(
        offset=offset,
        total=total,
        time=struct.unpack_from("<f", data, offset + 13)[0],
        message=struct.unpack_from("<I", data, offset + 17)[0],
        body_start=offset + ENVELOPE_HEADER_BYTES,
        body_end=offset + total,
    )


def fields(data: bytes, envelope: Envelope) -> tuple[list[Field], bytes]:
    """Split an envelope body into 17-byte fields plus any trailing blob."""
    out: list[Field] = []
    pos = envelope.body_start
    end = envelope.body_end
    while pos + FIELD_BYTES <= end:
        if (
            data[pos + 4 : pos + 8] != FIELD_SEP
            or data[pos + 9 : pos + 13] != FIELD_SEP
        ):
            break
        out.append(
            Field(
                hash=struct.unpack_from("<I", data, pos)[0],
                flag=data[pos + 8],
                raw=data[pos + 13 : pos + 17],
            )
        )
        pos += FIELD_BYTES
    return out, data[pos:end]


def find_chain_anchor(data: bytes, confirm: int = 6) -> int | None:
    """Find the first offset where `confirm` envelopes link back to back."""
    pos = 0
    while True:
        pos = data.find(ENVELOPE_TAG, pos)
        if pos < 0:
            return None
        cursor, ok = pos, True
        for _ in range(confirm):
            envelope = read_envelope(data, cursor)
            if envelope is None:
                ok = False
                break
            cursor += envelope.total
        if ok:
            return pos
        pos += 1


def walk(data: bytes, anchor: int | None = None) -> Iterator[Envelope]:
    """Yield every envelope in stream order, resynchronising across damage."""
    if anchor is None:
        anchor = find_chain_anchor(data)
    if anchor is None:
        return
    cursor = anchor
    while cursor < len(data):
        envelope = read_envelope(data, cursor)
        if envelope is None:
            nxt = data.find(ENVELOPE_TAG, cursor + 1)
            if nxt < 0:
                return
            cursor = nxt
            continue
        yield envelope
        cursor += envelope.total


# Recorder's aSlot value, written in the metadata chunk. Stored little-endian as
# ad 07 93 4b, i.e. the u32 0x4b9307ad.
POV_PLAYER_HASH = 0x4B9307AD


def recorder_slot(data: bytes) -> int | None:
    """Read the point-of-view player slot from the metadata region."""
    pos = data.find(struct.pack("<I", POV_PLAYER_HASH))
    if pos < 0 or data[pos + 4 : pos + 8] != FIELD_SEP:
        return None
    slot = struct.unpack_from("<I", data, pos + 13)[0]
    return slot if slot <= 15 else None


_IDENTIFIER = re.compile(rb"[A-Za-z_][A-Za-z0-9_]{2,80}")


class NameTable:
    """Inverse BinTag hash table built from identifier strings in the binaries."""

    def __init__(self, binaries: Sequence[str | pathlib.Path]):
        self._table: dict[int, set[str]] = {}
        for binary in binaries:
            path = pathlib.Path(binary)
            if not path.is_file():
                continue
            for match in _IDENTIFIER.finditer(path.read_bytes()):
                token = match.group()
                try:
                    text = token.decode("ascii")
                except UnicodeDecodeError:
                    continue
                self._table.setdefault(zlib.adler32(token), set()).add(text)

    def lookup(self, value: int) -> str | None:
        names = self._table.get(value)
        if not names or len(names) > 1:
            return None
        return next(iter(names))

    def candidates(self, value: int) -> list[str]:
        return sorted(self._table.get(value, ()))
