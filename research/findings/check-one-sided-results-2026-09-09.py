"""Compare the production roster policy with the two saved corpus baselines."""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2] / "local/generated"


def load(name):
    return {
        r["input"]["sha256"]: r
        for line in (ROOT / name).open()
        if (r := json.loads(line))
    }


def both(rows):
    teams = {p.get("team") for p in rows}
    return 3 in teams and bool(teams & {1, 2})


old = load("departure-roster-corpus-2026-09-08.jsonl")
final = load("final-screen-candidate-corpus-2026-09-09.jsonl")
new = load("one-sided-screen-corpus-2026-09-09.jsonl")
report = dict(
    distinct=len(new),
    accepted=0,
    rejected=0,
    restored=0,
    unexpectedDifferences=[],
    oversizedTeams=[],
    stillOneSided=[],
)
for sha, row in new.items():
    if "error" in row:
        report["rejected"] += 1
        assert "error" in old[sha] and "error" in final[sha]
        continue
    report["accepted"] += 1
    candidate = final[sha]["data"]["candidate"]
    historical = old[sha]["data"]["current"]
    restore = candidate is not None and not both(candidate) and both(historical)
    report["restored"] += restore
    expected = historical if candidate is None or restore else candidate
    actual = row["players"]
    # The native probe emits parser rows after summary score recovery. Normalize
    # those to DetailView's existing nullable-category and scoreBeforeLeave shape.
    if candidate is None or restore:
        for player in actual:
            evidence = next(
                (
                    e.get("scoreBeforeLeave")
                    for e in old[sha]["data"]["evidence"]
                    if e["playerId"] == player["id"]
                ),
                None,
            )
            if evidence:
                player["scoreBeforeLeave"] = evidence
                for key in list(player):
                    if key.startswith("score") and key not in (
                        "score",
                        "scoreBeforeLeave",
                    ):
                        player[key] = None
    if actual != expected:
        report["unexpectedDifferences"].append(sha)
    if not both(actual):
        report["stillOneSided"].append(row["input"])
    if any(sum(p.get("team") == team for p in actual) > 8 for team in (1, 2, 3)):
        report["oversizedTeams"].append(sha)
(ROOT / "one-sided-screen-comparison-2026-09-09.json").write_text(
    json.dumps(report, indent=2) + "\n"
)
print({k: len(v) if isinstance(v, list) else v for k, v in report.items()})
assert not report["unexpectedDifferences"]
assert not report["oversizedTeams"]
