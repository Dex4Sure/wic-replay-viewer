"""Independent Python verification of changed and remaining overflow replays."""

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "parser"))
from wic_replay_parser import WICReplayParserV4  # noqa: E402


def main():
    findings = ROOT / "local" / "generated"
    audit = json.loads(
        (findings / "departure-roster-check-2026-09-08.json").read_text()
    )
    selected = {
        r["input"]["sha256"]: r["input"]
        for r in audit["changed"] + audit["unresolvedOverflow"]
    }
    native = {
        r["input"]["sha256"]: r["data"]["raw"]["players"]
        for r in map(
            json.loads, (findings / "departure-roster-corpus-2026-09-08.jsonl").open()
        )
        if r["input"]["sha256"] in selected
    }
    results = []
    for sha, source in selected.items():
        assert hashlib.sha256(Path(source["path"]).read_bytes()).hexdigest() == sha
        parsed = WICReplayParserV4(source["path"]).parse()
        expected = {p["id"]: p for p in native[sha]}
        issues = []
        for player in parsed.players:
            row = expected.pop(player.id)
            for py, rs in [
                ("name", "name"),
                ("team", "team"),
                ("score", "score"),
                ("role", "role"),
                ("left_at_seconds", "leftAtSeconds"),
            ]:
                if getattr(player, py) != row[rs]:
                    issues.append((player.id, py, getattr(player, py), row[rs]))
        assert not expected
        results.append({"input": source, "issues": issues})
        print(Path(source["path"]).name, issues, flush=True)
    (findings / "departure-roster-python-check-2026-09-08.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=2)
    )
    assert not any(r["issues"] for r in results)


if __name__ == "__main__":
    main()
