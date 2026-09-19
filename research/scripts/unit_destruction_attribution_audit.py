#!/usr/bin/env python3
"""Build a conservative evidence ledger for unit destructions.

Exact replay facts stay separate from causal candidates. ``SendTATaunt`` proves a
tactical-aid actor and affected player at a cumulative damage threshold; nearby or
co-timed records remain research evidence and never become kill attribution here.
"""

from __future__ import annotations

import argparse
import bisect
import concurrent.futures
import hashlib
import json
import math
import multiprocessing
import os
import pathlib
import struct
from collections import Counter
from dataclasses import dataclass

from ta_support_catalogue import parse_support_localization
from wic_bintag import decompress, fields, name_hash, recorder_slot, walk
from wic_ice import (
    BridgeInstance,
    CloudTypeDefinition,
    UnitTypeParasites,
    shipped_bridge_instances,
    shipped_support_cloud_types,
    shipped_unit_type_parasites,
)
from wic_sdf import SdfArchive

MSG_CLOCK = name_hash("SetGameModeData_Float")
MSG_CHANGE_HONORS = name_hash("ChangeHonors")
MSG_TAUNT = name_hash("SendTATaunt")
MSG_SUPPORT_USED = name_hash("SupportThingUsed")
MSG_SUPPORT_DEPLOYED = name_hash("SupportThingSpawnedDelayed")
MSG_SUPPORT_MARKER = name_hash("SupportThingMarker")
MSG_UNIT_CREATE = name_hash("UnitCreate")
MSG_BLINK_UNIT = name_hash("BlinkUnit")
MSG_RESUME_DISPAND = name_hash("ResumeDispand")
MSG_BLOWER_BLEW = name_hash("BlowerBlew")
MSG_CREATE_CLOUD = name_hash("CreateCloud")
MSG_UNIT_REMOVE = name_hash("UnitRemove")
MSG_UNIT_DESTROY = name_hash("UnitDestroy")
MSG_BUILDING_DAMAGED = name_hash("BuildingDamaged")
MSG_BUILDING_SET_SLOT_STATE = name_hash("BuildingSetSlotState")
MSG_EXPLOSION = name_hash("SpawnExplosion")
MSG_EXPLOSION_WITH_CRATER = name_hash("SpawnExplosionWithCrater")
MSG_SHOOTER_ACQUIRED_TARGET = name_hash("ShooterAcquiredTarget")
MSG_SHOOTER_ATTACKING_UNIT = name_hash("ShooterIsAttacking_Unit")
MSG_UNIT_HEALTH = name_hash("UnitHealth")
MSG_SET_SCORE = name_hash("SetScore")
MSG_REPAIRABLE_PROP_DAMAGED = name_hash("RepairablePropDamaged")
MSG_UNIT_FRAME = name_hash("UnitFrame")
FIELD_UNIT_FRAME_COMPACT = name_hash("UnitFrameDataCompact")
FIELD_UNIT_FRAME_FULL = name_hash("UnitFrameData")

NORMAL_PROJECTILES = {
    name_hash(name): name
    for name in (
        "ProjectileStraightCreate",
        "ProjectileBallisticCreate",
        "ProjectileHomingTargetCreate",
        "ProjectileHomingUnitCreate",
    )
}
SUPPORT_PROJECTILES = {
    name_hash(name): name
    for name in (
        "ProjectileStraightSupportCreate",
        "ProjectileBallisticSupportCreate",
        "ProjectileHomingSupportCreate_Position",
        "ProjectileHomingSupportCreate_Unit",
    )
}
FIELD_AUNIT = name_hash("aUnit")
FIELD_ATYPE = name_hash("aType")
FIELD_AHEADING = name_hash("aHeading")
FIELD_ATIME_TO_LIVE = name_hash("aTimeToLive")
FIELD_ATEAM = name_hash("aTeam")
FIELD_AFORTIFICATION_ID = name_hash("aFortificationId")
FIELD_ABUILDING_NAME = name_hash("aBuildingName")
FIELD_AREAL_SLOT_ID = name_hash("aRealSlotId")
FIELD_AWILL_OCCUPY_SLOT_FLAG = name_hash("aWillOccupySlotFlag")
FIELD_ANAME = name_hash("aName")
FIELD_AHEALTH = name_hash("aHealth")
FIELD_ASTATE = name_hash("aState")
FIELD_AFLAG = name_hash("aFlag")
FIELD_BLINK_ON_OFF_FLAG = name_hash("aBlinkOnOffFlag")
FIELD_BLINK_TIME = name_hash("aBlinkTime")
FIELD_BLINK_FREQ = name_hash("aBlinkFreq")
FIELD_REMOVE_SPECIALIST_FLAG = name_hash("aIsToBeReplacedBySpecialistFlag")

DEFAULT_SUPPORT_ARCHIVE = "local/binaries/server/wic_ds.sdf"
DEFAULT_SUPPORT_LOCALIZATION = "maps/supportweapons.loc"
KILLER_UNIT_SENTINEL = 512
CLOCK_RESET_THRESHOLD_SECONDS = 5.0
RECORDING_RESET_DROP_SECONDS = 1.0
MAX_CLOCK_SECONDS = 24.0 * 60.0 * 60.0
FUTURE_ORDER_TOLERANCE_SECONDS = 0.25
NORMAL_PROJECTILE_LOOKBACK_SECONDS = 0.5
SUPPORT_EVIDENCE_LOOKBACK_SECONDS = 3.0
SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS = 30.0
SHOOTER_TARGET_LOOKBACK_SECONDS = 10.0
SHOOTER_TARGET_WINDOWS_SECONDS = (0.5, 1.0, 3.0, 10.0)
PROJECTILE_SPATIAL_THRESHOLDS = (1.0, 3.0, 5.0, 10.0, 25.0, 50.0)
SUPPORT_IMPACT_THRESHOLDS = (1.0, 3.0, 5.0, 10.0, 25.0, 50.0)
MULTI_VICTIM_CLUSTER_WINDOWS_SECONDS = (0.25, 1.0, 3.0)
SCORE_DELTA_WINDOWS_SECONDS = (0.25, 1.0, 3.0)
HONORS_EVENT_GAP_THRESHOLDS = (1, 2, 3, 5, 10)
BRIDGE_DESTRUCTION_WINDOWS_SECONDS = (0.0, 0.1, 0.25, 0.5)
BRIDGE_SPATIAL_THRESHOLDS = (5.0, 10.0, 25.0, 50.0, 100.0)
BLOWER_FUSE_SECONDS = 0.7
BLOWER_FUSE_WINDOWS_SECONDS = (0.6, 0.7, 0.75, 1.0, 2.0)
CLOUD_TTL_TOLERANCE_SECONDS = 0.001
COMPACT_FRAME_MAX_POSITION_ERROR = math.sqrt(3.0) / (2.0 * 42.0)
MULTIPLAYER_SUPPORT_SECTIONS = frozenset(("US", "USSR", "NATO"))

INFANTRY_SOLDIER_TYPE_IDS = {
    0x0CA502EA,
    0x0E970333,
    0x107F0379,
    0x11D80374,
    0x12CD038F,
    0x15EB03EE,
    0x163C0403,
    0x16FB0409,
    0x174C041E,
    0x17D00422,
    0x180B0435,
    0x1EA104AC,
    0x1EDC04BF,
    0x1FE704C7,
    0x202204DA,
    0x240F0534,
    0x2C7E05BE,
    0x2E1505D9,
    0x38A0068B,
    0x3F7906E0,
    0x42AD0715,
    0x44950730,
    0x4A10076A,
    0x4C130785,
    0x5B000845,
    0x6BD30900,
    0x6E42091B,
}


@dataclass(frozen=True)
class ClockSample:
    offset: int
    remaining_seconds: float
    elapsed_seconds: float


def _rounded(value: float) -> float:
    return round(value, 6)


def _f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def _f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def synthetic_terminal_direction_bits() -> frozenset[tuple[int, int, int]]:
    """Enumerate the exact normalized rand()%100-50 directions from b35."""
    directions = set()
    for x in range(-50, 50):
        for z in range(-50, 50):
            norm = _f32(math.sqrt(_f32(float(x * x + z * z))))
            if norm > 0.0:
                inverse = _f32(1.0 / norm)
                direction_x = _f32(inverse * _f32(float(x)))
                direction_z = _f32(inverse * _f32(float(z)))
            else:
                direction_x = 0.0
                direction_z = 0.0
            directions.add(
                (_f32_bits(direction_x), _f32_bits(0.0), _f32_bits(direction_z))
            )
    return frozenset(directions)


SYNTHETIC_TERMINAL_DIRECTION_BITS = synthetic_terminal_direction_bits()


def _timeline_rounded(value: float) -> float:
    return math.floor(value * 1000.0 + 0.5) / 1000.0


def _position(body, start: int) -> list[float]:
    return [_rounded(field.f32) for field in body[start : start + 3]]


def unit_create_fortification_id(body) -> int | None:
    """Return the exact optional UnitCreate fortification link."""
    if len(body) <= 14 or body[14].hash != FIELD_AFORTIFICATION_ID:
        return None
    return body[14].u32 if body[14].i32 >= 0 else None


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def _message_offset(envelope) -> int:
    return envelope.body_start - 4


def primary_chain_end_offset(data: bytes) -> int:
    """Return the canonical cut before the replay's duplicated summary pass."""
    previous_time = None
    for envelope in walk(data):
        if (
            previous_time is not None
            and previous_time - envelope.time > RECORDING_RESET_DROP_SECONDS
        ):
            return envelope.offset
        previous_time = envelope.time
    return len(data)


def build_clock_samples(raw_samples: list[tuple[int, float]]) -> list[ClockSample]:
    """Mirror the schema-v10 countdown timeline construction."""
    if not raw_samples:
        return []
    first_offset, first_remaining = raw_samples[0]
    samples = [ClockSample(first_offset, first_remaining, 0.0)]
    phase_start_elapsed = 0.0
    phase_start_remaining = first_remaining
    phase_min_remaining = first_remaining
    previous_remaining = first_remaining
    for offset, remaining in raw_samples[1:]:
        if remaining - previous_remaining > CLOCK_RESET_THRESHOLD_SECONDS:
            phase_start_elapsed += max(phase_start_remaining - phase_min_remaining, 0.0)
            phase_start_remaining = remaining
            phase_min_remaining = remaining
        else:
            phase_min_remaining = min(phase_min_remaining, remaining)
        samples.append(
            ClockSample(
                offset,
                remaining,
                phase_start_elapsed
                + max(phase_start_remaining - phase_min_remaining, 0.0),
            )
        )
        previous_remaining = remaining
    return samples


def timeline_time_at_offset(
    samples: list[ClockSample], offset: int, offsets: list[int] | None = None
) -> float:
    """Mirror schema-v10 byte-offset interpolation between countdown samples."""
    if not samples or offset <= samples[0].offset:
        return 0.0
    if offsets is None:
        offsets = [sample.offset for sample in samples]
    insertion = bisect.bisect_right(offsets, offset)
    if insertion >= len(samples):
        return samples[-1].elapsed_seconds
    previous = samples[insertion - 1]
    following = samples[insertion]
    byte_span = following.offset - previous.offset
    if byte_span == 0 or following.elapsed_seconds <= previous.elapsed_seconds:
        return previous.elapsed_seconds
    byte_progress = (offset - previous.offset) / byte_span
    return (
        previous.elapsed_seconds
        + (following.elapsed_seconds - previous.elapsed_seconds) * byte_progress
    )


def _extract_clock_samples(data: bytes, end_offset: int) -> list[ClockSample]:
    raw_samples = []
    countdown_started = False
    for envelope in walk(data):
        if envelope.offset >= end_offset:
            break
        if envelope.message != MSG_CLOCK:
            continue
        body, trailing = fields(data, envelope)
        if len(body) < 2 or trailing:
            continue
        remaining = body[1].f32
        if (
            body[0].u32 != 1
            or not math.isfinite(remaining)
            or abs(remaining) > MAX_CLOCK_SECONDS
        ):
            continue
        countdown_started |= remaining != 0.0
        if countdown_started:
            raw_samples.append((_message_offset(envelope), remaining))
    return build_clock_samples(raw_samples)


def load_support_catalogue(
    archive_path: str | pathlib.Path,
    entry_path: str = DEFAULT_SUPPORT_LOCALIZATION,
) -> tuple[dict[int, str], set[int]]:
    """Return exact support names and schema-v17 faction-TA projectile IDs."""
    archive = SdfArchive.open(archive_path)
    matches = [entry for entry in archive.entries if entry.path == entry_path]
    if len(matches) != 1:
        raise ValueError(
            f"expected exactly one {entry_path!r} entry, found {len(matches)}"
        )
    definitions = parse_support_localization(archive.read_entry(matches[0]))
    support_names = {
        name_hash(definition.internal_name): definition.internal_name
        for definition in definitions.values()
    }
    tactical_aid_projectile_ids = {
        name_hash(definition.internal_name)
        for definition in definitions.values()
        if (
            definition.section in {"US", "NATO", "USSR"}
            and (
                "CHILD_" not in definition.internal_name
                or definition.internal_name.startswith("CHILD_HeavyAirSupport_")
            )
        )
    }
    return support_names, tactical_aid_projectile_ids


def replay_map_name(data: bytes) -> str | None:
    """Read the exact map path from the replay metadata prefix."""
    if len(data) <= 47:
        return None
    end = data.find(b"\0", 47)
    if end < 0:
        return None
    try:
        value = data[47:end].decode("utf-8")
    except UnicodeDecodeError:
        return None
    return value or None


def decode_compact_bool_prefix(
    data: bytes, expected_hash: int
) -> tuple[bool, bytes] | None:
    """Decode one compact 14-byte BinTag boolean field and retain its suffix."""
    if len(data) < 14:
        return None
    field_hash, first_size, flag, second_size, value = struct.unpack_from(
        "<IIBIB", data
    )
    if (
        field_hash != expected_hash
        or first_size != 14
        or flag != 3
        or second_size != 14
        or value not in (0, 1)
    ):
        return None
    return value == 1, data[14:]


def decode_trailing_bool_field(data: bytes, expected_hash: int) -> bool | None:
    """Decode an exact compact boolean trailer with no following fields."""
    decoded = decode_compact_bool_prefix(data, expected_hash)
    if decoded is None or decoded[1]:
        return None
    return decoded[0]


def decode_blink_trailing_fields(data: bytes) -> tuple[bool, float, float] | None:
    """Decode BlinkUnit's compact bool followed by two standard float fields."""
    decoded = decode_compact_bool_prefix(data, FIELD_BLINK_ON_OFF_FLAG)
    if decoded is None:
        return None
    blink_on, remaining = decoded
    values = []
    for expected_hash in (FIELD_BLINK_TIME, FIELD_BLINK_FREQ):
        if len(remaining) < 17:
            return None
        field_hash, first_size, flag, second_size = struct.unpack_from(
            "<IIBI", remaining
        )
        if (
            field_hash != expected_hash
            or first_size != 17
            or flag != 2
            or second_size != 17
        ):
            return None
        values.append(struct.unpack_from("<f", remaining, 13)[0])
        remaining = remaining[17:]
    if remaining:
        return None
    return blink_on, values[0], values[1]


def load_support_names(
    archive_path: str | pathlib.Path,
    entry_path: str = DEFAULT_SUPPORT_LOCALIZATION,
) -> dict[int, str]:
    """Return exact Adler-32 support names from shipped localization."""
    return load_support_catalogue(archive_path, entry_path)[0]


def link_taunts(
    catalogue: list[int],
    taunts: list[dict],
    destructions: list[dict],
    support_names: dict[int, str],
) -> list[dict]:
    """Resolve taunt indices and attach same-tick victim destruction candidates."""
    output = []
    for taunt in taunts:
        ta_index = taunt["taIndex"]
        support_id = catalogue[ta_index] if ta_index < len(catalogue) else None
        candidates = [
            destruction
            for destruction in destructions
            if destruction["rawEventTime"] == taunt["rawEventTime"]
            and destruction["victimPlayerId"] == taunt["playerTaunted"]
        ]
        output.append(
            {
                **{k: v for k, v in taunt.items() if not k.startswith("_")},
                "supportId": support_id,
                "supportIdHex": (
                    f"0x{support_id:08x}" if support_id is not None else None
                ),
                "supportName": support_names.get(support_id),
                "attribution": {
                    "class": "exact" if support_id is not None else "invalid",
                    "basis": (
                        "serializedSendTATauntWithReplayCatalogueIndex"
                        if support_id is not None
                        else "taIndexOutsideReplayCatalogue"
                    ),
                    "scope": "damageThresholdNotUnitKill",
                },
                "sameTickVictimDestructions": candidates,
                "destructionCandidateBasis": (
                    "sameRawTimestampAndVictimPlayer" if candidates else None
                ),
                "destructionAttribution": "candidate" if candidates else "none",
            }
        )
    return output


