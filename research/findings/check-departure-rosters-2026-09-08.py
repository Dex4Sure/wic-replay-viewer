"""Compare departure markers and overflow-only presentation with the v43 baseline."""

import collections
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "local/generated"


def without_marker(player):
    return {k: v for k, v in player.items() if k != "leftAtSeconds"}


def main():
    baseline = {
        r["input"]["sha256"]: r
        for r in map(
            json.loads, (ROOT / "targeted-roster-corpus-2026-09-08.jsonl").open()
        )
    }
    rows = list(
        map(json.loads, (ROOT / "departure-roster-corpus-2026-09-08.jsonl").open())
    )
    assert len(rows) == len(baseline) == 2499
    changed = []
    unresolved = []
    markers = 0
    for r in rows:
        b = baseline[r["input"]["sha256"]]
        assert r.get("error") == b.get("error"), r["input"]
        if "data" not in r:
            continue
        new, old = r["data"], b["data"]
        normalized = dict(new["raw"])
        normalized["players"] = [without_marker(p) for p in new["raw"]["players"]]
        assert normalized == old["raw"], r["input"]
        markers += sum(p["leftAtSeconds"] is not None for p in new["raw"]["players"])
        for key in new.keys() - {"raw", "current"}:
            assert new[key] == old[key], (r["input"], key)
        previous = {p["id"]: p for p in old["current"]}
        current = {p["id"]: p for p in new["current"]}
        assert current.keys() <= previous.keys()
        for slot, player in current.items():
            assert without_marker(player) == previous[slot], (r["input"], slot)
        removed = previous.keys() - current.keys()
        before_counts = collections.Counter(
            p["team"] for p in previous.values() if p["team"] in (1, 2, 3)
        )
        after_counts = collections.Counter(
            p["team"] for p in current.values() if p["team"] in (1, 2, 3)
        )
        marked = {p["id"]: p for p in new["raw"]["players"]}
        for slot in removed:
            assert before_counts[previous[slot]["team"]] > 8
            assert marked[slot]["leftAtSeconds"] is not None
        if removed:
            changed.append(
                {
                    "input": r["input"],
                    "before": dict(before_counts),
                    "after": dict(after_counts),
                    "removed": [marked[s] for s in sorted(removed)],
                }
            )
        if any(count > 8 for count in after_counts.values()):
            unresolved.append({"input": r["input"], "counts": dict(after_counts)})
    result = {
        "paths": sum(len(r["input"]["aliases"]) for r in rows),
        "distinct": len(rows),
        "markedRows": markers,
        "changed": changed,
        "unresolvedOverflow": unresolved,
    }
    (ROOT / "departure-roster-check-2026-09-08.json").write_text(
        json.dumps(result, ensure_ascii=False, indent=2)
    )
    print(
        "Markers:",
        markers,
        "Changed:",
        len(changed),
        "Copies:",
        sum(len(r["input"]["aliases"]) for r in changed),
        "Remaining overflow:",
        len(unresolved),
    )
    for r in changed:
        print(
            Path(r["input"]["path"]).name,
            r["before"],
            "->",
            r["after"],
            [(p["id"], p["name"], p["leftAtSeconds"]) for p in r["removed"]],
        )


if __name__ == "__main__":
    main()
