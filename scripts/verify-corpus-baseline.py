#!/usr/bin/env python3
"""Compare path-free aggregate parser results with the reviewed corpus baseline."""

import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path


SUMMED_FIELDS = (
    "tacticalAidUses",
    "tacticalAidMarkers",
    "tacticalAidDeployments",
    "unitDestructions",
    "spectatorViewChanges",
)


def aggregate(rows: list[dict]) -> dict:
    normalized = [
        {key: value for key, value in row.items() if key != "path"} for row in rows
    ]
    accepted = [row for row in rows if not row.get("error")]
    schemas = Counter(str(row.get("schemaVersion")) for row in accepted)
    return {
        "totalFiles": len(rows),
        "accepted": len(accepted),
        "rejected": len(rows) - len(accepted),
        "schemaVersions": dict(sorted(schemas.items())),
        "numericTotals": {
            field: sum(int(row.get(field, 0) or 0) for row in accepted)
            for field in SUMMED_FIELDS
        },
        "contentSha256": hashlib.sha256(
            json.dumps(
                normalized, ensure_ascii=False, sort_keys=True, separators=(",", ":")
            ).encode()
        ).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--update", action="store_true")
    args = parser.parse_args()
    current = aggregate(json.load(sys.stdin))
    rendered = json.dumps(current, indent=2, sort_keys=True) + "\n"
    if args.update:
        args.baseline.write_text(rendered, encoding="utf-8")
        print(f"Updated {args.baseline}; review the diff before accepting it.")
        return 0
    if not args.baseline.is_file():
        print(f"Missing corpus baseline: {args.baseline}", file=sys.stderr)
        return 1
    expected = json.loads(args.baseline.read_text(encoding="utf-8"))
    if current != expected:
        print("Corpus aggregate differs from the reviewed baseline.", file=sys.stderr)
        print(rendered, file=sys.stderr)
        return 1
    print(
        f"Corpus baseline matches exactly: {current['accepted']}/{current['totalFiles']} accepted, "
        f"{current['rejected']} rejected."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
