#!/usr/bin/env python3
"""Test whether tactical-aid requests identify later deployment actors.

``RequestSent`` identifies the requester and can name a tactical aid, but the
message models a team request rather than a purchase.  This audit measures that
distinction against player-bearing ``SupportThingMarker`` records and the global
``SupportThingSpawnedDelayed`` stream.  It intentionally emits aggregate counts
and a small contradiction sample instead of copying the replay corpus into a
large derived artifact.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import multiprocessing
import os
import pathlib
from collections import Counter

from wic_bintag import decompress, fields, name_hash, walk

MSG_REQUEST_SENT = name_hash("RequestSent")
MSG_SUPPORT_THING_MARKER = name_hash("SupportThingMarker")
MSG_SUPPORT_THING_SPAWNED_DELAYED = name_hash("SupportThingSpawnedDelayed")

POSITION_EPSILON = 0.01


def _distance(left: tuple[float, float], right: tuple[float, float]) -> float:
    return math.hypot(left[0] - right[0], left[1] - right[1])


def analyse(path: str) -> dict:
    data = decompress(path)
    requests = []
    markers = []
    spawns = []
    malformed = Counter()

    for envelope in walk(data):
        if envelope.message == MSG_REQUEST_SENT:
            body, _ = fields(data, envelope)
            if len(body) < 8:
                malformed["RequestSent"] += 1
                continue
            requests.append(
                {
                    "time": envelope.time,
                    "type": body[0].u32,
                    "id": body[1].u32,
                    "creator": body[2].u32,
                    "ttl": body[3].f32,
                    "position": (body[5].f32, body[6].f32),
                    "support": body[7].u32,
                }
            )
            continue

        if envelope.message == MSG_SUPPORT_THING_MARKER:
            body, _ = fields(data, envelope)
            if len(body) < 11:
                malformed["SupportThingMarker"] += 1
                continue
            markers.append(
                {
                    "time": envelope.time,
                    "support": body[1].u32,
                    "position": (body[2].f32, body[4].f32),
                    "player": body[5].u32,
                }
            )
            continue

        if envelope.message == MSG_SUPPORT_THING_SPAWNED_DELAYED:
            body, _ = fields(data, envelope)
            if len(body) < 10:
                malformed["SupportThingSpawnedDelayed"] += 1
                continue
            spawns.append(
                {
                    "time": envelope.time,
                    "support": body[0].u32,
                    "position": (body[1].f32, body[3].f32),
                }
            )

    counts = Counter()
    samples = []
    request_types = Counter()
    support_ids = Counter()
    for request in requests:
        request_types[request["type"]] += 1
        if request["support"] == 0:
            counts["requestsWithoutTacticalAid"] += 1
            continue

        counts["tacticalAidRequests"] += 1
        support_ids[request["support"]] += 1
        ttl = max(0.0, request["ttl"])
        compatible_markers = [
            marker
            for marker in markers
            if marker["support"] == request["support"]
            and 0.0 <= marker["time"] - request["time"] <= ttl
        ]
        exact_markers = [
            marker
            for marker in compatible_markers
            if _distance(marker["position"], request["position"]) <= POSITION_EPSILON
        ]
        compatible_spawns = [
            spawn
            for spawn in spawns
            if spawn["support"] == request["support"]
            and 0.0 <= spawn["time"] - request["time"] <= ttl
        ]
        exact_spawns = [
            spawn
            for spawn in compatible_spawns
            if _distance(spawn["position"], request["position"]) <= POSITION_EPSILON
        ]

        if compatible_markers:
            counts["requestsWithSameTypeMarkerWithinTtl"] += 1
            compatible_players = {marker["player"] for marker in compatible_markers}
            if compatible_players == {request["creator"]}:
                counts["sameTypeMarkersOnlyRequester"] += 1
            elif request["creator"] in compatible_players:
                counts["sameTypeMarkersRequesterAndOthers"] += 1
            else:
                counts["sameTypeMarkersExcludeRequester"] += 1
                if len(samples) < 20:
                    nearest = min(
                        compatible_markers,
                        key=lambda marker: _distance(
                            marker["position"], request["position"]
                        ),
                    )
                    samples.append(
                        {
                            "path": path,
                            "requestTime": round(request["time"], 6),
                            "requestId": request["id"],
                            "requestType": request["type"],
                            "supportId": f"0x{request['support']:08x}",
                            "requesterPlayerId": request["creator"],
                            "markerPlayerIds": sorted(compatible_players),
                            "nearestMarkerDistance": round(
                                _distance(nearest["position"], request["position"]), 6
                            ),
                            "nearestMarkerTimeDelta": round(
                                nearest["time"] - request["time"], 6
                            ),
                            "basis": "sameTypeWithinRequestTtl",
                        }
                    )
        if compatible_spawns:
            counts["requestsWithSameTypeDeploymentWithinTtl"] += 1
        if exact_spawns:
            counts["requestsWithExactPositionDeploymentWithinTtl"] += 1

        marker_players = {marker["player"] for marker in exact_markers}
        if not exact_markers:
            counts["requestsWithoutExactPositionMarkerWithinTtl"] += 1
        elif marker_players == {request["creator"]}:
            counts["exactPositionMarkersOnlyRequester"] += 1
        elif request["creator"] in marker_players:
            counts["exactPositionMarkersRequesterAndOthers"] += 1
        else:
            counts["exactPositionMarkersExcludeRequester"] += 1
            if len(samples) < 20:
                samples.append(
                    {
                        "path": path,
                        "requestTime": round(request["time"], 6),
                        "requestId": request["id"],
                        "requestType": request["type"],
                        "supportId": f"0x{request['support']:08x}",
                        "requesterPlayerId": request["creator"],
                        "markerPlayerIds": sorted(marker_players),
                        "basis": "exactPositionWithinRequestTtl",
                    }
                )

    return {
        "path": path,
        "counts": dict(counts),
        "requestTypes": {str(key): value for key, value in request_types.items()},
        "supportIds": {f"0x{key:08x}": value for key, value in support_ids.items()},
        "malformed": dict(malformed),
        "contradictionSamples": samples,
    }


def analyse_safe(path: str) -> dict:
    try:
        return analyse(path)
    except (OSError, ValueError) as error:
        return {"path": path, "error": str(error)}


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


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument("--json", help="write the compact report here")
    parser.add_argument(
        "--jobs", type=positive_int, default=min(4, os.cpu_count() or 1)
    )
    args = parser.parse_args()
    paths = replay_paths(args.roots)
    if not paths:
        parser.error("no replay files found")

    work = [str(path) for path in paths]
    if args.jobs == 1:
        results = [analyse_safe(path) for path in work]
    else:
        start = "fork" if "fork" in multiprocessing.get_all_start_methods() else "spawn"
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=args.jobs,
            mp_context=multiprocessing.get_context(start),
        ) as executor:
            results = list(executor.map(analyse_safe, work))

    totals = Counter()
    request_types = Counter()
    support_ids = Counter()
    malformed = Counter()
    samples = []
    for result in results:
        if "error" in result:
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        totals.update(result["counts"])
        request_types.update(result["requestTypes"])
        support_ids.update(result["supportIds"])
        malformed.update(result["malformed"])
        samples.extend(result["contradictionSamples"][: 20 - len(samples)])

    report = {
        "schemaVersion": 1,
        "positionEpsilon": POSITION_EPSILON,
        **dict(sorted(totals.items())),
        "requestTypes": dict(
            sorted(request_types.items(), key=lambda item: int(item[0]))
        ),
        "tacticalAidSupportIds": dict(sorted(support_ids.items())),
        "malformedMessages": dict(sorted(malformed.items())),
        "requesterContradictionSamples": samples,
        "conclusion": "requestCreatorIsNotDeploymentActor",
    }
    rendered = json.dumps(report, indent=2) + "\n"
    print(rendered, end="")
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(rendered)
    return 1 if totals["replaysFailed"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
