#!/usr/bin/env python3
"""Audit single-replay player bridges for opposing tactical-aid deployments.

The release timeline deliberately leaves both-faction deployments team-only because
``SupportThingSpawnedDelayed`` has no player field.  This research tool tests other
serialized evidence against visible-faction ``SupportThingMarker`` ground truth
before any bridge is considered for the canonical parser.

The first bridge under test is the unit-spawn path.  Shipped support definitions
name the unit type created by each of the nine multiplayer unit-drop aids, while a
``UnitCreate`` record serializes the owning player, faction, unit type, position,
and ``aSpawnSource``.  The latter follows a one-byte boolean field that uses a
different BinTag separator, so it is recovered by its validated field hash rather
than by positional guessing.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import multiprocessing
import os
import pathlib
import struct
from collections import Counter, defaultdict
from dataclasses import dataclass

from wic_bintag import FIELD_SEP, decompress, fields, name_hash, recorder_slot, walk

MSG_UNIT_CREATE = name_hash("UnitCreate")
MSG_SUPPORT_THING_SPAWNED_DELAYED = name_hash("SupportThingSpawnedDelayed")
MSG_SUPPORT_THING_MARKER = name_hash("SupportThingMarker")
MSG_PLAYER_JOINED_TEAM = name_hash("PlayerJoinedTeam")
MSG_SPECTATOR_JOINED_TEAM = name_hash("SpectatorJoinedTeam")

FIELD_SPAWN_SOURCE = name_hash("aSpawnSource")

# Broad observational bounds used to inventory candidates, not to prove a link.
# Exactness is evaluated independently against marker player slots below.
CANDIDATE_MAX_HORIZONTAL_DISTANCE = 256.0
CANDIDATE_MAX_RAW_TIME_DELTA = 120.0

# Corpus acceptance windows for the binary-proven spawn-source-1 unit path.  These
# are deliberately narrower than the candidate inventory.  They cover the stable
# flight-animation arrival bands while rejecting unrelated later drops.  Exact
# player emission additionally requires that every compatible unit is owned by the
# same player.  The marker ledger validates this rule independently below.
STRICT_BOUNDS: dict[str, tuple[float, float, float]] = {
    "airborneInfantry": (29.0, 35.0, 30.0),
    "airdroppedLightTank": (16.0, 22.0, 15.0),
    "airdroppedTransport": (16.0, 22.0, 15.0),
}


@dataclass(frozen=True)
class DropSpec:
    family: str
    faction: int
    support_name: str
    unit_type_name: str

    @property
    def unit_type_id(self) -> int:
        return name_hash(self.unit_type_name)


DROP_SPECS: dict[int, DropSpec] = {
    0x2A640597: DropSpec("airborneInfantry", 1, "Paratrooper_US", "US_Squad_Airborne"),
    0x2861054E: DropSpec("airdroppedTransport", 1, "Repair_Jeep_US", "US_HUMWEE"),
    0x22DC04ED: DropSpec("airdroppedLightTank", 1, "Light_Tank_US", "US_Sheridan"),
    0x36370621: DropSpec(
        "airborneInfantry", 2, "Paratrooper_NATO", "NATO_Squad_Airborne"
    ),
    0x33A205D8: DropSpec(
        "airdroppedTransport", 2, "Repair_Jeep_NATO", "NATO_LandRover"
    ),
    0x2D5B0577: DropSpec("airdroppedLightTank", 2, "Light_Tank_NATO", "NATO_AMX30"),
    0x368A063C: DropSpec(
        "airborneInfantry", 3, "Paratrooper_USSR", "USSR_Squad_Airborne"
    ),
    0x33F505F3: DropSpec("airdroppedTransport", 3, "Repair_Jeep_USSR", "USSR_UAZ469"),
    0x2DAE0592: DropSpec("airdroppedLightTank", 3, "Light_Tank_USSR", "USSR_Bmp_R"),
}

EXPECTED_UNIT_TYPES = {
    spec.unit_type_id: support_id for support_id, spec in DROP_SPECS.items()
}


@dataclass(frozen=True)
class Deployment:
    event_index: int
    offset: int
    raw_time: float
    deployment_time: float
    support_id: int
    team: int
    position: tuple[float, float, float]
    recorder_view: str


@dataclass(frozen=True)
class Marker:
    offset: int
    support_id: int
    player_id: int
    position: tuple[float, float, float]


@dataclass(frozen=True)
class UnitCreate:
    event_index: int
    offset: int
    raw_time: float
    player_id: int
    team: int
    unit_id: int
    unit_type_id: int
    position: tuple[float, float, float]
    spawn_source: int


def _position(values, start: int) -> tuple[float, float, float]:
    return tuple(field.f32 for field in values[start : start + 3])


def _field_u32(
    data: bytes, body_start: int, body_end: int, field_hash: int
) -> int | None:
    """Read a named four-byte BinTag field inside one message body.

    UnitCreate's preceding one-byte boolean field uses the 0x0e separator, which
    stops the generic 17-byte field walk.  Searching only inside the validated
    envelope and requiring the normal separator on the target field keeps this
    bounded and structural.
    """
    tag = struct.pack("<I", field_hash)
    cursor = body_start
    while True:
        cursor = data.find(tag, cursor, body_end)
        if cursor < 0:
            return None
        if (
            cursor + 17 <= body_end
            and data[cursor + 4 : cursor + 8] == FIELD_SEP
            and data[cursor + 9 : cursor + 13] == FIELD_SEP
        ):
            return struct.unpack_from("<I", data, cursor + 13)[0]
        cursor += 1


def horizontal_distance(
    left: tuple[float, float, float], right: tuple[float, float, float]
) -> float:
    return math.hypot(left[0] - right[0], left[2] - right[2])


def _group_marker_players(markers: list[Marker]) -> dict[tuple, set[int]]:
    grouped: dict[tuple, set[int]] = defaultdict(set)
    for marker in markers:
        grouped[(marker.support_id, marker.position)].add(marker.player_id)
    return grouped


def analyse_replay(path: str) -> dict:
    replay_path = pathlib.Path(path)
    data = decompress(replay_path)
    recorder = recorder_slot(data)
    recorder_view = "notSpectating"
    deployments: list[Deployment] = []
    markers: list[Marker] = []
    expected_units: list[UnitCreate] = []
    all_spawn_sources = Counter()
    malformed = Counter()

    for event_index, envelope in enumerate(walk(data)):
        if envelope.message == MSG_PLAYER_JOINED_TEAM:
            body, _ = fields(data, envelope)
            if len(body) >= 2 and body[0].u32 == recorder:
                recorder_view = "notSpectating"
            continue

        if envelope.message == MSG_SPECTATOR_JOINED_TEAM:
            body, _ = fields(data, envelope)
            if len(body) >= 3 and body[0].u32 == recorder:
                recorder_view = (
                    "oneTeam"
                    if body[2].u32 == 1 and body[1].u32 in {1, 2, 3}
                    else "allTeams"
                    if body[2].u32 == 2
                    else "spectatorUnknown"
                )
            continue

        if envelope.message == MSG_UNIT_CREATE:
            body, _ = fields(data, envelope)
            spawn_source = _field_u32(
                data, envelope.body_start, envelope.body_end, FIELD_SPAWN_SOURCE
            )
            if len(body) < 8 or spawn_source is None:
                malformed["UnitCreate"] += 1
                continue
            all_spawn_sources[spawn_source] += 1
            if body[4].u32 not in EXPECTED_UNIT_TYPES:
                continue
            expected_units.append(
                UnitCreate(
                    event_index=event_index,
                    offset=envelope.offset,
                    raw_time=envelope.time,
                    player_id=body[1].u32,
                    team=body[2].i32,
                    unit_id=body[3].u32,
                    unit_type_id=body[4].u32,
                    position=_position(body, 5),
                    spawn_source=spawn_source,
                )
            )
            continue

        if envelope.message == MSG_SUPPORT_THING_SPAWNED_DELAYED:
            body, _ = fields(data, envelope)
            if len(body) < 10:
                malformed["SupportThingSpawnedDelayed"] += 1
                continue
            support_id = body[0].u32
            if support_id not in DROP_SPECS:
                continue
            deployments.append(
                Deployment(
                    event_index=event_index,
                    offset=envelope.offset,
                    raw_time=envelope.time,
                    deployment_time=envelope.time - body[9].f32,
                    support_id=support_id,
                    team=body[4].i32,
                    position=_position(body, 1),
                    recorder_view=recorder_view,
                )
            )
            continue

        if envelope.message == MSG_SUPPORT_THING_MARKER:
            body, _ = fields(data, envelope)
            if len(body) < 11:
                malformed["SupportThingMarker"] += 1
                continue
            support_id = body[1].u32
            if support_id not in DROP_SPECS:
                continue
            markers.append(
                Marker(
                    offset=envelope.offset,
                    support_id=support_id,
                    player_id=body[5].i32,
                    position=_position(body, 2),
                )
            )

    marker_players = _group_marker_players(markers)
    rows = []
    for deployment in deployments:
        spec = DROP_SPECS[deployment.support_id]
        exact_players = marker_players.get(
            (deployment.support_id, deployment.position), set()
        )
        exact_player = next(iter(exact_players)) if len(exact_players) == 1 else None

        candidates = []
        for unit in expected_units:
            if unit.unit_type_id != spec.unit_type_id or unit.team != deployment.team:
                continue
            raw_delta = unit.raw_time - deployment.raw_time
            if not 0.0 <= raw_delta <= CANDIDATE_MAX_RAW_TIME_DELTA:
                continue
            distance = horizontal_distance(unit.position, deployment.position)
            if distance > CANDIDATE_MAX_HORIZONTAL_DISTANCE:
                continue
            if unit.spawn_source == 0:
                continue
            deployment_delta = unit.raw_time - deployment.deployment_time
            candidates.append((unit, distance, raw_delta, deployment_delta))

        candidates.sort(
            key=lambda item: (item[1], item[3], item[0].offset, item[0].unit_id)
        )
        candidate_players = sorted({item[0].player_id for item in candidates})
        candidate_class = (
            "uniquePlayer"
            if len(candidate_players) == 1
            else "ambiguousPlayers"
            if candidate_players
            else "unmatched"
        )
        if exact_player is None:
            validation = "noMarkerGroundTruth"
        elif candidate_class == "uniquePlayer" and candidate_players[0] == exact_player:
            validation = "agrees"
        elif candidate_class == "uniquePlayer":
            validation = "contradicts"
        elif exact_player in candidate_players:
            validation = "containsExactAmongAmbiguous"
        else:
            validation = "missesExact"

        strict_min_time, strict_max_time, strict_max_distance = STRICT_BOUNDS[
            spec.family
        ]
        strict_candidates = [
            item
            for item in candidates
            if item[0].spawn_source == 1
            and strict_min_time <= item[3] <= strict_max_time
            and item[1] <= strict_max_distance
        ]
        strict_players = sorted({item[0].player_id for item in strict_candidates})
        strict_player = strict_players[0] if len(strict_players) == 1 else None
        if exact_player is None:
            strict_validation = "noMarkerGroundTruth"
        elif strict_player is None:
            strict_validation = "unresolved"
        elif strict_player == exact_player:
            strict_validation = "agrees"
        else:
            strict_validation = "contradicts"

        rows.append(
            {
                "eventIndex": deployment.event_index,
                "offset": deployment.offset,
                "rawTime": round(deployment.raw_time, 6),
                "deploymentTime": round(deployment.deployment_time, 6),
                "supportId": f"0x{deployment.support_id:08x}",
                "supportName": spec.support_name,
                "family": spec.family,
                "team": deployment.team,
                "recorderView": deployment.recorder_view,
                "position": [round(value, 6) for value in deployment.position],
                "markerPlayerIds": sorted(exact_players),
                "candidateClass": candidate_class,
                "candidatePlayerIds": candidate_players,
                "markerValidation": validation,
                "strictUnitBridgePlayerId": strict_player,
                "strictUnitBridgeCandidatePlayerIds": strict_players,
                "strictUnitBridgeCandidateCount": len(strict_candidates),
                "strictUnitBridgeValidation": strict_validation,
                "unitCandidates": [
                    {
                        "eventIndex": unit.event_index,
                        "offset": unit.offset,
                        "rawTime": round(unit.raw_time, 6),
                        "rawTimeDelta": round(raw_delta, 6),
                        "deploymentTimeDelta": round(deployment_delta, 6),
                        "horizontalDistance": round(distance, 6),
                        "playerId": unit.player_id,
                        "team": unit.team,
                        "unitId": unit.unit_id,
                        "unitTypeId": f"0x{unit.unit_type_id:08x}",
                        "unitTypeName": spec.unit_type_name,
                        "spawnSource": unit.spawn_source,
                        "position": [round(value, 6) for value in unit.position],
                    }
                    for unit, distance, raw_delta, deployment_delta in candidates
                ],
            }
        )

    return {
        "path": str(replay_path),
        "deployments": rows,
        "metrics": {
            "unitDropDeployments": len(deployments),
            "unitDropMarkers": len(markers),
            "expectedUnitCreates": len(expected_units),
            "unitCreateSpawnSources": {
                str(key): value for key, value in sorted(all_spawn_sources.items())
            },
            "malformedMessages": dict(sorted(malformed.items())),
        },
    }


def analyse_replay_safe(path: str) -> dict:
    try:
        return analyse_replay(path)
    except (OSError, ValueError) as error:
        return {"path": path, "status": "failed", "error": str(error)}


def summarise(results: list[dict]) -> dict:
    totals = Counter()
    by_family: dict[str, Counter] = defaultdict(Counter)
    spawn_sources = Counter()
    samples: dict[str, list[dict]] = defaultdict(list)

    for result in results:
        if result.get("status") == "failed":
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        for source, count in result["metrics"]["unitCreateSpawnSources"].items():
            spawn_sources[int(source)] += count
        for row in result["deployments"]:
            totals["unitDropDeployments"] += 1
            totals[f"candidate_{row['candidateClass']}"] += 1
            totals[f"validation_{row['markerValidation']}"] += 1
            family = by_family[row["family"]]
            family["deployments"] += 1
            family[f"candidate_{row['candidateClass']}"] += 1
            family[f"validation_{row['markerValidation']}"] += 1
            totals[f"strict_{row['strictUnitBridgeValidation']}"] += 1
            family[f"strict_{row['strictUnitBridgeValidation']}"] += 1
            if row["strictUnitBridgePlayerId"] is not None:
                totals["strict_attributed"] += 1
                totals[f"strict_attributed_view_{row['recorderView']}"] += 1
                family["strict_attributed"] += 1
                if row["markerPlayerIds"]:
                    totals["strict_attributed_with_marker"] += 1
                else:
                    totals["strict_attributed_without_marker"] += 1
                    totals[
                        f"strict_attributed_without_marker_view_{row['recorderView']}"
                    ] += 1
            elif row["markerPlayerIds"]:
                totals["strict_unattributed_with_marker"] += 1
            else:
                totals["strict_unattributed_without_marker"] += 1
            key = row["markerValidation"]
            if key != "agrees" and len(samples[key]) < 20:
                samples[key].append(
                    {
                        "path": result["path"],
                        "supportId": row["supportId"],
                        "team": row["team"],
                        "rawTime": row["rawTime"],
                        "markerPlayerIds": row["markerPlayerIds"],
                        "candidatePlayerIds": row["candidatePlayerIds"],
                        "candidateCount": len(row["unitCandidates"]),
                    }
                )

    return {
        "schemaVersion": 2,
        "candidateInventoryBounds": {
            "maximumHorizontalDistance": CANDIDATE_MAX_HORIZONTAL_DISTANCE,
            "maximumRawTimeDelta": CANDIDATE_MAX_RAW_TIME_DELTA,
            "requiresExpectedUnitType": True,
            "requiresSameFaction": True,
            "requiresNonzeroSpawnSource": True,
            "status": "researchOnlyNotAnAttributionRule",
        },
        "strictUnitBridge": {
            "requiresSpawnSource": 1,
            "requiresExpectedUnitType": True,
            "requiresSameFaction": True,
            "requiresSingleCandidatePlayer": True,
            "boundsByFamily": {
                family: {
                    "minimumRawTimeDelta": values[0],
                    "maximumRawTimeDelta": values[1],
                    "maximumHorizontalDistance": values[2],
                }
                for family, values in STRICT_BOUNDS.items()
            },
            "acceptanceGate": "zeroMarkerGroundTruthContradictions",
        },
        **dict(sorted(totals.items())),
        "unitCreateSpawnSources": {
            str(key): value for key, value in sorted(spawn_sources.items())
        },
        "families": {
            family: dict(sorted(counts.items()))
            for family, counts in sorted(by_family.items())
        },
        "samples": dict(sorted(samples.items())),
    }


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def replay_paths(roots: list[str]) -> list[pathlib.Path]:
    paths = []
    seen: set[pathlib.Path] = set()
    for root in roots:
        node = pathlib.Path(root)
        candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved not in seen:
                seen.add(resolved)
                paths.append(candidate)
    return paths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument("--json", help="write summary and per-replay rows here")
    parser.add_argument(
        "--summary-only",
        action="store_true",
        help="write only the compact summary when used with --json",
    )
    parser.add_argument(
        "--jobs",
        type=positive_int,
        default=min(4, os.cpu_count() or 1),
        help="parallel replay workers (default: auto, capped at 4)",
    )
    args = parser.parse_args()

    paths = replay_paths(args.roots)
    if not paths:
        parser.error("no replay files found")
    work = [str(path) for path in paths]
    if args.jobs == 1:
        results = [analyse_replay_safe(path) for path in work]
    else:
        start_method = (
            "fork" if "fork" in multiprocessing.get_all_start_methods() else "spawn"
        )
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=args.jobs,
            mp_context=multiprocessing.get_context(start_method),
        ) as executor:
            results = list(executor.map(analyse_replay_safe, work))

    summary = summarise(results)
    print(json.dumps(summary, indent=2))
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        report = (
            summary if args.summary_only else {"summary": summary, "replays": results}
        )
        target.write_text(json.dumps(report, indent=2) + "\n")
    return 1 if summary.get("replaysFailed", 0) else 0


if __name__ == "__main__":
    raise SystemExit(main())
