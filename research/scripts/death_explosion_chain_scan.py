#!/usr/bin/env python3
"""Phase 9: test the death-explosion chain hypothesis for sentinel deaths.

wic_ds.exe queues a blast from ``EXG_Death`` (0x00513450, reading myDamage /
myBlastRadius) and applies it one tick later in ``EXG_BlastContainer``
(0x004f2b50) with a hard-coded ``aKillerUnit = EX_MAX_UNITS`` and a real
position. A death caused that way must therefore be preceded, within a short
window, by the UnitDestroy of the entity that exploded.
"""

from __future__ import annotations

import argparse
import bisect
import concurrent.futures
import json
import pathlib
import struct
import sys
from collections import Counter

sys.path.insert(0, "scripts")

from wic_bintag import decompress, fields, name_hash, walk

MSG = {n: name_hash(n) for n in ("UnitDestroy",)}
HASH_TO_NAME = {v: k for k, v in MSG.items()}
SENTINEL = 512
WINDOWS = (0.0, 0.1, 0.25, 0.5, 1.0, 2.0)


def _bits(v: float) -> int:
    return struct.unpack("<I", struct.pack("<f", v))[0]


def synthetic_direction_bits() -> frozenset[tuple[int, int, int]]:
    out = set()
    for x in range(-50, 50):
        for z in range(-50, 50):
            ln = struct.unpack("<f", struct.pack("<f", (x * x + z * z) ** 0.5))[0]
            if ln > 0.0:
                inv = struct.unpack("<f", struct.pack("<f", 1.0 / ln))[0]
                dx = struct.unpack("<f", struct.pack("<f", inv * float(x)))[0]
                dz = struct.unpack("<f", struct.pack("<f", inv * float(z)))[0]
            else:
                dx = dz = 0.0
            out.add((_bits(dx), _bits(0.0), _bits(dz)))
    return frozenset(out)


SYNTHETIC = synthetic_direction_bits()
ZERO = (_bits(0.0), _bits(0.0), _bits(0.0))


def scan(path: str) -> dict:
    data = decompress(path)
    deaths = []
    for env in walk(data):
        if HASH_TO_NAME.get(env.message) != "UnitDestroy":
            continue
        body, _ = fields(data, env)
        if len(body) < 7:
            continue
        bits = tuple(f.u32 for f in body[3:6])
        deaths.append((float(env.time), body[1].u32, bits))

    deaths.sort(key=lambda d: d[0])
    times = [d[0] for d in deaths]

    out = Counter()
    for i, (t, killer, bits) in enumerate(deaths):
        if bits == ZERO:
            klass = "zeroDirection"
        elif bits in SYNTHETIC:
            klass = "synthetic"
        else:
            klass = "directional"
        klass = ("sentinel_" if killer == SENTINEL else "attributed_") + klass
        out[klass] += 1

        # nearest strictly-earlier death by another unit
        j = bisect.bisect_left(times, t)
        prior = None
        for k in range(j - 1, -1, -1):
            if times[k] < t:
                prior = t - times[k]
                break
        same = (
            sum(
                1
                for k in range(max(0, j - 40), min(len(times), j + 40))
                if times[k] == t
            )
            - 1
        )
        if same > 0:
            out[klass + "_sameTick"] += 1
        for w in WINDOWS:
            if prior is not None and prior <= w + 1e-9:
                out[f"{klass}_prior<={w}"] += 1
        if prior is None:
            out[klass + "_noPrior"] += 1
    return dict(out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("roots", nargs="+")
    ap.add_argument("--jobs", type=int, default=8)
    ap.add_argument("--json", required=True)
    a = ap.parse_args()
    paths = sorted(
        {
            str(p.resolve())
            for r in a.roots
            for p in (
                [pathlib.Path(r)]
                if pathlib.Path(r).is_file()
                else pathlib.Path(r).rglob("*.wicdemo")
            )
        }
    )
    tot, fails = Counter(), []
    with concurrent.futures.ProcessPoolExecutor(max_workers=a.jobs) as pool:
        futs = {pool.submit(scan, p): p for p in paths}
        for f in concurrent.futures.as_completed(futs):
            try:
                tot.update(f.result())
            except Exception as e:  # noqa: BLE001
                fails.append({"path": futs[f], "error": repr(e)})
    rep = {
        "paths": len(paths),
        "analysed": len(paths) - len(fails),
        "failures": fails,
        "counts": dict(sorted(tot.items())),
    }
    pathlib.Path(a.json).write_text(json.dumps(rep, indent=2))
    print(json.dumps(rep["counts"], indent=2))
    print("failures:", len(fails))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
