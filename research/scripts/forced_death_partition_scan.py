#!/usr/bin/env python3
"""Phase 8: partition synthetic-terminal-direction unit destructions.

Every ``UnitDestroy`` whose serialized hit direction lies in the exact finite
``wic_ds.exe:0x004cc280`` set was produced by the forced-death helper
``0x004ce880`` or the inherited-death helper ``0x004ce8a0``; no other caller of
the fatal routine ``0x004ccef0`` supplies that direction. This scan measures how
those deaths distribute across replay-visible same-raw-tick companions.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import pathlib
import struct
import sys
from collections import Counter

sys.path.insert(0, "scripts")

from wic_bintag import decompress, fields, name_hash, walk

MSG = {
    n: name_hash(n)
    for n in (
        "UnitDestroy",
        "UnitCreate",
        "UnitRemove",
        "BuildingDamaged",
        "PropDamaged",
        "RepairablePropDamaged",
        "BlowerBlew",
        "BlinkUnit",
        "PlayerLeavesGame",
        "PlayerJoinedTeam",
        "PlayerSetRole",
        "SpawnerSpawningDone",
        "DeployableDamaged",
    )
}
HASH_TO_NAME = {v: k for k, v in MSG.items()}


def _f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def synthetic_direction_bits() -> frozenset[tuple[int, int, int]]:
    out = set()
    for x in range(-50, 50):
        for z in range(-50, 50):
            length = struct.unpack("<f", struct.pack("<f", (x * x + z * z) ** 0.5))[0]
            if length > 0.0:
                inv = struct.unpack("<f", struct.pack("<f", 1.0 / length))[0]
                dx = struct.unpack("<f", struct.pack("<f", inv * float(x)))[0]
                dz = struct.unpack("<f", struct.pack("<f", inv * float(z)))[0]
            else:
                dx = dz = 0.0
            out.add((_f32_bits(dx), _f32_bits(0.0), _f32_bits(dz)))
    return frozenset(out)


SYNTHETIC = synthetic_direction_bits()
SENTINEL = 512


def scan(path: str) -> dict:
    data = decompress(path)
    events = []
    for env in walk(data):
        name = HASH_TO_NAME.get(env.message)
        if name is None:
            continue
        body, _ = fields(data, env)
        events.append((_f32_bits(env.time), name, body))

    by_tick: dict[int, list[tuple[str, list]]] = {}
    for tick, name, body in events:
        by_tick.setdefault(tick, []).append((name, body))

    result = Counter()
    prop_states = Counter()
    for tick, name, body in events:
        if name == "BlowerBlew":
            result["blowerBlew"] += 1
        if name in ("PropDamaged", "RepairablePropDamaged") and len(body) >= 2:
            prop_states[f"{name}:{body[1].u32}"] += 1
        if name != "UnitDestroy" or len(body) < 7:
            continue
        unit = body[0].u32
        killer = body[1].u32
        bits = tuple(f.u32 for f in body[3:6])
        synthetic = bits in SYNTHETIC
        sentinel = killer == SENTINEL
        klass = ("sentinel" if sentinel else "known") + (
            "Synthetic" if synthetic else "Ordinary"
        )
        result[klass] += 1
        if not synthetic:
            continue

        same = by_tick[tick]
        building3 = any(
            n == "BuildingDamaged" and len(b) >= 3 and b[2].u32 == 3 for n, b in same
        )
        destroys = sum(1 for n, _ in same if n == "UnitDestroy")
        spawn_kill = any(
            n == "UnitCreate" and len(b) >= 4 and b[3].u32 == unit for n, b in same
        )
        prop = any(n in ("PropDamaged", "RepairablePropDamaged") for n, b in same)
        prefix = klass
        if building3:
            result[prefix + "_buildingState3"] += 1
        if destroys > 1:
            result[prefix + "_multiDestroy"] += 1
        if spawn_kill:
            result[prefix + "_spawnTickCreate"] += 1
        if prop:
            result[prefix + "_propDamaged"] += 1
        if not (building3 or destroys > 1 or spawn_kill or prop):
            result[prefix + "_noCompanion"] += 1

    out = dict(result)
    out["_propStates"] = dict(prop_states)
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("roots", nargs="+")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--json", required=True)
    args = parser.parse_args()

    paths = sorted(
        {
            str(p.resolve())
            for root in args.roots
            for p in (
                [pathlib.Path(root)]
                if pathlib.Path(root).is_file()
                else pathlib.Path(root).rglob("*.wicdemo")
            )
        }
    )
    totals = Counter()
    prop_states = Counter()
    failures = []
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(scan, p): p for p in paths}
        for future in concurrent.futures.as_completed(futures):
            path = futures[future]
            try:
                res = future.result()
            except Exception as error:  # noqa: BLE001
                failures.append({"path": path, "error": repr(error)})
                continue
            prop_states.update(res.pop("_propStates", {}))
            totals.update(res)
    report = {
        "paths": len(paths),
        "analysed": len(paths) - len(failures),
        "failures": failures,
        "syntheticDirectionPossibilities": len(SYNTHETIC),
        "counts": dict(sorted(totals.items())),
        "propDamagedStates": dict(sorted(prop_states.items())),
    }
    pathlib.Path(args.json).write_text(json.dumps(report, indent=2))
    print(json.dumps(report["counts"], indent=2))
    print(json.dumps(report["propDamagedStates"], indent=2))
    print("failures:", len(failures))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