def events_in_window(
    events: list[dict],
    death_time: float,
    lookback: float,
    times: list[float] | None = None,
) -> list[dict]:
    """Return events within the documented temporal prescreen."""
    if times is None:
        times = [event["_rawTime"] for event in events]
    start = bisect.bisect_left(times, death_time - lookback)
    end = bisect.bisect_right(times, death_time + FUTURE_ORDER_TOLERANCE_SECONDS)
    return events[start:end]


def _same_tick(
    events: list[dict], raw_time: float, times: list[float] | None = None
) -> list[dict]:
    if times is None:
        times = [event["_rawTime"] for event in events]
    return events[
        bisect.bisect_left(times, raw_time) : bisect.bisect_right(times, raw_time)
    ]


def exact_tactical_aid_cause(
    destruction: dict,
    support_projectiles: list[dict],
    deployments: list[dict],
    tactical_aid_projectile_ids: set[int],
) -> tuple[int, int] | None:
    """Mirror schema-v17's exact-target support/team attribution boundary."""
    if destruction["killerUnitId"] != KILLER_UNIT_SENTINEL:
        return None
    victim = destruction.get("victimCreation")
    if victim is None:
        return None
    causes = []
    for projectile in support_projectiles:
        lag = destruction["_rawTime"] - projectile["_rawTime"]
        if (
            projectile["eventIndex"] >= destruction["eventIndex"]
            or not 0.0 <= lag <= SUPPORT_EVIDENCE_LOOKBACK_SECONDS
            or projectile.get("targetUnitId") != destruction["unitId"]
            or projectile.get("targetUnitCreationOffset") != victim.get("offset")
            or projectile["supportId"] not in tactical_aid_projectile_ids
        ):
            continue
        teams = {
            deployment["team"]
            for deployment in deployments
            if deployment["supportId"] == projectile["supportId"]
            and deployment["team"] in {1, 2, 3}
            and deployment["eventIndex"] < projectile["eventIndex"]
            and 0.0
            <= projectile["_rawTime"] - deployment["_rawTime"]
            <= SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS
        }
        if len(teams) == 1:
            causes.append((projectile["supportId"], teams.pop()))
    if causes and all(cause == causes[0] for cause in causes[1:]):
        return causes[0]
    return None


def cluster_multi_player_deaths(
    deaths: list[dict], window_seconds: float
) -> tuple[int, int, int]:
    """Partition adjacent deaths and count candidate multi-player clusters."""
    if not deaths:
        return 0, 0, 0
    ordered = sorted(deaths, key=lambda death: (death["_rawTime"], death["eventIndex"]))
    groups = [[ordered[0]]]
    for death in ordered[1:]:
        if death["_rawTime"] - groups[-1][-1]["_rawTime"] <= window_seconds:
            groups[-1].append(death)
        else:
            groups.append([death])
    multi_player = [
        group
        for group in groups
        if len({death["victimPlayerId"] for death in group}) >= 2
    ]
    return (
        len(multi_player),
        sum(len(group) for group in multi_player),
        sum(
            len({death["victimPlayerId"] for death in group}) for group in multi_player
        ),
    )


def _public_event(event: dict, death_time: float, reason: str) -> dict:
    row = {
        **event,
        "rawTimeDeltaFromDestruction": _rounded(death_time - event["_rawTime"]),
        "candidateBasis": reason,
        "causalAttribution": "candidateOnly",
    }
    return {key: value for key, value in row.items() if not key.startswith("_")}


def _without_private_fields(event: dict) -> dict:
    """Return a raw replay event without audit-only indexing fields."""
    return {key: value for key, value in event.items() if not key.startswith("_")}


def positive_score_candidate_windows(
    score_events: list[dict],
    death_time: float,
    death_event_index: int,
    event_times: list[float] | None = None,
) -> dict[str, list[dict]]:
    """Return positive cumulative score changes around one destruction.

    These are positive-control candidates only. ``SetScore`` is a periodic
    cumulative snapshot and does not identify the event responsible for a delta.
    """
    if event_times is None:
        event_times = [event["_rawTime"] for event in score_events]
    same_tick_scores = _same_tick(score_events, death_time, event_times)
    windows = {
        "sameTickAny": same_tick_scores,
        "sameTickBefore": [
            event
            for event in same_tick_scores
            if event["eventIndex"] < death_event_index
        ],
        "sameTickAfter": [
            event
            for event in same_tick_scores
            if event["eventIndex"] > death_event_index
        ],
    }
    for window in SCORE_DELTA_WINDOWS_SECONDS:
        windows[f"preceding{window:g}s"] = [
            event
            for event in events_in_window(
                score_events,
                death_time,
                window,
                event_times,
            )
            if event["_rawTime"] <= death_time
            and event["eventIndex"] < death_event_index
        ]
    return {
        label: [
            event
            for event in candidates
            if event["playerId"] is not None
            and event["delta"] is not None
            and event["delta"] > 0
        ]
        for label, candidates in windows.items()
    }


def preceding_positive_honors_gap(
    honors_events: list[dict],
    death_time: float,
    death_event_index: int,
    event_times: list[float] | None = None,
) -> int | None:
    """Return the event gap to the nearest same-tick positive honors delta.

    ``ChangeHonors`` is recorder-scoped and carries no victim or firing-unit
    identity. The gap is retained solely to measure positive-control failures.
    """
    if event_times is None:
        event_times = [event["_rawTime"] for event in honors_events]
    candidates = [
        event
        for event in _same_tick(honors_events, death_time, event_times)
        if event["eventIndex"] < death_event_index and event["delta"] > 0.0
    ]
    if not candidates:
        return None
    nearest = max(candidates, key=lambda event: event["eventIndex"])
    return death_event_index - nearest["eventIndex"]


def analyse_blink_lifecycles(
    blink_events: list[dict],
    lifecycle_terminals: dict[int, dict],
    destructions: list[dict],
) -> tuple[Counter, list[dict]]:
    """Measure pending-removal outcomes without treating blink as deletion.

    The server can terminate a blinking lifecycle either with ``UnitRemove`` or
    the independently lethal ``UnitDestroy`` path. This helper records that raw
    distinction and deliberately emits no causal classification.
    """
    public_destructions = [_without_private_fields(item) for item in destructions]
    destruction_by_creation_offset = {
        item["victimCreation"]["offset"]: item
        for item in public_destructions
        if item["victimCreation"] is not None
    }
    metrics = Counter()
    outcomes = []
    for blink in blink_events:
        public_blink = _without_private_fields(blink)
        metrics["events"] += 1
        metrics["onEvents" if blink["blinkOn"] else "offEvents"] += 1
        creation_offset = blink["unitCreationOffset"]
        if creation_offset is None:
            metrics["eventsWithoutActiveLifecycle"] += 1
            continue
        metrics["eventsWithActiveLifecycle"] += 1
        if not blink["blinkOn"]:
            continue
        terminal = lifecycle_terminals.get(creation_offset)
        if terminal is None or terminal["eventIndex"] <= blink["eventIndex"]:
            metrics["onWithoutLaterTerminal"] += 1
            continue
        delta = _rounded(terminal["rawEventTime"] - blink["rawEventTime"])
        advertised = blink["blinkTimeSeconds"]
        outcome = {
            "blink": public_blink,
            "terminal": terminal,
            "terminalDeltaSeconds": delta,
        }
        if terminal["reason"] == "UnitRemove":
            metrics["onThenUnitRemove"] += 1
            metrics["onThenUnitRemoveBeforeAdvertisedTime"] += delta + 1e-6 < advertised
            metrics["onThenUnitRemoveAtOrAfterAdvertisedTime"] += (
                delta + 1e-6 >= advertised
            )
        elif terminal["reason"] == "UnitDestroy":
            metrics["onThenUnitDestroy"] += 1
            metrics["onThenUnitDestroyBeforeAdvertisedTime"] += (
                delta + 1e-6 < advertised
            )
            metrics["onThenUnitDestroyAtOrAfterAdvertisedTime"] += (
                delta + 1e-6 >= advertised
            )
            destruction = destruction_by_creation_offset.get(creation_offset)
            if destruction is not None:
                if destruction["killerUnitId"] == KILLER_UNIT_SENTINEL:
                    metrics["onThenUnitDestroyKiller512"] += 1
                elif destruction["killerPlayerId"] is not None:
                    metrics["onThenUnitDestroyActiveKiller"] += 1
                else:
                    metrics["onThenUnitDestroyOtherUnresolvedKiller"] += 1
        else:
            metrics["onThenLifecycleReplacement"] += 1
        outcomes.append(outcome)
    return metrics, outcomes


def analyse_blower_lifecycles(
    blower_events: list[dict],
    lifecycle_terminals: dict[int, dict],
    destructions: list[dict],
) -> tuple[Counter, list[dict]]:
    """Measure exact owner-triggered blower requests and later outcomes.

    Ghidra proves ``BlowerBlew`` is broadcast only after the server accepts a
    request from the unit's owner. The server arms a 0.7-second fuse, but the
    replay does not serialize the later update that actually executes the blast.
    Outcomes therefore remain evidence rows until corpus controls establish
    whether fuse timing can distinguish detonation from an intervening kill.
    """
    public_destructions = [_without_private_fields(item) for item in destructions]
    destruction_by_creation_offset = {
        item["victimCreation"]["offset"]: item
        for item in public_destructions
        if item["victimCreation"] is not None
    }
    metrics = Counter()
    outcomes = []
    for event in blower_events:
        public_event = _without_private_fields(event)
        metrics["events"] += 1
        creation_offset = event["unitCreationOffset"]
        if creation_offset is None:
            metrics["eventsWithoutActiveLifecycle"] += 1
            continue
        metrics["eventsWithActiveLifecycle"] += 1
        terminal = lifecycle_terminals.get(creation_offset)
        if terminal is None or terminal["eventIndex"] <= event["eventIndex"]:
            metrics["eventsWithoutLaterTerminal"] += 1
            continue
        delta = _rounded(terminal["rawEventTime"] - event["rawEventTime"])
        outcome = {
            "blowerBlew": public_event,
            "terminal": terminal,
            "terminalDeltaSeconds": delta,
            "scheduledFuseSeconds": BLOWER_FUSE_SECONDS,
            "causalAttribution": "candidateOnly",
        }
        reason = terminal["reason"]
        metrics[f"then{reason}"] += 1
        for window in BLOWER_FUSE_WINDOWS_SECONDS:
            metrics[f"terminalWithin{window:g}s"] += delta <= window + 1e-6
        if reason == "UnitDestroy":
            destruction = destruction_by_creation_offset.get(creation_offset)
            if destruction is not None:
                if destruction["killerUnitId"] == KILLER_UNIT_SENTINEL:
                    metrics["thenUnitDestroyKiller512"] += 1
                elif destruction["killerPlayerId"] is not None:
                    metrics["thenUnitDestroyActiveKiller"] += 1
                else:
                    metrics["thenUnitDestroyOtherUnresolvedKiller"] += 1
                outcome["destruction"] = destruction
        outcomes.append(outcome)
    return metrics, outcomes


def _distance(left: list[float], right: list[float]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(left, right, strict=True)))


def bridge_destruction_candidates(
    destruction: dict,
    bridge_destructions: list[dict],
    window_seconds: float,
) -> list[dict]:
    """Return exact bridge-state events near a death without claiming cause."""
    victim_position = destruction.get("victimPositionAtDestruction")
    victim_frame = destruction.get("victimLastFrame")
    candidates = []
    for bridge in bridge_destructions:
        delta = destruction["_rawTime"] - bridge["_rawTime"]
        if abs(delta) > window_seconds:
            continue
        row = {
            **_without_private_fields(bridge),
            "rawTimeDeltaFromBridgeDestruction": _rounded(delta),
            "eventIndexDeltaFromBridgeDestruction": (
                destruction["eventIndex"] - bridge["eventIndex"]
            ),
            "horizontalDistanceFromBridge": None,
            "insideExactServerKillBounds": None,
            "victimLastFrameAgeSeconds": None,
            "victimLastFrameSameRawTick": False,
            "syntheticTerminalDirection": destruction.get(
                "syntheticTerminalDirection", False
            ),
            "candidateBasis": "exactMapBridgeState3WithinTimeWindow",
            "causalAttribution": "candidateOnly",
        }
        if victim_position is not None:
            row["horizontalDistanceFromBridge"] = _rounded(
                math.hypot(
                    victim_position[0] - bridge["position"][0],
                    victim_position[2] - bridge["position"][2],
                )
            )
            min_x, min_z, max_x, max_z = bridge["killBoundsXZ"]
            row["insideExactServerKillBounds"] = (
                min_x < victim_position[0] < max_x
                and min_z < victim_position[2] < max_z
            )
        if victim_frame is not None:
            frame_age = destruction["_rawTime"] - victim_frame["rawEventTime"]
            row["victimLastFrameAgeSeconds"] = _rounded(frame_age)
            row["victimLastFrameSameRawTick"] = victim_frame[
                "rawTimeBits"
            ] == _f32_bits(destruction["_rawTime"])
        candidates.append(row)
    return candidates


def preceding_same_tick_blast_candidates(
    spatial_explosions: list[dict], destruction_event_index: int
) -> list[dict]:
    """Return ordered spatial candidates without claiming cause or actor."""
    return [
        event
        for event in spatial_explosions
        if event["eventIndex"] < destruction_event_index
        and event["distanceToVictim"] <= event["radius"]
    ]


def support_impact_causes(
    projectiles: list[dict],
    explosions: list[dict],
    deployments: list[dict],
    tactical_aid_projectile_ids: set[int],
    threshold: float,
) -> set[tuple[int, int]]:
    """Return uniquely teamed projectile/impact hypotheses, never attribution."""
    causes = set()
    for projectile in projectiles:
        if projectile["supportId"] not in tactical_aid_projectile_ids:
            continue
        teams = {
            deployment["team"]
            for deployment in deployments
            if deployment["supportId"] == projectile["supportId"]
            and deployment["team"] in {1, 2, 3}
            and deployment["eventIndex"] < projectile["eventIndex"]
            and 0.0
            <= projectile["_rawTime"] - deployment["_rawTime"]
            <= SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS
        }
        if len(teams) != 1:
            continue
        for explosion in explosions:
            lag = explosion["_rawTime"] - projectile["_rawTime"]
            if (
                projectile["eventIndex"] >= explosion["eventIndex"]
                or not 0.0 <= lag <= SUPPORT_EVIDENCE_LOOKBACK_SECONDS
            ):
                continue
            projected = [
                projectile["position"][index] + projectile["vector"][index] * lag
                for index in range(3)
            ]
            if _distance(projected, explosion["position"]) <= threshold:
                causes.add((projectile["supportId"], next(iter(teams))))
                break
    return causes


def projectile_spatial_event(
    event: dict, death_time: float, victim_position: list[float]
) -> dict:
    """Attach source and constant-vector projections without claiming ballistics."""
    delta = death_time - event["_rawTime"]
    projected = [
        position + vector * delta
        for position, vector in zip(event["position"], event["vector"], strict=True)
    ]
    row = _public_event(event, death_time, "sameTimeWindowSpatialResearch")
    row["sourceDistanceToVictim"] = _rounded(
        _distance(event["position"], victim_position)
    )
    row["constantVectorProjectedPosition"] = [_rounded(value) for value in projected]
    row["constantVectorDistanceToVictim"] = _rounded(
        _distance(projected, victim_position)
    )
    row["constantVectorSemantics"] = "researchHypothesisNotValidatedBallistics"
    return row


