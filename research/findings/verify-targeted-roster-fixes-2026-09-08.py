"""Verify every changed replay with the independent Python reference parser."""

import concurrent.futures
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "parser"))
from wic_replay_parser import WICReplayParserV4  # noqa: E402


def verify(item):
    change, native = item
    source = change["input"]
    assert (
        hashlib.sha256(Path(source["path"]).read_bytes()).hexdigest()
        == source["sha256"]
    )
    parser = WICReplayParserV4(source["path"])
    replay = parser.parse()
    python_rows = {p.id: p for p in replay.players}
    native_rows = {p["id"]: p for p in native["raw"]["players"]}
    issues = []
    # Compare all result names/teams/scores/roles, not only the changed row.
    for slot in python_rows.keys() | native_rows.keys():
        py, rs = python_rows.get(slot), native_rows.get(slot)
        for field in ("name", "team", "score", "role"):
            left = getattr(py, field, None)
            right = rs.get(field) if rs else None
            if left != right:
                issues.append(
                    {"slot": slot, "field": field, "python": left, "rust": right}
                )
    return {"input": source, "issues": issues}


def main():
    root = ROOT / "local" / "generated"
    changes = json.loads((root / "targeted-roster-diff-2026-09-08.json").read_text())[
        "changes"
    ]
    rows = {
        r["input"]["sha256"]: r["data"]
        for r in map(
            json.loads, (root / "targeted-roster-corpus-2026-09-08.jsonl").open()
        )
        if "data" in r
    }
    with concurrent.futures.ProcessPoolExecutor(max_workers=4) as pool:
        results = []
        for i, result in enumerate(
            pool.map(verify, [(c, rows[c["input"]["sha256"]]) for c in changes]), 1
        ):
            results.append(result)
            if i % 20 == 0:
                print(f"Checked {i}/{len(changes)}", flush=True)
    (root / "targeted-roster-python-parity-2026-09-08.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=2)
    )
    print(
        "Checked",
        len(results),
        "replays;",
        sum(len(r["issues"]) for r in results),
        "differences",
        flush=True,
    )


if __name__ == "__main__":
    main()
