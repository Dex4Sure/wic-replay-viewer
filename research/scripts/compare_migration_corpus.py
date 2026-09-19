"""Compare saved native roster-probe JSONL outputs without altering either input."""

import argparse
import json
from pathlib import Path


def load(path):
    result = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        identity = row["input"]["sha256"]
        if identity in result:
            raise ValueError(f"Duplicate content identity: {identity}")
        result[identity] = row
    return result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("baseline", type=Path)
    cli.add_argument("candidate", type=Path)
    args = cli.parse_args()
    baseline, candidate = load(args.baseline), load(args.candidate)
    changed = [
        key
        for key in baseline.keys() | candidate.keys()
        if baseline.get(key) != candidate.get(key)
    ]
    print(
        json.dumps(
            {
                "distinct": len(candidate),
                "paths": sum(
                    len(row["input"]["aliases"]) for row in candidate.values()
                ),
                "accepted": sum("players" in row for row in candidate.values()),
                "rejected": sum("error" in row for row in candidate.values()),
                "differences": sorted(changed),
            },
            indent=2,
        )
    )
    raise SystemExit(bool(changed))


if __name__ == "__main__":
    main()
