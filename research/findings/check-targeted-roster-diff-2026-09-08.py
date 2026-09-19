"""Compare targeted parser output against the immutable event-roster baseline."""

import collections
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2] / "local/generated"
old = {
    r["input"]["sha256"]: r
    for r in map(json.loads, (root / "event-roster-extract-2026-09-08.jsonl").open())
}
cases = {
    r["input"]["sha256"]: r
    for r in json.loads((root / "event-roster-comparison-2026-09-08.json").read_text())[
        "cases"
    ]
}
counts = collections.Counter()
changes = []
unresolved = []
errors = []
for row in map(json.loads, (root / "targeted-roster-corpus-2026-09-08.jsonl").open()):
    sha = row["input"]["sha256"]
    before = old[sha]
    if row.get("error") != before.get("error"):
        errors.append(row["input"])
    if "data" not in row:
        continue
    a, b = row["data"], before["data"]
    diffs = {}
    for mode in ("raw", "current"):
        ar = a[mode]["players"] if mode == "raw" else a[mode]
        br = b[mode]["players"] if mode == "raw" else b[mode]
        ap, bp = {p["id"]: p for p in ar}, {p["id"]: p for p in br}
        for slot in ap.keys() | bp.keys():
            p, q = ap.get(slot, {}), bp.get(slot, {})
            delta = {
                k: [q.get(k), p.get(k)]
                for k in p.keys() | q.keys()
                if p.get(k) != q.get(k)
            }
            if delta:
                diffs.setdefault(mode, []).append(
                    {"slot": slot, "name": p.get("name"), "changes": delta}
                )
                counts.update(f"{mode}.{k}" for k in delta)
    metadata = {
        k: [b["raw"].get(k), a["raw"].get(k)]
        for k in a["raw"]
        if k != "players" and a["raw"].get(k) != b["raw"].get(k)
    }
    if diffs or metadata:
        changes.append({"input": row["input"], "players": diffs, "metadata": metadata})
    ap = {p["id"]: p for p in a["current"]}
    for c in cases.get(sha, {}).get("candidates", []):
        key = "name" if c["kind"] == "identity-review" else "team"
        if ap[c["slot"]][key] != c.get("proposedName", 0):
            unresolved.append(
                {"input": row["input"], "slot": c["slot"], "kind": c["kind"]}
            )
    for c in cases.get(sha, {}).get("scoreDifferences", []):
        if ap[int(c["slot"])]["score"] != c["lastLive"][2]:
            unresolved.append({"input": row["input"], "score": c})
result = {
    "counts": dict(counts),
    "acceptanceChanges": errors,
    "unresolved": unresolved,
    "changes": changes,
}
(root / "targeted-roster-diff-2026-09-08.json").write_text(
    json.dumps(result, ensure_ascii=False, indent=2)
)
print(
    json.dumps(
        {
            "counts": dict(counts),
            "distinctChanged": len(changes),
            "copiesChanged": sum(len(c["input"]["aliases"]) for c in changes),
            "acceptanceChanges": len(errors),
            "unresolved": len(unresolved),
        },
        indent=2,
    )
)
for c in unresolved:
    print("UNRESOLVED", Path(c["input"]["path"]).name, c.get("slot"))
for c in changes:
    for p in c["players"].get("current", []):
        if set(p["changes"]) <= {"score"}:
            continue
        print(
            Path(c["input"]["path"]).name,
            p["slot"],
            p["name"],
            {k: v for k, v in p["changes"].items() if not k.startswith("score")},
            p["changes"].get("score"),
        )