def _normal_projectile(
    body, envelope, event_index: int, units: dict[int, dict]
) -> dict:
    firing_unit_id = next(
        (field.u32 for field in reversed(body) if field.hash == FIELD_AUNIT), None
    )
    firing_unit = units.get(firing_unit_id) if firing_unit_id is not None else None
    row = {
        "_rawTime": envelope.time,
        "eventIndex": event_index,
        "offset": envelope.offset,
        "rawEventTime": _rounded(envelope.time),
        "kind": NORMAL_PROJECTILES[envelope.message],
        "projectileId": body[0].u32,
        "position": _position(body, 1),
        "vector": _position(body, 4),
        "shooterIndex": body[7].u32,
        "firingUnitId": firing_unit_id,
        "firingPlayerId": firing_unit.get("playerId") if firing_unit else None,
        "firingTeam": firing_unit.get("team") if firing_unit else None,
    }
    if envelope.message == name_hash("ProjectileHomingUnitCreate") and len(body) >= 11:
        row["targetUnitId"] = body[9].u32
        target = units.get(row["targetUnitId"])
        row["targetUnitCreationOffset"] = target.get("offset") if target else None
    return row


def exact_target_normal_actors(
    destruction: dict, projectiles: list[dict]
) -> set[tuple[int, int]]:
    """Return exact-target actors without claiming which projectile was lethal."""
    victim = destruction.get("victimCreation")
    if victim is None:
        return set()
    return {
        (projectile["firingPlayerId"], projectile["firingTeam"])
        for projectile in projectiles
        if projectile["eventIndex"] < destruction["eventIndex"]
        and 0.0
        <= destruction["_rawTime"] - projectile["_rawTime"]
        <= NORMAL_PROJECTILE_LOOKBACK_SECONDS
        and projectile.get("targetUnitId") == destruction["unitId"]
        and projectile.get("targetUnitCreationOffset") == victim.get("offset")
        and projectile.get("firingPlayerId") is not None
        and projectile.get("firingTeam") is not None
    }


def _support_projectile(
    body,
    envelope,
    event_index: int,
    support_names: dict[int, str],
    active_units: dict[int, dict],
) -> dict:
    support_id = body[1].u32
    row = {
        "_rawTime": envelope.time,
        "eventIndex": event_index,
        "offset": envelope.offset,
        "rawEventTime": _rounded(envelope.time),
        "kind": SUPPORT_PROJECTILES[envelope.message],
        "projectileId": body[0].u32,
        "supportId": support_id,
        "supportIdHex": f"0x{support_id:08x}",
        "supportName": support_names.get(support_id),
        "position": _position(body, 2),
        "vector": _position(body, 5),
        "upgradeLevel": body[8].u32 if len(body) >= 9 else None,
        "playerId": None,
    }
    if envelope.message == name_hash("ProjectileHomingSupportCreate_Unit"):
        unit_fields = [field.u32 for field in body[2:] if field.hash == FIELD_AUNIT]
        row["targetUnitId"] = unit_fields[0] if unit_fields else None
        target = active_units.get(row["targetUnitId"])
        row["targetUnitCreationOffset"] = target.get("offset") if target else None
    return row


def _shooter_target_event(
    body, envelope, event_index: int, active_units: dict[int, dict]
) -> dict:
    firing_unit_id = body[0].u32
    target_unit_id = body[1].u32
    firing_unit = active_units.get(firing_unit_id)
    target_unit = active_units.get(target_unit_id)
    return {
        "_rawTime": envelope.time,
        "eventIndex": event_index,
        "offset": envelope.offset,
        "rawEventTime": _rounded(envelope.time),
        "kind": (
            "ShooterAcquiredTarget"
            if envelope.message == MSG_SHOOTER_ACQUIRED_TARGET
            else "ShooterIsAttacking_Unit"
        ),
        "firingUnitId": firing_unit_id,
        "firingPlayerId": firing_unit.get("playerId") if firing_unit else None,
        "firingTeam": firing_unit.get("team") if firing_unit else None,
        "firingUnitCreationOffset": firing_unit.get("offset") if firing_unit else None,
        "targetUnitId": target_unit_id,
        "targetUnitCreationOffset": target_unit.get("offset") if target_unit else None,
    }


def decode_unit_frame(data: bytes, envelope) -> dict | None:
    """Decode the fixed unit ID and position from full or compact UnitFrame data."""
    body = data[envelope.body_start : envelope.body_end]
    if len(body) < 13:
        return None
    field_hash, total = struct.unpack_from("<II", body)
    if body[8] != 6 or struct.unpack_from("<I", body, 9)[0] != total:
        return None
    if total != len(body):
        return None
    payload = body[13:]
    if field_hash == FIELD_UNIT_FRAME_COMPACT:
        if len(payload) < 16 or (len(payload) - 16) % 5:
            return None
        header = struct.unpack_from("<H", payload)[0]
        child_count = (header >> 12) & 7
        if len(payload) != 16 + child_count * 5:
            return None
        position = [
            _rounded(value / 42.0) for value in struct.unpack_from("<HHH", payload, 2)
        ]
        return {
            "encoding": "compact",
            "unitId": header & 0x0FFF,
            "position": position,
            "childCount": child_count,
        }
    if field_hash == FIELD_UNIT_FRAME_FULL:
        if len(payload) < 31:
            return None
        child_count = payload[30]
        if len(payload) != 31 + child_count * 10:
            return None
        return {
            "encoding": "full",
            "unitId": struct.unpack_from("<H", payload, 28)[0],
            "position": [
                _rounded(value) for value in struct.unpack_from("<fff", payload)
            ],
            "childCount": child_count,
        }
    return None


def _sort_events(events: list[dict]) -> None:
    events.sort(key=lambda event: (event["_rawTime"], event["offset"]))


def _cloud_affects_unit_category(
    definition: CloudTypeDefinition, unit: UnitTypeParasites
) -> bool:
    return (
        definition.affects_infantry,
        definition.affects_vehicle,
        definition.affects_tanks,
        definition.affects_copters,
        definition.affects_misc,
        False,
        False,
    )[unit.unit_category]


def _cloud_damage_multiplier(
    definition: CloudTypeDefinition, unit: UnitTypeParasites
) -> float:
    return (
        definition.air_damage_multiplier,
        definition.ground_damage_multiplier,
        definition.infantry_damage_multiplier,
        definition.heavy_armor_damage_multiplier,
    )[unit.meta_type]


def damaging_cloud_candidates(
    destruction: dict,
    cloud_events: list[dict],
    unit_type_definitions: dict[int, UnitTypeParasites],
) -> list[dict]:
    """Return bounded damaging-cloud matches without declaring a cause.

    The server's cloud update proves type/category filtering, faction relationship,
    radius, per-metatype damage, and lifetime. The replay does not serialize which
    cloud instance actually supplied a fatal health change, so even the strongest
    sufficient-damage rows remain candidates until positive controls establish an
    exclusive replay-visible bridge.
    """
    unit = unit_type_definitions.get(destruction.get("unitTypeId"))
    frame = destruction.get("victimLastFrame")
    victim_position = destruction.get("victimPositionAtDestruction")
    if unit is None:
        return []
    matches = []
    for cloud in cloud_events:
        definition = cloud.get("_definition")
        if (
            definition is None
            or definition.support_section not in MULTIPLAYER_SUPPORT_SECTIONS
            or definition.health_change >= 0
            or cloud.get("team") is None
            or cloud["eventIndex"] >= destruction["eventIndex"]
        ):
            continue
        age = destruction["_rawTime"] - cloud["_rawTime"]
        serialized_ttl = cloud["timeToLiveSeconds"]
        if age < 0.0 or age > serialized_ttl + CLOUD_TTL_TOLERANCE_SECONDS:
            continue
        category_affected = _cloud_affects_unit_category(definition, unit)
        victim_team = destruction.get("victimTeam")
        relationship_affected = (
            definition.affect_friendly
            if victim_team == cloud["team"]
            else definition.affect_enemy
        )
        multiplier = _cloud_damage_multiplier(definition, unit)
        effective_health_change = int(definition.health_change * multiplier)
        fresh_creation = (
            abs(serialized_ttl - definition.time_to_live) <= CLOUD_TTL_TOLERANCE_SECONDS
        )
        frame_error = None
        distance = None
        inside_by_frame = None
        definitely_inside = None
        frame_age = None
        frame_same_raw_tick = False
        if frame is not None and victim_position is not None:
            distance = _distance(cloud["position"], victim_position)
            frame_error = (
                COMPACT_FRAME_MAX_POSITION_ERROR
                if frame["encoding"] == "compact"
                else 0.0
            )
            inside_by_frame = distance <= definition.radius
            definitely_inside = distance + frame_error <= definition.radius
            frame_age = destruction["_rawTime"] - frame["rawEventTime"]
            frame_same_raw_tick = (
                frame["rawTimeBits"] == destruction["rawTimeBits"]
                and frame["eventIndex"] < destruction["eventIndex"]
            )
        fresh_first_tick = (
            fresh_creation
            and cloud["rawTimeBits"] == destruction["rawTimeBits"]
            and definition.initial_logic_delay == 0.0
        )
        maximum_health_lethal = (
            category_affected
            and relationship_affected
            and effective_health_change < 0
            and -effective_health_change >= unit.max_health
        )
        binary_sufficient_candidate = (
            fresh_first_tick
            and frame_same_raw_tick
            and definitely_inside is True
            and maximum_health_lethal
            and destruction.get("victimBuildingOccupancy") is None
        )
        matches.append(
            {
                "eventIndex": cloud["eventIndex"],
                "offset": cloud["offset"],
                "rawEventTime": cloud["rawEventTime"],
                "typeIndex": definition.index,
                "supportId": definition.support_id,
                "supportIdHex": f"0x{definition.support_id:08x}",
                "supportName": definition.support_name,
                "supportSection": definition.support_section,
                "team": cloud["team"],
                "position": cloud["position"],
                "ageAtDeathSeconds": _rounded(age),
                "serializedTimeToLiveSeconds": serialized_ttl,
                "definitionTimeToLiveSeconds": definition.time_to_live,
                "freshCreationLifetime": fresh_creation,
                "initialLogicDelaySeconds": definition.initial_logic_delay,
                "healthChange": definition.health_change,
                "healthChangeIntervalSeconds": definition.health_change_interval,
                "unitMetaType": unit.meta_type_name,
                "unitCategory": unit.unit_category_name,
                "unitCategoryAffected": category_affected,
                "victimRelationship": (
                    "friendly" if victim_team == cloud["team"] else "enemy"
                ),
                "victimRelationshipAffected": relationship_affected,
                "damageMultiplier": multiplier,
                "effectiveHealthChange": effective_health_change,
                "victimMaximumHealth": unit.max_health,
                "maximumHealthLethal": maximum_health_lethal,
                "distanceFromLastFrame": (
                    _rounded(distance) if distance is not None else None
                ),
                "radius": definition.radius,
                "lastFrameAgeSeconds": (
                    _rounded(frame_age) if frame_age is not None else None
                ),
                "lastFrameEncoding": frame.get("encoding") if frame else None,
                "lastFrameSameRawTick": frame_same_raw_tick,
                "framePositionErrorBound": (
                    _rounded(frame_error) if frame_error is not None else None
                ),
                "insideRadiusByLastFrame": inside_by_frame,
                "definitelyInsideRadiusByFreshFrame": definitely_inside,
                "freshFirstDamageTickCandidate": fresh_first_tick,
                "binarySufficientCloudCandidate": binary_sufficient_candidate,
                "candidateSemantics": (
                    "serverRulesAndFreshReplayGeometryButNoExclusiveFatalSourceField"
                    if binary_sufficient_candidate
                    else "activeDamagingCloudResearchCandidate"
                ),
                "causalAttribution": "candidateOnly",
            }
        )
    return matches


def building_resident_damage_candidates(
    destruction: dict, building_damage_events: list[dict]
) -> list[dict]:
    """Return exact-order nonterminal damage rows for an occupied building.

    The dedicated server reports ``BuildingDamaged`` before synchronously passing
    nonlethal building damage to its current residents. This remains a mechanical
    context candidate rather than actor attribution: the resident call deliberately
    supplies no killer unit, and map callbacks run between the building report and
    the resident loop.
    """
    occupancy = destruction.get("victimBuildingOccupancy")
    if (
        occupancy is None
        or destruction["killerUnitId"] != KILLER_UNIT_SENTINEL
        or not destruction["syntheticTerminalDirection"]
    ):
        return []
    same_tick_building_events = [
        event
        for event in building_damage_events
        if event["buildingId"] == occupancy["buildingId"]
        and event["rawTimeBits"] == destruction["rawTimeBits"]
    ]
    if any(event["state"] == 3 for event in same_tick_building_events):
        return []
    return [
        event
        for event in same_tick_building_events
        if event["state"] != 3 and event["eventIndex"] < destruction["eventIndex"]
    ]


def classify_destruction(
    destruction: dict,
    candidate_count: int,
    killer_projectiles: list[dict],
) -> str:
    """Classify evidence without turning candidates into attribution."""
    if destruction["killerPlayerId"] is not None:
        return "exactActiveKillerUnit"
    if (
        destruction["killerUnitId"] != KILLER_UNIT_SENTINEL
        and len(destruction["historicalKillerPlayerIds"]) == 1
    ):
        if any(
            projectile.get("targetUnitId") == destruction["unitId"]
            for projectile in killer_projectiles
        ):
            return "historicalOwnerExactTargetProjectileCandidate"
        if killer_projectiles:
            return "historicalOwnerProjectileCandidate"
        return "historicalOwnerCandidate"
    if candidate_count == 0:
        return "noCandidate"
    if candidate_count == 1:
        return "candidateOnly"
    return "ambiguous"


