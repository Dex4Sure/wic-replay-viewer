"""Compare native final-screen candidates with the saved historical viewer audit."""

import collections
import json
from pathlib import Path

root = Path(__file__).resolve().parents[2] / "local/generated"
baseline = {}
for line in (root / "departure-roster-corpus-2026-09-08.jsonl").open():
    row = json.loads(line)
    baseline[row["input"]["sha256"]] = row
counts = collections.Counter()
changes = []
fallbacks = []
for line in (root / "final-screen-candidate-corpus-2026-09-09.jsonl").open():
    row = json.loads(line)
    old = baseline[row["input"]["sha256"]]
    counts["contents"] += 1
    counts["paths"] += len(row["input"]["aliases"])
    assert row.get("error") == old.get("error"), row["input"]
    if "error" in row:
        counts["rejected"] += 1
        continue
    data = row["data"]
    candidate = data["candidate"]
    if candidate is None:
        counts["fallbacks"] += 1
        fallbacks.append(row["input"])
        continue
    counts["finalScreens"] += 1
    assert not data["errors"] and data["result"] is not None
    recorded = {p["slot"]: p for p in data["rows"]}
    assert len(candidate) == len({p["id"] for p in candidate})
    assert len(candidate) <= 16
    for p in candidate:
        record = recorded[p["id"]]
        state = record["state"]
        assert state["active"] and state["kind"] != 2
        assert p["name"] == state["name"]
        assert p["team"] == (0 if state["spectator"] else state["team"])
        assert p["score"] == p["scoreTotal"] == record["scoreTotal"]
        assert p["leftAtSeconds"] is None
        for key in p:
            if key.startswith("score") and key != "score":
                assert p[key] == record[key]
    for team in (1, 2, 3):
        assert sum(p["team"] == team for p in candidate) <= 8
    before = {p["id"]: p for p in old["data"]["current"]}
    after = {p["id"]: p for p in candidate}
    removed = [p for i, p in before.items() if i not in after]
    added = [p for i, p in after.items() if i not in before]
    differences = []
    for slot in before.keys() & after.keys():
        fields = {
            k: [before[slot].get(k), after[slot].get(k)]
            for k in ("name", "team", "score", "role")
            if before[slot].get(k) != after[slot].get(k)
        }
        if fields:
            differences.append({"id": slot, "fields": fields})
            counts.update(fields.keys())
    counts["removedRows"] += len(removed)
    counts["addedRows"] += len(added)
    if removed or added or differences:
        counts["changedContents"] += 1
        changes.append(
            {
                "input": row["input"],
                "removed": removed,
                "added": added,
                "differences": differences,
            }
        )
report = {"counts": dict(counts), "fallbacks": fallbacks, "changes": changes}
(root / "final-screen-comparison-2026-09-09.json").write_text(
    json.dumps(report, ensure_ascii=False, indent=2) + "\n"
)
print(json.dumps(report["counts"], indent=2))
