#!/usr/bin/env python3
"""Audit tactical-aid usage records across a `.wicdemo` corpus.

Replaces the throwaway `/tmp` probe with a reproducible run. It walks the Event
envelope chain (see `research/scripts/wic_bintag.py`) and classifies every
`SupportThingUsed` message as either

* a **catalogue** entry -- emitted in a burst right after `StartGameTime`, with a
  zero world position, enumerating the support things the match has available; or
* a **use** -- a real activation carrying a world position.

For each use it records the immediately preceding envelope so the
`ChangeHonors`/`SupportThingUsed` pairing can be measured rather than assumed,
and it cross-checks `ShowPlayerGiveTANotification` against `ChangeHonors` to test
whether the honors ledger is point-of-view local.

Usage:

    research/scripts/ta-usage-audit.py local/replays/main --json local/generated/ta-usage-audit.json
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import hashlib
import json
import multiprocessing
import os
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from wic_bintag import (  # noqa: E402
    NameTable,
    decompress,
    fields,
    name_hash,
    recorder_slot,
    walk,
)

MSG_CHANGE_HONORS = name_hash("ChangeHonors")
MSG_SUPPORT_THING_USED = name_hash("SupportThingUsed")
MSG_START_GAME_TIME = name_hash("StartGameTime")
MSG_GIVE_TA = name_hash("ShowPlayerGiveTANotification")

DEFAULT_BINARIES = (
    "local/binaries/game/wic.exe",
    "local/binaries/game/wic_ds.exe",
    "local/binaries/game/wic_online.exe",
)


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def analyse(path: pathlib.Path) -> dict:
    data = decompress(path)
    envelopes = list(walk(data))
    result: dict = {
        "path": str(path),
        "sha256": sha256(path),
        "decompressed_bytes": len(data),
        "envelopes": len(envelopes),
        "recorder_slot": recorder_slot(data),
        "catalogue": [],
        "uses": [],
        "gifts": [],
        "change_honors_total": 0,
        "change_honors_negative": 0,
    }
    if not envelopes:
        result["error"] = "no Event chain"
        return result

    # Chain coverage tells us whether envelope walking stayed in sync.
    covered = sum(e.total for e in envelopes)
    span = envelopes[-1].body_end - envelopes[0].offset
    result["chain_coverage"] = round(covered / span, 6) if span else 0.0

    for index, envelope in enumerate(envelopes):
        if envelope.message == MSG_CHANGE_HONORS:
            body, _ = fields(data, envelope)
            result["change_honors_total"] += 1
            if body and body[0].f32 < 0:
                result["change_honors_negative"] += 1
            continue

        if envelope.message == MSG_GIVE_TA:
            body, _ = fields(data, envelope)
            if len(body) < 3:
                continue
            nearby = []
            for probe in range(max(0, index - 4), min(len(envelopes), index + 5)):
                if envelopes[probe].message != MSG_CHANGE_HONORS:
                    continue
                delta, _ = fields(data, envelopes[probe])
                if delta:
                    nearby.append([probe - index, round(delta[0].f32, 4)])
            result["gifts"].append(
                {
                    "time": round(envelope.time, 3),
                    "from_slot": body[0].u32,
                    "to_slot": body[1].u32,
                    "amount": body[2].u32,
                    "nearby_change_honors": nearby,
                }
            )
            continue

        if envelope.message != MSG_SUPPORT_THING_USED:
            continue

        body, _ = fields(data, envelope)
        if not body:
            continue
        support_id = body[0].u32
        position = [round(f.f32, 2) for f in body[1:4]] if len(body) >= 4 else []
        previous = envelopes[index - 1] if index else None
        previous_message = previous.message if previous else 0

        if position == [0.0, 0.0, 0.0]:
            result["catalogue"].append(support_id)
            # Cross-check: a catalogue entry must never be priced.
            if previous_message == MSG_CHANGE_HONORS:
                result["catalogue_priced"] = result.get("catalogue_priced", 0) + 1
            continue

        cost = None
        if previous is not None and previous_message == MSG_CHANGE_HONORS:
            delta, _ = fields(data, previous)
            if delta:
                cost = round(delta[0].f32, 4)
        result["uses"].append(
            {
                "time": round(envelope.time, 3),
                "offset": envelope.offset,
                "support_id": f"{support_id:08x}",
                "position": position,
                "honors_delta": cost,
                "previous_message": f"{previous_message:08x}",
            }
        )

    return result


def analyse_safe(path: pathlib.Path) -> dict:
    try:
        return analyse(path)
    except Exception as exc:  # noqa: BLE001 - corpus contains corrupt files
        return {"path": str(path), "error": f"{type(exc).__name__}: {exc}"}


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def summarise(results: list[dict], table: NameTable) -> dict:
    per_id_costs: dict[str, collections.Counter] = collections.defaultdict(
        collections.Counter
    )
    catalogue_union: collections.Counter = collections.Counter()
    catalogue_sizes: collections.Counter = collections.Counter()
    uses_with_cost = uses_total = 0
    catalogue_priced = 0
    preceding_messages = collections.Counter()
    gift_roles = collections.Counter()

    for entry in results:
        if entry.get("error"):
            continue
        for support_id in entry["catalogue"]:
            catalogue_union[f"{support_id:08x}"] += 1
        catalogue_sizes[len(entry["catalogue"])] += 1
        catalogue_priced += entry.get("catalogue_priced", 0)
        for use in entry["uses"]:
            uses_total += 1
            preceding_messages[use["previous_message"]] += 1
            if use["honors_delta"] is not None:
                uses_with_cost += 1
                per_id_costs[use["support_id"]][abs(use["honors_delta"])] += 1
        recorder = entry.get("recorder_slot")
        for gift in entry["gifts"]:
            if recorder is None:
                role = "unknown-recorder"
            elif gift["from_slot"] == recorder:
                role = "recorder-is-sender"
            elif gift["to_slot"] == recorder:
                role = "recorder-is-receiver"
            else:
                role = "between-others"
            matched = any(
                abs(abs(delta) - gift["amount"]) < 0.001
                for _, delta in gift["nearby_change_honors"]
            )
            gift_roles[(role, matched)] += 1

    cost_table = {}
    for support_id, costs in sorted(per_id_costs.items()):
        names = table.candidates(int(support_id, 16))
        cost_table[support_id] = {
            "uses": sum(costs.values()),
            "costs": {str(k): v for k, v in sorted(costs.items())},
            "distinct_costs": len(costs),
            "binary_names": names,
        }

    return {
        "replays_analysed": sum(1 for r in results if not r.get("error")),
        "replays_failed": sum(1 for r in results if r.get("error")),
        "uses_total": uses_total,
        "uses_with_change_honors": uses_with_cost,
        "catalogue_entries_priced": catalogue_priced,
        "use_preceding_message_census": dict(preceding_messages.most_common(10)),
        "catalogue_union_size": len(catalogue_union),
        "catalogue_size_distribution": dict(sorted(catalogue_sizes.items())),
        "gift_attribution": {
            f"{role}|matched={m}": c for (role, m), c in sorted(gift_roles.items())
        },
        "support_costs": cost_table,
        "catalogue_ids": {
            support_id: table.candidates(int(support_id, 16))
            for support_id, _ in catalogue_union.most_common()
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument("--json", help="write the full result set here")
    parser.add_argument("--limit", type=int, default=0, help="stop after N replays")
    parser.add_argument(
        "--jobs",
        type=positive_int,
        default=min(4, os.cpu_count() or 1),
        help="parallel replay workers (default: auto, capped at 4)",
    )
    parser.add_argument(
        "--binary",
        action="append",
        default=None,
        help="binary to mine for identifier strings (repeatable)",
    )
    args = parser.parse_args()

    table = NameTable(args.binary or DEFAULT_BINARIES)

    paths: list[pathlib.Path] = []
    seen: set[pathlib.Path] = set()
    for root in args.roots:
        node = pathlib.Path(root)
        candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            paths.append(candidate)
    if args.limit:
        paths = paths[: args.limit]

    if args.jobs == 1:
        result_iterator = map(analyse_safe, paths)
        results = []
        for index, result in enumerate(result_iterator, 1):
            results.append(result)
            if index % 100 == 0:
                print(f"  ... {index}/{len(paths)}", file=sys.stderr)
    else:
        results = []
        start_method = (
            "fork" if "fork" in multiprocessing.get_all_start_methods() else "spawn"
        )
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=args.jobs,
            mp_context=multiprocessing.get_context(start_method),
        ) as executor:
            for index, result in enumerate(executor.map(analyse_safe, paths), 1):
                results.append(result)
                if index % 100 == 0:
                    print(f"  ... {index}/{len(paths)}", file=sys.stderr)

    summary = summarise(results, table)
    print(json.dumps(summary, indent=2))

    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            json.dumps({"summary": summary, "replays": results}, indent=2) + "\n"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