def analyse_replay(
    path: str | pathlib.Path,
    support_names: dict[int, str],
    tactical_aid_projectile_ids: set[int],
    bridge_instances_by_map: dict[str, dict[int, BridgeInstance]] | None = None,
    self_destruct_unit_type_ids: set[int] | None = None,
    support_cloud_definitions: tuple[CloudTypeDefinition, ...] = (),
    unit_type_definitions: dict[int, UnitTypeParasites] | None = None,
    include_events: bool = True,
) -> dict:
    """Extract exact destruction facts and bounded temporal candidates."""
    replay_path = pathlib.Path(path)
    data = decompress(replay_path)
    primary_end_offset = primary_chain_end_offset(data)
    map_name = replay_map_name(data)
    map_bridges = (
        bridge_instances_by_map.get(map_name, {})
        if bridge_instances_by_map is not None and map_name is not None
        else {}
    )
    recorder_player_id = recorder_slot(data)
    clock_samples = _extract_clock_samples(data, primary_end_offset)
    clock_offsets = [sample.offset for sample in clock_samples]
    catalogue: list[int] = []
    active_units: dict[int, dict] = {}
    building_occupancy: dict[int, dict] = {}
    historical_units: dict[int, list[dict]] = {}
    unit_id_reuse = Counter()
    blink_events: list[dict] = []
    resume_disband_events: list[dict] = []
    blower_events: list[dict] = []
    cloud_events: list[dict] = []
    lifecycle_terminals: dict[int, dict] = {}
    removals: list[dict] = []
    destructions: list[dict] = []
    taunts: list[dict] = []
    normal_projectiles: list[dict] = []
    shooter_target_events: list[dict] = []
    unit_health_events: list[dict] = []
    score_events: list[dict] = []
    honors_events: list[dict] = []
    previous_player_scores: dict[int, int] = {}
    bridge_damage_events: list[dict] = []
    bridge_destructions: list[dict] = []
    building_damage_events: list[dict] = []
    support_projectiles: list[dict] = []
    deployments: list[dict] = []
    markers: list[dict] = []
    explosions: list[dict] = []
    malformed = Counter()
    unit_frame_counts = Counter()
    self_destruct_unit_type_ids = self_destruct_unit_type_ids or set()
    unit_type_definitions = unit_type_definitions or {}
    support_cloud_definitions_by_index = {
        definition.index: definition for definition in support_cloud_definitions
    }
    self_destruct_unit_creates = 0

    for event_index, envelope in enumerate(walk(data)):
        if envelope.offset >= primary_end_offset:
            break
        timeline_seconds = _timeline_rounded(
            timeline_time_at_offset(
                clock_samples, _message_offset(envelope), clock_offsets
            )
        )
        body = None

        if envelope.message == MSG_SUPPORT_USED:
            body, _ = fields(data, envelope)
            if len(body) < 4:
                malformed["SupportThingUsed"] += 1
            elif all(value.f32 == 0.0 for value in body[1:4]):
                catalogue.append(body[0].u32)
            continue

        if envelope.message == MSG_UNIT_CREATE:
            body, _ = fields(data, envelope)
            if len(body) < 8:
                malformed["UnitCreate"] += 1
                continue
            unit = {
                "eventIndex": event_index,
                "offset": envelope.offset,
                "rawEventTime": _rounded(envelope.time),
                "timelineSeconds": timeline_seconds,
                "persistenceKey": body[0].u32,
                "playerId": body[1].u32 if body[1].u32 <= 15 else None,
                "rawPlayerId": body[1].u32,
                "team": body[2].i32 if body[2].i32 != 0 else None,
                "rawTeam": body[2].i32,
                "unitTypeId": body[4].u32,
                "unitTypeIdHex": f"0x{body[4].u32:08x}",
                "createPosition": _position(body, 5),
                "positionSemantics": "creationOnlyNotPositionAtDestruction",
                "fortificationId": unit_create_fortification_id(body),
            }
            unit_id = body[3].u32
            previous = active_units.pop(unit_id, None)
            building_occupancy.pop(unit_id, None)
            if previous is not None:
                previous["lifecycleEnd"] = {
                    "reason": "replacedByCreate",
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                }
                lifecycle_terminals[previous["offset"]] = previous["lifecycleEnd"]
            unit_id_reuse[unit_id] += 1
            active_units[unit_id] = unit
            historical_units.setdefault(unit_id, []).append(unit)
            self_destruct_unit_creates += (
                unit["unitTypeId"] in self_destruct_unit_type_ids
            )
            continue

        if envelope.message == MSG_BLINK_UNIT:
            body, trailing = fields(data, envelope)
            blink_fields = decode_blink_trailing_fields(trailing)
            if len(body) != 1 or body[0].hash != FIELD_AUNIT or blink_fields is None:
                malformed["BlinkUnit"] += 1
                continue
            blink_on, blink_time, blink_frequency = blink_fields
            unit_id = body[0].u32
            unit = active_units.get(unit_id)
            blink_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "unitId": unit_id,
                    "unitCreationOffset": unit.get("offset") if unit else None,
                    "playerId": unit.get("playerId") if unit else None,
                    "team": unit.get("team") if unit else None,
                    "blinkOn": blink_on,
                    "rawBlinkFlag": int(blink_on),
                    "blinkTimeSeconds": _rounded(blink_time),
                    "blinkFrequencySeconds": _rounded(blink_frequency),
                }
            )
            continue

        if envelope.message == MSG_RESUME_DISPAND:
            body, trailing = fields(data, envelope)
            if len(body) != 1 or body[0].hash != FIELD_AUNIT or trailing:
                malformed["ResumeDispand"] += 1
                continue
            unit_id = body[0].u32
            unit = active_units.get(unit_id)
            resume_disband_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "unitId": unit_id,
                    "unitCreationOffset": unit.get("offset") if unit else None,
                    "playerId": unit.get("playerId") if unit else None,
                    "team": unit.get("team") if unit else None,
                    "wireSemantics": "clientResumePostBlinkVisualState",
                    "causalAttribution": "none",
                }
            )
            continue

        if envelope.message == MSG_CREATE_CLOUD:
            body, trailing = fields(data, envelope)
            if (
                len(body) != 7
                or trailing
                or body[0].hash != FIELD_ATYPE
                or body[4].hash != FIELD_AHEADING
                or body[5].hash != FIELD_ATIME_TO_LIVE
                or body[6].hash != FIELD_ATEAM
            ):
                malformed["CreateCloud"] += 1
                continue
            definition = support_cloud_definitions_by_index.get(body[0].u32)
            cloud_events.append(
                {
                    "_rawTime": envelope.time,
                    "_definition": definition,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "rawTimeBits": _f32_bits(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "typeIndex": body[0].u32,
                    "position": _position(body, 1),
                    "heading": _rounded(body[4].f32),
                    "timeToLiveSeconds": _rounded(body[5].f32),
                    "team": body[6].i32 if body[6].i32 in {1, 2, 3} else None,
                    "rawTeam": body[6].i32,
                    "supportId": definition.support_id if definition else None,
                    "supportIdHex": (
                        f"0x{definition.support_id:08x}" if definition else None
                    ),
                    "supportName": definition.support_name if definition else None,
                    "supportSection": (
                        definition.support_section if definition else None
                    ),
                    "healthChange": definition.health_change if definition else None,
                    "radius": definition.radius if definition else None,
                    "wireSemantics": (
                        "runtimeCloudTypePositionRemainingLifetimeAndOwningFaction;"
                        "shippedTypeAddsDamageRulesButNoVictimIdentity"
                    ),
                    "causalAttribution": "none",
                }
            )
            continue

        if envelope.message == MSG_BLOWER_BLEW:
            body, trailing = fields(data, envelope)
            if len(body) != 1 or trailing:
                malformed["BlowerBlew"] += 1
                continue
            unit_id = body[0].u32
            unit = active_units.get(unit_id)
            blower_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "unitId": unit_id,
                    "unitCreationOffset": unit.get("offset") if unit else None,
                    "playerId": unit.get("playerId") if unit else None,
                    "team": unit.get("team") if unit else None,
                    "unitTypeId": unit.get("unitTypeId") if unit else None,
                    "unitTypeIdHex": unit.get("unitTypeIdHex") if unit else None,
                    "requestSemantics": "acceptedOwnerTriggeredBlowerDetonation",
                }
            )
            continue

        if envelope.message == MSG_UNIT_REMOVE:
            body, trailing = fields(data, envelope)
            specialist_replacement = decode_trailing_bool_field(
                trailing, FIELD_REMOVE_SPECIALIST_FLAG
            )
            if (
                len(body) == 1
                and body[0].hash == FIELD_AUNIT
                and specialist_replacement is not None
            ):
                unit_id = body[0].u32
                removed = active_units.pop(unit_id, None)
                building_occupancy.pop(unit_id, None)
                terminal = {
                    "reason": "UnitRemove",
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                }
                if removed is not None:
                    removed["lifecycleEnd"] = terminal
                    lifecycle_terminals[removed["offset"]] = terminal
                removals.append(
                    {
                        "_rawTime": envelope.time,
                        **terminal,
                        "unitId": unit_id,
                        "unitCreationOffset": removed.get("offset")
                        if removed
                        else None,
                        "playerId": removed.get("playerId") if removed else None,
                        "team": removed.get("team") if removed else None,
                        "unitTypeId": removed.get("unitTypeId") if removed else None,
                        "unitTypeIdHex": (
                            removed.get("unitTypeIdHex") if removed else None
                        ),
                        "fortificationId": (
                            removed.get("fortificationId") if removed else None
                        ),
                        "isToBeReplacedBySpecialist": specialist_replacement,
                        "rawSpecialistFlag": int(specialist_replacement),
                    }
                )
            else:
                malformed["UnitRemove"] += 1
            continue

        if envelope.message == MSG_BUILDING_SET_SLOT_STATE:
            body, trailing = fields(data, envelope)
            if (
                len(body) != 4
                or trailing
                or body[0].hash != FIELD_ABUILDING_NAME
                or body[1].hash != FIELD_AREAL_SLOT_ID
                or body[2].hash != FIELD_AWILL_OCCUPY_SLOT_FLAG
                or body[3].hash != FIELD_AUNIT
            ):
                malformed["BuildingSetSlotState"] += 1
                continue
            building_id = body[0].u32
            unit_id = body[3].u32
            if body[2].u32 != 0:
                unit = active_units.get(unit_id)
                if unit is not None:
                    building_occupancy[unit_id] = {
                        "buildingId": building_id,
                        "unitCreationOffset": unit["offset"],
                        "occupiedEventIndex": event_index,
                        "occupiedOffset": envelope.offset,
                    }
            elif building_occupancy.get(unit_id, {}).get("buildingId") == building_id:
                building_occupancy.pop(unit_id, None)
            continue

        if envelope.message == MSG_BUILDING_DAMAGED:
            body, trailing = fields(data, envelope)
            if (
                len(body) != 4
                or trailing
                or body[0].hash != FIELD_ANAME
                or body[1].hash != FIELD_AHEALTH
                or body[2].hash != FIELD_ASTATE
                or body[3].hash != FIELD_AFLAG
            ):
                malformed["BuildingDamaged"] += 1
                continue
            building_damage_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "rawTimeBits": _f32_bits(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "buildingId": body[0].u32,
                    "health": body[1].i32,
                    "state": body[2].i32,
                    "flag": body[3].u32,
                }
            )
            continue

        if envelope.message in NORMAL_PROJECTILES:
            body, _ = fields(data, envelope)
            if len(body) < 9:
                malformed[NORMAL_PROJECTILES[envelope.message]] += 1
            else:
                row = _normal_projectile(body, envelope, event_index, active_units)
                row["timelineSeconds"] = timeline_seconds
                normal_projectiles.append(row)
            continue

        if envelope.message in (
            MSG_SHOOTER_ACQUIRED_TARGET,
            MSG_SHOOTER_ATTACKING_UNIT,
        ):
            body, trailing = fields(data, envelope)
            kind = (
                "ShooterAcquiredTarget"
                if envelope.message == MSG_SHOOTER_ACQUIRED_TARGET
                else "ShooterIsAttacking_Unit"
            )
            if len(body) < 2 or trailing:
                malformed[kind] += 1
            else:
                row = _shooter_target_event(body, envelope, event_index, active_units)
                row["timelineSeconds"] = timeline_seconds
                shooter_target_events.append(row)
            continue

        if envelope.message == MSG_UNIT_FRAME:
            frame = decode_unit_frame(data, envelope)
            if frame is None:
                malformed["UnitFrame"] += 1
            else:
                unit_frame_counts[frame["encoding"]] += 1
                unit = active_units.get(frame["unitId"])
                if unit is not None:
                    unit["lastFrame"] = {
                        "eventIndex": event_index,
                        "offset": envelope.offset,
                        "rawEventTime": _rounded(envelope.time),
                        "rawTimeBits": _f32_bits(envelope.time),
                        "timelineSeconds": timeline_seconds,
                        "position": frame["position"],
                        "encoding": frame["encoding"],
                        "childCount": frame["childCount"],
                    }
            continue

        if envelope.message == MSG_UNIT_HEALTH:
            body, trailing = fields(data, envelope)
            if len(body) < 3 or trailing:
                malformed["UnitHealth"] += 1
            else:
                unit_id = body[0].u32
                unit = active_units.get(unit_id)
                unit_health_events.append(
                    {
                        "_rawTime": envelope.time,
                        "eventIndex": event_index,
                        "offset": envelope.offset,
                        "rawEventTime": _rounded(envelope.time),
                        "timelineSeconds": timeline_seconds,
                        "unitId": unit_id,
                        "unitCreationOffset": unit.get("offset") if unit else None,
                        "health": body[1].i32,
                        "direction": body[2].u32,
                    }
                )
            continue

        if envelope.message == MSG_SET_SCORE:
            body, trailing = fields(data, envelope)
            if len(body) < 2 or trailing:
                malformed["SetScore"] += 1
                continue
            raw_player_id = body[0].u32
            player_id = raw_player_id if raw_player_id <= 15 else None
            score = body[1].i32
            previous_score = (
                previous_player_scores.get(player_id) if player_id is not None else None
            )
            active_teams = sorted(
                {
                    unit["team"]
                    for unit in active_units.values()
                    if unit["playerId"] == player_id and unit["team"] is not None
                }
            )
            score_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "playerId": player_id,
                    "rawPlayerId": raw_player_id,
                    "score": score,
                    "previousScore": previous_score,
                    "delta": score - previous_score
                    if previous_score is not None
                    else None,
                    "activeTeams": active_teams,
                    "team": active_teams[0] if len(active_teams) == 1 else None,
                }
            )
            if player_id is not None:
                previous_player_scores[player_id] = score
            continue

        if envelope.message == MSG_CHANGE_HONORS:
            body, trailing = fields(data, envelope)
            if not body or trailing:
                malformed["ChangeHonors"] += 1
                continue
            honors_events.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "delta": _rounded(body[0].f32),
                    "recorderPlayerId": recorder_player_id,
                }
            )
            continue

        if envelope.message == MSG_REPAIRABLE_PROP_DAMAGED:
            body, trailing = fields(data, envelope)
            if len(body) < 5 or trailing:
                malformed["RepairablePropDamaged"] += 1
                continue
            instance = map_bridges.get(body[0].u32)
            if instance is None:
                continue
            row = {
                "_rawTime": envelope.time,
                "eventIndex": event_index,
                "offset": envelope.offset,
                "rawEventTime": _rounded(envelope.time),
                "timelineSeconds": timeline_seconds,
                "mapName": map_name,
                "nameHash": body[0].u32,
                "nameHashHex": f"0x{body[0].u32:08x}",
                "name": instance.name,
                "typeName": instance.type_name,
                "position": list(instance.position),
                "hpbDegrees": list(instance.hpb_degrees),
                "killBoundsXZ": [_rounded(value) for value in instance.kill_bounds_xz],
                "state": body[1].u32,
                "rawHealth": body[2].i32,
                "rawFaction": body[3].i32,
                "createDeathEffect": body[4].u32 != 0,
            }
            bridge_damage_events.append(row)
            if row["state"] == 3:
                bridge_destructions.append(row)
            continue

        if envelope.message in SUPPORT_PROJECTILES:
            body, _ = fields(data, envelope)
            if len(body) < 9:
                malformed[SUPPORT_PROJECTILES[envelope.message]] += 1
            else:
                row = _support_projectile(
                    body, envelope, event_index, support_names, active_units
                )
                row["timelineSeconds"] = timeline_seconds
                support_projectiles.append(row)
            continue

        if envelope.message == MSG_SUPPORT_DEPLOYED:
            body, _ = fields(data, envelope)
            if len(body) < 10:
                malformed["SupportThingSpawnedDelayed"] += 1
                continue
            support_id = body[0].u32
            deployments.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "supportId": support_id,
                    "supportIdHex": f"0x{support_id:08x}",
                    "supportName": support_names.get(support_id),
                    "position": _position(body, 1),
                    "team": body[4].i32,
                    "upgradeLevel": body[5].u32,
                    "direction": _position(body, 6),
                    "ageSeconds": _rounded(body[9].f32),
                    "playerId": None,
                }
            )
            continue

        if envelope.message == MSG_SUPPORT_MARKER:
            body, _ = fields(data, envelope)
            if len(body) < 11:
                malformed["SupportThingMarker"] += 1
                continue
            support_id = body[1].u32
            markers.append(
                {
                    "_rawTime": envelope.time,
                    "_endTime": envelope.time + max(body[10].f32, 0.0),
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "eventId": body[0].u32,
                    "supportId": support_id,
                    "supportIdHex": f"0x{support_id:08x}",
                    "supportName": support_names.get(support_id),
                    "position": _position(body, 2),
                    "playerId": body[5].i32,
                    "upgradeLevel": body[6].u32,
                    "direction": _position(body, 7),
                    "durationSeconds": _rounded(body[10].f32),
                }
            )
            continue

        if envelope.message in (MSG_EXPLOSION, MSG_EXPLOSION_WITH_CRATER):
            body, _ = fields(data, envelope)
            kind = (
                "SpawnExplosion"
                if envelope.message == MSG_EXPLOSION
                else "SpawnExplosionWithCrater"
            )
            minimum_fields = 7 if envelope.message == MSG_EXPLOSION else 8
            if len(body) < minimum_fields:
                malformed[kind] += 1
                continue
            explosions.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "kind": kind,
                    "position": _position(body, 0),
                    "radius": _rounded(body[3].f32),
                    "strength": _rounded(body[4].f32),
                    "explosionForce": _rounded(body[5].f32),
                    "forestDestroyRadius": _rounded(body[6].f32),
                    "craterHitEffectIndex": (
                        body[7].u32
                        if envelope.message == MSG_EXPLOSION_WITH_CRATER
                        else None
                    ),
                    "playerId": None,
                }
            )
            continue

        if envelope.message == MSG_UNIT_DESTROY:
            body, _ = fields(data, envelope)
            if len(body) < 3:
                malformed["UnitDestroy"] += 1
                continue
            unit_id = body[0].u32
            killer_unit_id = body[1].u32
            victim = active_units.get(unit_id)
            active_killer = (
                active_units.get(killer_unit_id)
                if killer_unit_id != KILLER_UNIT_SENTINEL
                else None
            )
            historical_killers = (
                historical_units.get(killer_unit_id, [])
                if killer_unit_id != KILLER_UNIT_SENTINEL
                else []
            )
            hit_direction_bits = (
                tuple(field.u32 for field in body[3:6]) if len(body) >= 6 else None
            )
            destructions.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "rawTimeBits": _f32_bits(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "unitId": unit_id,
                    "victimPlayerId": victim.get("playerId") if victim else None,
                    "victimTeam": victim.get("team") if victim else None,
                    "unitTypeId": victim.get("unitTypeId") if victim else None,
                    "unitTypeIdHex": victim.get("unitTypeIdHex") if victim else None,
                    "victimCreation": victim,
                    "victimBuildingOccupancy": (
                        building_occupancy.get(unit_id)
                        if victim is not None
                        and building_occupancy.get(unit_id, {}).get(
                            "unitCreationOffset"
                        )
                        == victim["offset"]
                        else None
                    ),
                    "victimPositionAtDestruction": (
                        victim.get("lastFrame", {}).get("position") if victim else None
                    ),
                    "victimPositionStatus": (
                        "lastSerializedUnitFrame"
                        if victim and victim.get("lastFrame")
                        else "unavailable"
                    ),
                    "victimLastFrame": victim.get("lastFrame") if victim else None,
                    "killerUnitId": killer_unit_id,
                    "killerUnitSemantics": (
                        "invalidOrNoUnitSentinel"
                        if killer_unit_id == KILLER_UNIT_SENTINEL
                        else "serializedUnitId"
                    ),
                    "activeKiller": (
                        {"unitId": killer_unit_id, **active_killer}
                        if active_killer
                        else None
                    ),
                    "killerPlayerId": (
                        active_killer.get("playerId") if active_killer else None
                    ),
                    "killerTeam": (
                        active_killer.get("team") if active_killer else None
                    ),
                    "historicalKillerPlayerIds": sorted(
                        {
                            unit["playerId"]
                            for unit in historical_killers
                            if unit["playerId"] is not None
                        }
                    ),
                    "historicalKillerRecords": historical_killers,
                    "killerExperience": body[2].u32,
                    "hitDirection": (
                        [_rounded(value.f32) for value in body[3:6]]
                        if len(body) >= 6
                        else None
                    ),
                    "hitDirectionBits": (
                        [f"0x{value:08x}" for value in hit_direction_bits]
                        if hit_direction_bits is not None
                        else None
                    ),
                    "syntheticTerminalDirection": (
                        hit_direction_bits in SYNTHETIC_TERMINAL_DIRECTION_BITS
                        if hit_direction_bits is not None
                        else False
                    ),
                    "explosionForce": (
                        _rounded(body[6].f32) if len(body) >= 7 else None
                    ),
                }
            )
            removed = active_units.pop(unit_id, None)
            building_occupancy.pop(unit_id, None)
            if removed is not None:
                removed["lifecycleEnd"] = {
                    "reason": "UnitDestroy",
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                }
                lifecycle_terminals[removed["offset"]] = removed["lifecycleEnd"]
            continue

        if envelope.message == MSG_TAUNT:
            body, trailing = fields(data, envelope)
            if len(body) < 4 or trailing:
                malformed["SendTATaunt"] += 1
                continue
            taunts.append(
                {
                    "_rawTime": envelope.time,
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "timelineSeconds": timeline_seconds,
                    "playerFrom": body[0].u32,
                    "playerTaunted": body[1].u32,
                    "taIndex": body[2].u32,
                    "supportUpgradeLevel": body[3].u32,
                }
            )

    event_groups = (
        blower_events,
        cloud_events,
        building_damage_events,
        normal_projectiles,
        shooter_target_events,
        unit_health_events,
        score_events,
        honors_events,
        bridge_destructions,
        support_projectiles,
        deployments,
        markers,
        explosions,
        taunts,
    )
    for event_group in event_groups:
        _sort_events(event_group)
    event_times = {
        id(event_group): [event["_rawTime"] for event in event_group]
        for event_group in event_groups
    }

    public_destructions = [
        {key: value for key, value in item.items() if not key.startswith("_")}
        for item in destructions
    ]
    public_blinks = [
        {key: value for key, value in item.items() if not key.startswith("_")}
        for item in blink_events
    ]
    public_removals = [
        {key: value for key, value in item.items() if not key.startswith("_")}
        for item in removals
    ]
    blink_lifecycle_metrics, blink_lifecycles = analyse_blink_lifecycles(
        blink_events, lifecycle_terminals, destructions
    )
    blower_lifecycle_metrics, blower_lifecycles = analyse_blower_lifecycles(
        blower_events, lifecycle_terminals, destructions
    )
    linked_taunts = link_taunts(catalogue, taunts, public_destructions, support_names)
    taunts_by_offset = {taunt["offset"]: taunt for taunt in linked_taunts}
    classifications = Counter()
    shooter_target_metrics = Counter()
    spatial_metrics = Counter()
    remaining_unknown_metrics = Counter()
    remaining_unknown_types = Counter()
    remaining_unknown_synthetic_types = Counter()
    remaining_unknown_nonsynthetic_types = Counter()
    remaining_unknown_support_candidates = Counter()
    area_effect_metrics = Counter()
    ordinary_fire_metrics = Counter()
    score_delta_metrics = Counter()
    honors_metrics = Counter()
    bridge_metrics = Counter()
    building_resident_metrics = Counter()
    cloud_metrics = Counter()
    cloud_cause_metrics: dict[str, Counter] = {}
    remaining_unknown_deaths = []
    exact_tactical_aid_deaths = []
    cloud_max_lookback = (
        max(
            (definition.time_to_live for definition in support_cloud_definitions),
            default=0.0,
        )
        + CLOUD_TTL_TOLERANCE_SECONDS
    )

    for destruction, public_destruction in zip(
        destructions, public_destructions, strict=True
    ):
        death_time = destruction["_rawTime"]
        resident_damage_candidates = building_resident_damage_candidates(
            destruction, building_damage_events
        )
        if destruction["victimBuildingOccupancy"] is not None:
            building_resident_metrics["occupiedUnitDeaths"] += 1
            building_resident_metrics["occupiedPlayerCompleteUnitDeaths"] += (
                destruction["victimPlayerId"] is not None
                and destruction["unitTypeId"] not in INFANTRY_SOLDIER_TYPE_IDS
            )
            building_resident_metrics["occupiedSentinelSyntheticDeaths"] += (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and destruction["syntheticTerminalDirection"]
            )
        if resident_damage_candidates:
            building_resident_metrics["deathsWithCandidate"] += 1
            building_resident_metrics["deathsWithOneCandidate"] += (
                len(resident_damage_candidates) == 1
            )
            building_resident_metrics["deathsWithMultipleCandidates"] += (
                len(resident_damage_candidates) > 1
            )
            nearest = max(
                resident_damage_candidates, key=lambda event: event["eventIndex"]
            )
            event_gap = destruction["eventIndex"] - nearest["eventIndex"]
            building_resident_metrics[f"nearestCandidateEventGap:{event_gap}"] += 1
            building_resident_metrics["candidateSentinelDeaths"] += (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
            )
            building_resident_metrics["candidateSentinelSyntheticDeaths"] += (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and destruction["syntheticTerminalDirection"]
            )
            building_resident_metrics["candidateKnownKillerDeaths"] += (
                destruction["killerUnitId"] != KILLER_UNIT_SENTINEL
            )
            building_resident_metrics["candidatePlayerCompleteUnitDeaths"] += (
                destruction["victimPlayerId"] is not None
                and destruction["unitTypeId"] not in INFANTRY_SOLDIER_TYPE_IDS
            )
            if include_events:
                public_destruction["buildingResidentDamageCandidates"] = [
                    _without_private_fields(event)
                    for event in resident_damage_candidates
                ]
        matching_taunts = [
            taunt
            for taunt in _same_tick(taunts, death_time, event_times[id(taunts)])
            if taunt["playerTaunted"] == destruction["victimPlayerId"]
        ]
        normal_candidates = events_in_window(
            normal_projectiles,
            death_time,
            NORMAL_PROJECTILE_LOOKBACK_SECONDS,
            event_times[id(normal_projectiles)],
        )
        killer_projectiles = [
            projectile
            for projectile in normal_candidates
            if projectile.get("firingUnitId") == destruction["killerUnitId"]
        ]
        shooter_target_candidates = [
            event
            for event in events_in_window(
                shooter_target_events,
                death_time,
                SHOOTER_TARGET_LOOKBACK_SECONDS,
                event_times[id(shooter_target_events)],
            )
            if destruction["victimCreation"] is not None
            and event["eventIndex"] < destruction["eventIndex"]
            and event["targetUnitId"] == destruction["unitId"]
            and event["targetUnitCreationOffset"]
            == destruction["victimCreation"].get("offset")
        ]
        health_candidates = [
            event
            for event in events_in_window(
                unit_health_events,
                death_time,
                SHOOTER_TARGET_LOOKBACK_SECONDS,
                event_times[id(unit_health_events)],
            )
            if destruction["victimCreation"] is not None
            and event["eventIndex"] < destruction["eventIndex"]
            and event["unitId"] == destruction["unitId"]
            and event["unitCreationOffset"]
            == destruction["victimCreation"].get("offset")
        ]
        score_windows = positive_score_candidate_windows(
            score_events,
            death_time,
            destruction["eventIndex"],
            event_times[id(score_events)],
        )
        honors_event_gap = preceding_positive_honors_gap(
            honors_events,
            death_time,
            destruction["eventIndex"],
            event_times[id(honors_events)],
        )
        support_candidates = events_in_window(
            support_projectiles,
            death_time,
            SUPPORT_EVIDENCE_LOOKBACK_SECONDS,
            event_times[id(support_projectiles)],
        )
        exact_ta_cause = exact_tactical_aid_cause(
            destruction,
            support_candidates,
            deployments,
            tactical_aid_projectile_ids,
        )
        nearby_cloud_events = events_in_window(
            cloud_events,
            death_time,
            cloud_max_lookback,
            event_times[id(cloud_events)],
        )
        cloud_candidates = damaging_cloud_candidates(
            destruction,
            nearby_cloud_events,
            unit_type_definitions,
        )
        eligible_cloud_candidates = [
            candidate
            for candidate in cloud_candidates
            if candidate["unitCategoryAffected"]
            and candidate["victimRelationshipAffected"]
            and candidate["effectiveHealthChange"] < 0
        ]
        fresh_geometry_cloud_candidates = [
            candidate
            for candidate in eligible_cloud_candidates
            if candidate["freshFirstDamageTickCandidate"]
            and candidate["lastFrameSameRawTick"]
            and candidate["definitelyInsideRadiusByFreshFrame"] is True
        ]
        sufficient_cloud_candidates = [
            candidate
            for candidate in cloud_candidates
            if candidate["binarySufficientCloudCandidate"]
        ]
        sufficient_cloud_causes = {
            (candidate["supportId"], candidate["team"])
            for candidate in sufficient_cloud_candidates
        }
        if include_events:
            public_destruction["activeDamagingCloudCandidates"] = cloud_candidates
        normal_exact_target_actors = exact_target_normal_actors(
            destruction, normal_candidates
        )
        deployment_candidates = events_in_window(
            deployments,
            death_time,
            SUPPORT_EVIDENCE_LOOKBACK_SECONDS,
            event_times[id(deployments)],
        )
        marker_candidates = [
            event
            for event in markers
            if event["_rawTime"] <= death_time <= event["_endTime"]
        ]
        explosion_candidates = _same_tick(
            explosions, death_time, event_times[id(explosions)]
        )
        candidate_count = sum(
            map(
                len,
                (
                    matching_taunts,
                    normal_candidates,
                    support_candidates,
                    deployment_candidates,
                    marker_candidates,
                    explosion_candidates,
                ),
            )
        )
        evidence = None
        if include_events:
            evidence = {
                "sameTickTacticalAidDamageThresholds": [
                    _public_event(
                        {
                            **{
                                key: value
                                for key, value in taunts_by_offset[
                                    taunt["offset"]
                                ].items()
                                if key != "sameTickVictimDestructions"
                            },
                            "_rawTime": taunt["_rawTime"],
                        },
                        death_time,
                        "sameRawTimestampAndVictimPlayer",
                    )
                    for taunt in matching_taunts
                ],
                "nearbyNormalProjectiles": [
                    _public_event(
                        event, death_time, "normalProjectileTemporalPrescreenOnly"
                    )
                    for event in normal_candidates
                ],
                "nearbySupportProjectiles": [
                    _public_event(
                        event, death_time, "supportProjectileTemporalPrescreenOnly"
                    )
                    for event in support_candidates
                ],
                "nearbyTacticalAidDeployments": [
                    _public_event(event, death_time, "deploymentTemporalPrescreenOnly")
                    for event in deployment_candidates
                ],
                "activeTacticalAidMarkers": [
                    _public_event(
                        event,
                        death_time,
                        "destructionInsideSerializedMarkerLifetime",
                    )
                    for event in marker_candidates
                ],
                "sameTickExplosions": [
                    _public_event(event, death_time, "sameRawTimestampOnly")
                    for event in explosion_candidates
                ],
            }
        classification = classify_destruction(
            destruction, candidate_count, killer_projectiles
        )
        public_destruction["candidateClassification"] = classification
        public_destruction["historicalKillerProjectileCount"] = len(killer_projectiles)
        public_destruction["historicalKillerExactTargetProjectileCount"] = sum(
            projectile.get("targetUnitId") == destruction["unitId"]
            for projectile in killer_projectiles
        )
        public_destruction["killerExactTargetProjectilePlayerIds"] = sorted(
            {
                projectile["firingPlayerId"]
                for projectile in killer_projectiles
                if projectile.get("targetUnitId") == destruction["unitId"]
                and projectile.get("firingPlayerId") is not None
            }
        )
        shooter_windows = {}
        for window in SHOOTER_TARGET_WINDOWS_SECONDS:
            candidates = [
                event
                for event in shooter_target_candidates
                if death_time - event["_rawTime"] <= window
            ]
            actors = sorted(
                {
                    (event["firingPlayerId"], event["firingTeam"])
                    for event in candidates
                    if event["firingPlayerId"] is not None
                    or event["firingTeam"] is not None
                },
                key=lambda actor: (
                    actor[0] is None,
                    actor[0] if actor[0] is not None else -1,
                    actor[1] is None,
                    actor[1] if actor[1] is not None else -1,
                ),
            )
            label = f"{window:g}s"
            shooter_windows[label] = {
                "candidateCount": len(candidates),
                "actors": [
                    {"playerId": player_id, "team": team} for player_id, team in actors
                ],
            }
            prefix = f"shooterTarget{window:g}s"
            if destruction["killerPlayerId"] is not None:
                shooter_target_metrics[f"{prefix}ActiveKillerControls"] += 1
                shooter_target_metrics[f"{prefix}ControlsWithOneActor"] += (
                    len(actors) == 1
                )
                shooter_target_metrics[f"{prefix}ControlsMatchingKiller"] += (
                    len(actors) == 1 and actors[0][0] == destruction["killerPlayerId"]
                )
                shooter_target_metrics[f"{prefix}ControlsMismatchingKiller"] += (
                    len(actors) == 1 and actors[0][0] != destruction["killerPlayerId"]
                )
            if (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and destruction["victimPlayerId"] is not None
            ):
                shooter_target_metrics[f"{prefix}SentinelDeaths"] += 1
                shooter_target_metrics[f"{prefix}SentinelWithOneActor"] += (
                    len(actors) == 1
                )
                shooter_target_metrics[f"{prefix}SentinelWithMultipleActors"] += (
                    len(actors) > 1
                )
        public_destruction["shooterTargetWindows"] = shooter_windows
        public_destruction["recentShooterTargetEvents"] = [
            _public_event(event, death_time, "sameTargetLifecycle")
            for event in shooter_target_candidates
        ]
        public_destruction["recentUnitHealthEvents"] = [
            _public_event(event, death_time, "sameVictimLifecycle")
            for event in health_candidates
        ]
        victim_position = public_destruction["victimPositionAtDestruction"]
        bridge_windows = {
            f"{window:g}s": bridge_destruction_candidates(
                destruction, bridge_destructions, window
            )
            for window in BRIDGE_DESTRUCTION_WINDOWS_SECONDS
        }
        public_destruction["bridgeDestructionWindows"] = bridge_windows
        same_tick_bridge_candidates = bridge_windows["0s"]
        exact_bridge_bounds_candidates = [
            candidate
            for candidate in same_tick_bridge_candidates
            if candidate["syntheticTerminalDirection"]
            and candidate["insideExactServerKillBounds"] is True
        ]
        exact_bridge_fresh_frame_candidates = [
            candidate
            for candidate in exact_bridge_bounds_candidates
            if candidate["victimLastFrameSameRawTick"]
        ]
        public_destruction["exactBridgeBoundsCandidates"] = (
            exact_bridge_bounds_candidates
        )
        public_destruction["exactBridgeFreshFrameCandidates"] = (
            exact_bridge_fresh_frame_candidates
        )
        spatial_projectiles = []
        if victim_position is not None:
            spatial_projectiles = [
                projectile_spatial_event(event, death_time, victim_position)
                for event in normal_candidates
                if event["eventIndex"] < destruction["eventIndex"]
                and event["firingPlayerId"] is not None
            ]
        public_destruction["spatialNormalProjectiles"] = spatial_projectiles
        spatial_explosions = []
        if victim_position is not None:
            spatial_explosions = [
                {
                    **_public_event(event, death_time, "sameTickSpatialResearch"),
                    "distanceToVictim": _rounded(
                        _distance(event["position"], victim_position)
                    ),
                }
                for event in explosion_candidates
            ]
        public_destruction["spatialSameTickExplosions"] = spatial_explosions
        preceding_blast_candidates = preceding_same_tick_blast_candidates(
            spatial_explosions, destruction["eventIndex"]
        )
        preceding_blast_event_indexes = {
            event["eventIndex"] for event in preceding_blast_candidates
        }
        preceding_blast_source_candidates = [
            event
            for event in explosion_candidates
            if event["eventIndex"] in preceding_blast_event_indexes
        ]
        complete_player_unit = (
            destruction["victimPlayerId"] is not None
            and destruction["unitTypeId"] not in INFANTRY_SOLDIER_TYPE_IDS
        )
        if complete_player_unit:
            remaining_unknown = (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and exact_ta_cause is None
            )
            cloud_metrics["completePlayerUnitDeaths"] += 1
            cloud_metrics["withActiveDamagingCloud"] += bool(eligible_cloud_candidates)
            cloud_metrics["withFreshSameTickGeometryCandidate"] += bool(
                fresh_geometry_cloud_candidates
            )
            cloud_metrics["withBinarySufficientCandidate"] += bool(
                sufficient_cloud_candidates
            )
            cloud_metrics["withOneBinarySufficientCause"] += (
                len(sufficient_cloud_causes) == 1
            )
            cloud_metrics["withMultipleBinarySufficientCauses"] += (
                len(sufficient_cloud_causes) > 1
            )
            if destruction["killerPlayerId"] is not None:
                cloud_metrics["activeKillerControls"] += 1
                cloud_metrics["activeKillerControlsWithActiveDamagingCloud"] += bool(
                    eligible_cloud_candidates
                )
                cloud_metrics[
                    "activeKillerControlsWithFreshSameTickGeometryCandidate"
                ] += bool(fresh_geometry_cloud_candidates)
                cloud_metrics["activeKillerControlsWithBinarySufficientCandidate"] += (
                    bool(sufficient_cloud_candidates)
                )
                cloud_metrics["activeKillerControlsWithOneBinarySufficientCause"] += (
                    len(sufficient_cloud_causes) == 1
                )
            if exact_ta_cause is not None:
                cloud_metrics["exactTacticalAidControls"] += 1
                cloud_metrics["exactTacticalAidControlsWithActiveDamagingCloud"] += (
                    bool(eligible_cloud_candidates)
                )
                cloud_metrics[
                    "exactTacticalAidControlsWithBinarySufficientCandidate"
                ] += bool(sufficient_cloud_candidates)
                cloud_metrics["exactTacticalAidControlsMatchingSufficientCause"] += (
                    exact_ta_cause in sufficient_cloud_causes
                )
                cloud_metrics[
                    "exactTacticalAidControlsMismatchingUniqueSufficientCause"
                ] += (
                    len(sufficient_cloud_causes) == 1
                    and exact_ta_cause not in sufficient_cloud_causes
                )
            if remaining_unknown:
                cloud_metrics["remainingUnknownDeaths"] += 1
                cloud_metrics["remainingUnknownWithActiveDamagingCloud"] += bool(
                    eligible_cloud_candidates
                )
                cloud_metrics["remainingUnknownWithFreshSameTickGeometryCandidate"] += (
                    bool(fresh_geometry_cloud_candidates)
                )
                cloud_metrics["remainingUnknownWithBinarySufficientCandidate"] += bool(
                    sufficient_cloud_candidates
                )
                cloud_metrics["remainingUnknownWithOneBinarySufficientCause"] += (
                    len(sufficient_cloud_causes) == 1
                )
                cloud_metrics["remainingUnknownWithMultipleBinarySufficientCauses"] += (
                    len(sufficient_cloud_causes) > 1
                )
            population = (
                "activeKillerControl"
                if destruction["killerPlayerId"] is not None
                else "exactTacticalAidControl"
                if exact_ta_cause is not None
                else "remainingUnknown"
                if remaining_unknown
                else "other"
            )
            for support_id, team in sufficient_cloud_causes:
                key = f"0x{support_id:08x}:team{team}"
                counts = cloud_cause_metrics.setdefault(key, Counter())
                counts["deaths"] += 1
                counts[population] += 1
            bridge_metrics["exactBoundsCompletePlayerUnitDeaths"] += 1
            bridge_metrics["exactBoundsSentinelSyntheticDeaths"] += (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and destruction["syntheticTerminalDirection"]
            )
            bridge_metrics["exactBoundsDeathsWithSameTickBridgeState3"] += bool(
                same_tick_bridge_candidates
            )
            bridge_metrics["exactBoundsDeathsWithCandidate"] += bool(
                exact_bridge_bounds_candidates
            )
            bridge_metrics["exactBoundsDeathsWithOneCandidate"] += (
                len(exact_bridge_bounds_candidates) == 1
            )
            bridge_metrics["exactBoundsDeathsWithMultipleCandidates"] += (
                len(exact_bridge_bounds_candidates) > 1
            )
            bridge_metrics["exactBoundsDeathsWithFreshFrameCandidate"] += bool(
                exact_bridge_fresh_frame_candidates
            )
            bridge_metrics["exactBoundsCandidatesWithBridgeBeforeDeath"] += any(
                candidate["eventIndexDeltaFromBridgeDestruction"] > 0
                for candidate in exact_bridge_bounds_candidates
            )
            bridge_metrics["exactBoundsCandidatesWithBridgeAfterDeath"] += any(
                candidate["eventIndexDeltaFromBridgeDestruction"] < 0
                for candidate in exact_bridge_bounds_candidates
            )
            bridge_metrics["exactBoundsRemainingUnknownDeaths"] += remaining_unknown
            bridge_metrics["exactBoundsRemainingUnknownMatches"] += (
                remaining_unknown and bool(exact_bridge_bounds_candidates)
            )
            bridge_metrics["exactBoundsRemainingUnknownFreshFrameMatches"] += (
                remaining_unknown and bool(exact_bridge_fresh_frame_candidates)
            )
            for label, candidates in bridge_windows.items():
                prefix = f"bridgeDestruction{label}"
                bridge_metrics[f"{prefix}CompletePlayerUnitDeaths"] += 1
                bridge_metrics[f"{prefix}DeathsWithCandidate"] += bool(candidates)
                bridge_metrics[f"{prefix}DeathsWithOneCandidate"] += (
                    len(candidates) == 1
                )
                bridge_metrics[f"{prefix}DeathsWithMultipleCandidates"] += (
                    len(candidates) > 1
                )
                if candidates:
                    bridge_metrics[f"{prefix}CandidatesBeforeDeath"] += any(
                        candidate["eventIndexDeltaFromBridgeDestruction"] > 0
                        for candidate in candidates
                    )
                    bridge_metrics[f"{prefix}CandidatesAfterDeath"] += any(
                        candidate["eventIndexDeltaFromBridgeDestruction"] < 0
                        for candidate in candidates
                    )
            widest_candidates = bridge_windows[
                f"{BRIDGE_DESTRUCTION_WINDOWS_SECONDS[-1]:g}s"
            ]
            for threshold in BRIDGE_SPATIAL_THRESHOLDS:
                prefix = f"bridgeDestruction0.5sWithin{threshold:g}u"
                spatial_candidates = [
                    candidate
                    for candidate in widest_candidates
                    if candidate["horizontalDistanceFromBridge"] is not None
                    and candidate["horizontalDistanceFromBridge"] <= threshold
                ]
                known_cause = (
                    destruction["killerPlayerId"] is not None
                    or exact_ta_cause is not None
                )
                bridge_metrics[f"{prefix}KnownCauseControls"] += known_cause
                bridge_metrics[f"{prefix}KnownCauseMatches"] += known_cause and bool(
                    spatial_candidates
                )
                bridge_metrics[f"{prefix}RemainingUnknownDeaths"] += remaining_unknown
                bridge_metrics[f"{prefix}RemainingUnknownMatches"] += (
                    remaining_unknown and bool(spatial_candidates)
                )
        if complete_player_unit and destruction["killerPlayerId"] is not None:
            for label, score_candidates in score_windows.items():
                prefix = f"scoreDelta{label}"
                actors = {event["playerId"] for event in score_candidates}
                teams = {
                    event["team"]
                    for event in score_candidates
                    if event["team"] is not None
                }
                score_delta_metrics[f"{prefix}KnownKillerControls"] += 1
                score_delta_metrics[f"{prefix}ControlsWithOneActor"] += len(actors) == 1
                score_delta_metrics[f"{prefix}ControlsWithMultipleActors"] += (
                    len(actors) > 1
                )
                score_delta_metrics[f"{prefix}ControlsMatchingKiller"] += actors == {
                    destruction["killerPlayerId"]
                }
                score_delta_metrics[f"{prefix}ControlsMismatchingKiller"] += len(
                    actors
                ) == 1 and actors != {destruction["killerPlayerId"]}
                score_delta_metrics[f"{prefix}ControlsWithOneTeam"] += len(teams) == 1
                score_delta_metrics[f"{prefix}ControlsMatchingKillerTeam"] += teams == {
                    destruction["killerTeam"]
                }
                score_delta_metrics[f"{prefix}ControlsMismatchingKillerTeam"] += len(
                    teams
                ) == 1 and teams != {destruction["killerTeam"]}
            for gap_threshold in HONORS_EVENT_GAP_THRESHOLDS:
                prefix = f"positiveHonorsGap{gap_threshold}"
                candidate = (
                    recorder_player_id is not None
                    and honors_event_gap is not None
                    and honors_event_gap <= gap_threshold
                )
                honors_metrics[f"{prefix}KnownKillerControls"] += 1
                honors_metrics[f"{prefix}ControlsWithCandidate"] += candidate
                honors_metrics[f"{prefix}ControlsMatchingKiller"] += (
                    candidate and recorder_player_id == destruction["killerPlayerId"]
                )
                honors_metrics[f"{prefix}ControlsMismatchingKiller"] += (
                    candidate and recorder_player_id != destruction["killerPlayerId"]
                )
        public_destruction["precedingSameTickBlastCandidates"] = (
            preceding_blast_candidates
        )
        support_impact_windows = {}
        for threshold in SUPPORT_IMPACT_THRESHOLDS:
            causes = support_impact_causes(
                support_candidates,
                preceding_blast_source_candidates,
                deployments,
                tactical_aid_projectile_ids,
                threshold,
            )
            label = f"{threshold:g}u"
            support_impact_windows[label] = [
                {
                    "supportId": support_id,
                    "supportIdHex": f"0x{support_id:08x}",
                    "supportName": support_names.get(support_id),
                    "team": team,
                }
                for support_id, team in sorted(causes)
            ]
            prefix = f"supportImpact{threshold:g}u"
            if complete_player_unit and exact_ta_cause is not None:
                area_effect_metrics[f"{prefix}ExactTaControls"] += 1
                area_effect_metrics[f"{prefix}ControlsWithOneCause"] += len(causes) == 1
                area_effect_metrics[f"{prefix}ControlsMatchingExactCause"] += (
                    causes == {exact_ta_cause}
                )
                area_effect_metrics[f"{prefix}ControlsMismatchingExactCause"] += len(
                    causes
                ) == 1 and causes != {exact_ta_cause}
                area_effect_metrics[f"{prefix}ControlsWithMultipleCauses"] += (
                    len(causes) > 1
                )
        public_destruction["supportImpactWindows"] = support_impact_windows
        for threshold in PROJECTILE_SPATIAL_THRESHOLDS:
            actors = sorted(
                {
                    event["firingPlayerId"]
                    for event in spatial_projectiles
                    if event["constantVectorDistanceToVictim"] <= threshold
                }
            )
            prefix = f"projectedProjectile{threshold:g}u"
            if destruction["killerPlayerId"] is not None:
                spatial_metrics[f"{prefix}ActiveKillerControls"] += 1
                spatial_metrics[f"{prefix}ControlsWithOneActor"] += len(actors) == 1
                spatial_metrics[f"{prefix}ControlsMatchingKiller"] += (
                    len(actors) == 1 and actors[0] == destruction["killerPlayerId"]
                )
                spatial_metrics[f"{prefix}ControlsMismatchingKiller"] += (
                    len(actors) == 1 and actors[0] != destruction["killerPlayerId"]
                )
            if (
                destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
                and destruction["victimPlayerId"] is not None
            ):
                spatial_metrics[f"{prefix}SentinelDeaths"] += 1
                spatial_metrics[f"{prefix}SentinelWithOneActor"] += len(actors) == 1
                spatial_metrics[f"{prefix}SentinelWithMultipleActors"] += (
                    len(actors) > 1
                )
                spatial_metrics[f"sameTickExplosion{threshold:g}uSentinelDeaths"] += 1
                spatial_metrics[f"sameTickExplosion{threshold:g}uSentinelMatches"] += (
                    any(
                        event["distanceToVictim"] <= threshold
                        for event in spatial_explosions
                    )
                )
        public_destruction["candidateEventCount"] = candidate_count
        public_destruction["candidateSemantics"] = "nonCausalResearchEvidence"
        public_destruction["parserCauseV17"] = (
            "tacticalAid"
            if exact_ta_cause is not None
            else "unit"
            if destruction["killerUnitId"] != KILLER_UNIT_SENTINEL
            else "unknown"
        )
        public_destruction["exactTacticalAidCause"] = (
            {
                "supportId": exact_ta_cause[0],
                "supportIdHex": f"0x{exact_ta_cause[0]:08x}",
                "supportName": support_names.get(exact_ta_cause[0]),
                "team": exact_ta_cause[1],
                "basis": "schemaV17ExactTargetLifecycleAndUniqueDeploymentTeam",
            }
            if exact_ta_cause is not None
            else None
        )
        public_destruction["exactTargetNormalProjectileActors"] = [
            {"playerId": player_id, "team": team}
            for player_id, team in sorted(normal_exact_target_actors)
        ]
        if complete_player_unit and exact_ta_cause is not None:
            exact_tactical_aid_deaths.append(destruction)
            area_effect_metrics["exactTaControls"] += 1
            area_effect_metrics["exactTaControlsWithBlastCandidate"] += bool(
                preceding_blast_candidates
            )
            area_effect_metrics["exactTaControlsWithUniqueBlastCandidate"] += (
                len(preceding_blast_candidates) == 1
            )
            area_effect_metrics["exactTaControlsWithMultipleBlastCandidates"] += (
                len(preceding_blast_candidates) > 1
            )
        if complete_player_unit and destruction["killerPlayerId"] is not None:
            normal_exact_target_teams = {
                team for _player_id, team in normal_exact_target_actors
            }
            ordinary_fire_metrics["activeKillerControls"] += 1
            ordinary_fire_metrics["controlsWithOneActor"] += (
                len(normal_exact_target_actors) == 1
            )
            ordinary_fire_metrics["controlsMatchingKiller"] += (
                normal_exact_target_actors
                == {(destruction["killerPlayerId"], destruction["killerTeam"])}
            )
            ordinary_fire_metrics["controlsMismatchingKiller"] += len(
                normal_exact_target_actors
            ) == 1 and normal_exact_target_actors != {
                (destruction["killerPlayerId"], destruction["killerTeam"])
            }
            ordinary_fire_metrics["controlsWithMultipleActors"] += (
                len(normal_exact_target_actors) > 1
            )
            ordinary_fire_metrics["controlsWithOneTeam"] += (
                len(normal_exact_target_teams) == 1
            )
            ordinary_fire_metrics["controlsMatchingKillerTeam"] += (
                normal_exact_target_teams == {destruction["killerTeam"]}
            )
            ordinary_fire_metrics["controlsMismatchingKillerTeam"] += len(
                normal_exact_target_teams
            ) == 1 and normal_exact_target_teams != {destruction["killerTeam"]}
        if (
            complete_player_unit
            and destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
            and exact_ta_cause is None
        ):
            remaining_unknown_deaths.append(destruction)
            remaining_unknown_metrics["deaths"] += 1
            remaining_unknown_metrics["withSyntheticTerminalDirection"] += destruction[
                "syntheticTerminalDirection"
            ]
            remaining_unknown_metrics[
                "withoutSyntheticTerminalDirection"
            ] += not destruction["syntheticTerminalDirection"]
            remaining_unknown_metrics["withLastUnitFramePosition"] += (
                victim_position is not None
            )
            remaining_unknown_metrics["withNearbyNormalProjectile"] += bool(
                normal_candidates
            )
            remaining_unknown_metrics["withNearbySupportProjectile"] += bool(
                support_candidates
            )
            remaining_unknown_metrics["withExactTargetSupportProjectile"] += any(
                projectile.get("targetUnitId") == destruction["unitId"]
                and projectile.get("targetUnitCreationOffset")
                == destruction["victimCreation"].get("offset")
                for projectile in support_candidates
            )
            ordinary_fire_metrics["remainingUnknownDeaths"] += 1
            for label, score_candidates in score_windows.items():
                prefix = f"scoreDelta{label}"
                actors = {event["playerId"] for event in score_candidates}
                teams = {
                    event["team"]
                    for event in score_candidates
                    if event["team"] is not None
                }
                score_delta_metrics[f"{prefix}RemainingUnknownDeaths"] += 1
                score_delta_metrics[f"{prefix}RemainingUnknownWithOneActor"] += (
                    len(actors) == 1
                )
                score_delta_metrics[f"{prefix}RemainingUnknownWithMultipleActors"] += (
                    len(actors) > 1
                )
                score_delta_metrics[f"{prefix}RemainingUnknownWithOneTeam"] += (
                    len(teams) == 1
                )
            for gap_threshold in HONORS_EVENT_GAP_THRESHOLDS:
                prefix = f"positiveHonorsGap{gap_threshold}"
                candidate = (
                    recorder_player_id is not None
                    and honors_event_gap is not None
                    and honors_event_gap <= gap_threshold
                )
                honors_metrics[f"{prefix}RemainingUnknownDeaths"] += 1
                honors_metrics[f"{prefix}RemainingUnknownWithCandidate"] += candidate
                honors_metrics[f"{prefix}CandidateVictimIsRecorder"] += (
                    candidate and recorder_player_id == destruction["victimPlayerId"]
                )
                honors_metrics[f"{prefix}CandidateVictimIsOtherPlayer"] += (
                    candidate and recorder_player_id != destruction["victimPlayerId"]
                )
            ordinary_fire_metrics["remainingUnknownWithOneActor"] += (
                len(normal_exact_target_actors) == 1
            )
            ordinary_fire_metrics["remainingUnknownWithMultipleActors"] += (
                len(normal_exact_target_actors) > 1
            )
            ordinary_fire_metrics["remainingUnknownWithOneTeam"] += (
                len({team for _player_id, team in normal_exact_target_actors}) == 1
            )
            remaining_unknown_metrics["withRecentTacticalAidDeployment"] += bool(
                deployment_candidates
            )
            remaining_unknown_metrics["insideActiveTacticalAidMarkerLifetime"] += bool(
                marker_candidates
            )
            remaining_unknown_metrics["withSameTickExplosion"] += bool(
                explosion_candidates
            )
            area_effect_metrics["remainingUnknownDeaths"] += 1
            area_effect_metrics["remainingUnknownWithBlastCandidate"] += bool(
                preceding_blast_candidates
            )
            area_effect_metrics["remainingUnknownWithUniqueBlastCandidate"] += (
                len(preceding_blast_candidates) == 1
            )
            area_effect_metrics["remainingUnknownWithMultipleBlastCandidates"] += (
                len(preceding_blast_candidates) > 1
            )
            for label, causes in support_impact_windows.items():
                prefix = f"supportImpact{label}"
                area_effect_metrics[f"{prefix}RemainingUnknownWithOneCause"] += (
                    len(causes) == 1
                )
                area_effect_metrics[f"{prefix}RemainingUnknownWithMultipleCauses"] += (
                    len(causes) > 1
                )
            signature = (
                "+".join(
                    label
                    for label, present in (
                        ("normalProjectile", bool(normal_candidates)),
                        ("supportProjectile", bool(support_candidates)),
                        ("deployment", bool(deployment_candidates)),
                        ("activeMarker", bool(marker_candidates)),
                        ("explosion", bool(explosion_candidates)),
                    )
                    if present
                )
                or "noTemporalCandidate"
            )
            remaining_unknown_metrics[f"signature:{signature}"] += 1
            remaining_unknown_types[destruction["unitTypeIdHex"] or "unknown"] += 1
            direction_types = (
                remaining_unknown_synthetic_types
                if destruction["syntheticTerminalDirection"]
                else remaining_unknown_nonsynthetic_types
            )
            direction_types[destruction["unitTypeIdHex"] or "unknown"] += 1
            for support_id in {
                projectile["supportId"] for projectile in support_candidates
            }:
                remaining_unknown_support_candidates[f"0x{support_id:08x}"] += 1
        if evidence is not None:
            public_destruction["evidence"] = evidence
        classifications[classification] += 1

    for window in MULTI_VICTIM_CLUSTER_WINDOWS_SECONDS:
        label = f"{window:g}s"
        clusters, deaths, victim_players = cluster_multi_player_deaths(
            remaining_unknown_deaths, window
        )
        remaining_unknown_metrics[f"multiPlayerClusters:{label}"] = clusters
        remaining_unknown_metrics[f"deathsInMultiPlayerClusters:{label}"] = deaths
        remaining_unknown_metrics[f"playerAppearancesInMultiPlayerClusters:{label}"] = (
            victim_players
        )
        ta_clusters, ta_deaths, ta_victim_players = cluster_multi_player_deaths(
            exact_tactical_aid_deaths, window
        )
        remaining_unknown_metrics[f"exactTaMultiPlayerClusters:{label}"] = ta_clusters
        remaining_unknown_metrics[f"exactTaDeathsInMultiPlayerClusters:{label}"] = (
            ta_deaths
        )
        remaining_unknown_metrics[
            f"exactTaPlayerAppearancesInMultiPlayerClusters:{label}"
        ] = ta_victim_players

    same_tick = [
        destruction
        for taunt in linked_taunts
        for destruction in taunt["sameTickVictimDestructions"]
    ]
    historical_recoverable = [
        destruction
        for destruction in public_destructions
        if destruction["candidateClassification"].startswith("historicalOwner")
    ]
    support_stats: dict[str, Counter] = {}
    for taunt in linked_taunts:
        support_id = taunt["supportIdHex"] or "unresolved"
        counts = support_stats.setdefault(support_id, Counter())
        counts["taunts"] += 1
        counts["withSameTickVictimDestruction"] += bool(
            taunt["sameTickVictimDestructions"]
        )
        counts["sameTickVictimDestructions"] += len(taunt["sameTickVictimDestructions"])

    fortification_stats: dict[str, Counter] = {}
    for generations in historical_units.values():
        for unit in generations:
            if unit["fortificationId"] is not None:
                fortification_stats.setdefault(unit["unitTypeIdHex"], Counter())[
                    "creates"
                ] += 1
    for removal in public_removals:
        if removal["fortificationId"] is not None:
            fortification_stats.setdefault(
                removal["unitTypeIdHex"] or "unknown", Counter()
            )["removals"] += 1
    for destruction in public_destructions:
        creation = destruction["victimCreation"]
        if creation is None or creation["fortificationId"] is None:
            continue
        counts = fortification_stats.setdefault(
            destruction["unitTypeIdHex"] or "unknown", Counter()
        )
        counts["destructions"] += 1
        counts["sentinelDestructions"] += (
            destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
        )
        counts["sentinelSyntheticDirectionDestructions"] += (
            destruction["killerUnitId"] == KILLER_UNIT_SENTINEL
            and destruction["syntheticTerminalDirection"]
        )

    result = {
        "schemaVersion": 13,
        "path": str(replay_path),
        "mapName": map_name,
        "sha256": _sha256(replay_path),
        "primaryChainEndOffset": primary_end_offset,
        "endSummaryPassOmitted": primary_end_offset < len(data),
        "candidateWindows": {
            "futureOrderingToleranceSeconds": FUTURE_ORDER_TOLERANCE_SECONDS,
            "normalProjectileLookbackSeconds": NORMAL_PROJECTILE_LOOKBACK_SECONDS,
            "supportEvidenceLookbackSeconds": SUPPORT_EVIDENCE_LOOKBACK_SECONDS,
            "supportDeploymentLookbackSeconds": SUPPORT_DEPLOYMENT_LOOKBACK_SECONDS,
            "multiVictimClusterWindowsSeconds": list(
                MULTI_VICTIM_CLUSTER_WINDOWS_SECONDS
            ),
            "scoreDeltaWindowsSeconds": list(SCORE_DELTA_WINDOWS_SECONDS),
            "honorsEventGapThresholds": list(HONORS_EVENT_GAP_THRESHOLDS),
            "bridgeDestructionWindowsSeconds": list(BRIDGE_DESTRUCTION_WINDOWS_SECONDS),
            "bridgeSpatialThresholds": list(BRIDGE_SPATIAL_THRESHOLDS),
            "shooterTargetLookbackSeconds": SHOOTER_TARGET_LOOKBACK_SECONDS,
            "shooterTargetWindowsSeconds": list(SHOOTER_TARGET_WINDOWS_SECONDS),
            "projectileSpatialThresholds": list(PROJECTILE_SPATIAL_THRESHOLDS),
            "supportImpactThresholds": list(SUPPORT_IMPACT_THRESHOLDS),
            "sameTickMeansExactRawFloatEquality": True,
        },
        "metrics": {
            "replayCatalogueRows": len(catalogue),
            "replayCatalogueUniqueSupportIds": len(set(catalogue)),
            "clockSamples": len(clock_samples),
            "sendTATaunts": len(linked_taunts),
            "tauntsWithValidCatalogueIndex": sum(
                taunt["supportId"] is not None for taunt in linked_taunts
            ),
            "tauntsWithSameTickVictimDestruction": sum(
                bool(taunt["sameTickVictimDestructions"]) for taunt in linked_taunts
            ),
            "sameTickVictimDestructions": len(same_tick),
            "sameTickVictimDestructionsWithKiller512": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL for item in same_tick
            ),
            "sameTickVictimDestructionsWithOtherKiller": sum(
                item["killerUnitId"] != KILLER_UNIT_SENTINEL for item in same_tick
            ),
            "unitDestructions": len(destructions),
            "unitDestructionsWithSyntheticTerminalDirection": sum(
                item["syntheticTerminalDirection"] for item in public_destructions
            ),
            "sentinelDestructionsWithSyntheticTerminalDirection": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["syntheticTerminalDirection"]
                for item in public_destructions
            ),
            "sentinelPlayerCompleteDestructionsWithSyntheticTerminalDirection": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["syntheticTerminalDirection"]
                and item["victimPlayerId"] is not None
                and item["unitTypeId"] not in INFANTRY_SOLDIER_TYPE_IDS
                for item in public_destructions
            ),
            "sentinelPlayerInfantryDestructionsWithSyntheticTerminalDirection": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["syntheticTerminalDirection"]
                and item["victimPlayerId"] is not None
                and item["unitTypeId"] in INFANTRY_SOLDIER_TYPE_IDS
                for item in public_destructions
            ),
            "blinkUnitEvents": len(blink_events),
            "resumeDispandEvents": len(resume_disband_events),
            "blowerBlewEvents": len(blower_events),
            "createCloudEvents": len(cloud_events),
            "unitCreatesWithSelfDestructParasite": self_destruct_unit_creates,
            "unitCreatesWithFortificationId": sum(
                counts["creates"] for counts in fortification_stats.values()
            ),
            "sentinelDestructionsOfSelfDestructUnitTypes": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["unitTypeId"] in self_destruct_unit_type_ids
                for item in public_destructions
            ),
            "unitRemovals": len(removals),
            "unitRemovalsWithFortificationId": sum(
                item["fortificationId"] is not None for item in public_removals
            ),
            "unitRemovalsWithSpecialistReplacementFlag": sum(
                item["isToBeReplacedBySpecialist"] for item in public_removals
            ),
            "identifiedOpposingPlayerDestructions": sum(
                item["victimPlayerId"] is not None
                and item["killerPlayerId"] is not None
                and item["victimPlayerId"] != item["killerPlayerId"]
                for item in public_destructions
            ),
            "identifiedSelfDestructions": sum(
                item["victimPlayerId"] is not None
                and item["killerPlayerId"] == item["victimPlayerId"]
                for item in public_destructions
            ),
            "playerOwnedDestructionsWithKiller512": sum(
                item["victimPlayerId"] is not None
                and item["killerUnitId"] == KILLER_UNIT_SENTINEL
                for item in public_destructions
            ),
            "playerOwnedDestructionsWithOtherUnresolvedKiller": sum(
                item["victimPlayerId"] is not None
                and item["killerUnitId"] != KILLER_UNIT_SENTINEL
                and item["killerPlayerId"] is None
                for item in public_destructions
            ),
            "worldOwnedOrUnknownVictimDestructions": sum(
                item["victimPlayerId"] is None for item in public_destructions
            ),
            "unitDestructionsWithFortificationId": sum(
                item["victimCreation"] is not None
                and item["victimCreation"]["fortificationId"] is not None
                for item in public_destructions
            ),
            "sentinelDestructionsWithFortificationId": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["victimCreation"] is not None
                and item["victimCreation"]["fortificationId"] is not None
                for item in public_destructions
            ),
            "sentinelSyntheticDestructionsWithFortificationId": sum(
                item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["syntheticTerminalDirection"]
                and item["victimCreation"] is not None
                and item["victimCreation"]["fortificationId"] is not None
                for item in public_destructions
            ),
            "destructionsWithLastUnitFramePosition": sum(
                item["victimPositionAtDestruction"] is not None
                for item in public_destructions
            ),
            "sentinelPlayerDeathsWithLastUnitFramePosition": sum(
                item["victimPlayerId"] is not None
                and item["killerUnitId"] == KILLER_UNIT_SENTINEL
                and item["victimPositionAtDestruction"] is not None
                for item in public_destructions
            ),
            "normalProjectileCreates": len(normal_projectiles),
            "shooterTargetEvents": len(shooter_target_events),
            "unitHealthEvents": len(unit_health_events),
            "setScoreEvents": len(score_events),
            "changeHonorsEvents": len(honors_events),
            "knownMapBridgeInstances": len(map_bridges),
            "knownBridgeDamageEvents": len(bridge_damage_events),
            "knownBridgeDestructionEvents": len(bridge_destructions),
            "positiveChangeHonorsEvents": sum(
                event["delta"] > 0.0 for event in honors_events
            ),
            "setScoreEventsWithPositiveDelta": sum(
                event["delta"] is not None and event["delta"] > 0
                for event in score_events
            ),
            "setScoreEventsWithNegativeDelta": sum(
                event["delta"] is not None and event["delta"] < 0
                for event in score_events
            ),
            "unitFramesCompact": unit_frame_counts["compact"],
            "unitFramesFull": unit_frame_counts["full"],
            "supportProjectileCreates": len(support_projectiles),
            "spawnExplosions": len(explosions),
            "spawnExplosionsWithCrater": sum(
                explosion["kind"] == "SpawnExplosionWithCrater"
                for explosion in explosions
            ),
            "tacticalAidDeployments": len(deployments),
            "tacticalAidMarkers": len(markers),
            "historicallyRecoverableKillerOwners": len(historical_recoverable),
            "activeKillerExactTargetProjectileLinks": sum(
                item["killerPlayerId"] is not None
                and item["historicalKillerExactTargetProjectileCount"] > 0
                for item in public_destructions
            ),
            "activeKillerExactTargetProjectilePlayerMismatches": sum(
                item["killerPlayerId"] is not None
                and bool(item["killerExactTargetProjectilePlayerIds"])
                and item["killerExactTargetProjectilePlayerIds"]
                != [item["killerPlayerId"]]
                for item in public_destructions
            ),
            "reusedUnitIds": sum(count > 1 for count in unit_id_reuse.values()),
            "candidateClassifications": dict(sorted(classifications.items())),
            "malformedMessages": dict(sorted(malformed.items())),
            **dict(sorted(shooter_target_metrics.items())),
            **dict(sorted(spatial_metrics.items())),
            **dict(sorted(area_effect_metrics.items())),
            **dict(sorted(score_delta_metrics.items())),
            **dict(sorted(honors_metrics.items())),
            **dict(sorted(bridge_metrics.items())),
            **{
                f"cloud{key[0].upper()}{key[1:]}": value
                for key, value in sorted(cloud_metrics.items())
            },
            **{
                f"buildingResident{key[0].upper()}{key[1:]}": value
                for key, value in sorted(building_resident_metrics.items())
            },
            **{
                f"blowerLifecycle{key[0].upper()}{key[1:]}": value
                for key, value in sorted(blower_lifecycle_metrics.items())
            },
            **{
                f"blinkLifecycle{key[0].upper()}{key[1:]}": value
                for key, value in sorted(blink_lifecycle_metrics.items())
            },
            **{
                f"ordinaryExactTarget{key[0].upper()}{key[1:]}": value
                for key, value in sorted(ordinary_fire_metrics.items())
            },
        },
        "bySupportId": {
            support: dict(sorted(counts.items()))
            for support, counts in sorted(support_stats.items())
        },
        "byFortificationUnitType": {
            unit_type: dict(sorted(counts.items()))
            for unit_type, counts in sorted(fortification_stats.items())
        },
        "byBinarySufficientCloudCause": {
            cause: dict(sorted(counts.items()))
            for cause, counts in sorted(cloud_cause_metrics.items())
        },
        "bySentinelVictimUnitType": dict(
            sorted(
                Counter(
                    item["unitTypeIdHex"] or "unknown"
                    for item in public_destructions
                    if item["victimPlayerId"] is not None
                    and item["killerUnitId"] == KILLER_UNIT_SENTINEL
                ).items()
            )
        ),
        "remainingUnknownCompletePlayerUnits": {
            "metrics": dict(sorted(remaining_unknown_metrics.items())),
            "byVictimUnitType": dict(sorted(remaining_unknown_types.items())),
            "bySyntheticDirectionVictimUnitType": dict(
                sorted(remaining_unknown_synthetic_types.items())
            ),
            "byNonSyntheticDirectionVictimUnitType": dict(
                sorted(remaining_unknown_nonsynthetic_types.items())
            ),
            "byNearbySupportProjectileId": dict(
                sorted(remaining_unknown_support_candidates.items())
            ),
            "scope": "playerOwnedCompleteUnitsWithSchemaV17CauseUnknown",
            "candidateSemantics": "temporalCensusOnlyNotCausalAttribution",
        },
    }
    if include_events:
        result["destructions"] = public_destructions
        result["blinkUnitEvents"] = public_blinks
        result["resumeDispandEvents"] = [
            _without_private_fields(event) for event in resume_disband_events
        ]
        result["blowerBlewEvents"] = [
            _without_private_fields(event) for event in blower_events
        ]
        result["createCloudEvents"] = [
            _without_private_fields(event) for event in cloud_events
        ]
        result["unitRemovals"] = public_removals
        result["setScoreEvents"] = [
            _without_private_fields(event) for event in score_events
        ]
        result["changeHonorsEvents"] = [
            _without_private_fields(event) for event in honors_events
        ]
        result["bridgeDamageEvents"] = [
            _without_private_fields(event) for event in bridge_damage_events
        ]
        result["buildingDamageEvents"] = [
            _without_private_fields(event) for event in building_damage_events
        ]
        result["blinkLifecycleOutcomes"] = blink_lifecycles
        result["blowerLifecycleOutcomes"] = blower_lifecycles
        result["taunts"] = linked_taunts
        result["historicallyRecoverableKillers"] = historical_recoverable
    return result


