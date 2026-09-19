#!/usr/bin/env python3
"""Audit time-varying player names carried by replay slot lifecycle events."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import pathlib
import struct
from collections import defaultdict
from typing import Any

from wic_bintag import decompress, fields, name_hash, walk

MSG_PLAYER_ENTERS_GAME = name_hash("PlayerEntersGame")
MSG_PLAYER_LEAVES_GAME = name_hash("PlayerLeavesGame")
FIELD_SLOT = name_hash("aSlot")

# The shipped binaries do not expose a string for this field name. Its position,
# type-5 framing, and UTF-16 payload are stable in every valid PlayerEntersGame
# record inspected so far.
FIELD_PLAYER_ENTRY_NAME = 0x041E01A2
FIXED_FIELD_BYTES = 17
STRING_FIELD_HEADER_BYTES = 13
MAX_PLAYER_NAME_UTF16_BYTES = 256


def decode_player_entry(data: bytes, envelope: Any) -> tuple[int, str] | None:
    """Return the exact slot/name pair from a valid PlayerEntersGame envelope."""
    body, _ = fields(data, envelope)
    if not body or body[0].hash != FIELD_SLOT or body[0].u32 > 15:
        return None

    name_pos = envelope.body_start + 2 * FIXED_FIELD_BYTES
    if name_pos + STRING_FIELD_HEADER_BYTES > envelope.body_end:
        return None
    if struct.unpack_from("<I", data, name_pos)[0] != FIELD_PLAYER_ENTRY_NAME:
        return None

    total = struct.unpack_from("<I", data, name_pos + 4)[0]
    if (
        total < STRING_FIELD_HEADER_BYTES + 2
        or total > STRING_FIELD_HEADER_BYTES + MAX_PLAYER_NAME_UTF16_BYTES
        or data[name_pos + 8] != 5
        or struct.unpack_from("<I", data, name_pos + 9)[0] != total
        or name_pos + total > envelope.body_end
    ):
        return None

    payload = data[name_pos + STRING_FIELD_HEADER_BYTES : name_pos + total]
    if len(payload) % 2 != 0 or payload[-2:] != b"\0\0":
        return None
    try:
        name = payload[:-2].decode("utf-16-le")
    except UnicodeDecodeError:
        return None
    if not name or "\0" in name:
        return None
    return body[0].u32, name


def decode_player_leave(data: bytes, envelope: Any) -> int | None:
    """Return the exact slot from a valid PlayerLeavesGame envelope."""
    body, trailing = fields(data, envelope)
    if trailing or not body or body[0].hash != FIELD_SLOT or body[0].u32 > 15:
        return None
    return body[0].u32


def audit_replay(path_text: str) -> dict[str, Any]:
    path = pathlib.Path(path_text)
    try:
        data = decompress(path)
        names_by_slot: dict[int, list[str]] = defaultdict(list)
        entry_count = 0
        valid_entry_count = 0
        malformed_entries = 0
        leave_count = 0
        malformed_leaves = 0

        for envelope in walk(data):
            if envelope.message == MSG_PLAYER_ENTERS_GAME:
                entry_count += 1
                decoded = decode_player_entry(data, envelope)
                if decoded is None:
                    malformed_entries += 1
                    continue
                valid_entry_count += 1
                slot, name = decoded
                if not names_by_slot[slot] or names_by_slot[slot][-1] != name:
                    names_by_slot[slot].append(name)
            elif envelope.message == MSG_PLAYER_LEAVES_GAME:
                leave_count += 1
                if decode_player_leave(data, envelope) is None:
                    malformed_leaves += 1

        replacements = {
            str(slot): names
            for slot, names in sorted(names_by_slot.items())
            if len(set(names)) > 1
        }
        result: dict[str, Any] = {
            "path": str(path),
            "entryEvents": entry_count,
            "validEntryEvents": valid_entry_count,
            "malformedEntryEvents": malformed_entries,
            "leaveEvents": leave_count,
            "malformedLeaveEvents": malformed_leaves,
            "slotsWithDifferentNames": len(replacements),
        }
        if replacements:
            result["sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
            result["replacements"] = replacements
        return result
    except Exception as error:  # noqa: BLE001 - corpus failures belong in the report
        return {"path": str(path), "error": f"{type(error).__name__}: {error}"}


def discover_replays(roots: list[pathlib.Path]) -> list[pathlib.Path]:
    """Discover replay files beneath real or symlinked corpus roots."""
    discovered: dict[pathlib.Path, pathlib.Path] = {}
    for root in roots:
        if root.is_file() and root.suffix.lower() == ".wicdemo":
            discovered.setdefault(root.resolve(), root)
            continue
        for directory, _, filenames in os.walk(root, followlinks=True):
            for filename in filenames:
                if filename.lower().endswith(".wicdemo"):
                    path = pathlib.Path(directory, filename)
                    discovered.setdefault(path.resolve(), path)
    return sorted(discovered.values(), key=lambda path: str(path))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", type=pathlib.Path)
    parser.add_argument("--jobs", type=int, default=max(1, os.cpu_count() or 1))
    parser.add_argument("--json", type=pathlib.Path)
    args = parser.parse_args()

    paths = discover_replays(args.roots)
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        results = list(pool.map(audit_replay, map(str, paths)))

    failures = [result for result in results if "error" in result]
    replacements = [
        result for result in results if result.get("slotsWithDifferentNames", 0) > 0
    ]
    summary = {
        "schemaVersion": 1,
        "replays": len(results),
        "failures": len(failures),
        "entryEvents": sum(result.get("entryEvents", 0) for result in results),
        "validEntryEvents": sum(
            result.get("validEntryEvents", 0) for result in results
        ),
        "malformedEntryEvents": sum(
            result.get("malformedEntryEvents", 0) for result in results
        ),
        "leaveEvents": sum(result.get("leaveEvents", 0) for result in results),
        "malformedLeaveEvents": sum(
            result.get("malformedLeaveEvents", 0) for result in results
        ),
        "replaysWithDifferentNameSlotReuse": len(replacements),
        "slotsWithDifferentNames": sum(
            result.get("slotsWithDifferentNames", 0) for result in replacements
        ),
        "replacementReplays": replacements,
        "failedReplays": failures,
    }
    encoded = json.dumps(summary, indent=2, ensure_ascii=False) + "\n"
    if args.json:
        args.json.write_text(encoded, encoding="utf-8")
    else:
        print(encoded, end="")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
