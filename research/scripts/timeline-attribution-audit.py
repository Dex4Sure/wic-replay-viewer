#!/usr/bin/env python3
"""Extract player-attribution evidence from `.wicdemo` replay files.

The research slice inventories tactical-aid purchases and match-visible effects,
emits binary-validated marker player slots, and generates conservative
purchase-to-spawn candidates. It does not change canonical parser output or claim
that an unvalidated candidate is fact.

Usage:

    research/scripts/timeline-attribution-audit.py local/replays/main/4600.wicdemo
    research/scripts/timeline-attribution-audit.py local/replays/main \
      --json local/generated/timeline-attribution-audit.json
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import multiprocessing
import os
import pathlib

from timeline_attribution import (
    POSITION_EPSILON_DEFAULT,
    analyse_replay_safe,
    summarise,
)


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def nonnegative_float(value: str) -> float:
    parsed = float(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be nonnegative")
    return parsed


def replay_paths(roots: list[str], limit: int) -> list[pathlib.Path]:
    paths = []
    seen: set[pathlib.Path] = set()
    for root in roots:
        node = pathlib.Path(root)
        candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            paths.append(candidate)
    return paths[:limit] if limit else paths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument("--json", help="write summary and per-replay evidence here")
    parser.add_argument("--limit", type=int, default=0, help="stop after N replays")
    parser.add_argument(
        "--jobs",
        type=positive_int,
        default=min(4, os.cpu_count() or 1),
        help="parallel replay workers (default: auto, capped at 4)",
    )
    parser.add_argument(
        "--position-epsilon",
        type=nonnegative_float,
        default=POSITION_EPSILON_DEFAULT,
        help="maximum world-position distance for a candidate (default: exact)",
    )
    parser.add_argument(
        "--include-event-rows",
        action="store_true",
        help="include verbose raw event rows in --json output",
    )
    args = parser.parse_args()

    paths = replay_paths(args.roots, args.limit)
    if not paths:
        parser.error("no replay files found")
    work = [
        (str(path), args.position_epsilon, args.include_event_rows) for path in paths
    ]
    if args.jobs == 1:
        results = [analyse_replay_safe(item) for item in work]
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
        target.write_text(
            json.dumps({"summary": summary, "replays": results}, indent=2) + "\n"
        )
    return 1 if summary.get("replaysFailed", 0) else 0


if __name__ == "__main__":
    raise SystemExit(main())