def analyse_safe(
    arguments: tuple[
        str,
        dict[int, str],
        set[int],
        dict[str, dict[int, BridgeInstance]],
        set[int],
        tuple[CloudTypeDefinition, ...],
        dict[int, UnitTypeParasites],
        bool,
    ],
) -> dict:
    (
        path,
        support_names,
        tactical_aid_projectile_ids,
        bridge_instances_by_map,
        self_destruct_unit_type_ids,
        support_cloud_definitions,
        unit_type_definitions,
        include_events,
    ) = arguments
    try:
        return analyse_replay(
            path,
            support_names,
            tactical_aid_projectile_ids,
            bridge_instances_by_map,
            self_destruct_unit_type_ids,
            support_cloud_definitions,
            unit_type_definitions,
            include_events=include_events,
        )
    except (OSError, ValueError) as error:
        return {
            "schemaVersion": 13,
            "path": path,
            "status": "failed",
            "error": f"{type(error).__name__}: {error}",
        }


def replay_paths(roots: list[str]) -> list[pathlib.Path]:
    paths = []
    seen = set()
    for root in roots:
        node = pathlib.Path(root)
        candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved not in seen:
                seen.add(resolved)
                paths.append(candidate)
    return paths


def summarise(results: list[dict]) -> dict:
    totals = Counter()
    classifications = Counter()
    by_support: dict[str, Counter] = {}
    by_fortification_unit_type: dict[str, Counter] = {}
    by_binary_sufficient_cloud_cause: dict[str, Counter] = {}
    sentinel_types = Counter()
    remaining_unknown_metrics = Counter()
    remaining_unknown_types = Counter()
    remaining_unknown_synthetic_types = Counter()
    remaining_unknown_nonsynthetic_types = Counter()
    remaining_unknown_support_candidates = Counter()
    for result in results:
        if result.get("status") == "failed":
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        for key, value in result["metrics"].items():
            if isinstance(value, int):
                totals[key] += value
        classifications.update(result["metrics"].get("candidateClassifications", {}))
        for support, result_counts in result.get("bySupportId", {}).items():
            counts = by_support.setdefault(support, Counter())
            counts.update(result_counts)
        for unit_type, result_counts in result.get(
            "byFortificationUnitType", {}
        ).items():
            counts = by_fortification_unit_type.setdefault(unit_type, Counter())
            counts.update(result_counts)
        for cause, result_counts in result.get(
            "byBinarySufficientCloudCause", {}
        ).items():
            counts = by_binary_sufficient_cloud_cause.setdefault(cause, Counter())
            counts.update(result_counts)
        sentinel_types.update(result.get("bySentinelVictimUnitType", {}))
        remaining = result.get("remainingUnknownCompletePlayerUnits", {})
        remaining_unknown_metrics.update(remaining.get("metrics", {}))
        remaining_unknown_types.update(remaining.get("byVictimUnitType", {}))
        remaining_unknown_synthetic_types.update(
            remaining.get("bySyntheticDirectionVictimUnitType", {})
        )
        remaining_unknown_nonsynthetic_types.update(
            remaining.get("byNonSyntheticDirectionVictimUnitType", {})
        )
        remaining_unknown_support_candidates.update(
            remaining.get("byNearbySupportProjectileId", {})
        )
    return {
        "schemaVersion": 13,
        **dict(sorted(totals.items())),
        "candidateClassifications": dict(sorted(classifications.items())),
        "bySupportId": {
            support: dict(sorted(counts.items()))
            for support, counts in sorted(by_support.items())
        },
        "byFortificationUnitType": {
            unit_type: dict(sorted(counts.items()))
            for unit_type, counts in sorted(by_fortification_unit_type.items())
        },
        "byBinarySufficientCloudCause": {
            cause: dict(sorted(counts.items()))
            for cause, counts in sorted(by_binary_sufficient_cloud_cause.items())
        },
        "bySentinelVictimUnitType": dict(sorted(sentinel_types.items())),
        "remainingUnknownCompletePlayerUnits": {
            "metrics": dict(sorted(remaining_unknown_metrics.items())),
            "byVictimUnitType": dict(sorted(remaining_unknown_types.items())),
            "bySyntheticDirectionVictimUnitType": dict(
                sorted(remaining_unknown_synthetic_types.items())
            ),
            "byNonSyntheticDirectionVictimUnitType": dict(
                sorted(remaining_unknown_nonsynthetic_types.items())
            ),
            "byNearbySupportProjectileId": dict(
                sorted(remaining_unknown_support_candidates.items())
            ),
            "scope": "playerOwnedCompleteUnitsWithSchemaV17CauseUnknown",
            "candidateSemantics": "temporalCensusOnlyNotCausalAttribution",
        },
        "tauntSemantics": "exactTacticalAidDamageThresholdAgainstPlayer",
        "sameTickDestructionSemantics": "candidateNotCausalAttribution",
        "historicalOwnershipSemantics": (
            "candidateWithLifecycleAndMatchingKillerProjectileEvidence"
        ),
        "temporalCandidateSemantics": "prescreenOnlyNotCausalAttribution",
        "areaEffectCandidateSemantics": (
            "precedingSameTickExplosionContainingLastSerializedVictimPosition;"
            "noActorBridgeAndNotCausalAttribution"
        ),
        "supportImpactCandidateSemantics": (
            "constantVectorProjectionToQualifiedBlastAndUniqueRecentDeploymentTeam;"
            "researchHypothesisNotValidatedBallistics"
        ),
        "ordinaryExactTargetCandidateSemantics": (
            "exactTargetLifecycleAndUnanimousActiveFiringActorOrTeamWithoutKillerUnitBridge;"
            "positiveControlResearchOnly"
        ),
        "scoreDeltaCandidateSemantics": (
            "preceding cumulative integer SetScore increases; positive-control research only"
        ),
        "positiveHonorsCandidateSemantics": (
            "recorder-scoped positive ChangeHonors preceding a destruction in the same raw tick;"
            "positive-control research only"
        ),
        "blinkLifecycleSemantics": (
            "BlinkUnit is a pending-removal signal shared by disband and other lifecycle paths;"
            "UnitRemove is non-destruction while UnitDestroy is the zero-health path"
        ),
        "resumeDispandSemantics": (
            "server snapshot/reporting state for a unit already on the shared blink path;"
            "not the initiating player disband request and not causal attribution"
        ),
        "createCloudSemantics": (
            "runtime type, position, remaining lifetime, and owning faction joined to shipped"
            " category, relationship, radius, metatype multiplier, and damage rules; even a"
            " fresh same-tick maximum-health-lethal geometry match is candidate-only because"
            " the wire omits the cloud instance that supplied the fatal health change"
        ),
        "blowerLifecycleSemantics": (
            "BlowerBlew proves an accepted owner-triggered detonation request for the exact active"
            " unit lifecycle; the server arms a 0.7-second fuse, but the replay does not serialize"
            " the later fuse-execution update, so terminal matches remain candidate evidence"
        ),
        "bridgeDestructionCandidateSemantics": (
            "exact shipped map bridge identity and RepairablePropDamaged state 3;"
            "server fatal bounds reconstructed from shipped death-physics boxes and exact map"
            " rotation; same-tick synthetic sentinel deaths inside those bounds remain research"
            " candidates unless replay position freshness is independently proven"
        ),
        "fortificationLifecycleSemantics": (
            "UnitCreate.aFortificationId binds the exact active fortification generation;"
            "sentinel synthetic-direction terminal events remain caller candidates until"
            "generic script-removal and other synthetic-fatal paths are excluded"
        ),
        "buildingResidentDamageCandidateSemantics": (
            "exact active-generation occupancy plus a preceding nonterminal BuildingDamaged "
            "row for the same building and identical raw float timestamp, sentinel killer, "
            "synthetic direction, and no same-building terminal transition in that tick; the "
            "dedicated server then synchronously applies damage to residents without a killer "
            "unit, but map callbacks execute between those operations, so this remains "
            "mechanical candidate evidence until shipped callback behavior and event-order "
            "controls are closed"
        ),
    }


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument("--json", help="write the report here")
    parser.add_argument(
        "--support-archive", default=DEFAULT_SUPPORT_ARCHIVE, help="shipped SDF"
    )
    parser.add_argument(
        "--summary-only", action="store_true", help="omit per-replay event rows"
    )
    parser.add_argument(
        "--jobs", type=positive_int, default=min(4, os.cpu_count() or 1)
    )
    args = parser.parse_args()

    paths = replay_paths(args.roots)
    if not paths:
        parser.error("no replay files found")
    support_names, tactical_aid_projectile_ids = load_support_catalogue(
        args.support_archive
    )
    bridge_instances_by_map = shipped_bridge_instances(args.support_archive)
    unit_type_definitions = shipped_unit_type_parasites(args.support_archive)
    self_destruct_parasite_hash = name_hash("SelfDestruct")
    self_destruct_unit_types = {
        type_id: definition
        for type_id, definition in unit_type_definitions.items()
        if self_destruct_parasite_hash in definition.parasite_type_hashes
    }
    support_cloud_types = shipped_support_cloud_types(args.support_archive)
    include_events = not args.summary_only
    work = [
        (
            str(path),
            support_names,
            tactical_aid_projectile_ids,
            bridge_instances_by_map,
            set(self_destruct_unit_types),
            support_cloud_types,
            unit_type_definitions,
            include_events,
        )
        for path in paths
    ]
    if args.jobs == 1:
        results = [analyse_safe(arguments) for arguments in work]
    else:
        start = "fork" if "fork" in multiprocessing.get_all_start_methods() else "spawn"
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=args.jobs,
            mp_context=multiprocessing.get_context(start),
        ) as executor:
            results = list(executor.map(analyse_safe, work))

    report = {
        "schemaVersion": 13,
        "summary": summarise(results),
        "shippedEvidence": {
            "multiplayerUnitDefinitionCount": len(unit_type_definitions),
            "minimumMultiplayerUnitMaxHealth": min(
                definition.max_health for definition in unit_type_definitions.values()
            ),
            "zeroMaxHealthMultiplayerUnitTypeCount": sum(
                definition.max_health == 0
                for definition in unit_type_definitions.values()
            ),
            "supportCloudDefinitionCount": len(support_cloud_types),
            "damagingSupportCloudDefinitionCount": sum(
                definition.health_change < 0 for definition in support_cloud_types
            ),
            "multiplayerDamagingSupportCloudDefinitionCount": sum(
                definition.health_change < 0
                and definition.support_section in MULTIPLAYER_SUPPORT_SECTIONS
                for definition in support_cloud_types
            ),
            "unitMetaTypeCounts": dict(
                sorted(
                    Counter(
                        definition.meta_type_name
                        for definition in unit_type_definitions.values()
                    ).items()
                )
            ),
            "unitCategoryCounts": dict(
                sorted(
                    Counter(
                        definition.unit_category_name
                        for definition in unit_type_definitions.values()
                    ).items()
                )
            ),
            "selfDestructUnitTypes": [
                {
                    "unitTypeId": type_id,
                    "unitTypeIdHex": f"0x{type_id:08x}",
                    "name": definition.name,
                }
                for type_id, definition in sorted(self_destruct_unit_types.items())
            ],
        },
        "replays": results,
    }
    rendered = json.dumps(report, indent=2) + "\n"
    print(rendered, end="")
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(rendered)
    return 1 if report["summary"].get("replaysFailed") else 0


if __name__ == "__main__":
    raise SystemExit(main())
