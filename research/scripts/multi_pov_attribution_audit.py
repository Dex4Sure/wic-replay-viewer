#!/usr/bin/env python3
"""Recover opposing-team tactical-aid players by aligning multi-POV recordings.

A single replay serializes player-bearing `SupportThingMarker` records for only
one faction, while top-level deployments cover both factions without a player.
Two recordings of the same match from different points of view therefore carry
complementary marker sets, and each can attribute the other's unknowns.

The join is exact rather than statistical. Both clients receive the same server
message, so a deployment's `(supportId, position, team)` triple is bit-identical
across recordings; positions are float triples, so the triple is effectively a
unique event identifier. Nothing here aligns timestamps: recording clocks have
per-recording origins and are never compared.

Matching runs in two phases:

1. A cheap `(serverName, dateTime, gameMode)` bucket proposes candidate groups.
   This bounds how many replays need a full timeline parse. It can miss a group
   whose header fields differ, but it cannot admit one, because every pair is
   then re-tested.
2. The strong test confirms a pair: rosters must agree on every shared slot, and
   the two deployment-key sets must share at least `--min-shared` keys. Pairs
   that fail are dropped.

Same-POV re-uploads are rejected before merging. They add no marker coverage and
would otherwise read as independent confirmation of attributions they merely
copy.

A deployment is attributed only when the other recordings name exactly one
player for its key. Disagreement is reported as a contradiction and never
resolved by majority.

Usage:

    research/scripts/multi_pov_attribution_audit.py local/replays/main \
        --binary target/release/wic_replay_parser \
        --json local/generated/multi-pov-attribution-audit.json
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import pathlib
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

PLAYING_TEAMS = (1, 2, 3)


def find_replays(roots: list[str]) -> list[pathlib.Path]:
    found: list[pathlib.Path] = []
    for root in roots:
        path = pathlib.Path(root)
        if path.is_file():
            found.append(path)
        else:
            found.extend(sorted(path.rglob("*.wicdemo")))
    return found


def run_parser(binary: str, flag: str, paths: list[pathlib.Path]) -> dict:
    result = subprocess.run(
        [binary, flag, *[str(p) for p in paths]],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "parser failed")
    return json.loads(result.stdout)


def deployment_key(event: dict) -> tuple:
    """Match identity: the full serialized triple, used to confirm one match."""
    return (event["supportId"], tuple(event["position"]), event["team"])


def marker_key(event: dict) -> tuple:
    """Join key shared by markers and deployments.

    A marker carries no team field of its own — its nominal `aTeam` is the
    issuing player slot — so the join drops team. The support definition already
    encodes the faction, and every corpus deployment's team agrees with it.
    """
    return (event["supportId"], tuple(event["position"]))


def summarize(args: tuple[str, str]) -> dict | None:
    """Reduce one replay to the records the merge needs."""
    binary, path = args
    try:
        document = run_parser(binary, "--timeline-json", [pathlib.Path(path)])
    except Exception as error:  # noqa: BLE001 - reported per replay, never fatal
        return {"path": path, "error": f"{type(error).__name__}: {error}"}
    replay = document["replay"]
    timeline = document["timeline"]

    deployments = []
    markers = []
    for event in timeline["events"]:
        if event["type"] == "tacticalAidDeployed":
            deployments.append(
                {
                    "key": deployment_key(event),
                    "joinKey": marker_key(event),
                    "playerId": event.get("playerId"),
                    "team": event["team"],
                    "supportName": event.get("supportName"),
                }
            )
        elif event["type"] == "tacticalAidMarker":
            markers.append({"key": marker_key(event), "playerId": event["playerId"]})

    return {
        "path": path,
        "sha256": hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest(),
        "recorder": replay.get("recorder"),
        "serverName": replay["gameInfo"].get("serverName"),
        "dateTime": replay["gameInfo"].get("dateTime"),
        "gameMode": replay["gameInfo"].get("gameMode"),
        "mapDisplayName": replay["gameInfo"].get("mapDisplayName"),
        "roster": {str(p["id"]): p["name"] for p in replay["players"]},
        "matchDurationSeconds": timeline.get("matchDurationSeconds"),
        "recordingSeconds": timeline.get("durationSeconds"),
        "deployments": deployments,
        "markers": markers,
    }


def rosters_agree(left: dict, right: dict) -> tuple[bool, int]:
    """Slots present in both must name the same player."""
    shared = set(left["roster"]) & set(right["roster"])
    if not shared:
        return False, 0
    for slot in shared:
        if left["roster"][slot] != right["roster"][slot]:
            return False, len(shared)
    return True, len(shared)


def same_match(left: dict, right: dict, min_shared: int) -> tuple[bool, str, int, int]:
    agree, shared_slots = rosters_agree(left, right)
    keys_left = {d["key"] for d in left["deployments"]}
    keys_right = {d["key"] for d in right["deployments"]}
    shared_keys = len(keys_left & keys_right)
    if not agree:
        return False, "rosterConflict", shared_slots, shared_keys
    if shared_keys < min_shared:
        return False, "insufficientSharedDeployments", shared_slots, shared_keys
    return True, "confirmed", shared_slots, shared_keys


def duplicate_pov(left: dict, right: dict) -> bool:
    """Same point of view re-uploaded: no independent marker coverage."""
    if left["sha256"] == right["sha256"]:
        return True
    marks_left = collections.Counter((m["key"], m["playerId"]) for m in left["markers"])
    marks_right = collections.Counter(
        (m["key"], m["playerId"]) for m in right["markers"]
    )
    return bool(marks_left) and marks_left == marks_right


def build_groups(
    records: list[dict], min_shared: int
) -> tuple[list[list[dict]], list[dict]]:
    """Bucket cheaply, then confirm every pair with the strong test."""
    buckets = collections.defaultdict(list)
    for record in records:
        buckets[(record["serverName"], record["dateTime"], record["gameMode"])].append(
            record
        )

    groups: list[list[dict]] = []
    rejected: list[dict] = []
    for bucket in buckets.values():
        if len(bucket) < 2:
            continue
        # Union members that the strong test confirms as the same match.
        parent = list(range(len(bucket)))

        def find(i: int) -> int:
            while parent[i] != i:
                parent[i] = parent[parent[i]]
                i = parent[i]
            return i

        for i in range(len(bucket)):
            for j in range(i + 1, len(bucket)):
                ok, reason, slots, keys = same_match(bucket[i], bucket[j], min_shared)
                if ok:
                    parent[find(i)] = find(j)
                else:
                    rejected.append(
                        {
                            "left": bucket[i]["path"],
                            "right": bucket[j]["path"],
                            "reason": reason,
                            "sharedSlots": slots,
                            "sharedDeploymentKeys": keys,
                        }
                    )
        clusters = collections.defaultdict(list)
        for i, record in enumerate(bucket):
            clusters[find(i)].append(record)
        groups.extend(c for c in clusters.values() if len(c) > 1)
    return groups, rejected


def merge_group(group: list[dict]) -> dict:
    """Attribute each member's unknown deployments from the other members."""
    kept: list[dict] = []
    dropped: list[dict] = []
    for record in group:
        if any(duplicate_pov(record, other) for other in kept):
            dropped.append(record["path"])
        else:
            kept.append(record)

    per_replay = []
    contradictions = []
    gained_total = ambiguous_total = confirmed_total = 0

    for record in kept:
        others = [o for o in kept if o is not record]
        index = collections.defaultdict(set)
        for other in others:
            for marker in other["markers"]:
                index[marker["key"]].add(marker["playerId"])

        before = sum(1 for d in record["deployments"] if d["playerId"] is not None)
        gained = ambiguous = confirmed = 0
        unresolved = collections.Counter()
        for deployment in record["deployments"]:
            candidates = index.get(deployment["joinKey"])
            if deployment["playerId"] is not None:
                # Independent cross-check of an attribution the replay already had.
                if candidates:
                    if candidates == {deployment["playerId"]}:
                        confirmed += 1
                    else:
                        contradictions.append(
                            {
                                "path": record["path"],
                                "supportName": deployment["supportName"],
                                "ownPlayerId": deployment["playerId"],
                                "otherPlayerIds": sorted(candidates),
                            }
                        )
                continue
            if not candidates:
                unresolved[deployment["supportName"] or "unknown"] += 1
            elif len(candidates) == 1:
                gained += 1
            else:
                ambiguous += 1
                unresolved[deployment["supportName"] or "unknown"] += 1

        gained_total += gained
        ambiguous_total += ambiguous
        confirmed_total += confirmed
        per_replay.append(
            {
                "path": record["path"],
                "recorder": record["recorder"],
                "deployments": len(record["deployments"]),
                "attributedAlone": before,
                "attributedWithGroup": before + gained,
                "gained": gained,
                "ambiguous": ambiguous,
                "crossConfirmed": confirmed,
                "unresolvedBySupport": dict(unresolved.most_common()),
            }
        )

    return {
        "mapDisplayName": kept[0]["mapDisplayName"],
        "dateTime": kept[0]["dateTime"],
        "serverName": kept[0]["serverName"],
        "members": len(kept),
        "duplicatePovDropped": dropped,
        "replays": per_replay,
        "contradictions": contradictions,
        "gained": gained_total,
        "ambiguous": ambiguous_total,
        "crossConfirmed": confirmed_total,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+")
    parser.add_argument("--binary", required=True)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument(
        "--min-shared",
        type=int,
        default=8,
        help="deployment keys two recordings must share to confirm one match",
    )
    parser.add_argument("--json", help="write the detailed audit here")
    args = parser.parse_args()

    replays = find_replays(args.roots)
    print(f"replays: {len(replays)}", file=sys.stderr)

    # Phase 1: cheap header bucket to bound the full parse.
    summaries = run_parser(args.binary, "--corpus-summary-json", replays)
    buckets = collections.defaultdict(list)
    for row in summaries:
        if row.get("status") != "parsed":
            continue
        buckets[(row.get("serverName"), row.get("dateTime"), row.get("mode"))].append(
            row["path"]
        )
    candidates = [p for group in buckets.values() if len(group) > 1 for p in group]
    print(
        f"candidate replays in shared header buckets: {len(candidates)}",
        file=sys.stderr,
    )

    # Phase 2: full parse and strong confirmation.
    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        records = list(pool.map(summarize, [(args.binary, p) for p in candidates]))
    errors = [r for r in records if r and "error" in r]
    records = [r for r in records if r and "error" not in r]

    groups, rejected = build_groups(records, args.min_shared)
    merged = [merge_group(g) for g in groups]
    # Duplicate removal can collapse a bucket to a single point of view, which
    # carries no cross-replay evidence.
    collapsed = [m for m in merged if m["members"] < 2]
    duplicates_dropped = sum(len(m["duplicatePovDropped"]) for m in merged)
    merged = [m for m in merged if m["members"] >= 2]

    report = {
        "summary": {
            "replaysScanned": len(replays),
            "candidateReplays": len(candidates),
            "parseErrors": len(errors),
            "confirmedGroups": len(merged),
            "groupsCollapsedToOnePov": len(collapsed),
            "rejectedPairs": len(rejected),
            "duplicatePovDropped": duplicates_dropped,
            "deployments": sum(r["deployments"] for m in merged for r in m["replays"]),
            "attributedAlone": sum(
                r["attributedAlone"] for m in merged for r in m["replays"]
            ),
            "attributedWithGroup": sum(
                r["attributedWithGroup"] for m in merged for r in m["replays"]
            ),
            "gained": sum(m["gained"] for m in merged),
            "ambiguous": sum(m["ambiguous"] for m in merged),
            "crossConfirmed": sum(m["crossConfirmed"] for m in merged),
            "contradictions": sum(len(m["contradictions"]) for m in merged),
        },
        "groups": merged,
        "rejectedPairs": rejected,
        "parseErrors": errors,
    }

    if args.json:
        pathlib.Path(args.json).write_text(json.dumps(report, indent=1))
    json.dump(report["summary"], sys.stdout, indent=1)
    print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
