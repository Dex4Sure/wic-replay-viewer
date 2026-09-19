#!/usr/bin/env python3
"""Extract named TA timing fields from immutable shipped SDF data.

Field semantics are established separately by the Ghidra timer finding.
This report preserves raw scalar values, definition offsets and source hashes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

from ta_support_catalogue import parse_support_localization, presentation_class
from wic_bintag import name_hash
from wic_ice import ice_tree_roots
from wic_sdf import SdfArchive

FIELDS = (
    "myRechargeTime",
    "myTimeBeforeActivation",
    "myInitialTimeDelay",
    "myMarkerTimeToLive",
    "myMarkerTimeDelay",
    "mySupportAnimationTimeToLive",
    "myTimeVisibleForOpponents",
)


def walk(node):
    yield node
    for child in node.children:
        yield from walk(child)


def extract(ice: bytes, localization: bytes) -> list[dict]:
    definitions = parse_support_localization(localization)
    names = {}
    for name, definition in definitions.items():
        if presentation_class(definition) == "topLevelFactionAid":
            key = name_hash(name)
            if key in names:
                raise ValueError("Ambiguous support definition hash")
            names[key] = name
    rows = []
    seen = set()
    for root in ice_tree_roots(ice):
        for node in walk(root):
            if node.key_hash not in names:
                continue
            name = names[node.key_hash]
            if name in seen:
                raise ValueError(f"Duplicate definition: {name}")
            seen.add(name)
            fields = {}
            for field in FIELDS:
                matches = [c for c in node.children if c.key_hash == name_hash(field)]
                if len(matches) != 1 or matches[0].value is None:
                    raise ValueError(f"Missing or ambiguous {name}.{field}")
                child = matches[0]
                fields[field] = {
                    "value": child.value.decode("ascii"),
                    "offset": hex(child.offset),
                    "typeHash": hex(child.type_hash),
                }
            rows.append(
                {
                    "name": name,
                    "labels": sorted(definitions[name].gui_names),
                    "hash": hex(node.key_hash),
                    "offset": hex(node.offset),
                    "fields": fields,
                }
            )
    if seen != set(names.values()):
        raise ValueError(f"Definitions missing: {set(names.values()) - seen}")
    return sorted(rows, key=lambda row: row["name"])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive", type=Path, default=Path("local/binaries/server/wic_ds.sdf")
    )
    parser.add_argument("--json", type=Path, required=True)
    args = parser.parse_args()
    archive = SdfArchive.open(args.archive)
    entries = {}
    for path in ("maps/supportweapons.ice", "maps/supportweapons.loc"):
        entry = next(entry for entry in archive.entries if entry.path == path)
        entries[path] = archive.read_entry(entry)
    rows = extract(
        entries["maps/supportweapons.ice"], entries["maps/supportweapons.loc"]
    )
    report = {
        "archiveSha256": hashlib.sha256(args.archive.read_bytes()).hexdigest(),
        "entries": {
            path: {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
            for path, data in entries.items()
        },
        "definitions": rows,
    }
    args.json.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Extracted {len(rows)} top-level Tactical Aid definitions to {args.json}")


if __name__ == "__main__":
    main()
