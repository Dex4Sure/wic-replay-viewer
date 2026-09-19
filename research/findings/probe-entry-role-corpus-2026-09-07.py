"""Historical role-extension probe; requires the v40 audit containing role evidence."""

import argparse
import collections
import json
import struct
import sys
import zlib
import multiprocessing
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor

sys.path.insert(0, "parser")
import wic_replay_parser as m  # noqa: E402

cli = argparse.ArgumentParser(description=__doc__)
cli.add_argument(
    "audit",
    type=Path,
    help="Historical last-role-audit.json with lastRecordedRole fields",
)
args = cli.parse_args()
ROLE = struct.pack("<I", zlib.adler32(b"role"))
known = {
    0x0E0002CB: "infantry",
    0x1ABF03DF: "support",
    0x10E00313: "armor",
    0x0AE8026E: "air",
}


def run(r):
    p = m.WICReplayParserV4(r["path"])
    d = p.get_full_decompressed_data()
    targets = {
        e["playerId"]: e
        for e in r["evidence"]
        if e["scoreBeforeLeave"] and not e["scoreBeforeLeave"]["lastRecordedRole"]
    }
    rows = []
    for pos in p._find_all(d, m._HASH_PLAYER_ENTERS_GAME):
        if not (envelope := p._read_envelope(d, pos - m._ENVELOPE_MESSAGE_OFFSET)):
            continue
        end, time = envelope
        slot = p._read_bintag_u32(d, pos + 4)
        if slot not in targets:
            continue
        row = {
            "slot": slot,
            "time": time,
            "offset": pos,
            "beforeLeave": time < targets[slot]["scoreBeforeLeave"]["leftAtSeconds"],
            "values": [],
        }
        for rel in p._find_all(d[pos:end], ROLE):
            val = p._read_bintag_u32(d, pos + rel)
            row["values"].append(
                {"offset": pos + rel, "value": val, "known": known.get(val)}
            )
        rows.append(row)
    # Search complete data, not only the known timeline chain.
    role_records = []
    for pos in p._find_all(d, m._HASH_AROLE_ID):
        slot = (
            p._read_bintag_u32(d, pos - 17)
            if d[pos - 17 : pos - 13] in [m._HASH_APOS, m._HASH_ASLOT]
            else None
        )
        if slot in targets:
            role_records.append(
                {
                    "offset": pos,
                    "slot": slot,
                    "value": p._read_bintag_u32(d, pos),
                    "known": known.get(p._read_bintag_u32(d, pos)),
                }
            )
    return {
        "path": r["path"],
        "sha256": r["sha256"],
        "targets": list(targets),
        "entries": rows,
        "roleRecords": role_records,
    }


if __name__ == "__main__":
    data = json.load(args.audit.open())
    unique = {r["sha256"]: r for r in data["changes"]}
    selected = [
        r
        for r in unique.values()
        if any(
            e["scoreBeforeLeave"] and not e["scoreBeforeLeave"]["lastRecordedRole"]
            for e in r["evidence"]
        )
    ]
    print("Selected", len(selected), "unique contents", flush=True)
    with ProcessPoolExecutor(
        max_workers=3, mp_context=multiprocessing.get_context("fork")
    ) as pool:
        results = []
        for i, r in enumerate(pool.map(run, selected), 1):
            results.append(r)
            if i % 30 == 0:
                print("Scanned", i, flush=True)
    out = {
        "files": len(results),
        "players": sum(len(r["targets"]) for r in results),
        "entryValueCounts": dict(
            collections.Counter(
                str(v["value"])
                for r in results
                for e in r["entries"]
                for v in e["values"]
            )
        ),
        "knownEntryValues": [
            dict(path=r["path"], entry=e)
            for r in results
            for e in r["entries"]
            if any(v["known"] for v in e["values"])
        ],
        "knownRoleRecords": [
            dict(path=r["path"], roles=r["roleRecords"])
            for r in results
            if any(e["known"] for e in r["roleRecords"])
        ],
        "replays": results,
    }
    Path("local/generated/missing-role-entry-corpus-probe-2026-09-07.json").write_text(
        json.dumps(out, ensure_ascii=False, indent=2)
    )
    print(
        json.dumps(
            {k: v for k, v in out.items() if k != "replays"}, ensure_ascii=False
        ),
        flush=True,
    )
