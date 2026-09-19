#!/usr/bin/env python3
"""Test whether support-projectile IDs join to player-owned projectile records.

Normal projectile records carry a firing unit that can be resolved through
``UnitCreate``. Tactical-aid projectile records instead carry a support type.
This audit tests the tempting hypothesis that both records share a projectile ID.
An overlap would be a serialized actor bridge; absence across the corpus rules out
that direct identifier path.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import multiprocessing
import os
import pathlib
from collections import Counter

from wic_bintag import decompress, fields, name_hash, walk

SUPPORT_PROJECTILES = {
    name_hash(name): name
    for name in (
        "ProjectileStraightSupportCreate",
        "ProjectileBallisticSupportCreate",
        "ProjectileHomingSupportCreate_Position",
        "ProjectileHomingSupportCreate_Unit",
    )
}
NORMAL_PROJECTILES = {
    name_hash(name): name
    for name in (
        "ProjectileStraightCreate",
        "ProjectileBallisticCreate",
        "ProjectileHomingTargetCreate",
        "ProjectileHomingUnitCreate",
    )
}


def analyse(path: str) -> dict:
    data = decompress(path)
    support: dict[int, list[tuple[str, float]]] = {}
    normal: dict[int, list[tuple[str, float]]] = {}
    counts = Counter()
    support_field_counts: dict[str, Counter] = {}
    support_trailing_bytes = Counter()

    for envelope in walk(data):
        if envelope.message in SUPPORT_PROJECTILES:
            body, trailing = fields(data, envelope)
            kind = SUPPORT_PROJECTILES[envelope.message]
            counts[f"support_{kind}"] += 1
            support_trailing_bytes[len(trailing)] += 1
            field_counts = support_field_counts.setdefault(kind, Counter())
            field_counts.update(field.hash for field in body)
            if body:
                support.setdefault(body[0].u32, []).append((kind, envelope.time))
            continue

        if envelope.message in NORMAL_PROJECTILES:
            body, _ = fields(data, envelope)
            kind = NORMAL_PROJECTILES[envelope.message]
            counts[f"normal_{kind}"] += 1
            if body:
                normal.setdefault(body[0].u32, []).append((kind, envelope.time))

    overlaps = sorted(support.keys() & normal.keys())
    return {
        "path": path,
        "counts": dict(counts),
        "supportIds": len(support),
        "normalIds": len(normal),
        "overlapIds": len(overlaps),
        "overlapSamples": [
            {
                "projectileId": projectile_id,
                "supportRecords": support[projectile_id],
                "normalRecords": normal[projectile_id],
            }
            for projectile_id in overlaps[:10]
        ],
        "supportFieldHashes": {
            kind: {f"0x{field_hash:08x}": count for field_hash, count in values.items()}
            for kind, values in support_field_counts.items()
        },
        "supportTrailingBytes": dict(support_trailing_bytes),
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
    field_shapes: dict[str, Counter] = {}
    trailing = Counter()
    samples = []
    for result in results:
        if "error" in result:
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        totals.update(result["counts"])
        totals["supportProjectileIds"] += result["supportIds"]
        totals["normalProjectileIds"] += result["normalIds"]
        totals["overlapProjectileIds"] += result["overlapIds"]
        trailing.update(result["supportTrailingBytes"])
        for kind, fields_by_hash in result["supportFieldHashes"].items():
            field_shapes.setdefault(kind, Counter()).update(fields_by_hash)
        samples.extend(result["overlapSamples"][: 20 - len(samples)])

    report = {
        "schemaVersion": 1,
        **dict(sorted(totals.items())),
        "supportProjectileTrailingByteLengths": dict(sorted(trailing.items())),
        "supportProjectileFieldHashes": {
            kind: dict(sorted(values.items()))
            for kind, values in sorted(field_shapes.items())
        },
        "overlapSamples": samples,
        "conclusion": (
            "sharedProjectileIdBridgeFound"
            if totals["overlapProjectileIds"]
            else "supportAndNormalProjectileIdNamespacesDoNotOverlapPerReplay"
        ),
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
