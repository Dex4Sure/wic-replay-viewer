#!/usr/bin/env python3
"""Phase 10: evaluate a deterministic `buildingDamage` mechanical context.

wic_ds.exe splashes building damage onto current residents in
``0x00517f10``, which calls ``0x004ce830`` with a literal null killer object.
``0x004ce830`` therefore selects ``aKillerUnit = 0x200`` (EX_MAX_UNITS) and a
synthetic terminal direction from ``0x004cc280``. A resident killed that way is
serialized as sentinel killer + synthetic direction, and the building's own
attacker -- which ``0x004da180`` stores at building ``+0x10c`` -- is discarded.

This audit tests whether such a death can be deterministically joined to a
same-raw-tick ``BuildingDamaged`` health decrease on a building the victim was
bound to by ``CreateBuildingRelation``.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import struct
import sys
from collections import Counter, defaultdict

sys.path.insert(0, "scripts")

from wic_bintag import decompress, fields, name_hash, walk

NAMES = (
    "UnitDestroy",
    "BuildingCreate",
    "BuildingDamaged",
    "CreateBuildingRelation",
    "DestroyBuildingRelations",
    "DestroyBuildingRelations_Unit",
)
MSG = {n: name_hash(n) for n in NAMES}
HASH_TO_NAME = {v: k for k, v in MSG.items()}
SENTINEL = 512


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


def scan(path: str) -> dict:
    data = decompress(path)
    ticks: dict[int, list] = {}
    order: list[int] = []
    for env in walk(data):
        name = HASH_TO_NAME.get(env.message)
        if name is None:
            continue
        body, _ = fields(data, env)
        t = _bits(env.time)
        if t not in ticks:
            ticks[t] = []
            order.append(t)
        ticks[t].append((name, body))

    out = Counter()
    rel_types = Counter()
    health: dict[int, int] = {}
    resident: dict[int, set[int]] = defaultdict(set)

    for t in order:
        events = ticks[t]

        # Buildings damaged this tick, with a strict health decrease.
        damaged: set[int] = set()
        for name, b in events:
            if name == "BuildingDamaged" and len(b) >= 2:
                bid, hp = b[0].u32, b[1].u32
                if bid in health and hp < health[bid]:
                    damaged.add(bid)

        # Deaths are evaluated against residency as of the start of the tick.
        for name, b in events:
            if name != "UnitDestroy" or len(b) < 7:
                continue
            unit, killer = b[0].u32, b[1].u32
            bits = tuple(f.u32 for f in b[3:6])
            synth = bits in SYNTHETIC
            klass = ("sentinel_" if killer == SENTINEL else "attributed_") + (
                "synthetic" if synth else "directional"
            )
            out[klass] += 1
            homes = resident.get(unit) or set()
            if homes:
                out[klass + "_resident"] += 1
                hit = homes & damaged
                if hit:
                    out[klass + "_residentDamagedSameTick"] += 1
                    if len(hit) > 1:
                        out[klass + "_residentAmbiguousBuilding"] += 1
                else:
                    out[klass + "_residentNoDamage"] += 1
            elif damaged:
                out[klass + "_nonResidentTickHadDamage"] += 1

        # Apply this tick's state changes.
        for name, b in events:
            if name == "BuildingCreate" and len(b) >= 11:
                health[b[0].u32] = b[10].u32
            elif name == "BuildingDamaged" and len(b) >= 2:
                health[b[0].u32] = b[1].u32
            elif name == "CreateBuildingRelation" and len(b) >= 3:
                rel_types[b[0].u32] += 1
                resident[b[1].u32].add(b[2].u32)
            elif name == "DestroyBuildingRelations" and len(b) >= 3:
                resident[b[1].u32].discard(b[2].u32)
            elif name == "DestroyBuildingRelations_Unit" and len(b) >= 1:
                resident.pop(b[0].u32, None)

    res = dict(out)
    res["_relTypes"] = dict(rel_types)
    return res


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
    tot, rel, fails = Counter(), Counter(), []
    with concurrent.futures.ProcessPoolExecutor(max_workers=a.jobs) as pool:
        futs = {pool.submit(scan, p): p for p in paths}
        for f in concurrent.futures.as_completed(futs):
            try:
                r = f.result()
            except Exception as e:  # noqa: BLE001
                fails.append({"path": futs[f], "error": repr(e)})
                continue
            rel.update(r.pop("_relTypes", {}))
            tot.update(r)
    rep = {
        "paths": len(paths),
        "analysed": len(paths) - len(fails),
        "failures": fails,
        "relationTypes": dict(sorted(rel.items())),
        "counts": dict(sorted(tot.items())),
    }
    pathlib.Path(a.json).write_text(json.dumps(rep, indent=2))
    print(json.dumps(rep["relationTypes"], indent=2))
    print(json.dumps(rep["counts"], indent=2))
    print("failures:", len(fails))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
