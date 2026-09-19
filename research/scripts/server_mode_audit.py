#!/usr/bin/env python3
"""Audit serialized server-mode evidence in World in Conflict replays.

This tool deliberately reports only replay facts. In particular, a replay with
all mode flags clear is not called Ranked: the demo header does not serialize the
dedicated server's RankedFlag, so ranked and unranked public games currently share
the same observed state.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import pathlib
import struct
import zlib
from collections import Counter
from typing import Any, Iterator

from wic_bintag import FIELD_SEP, RAW_HEADER_BYTES, ZLIB_HEADERS, name_hash

HASH_FPM_MODE = name_hash("myFPMModeFlag")
HASH_MATCH_MODE = name_hash("myMatchModeFlag")
HASH_TOURNAMENT_MATCH = name_hash("myIsTournamentMatchFlag")
HASH_CLAN_MATCH = name_hash("myIsClanMatchFlag")
HASH_PLAYER_TYPE = name_hash("myType")
FIELD_BYTES = 17


def inflate_chunks(raw: bytes) -> Iterator[bytes]:
    """Yield each valid independent zlib stream in replay order."""
    offset = RAW_HEADER_BYTES
    while offset < len(raw):
        if raw[offset : offset + 2] not in ZLIB_HEADERS:
            offset += 1
            continue
        try:
            obj = zlib.decompressobj()
            chunk = obj.decompress(raw[offset:])
            consumed = len(raw) - offset - len(obj.unused_data)
        except zlib.error:
            offset += 1
            continue
        if not obj.eof or not chunk or consumed <= 0:
            offset += 1
            continue
        yield chunk
        offset += consumed


def field_values(data: bytes, field_hash: int) -> list[int]:
    """Return structurally framed four-byte values for one BinTag field."""
    pattern = struct.pack("<I", field_hash)
    values: list[int] = []
    position = 0
    while True:
        position = data.find(pattern, position)
        if position < 0:
            return values
        end = position + FIELD_BYTES
        if (
            end <= len(data)
            and data[position + 4 : position + 8] == FIELD_SEP
            and data[position + 9 : position + 13] == FIELD_SEP
        ):
            values.append(struct.unpack_from("<I", data, position + 13)[0])
        position += 1


def unique_header_flag(metadata: bytes, field_hash: int) -> int | None:
    """Read one boolean-like demo-header field without choosing among conflicts."""
    values = field_values(metadata, field_hash)
    if len(values) != 1 or values[0] not in (0, 1):
        return None
    return values[0]


def observed_signals(flags: dict[str, int | None], player_types: set[int]) -> list[str]:
    """Return every positively observed mode signal without forcing exclusivity."""
    signals: list[str] = []
    if flags["fpm"] == 1:
        signals.append("fewPlayerMode")
    elif flags["match"] == 1:
        signals.append("matchMode")
    if 1 in player_types:
        signals.append("bots")
    return signals or ["unknown"]


def audit_replay(path_text: str) -> dict[str, Any]:
    path = pathlib.Path(path_text)
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    chunks = inflate_chunks(raw)
    try:
        metadata = next(chunks)
    except StopIteration:
        return {"path": str(path), "sha256": digest, "error": "no zlib data"}

    flags = {
        "fpm": unique_header_flag(metadata, HASH_FPM_MODE),
        "match": unique_header_flag(metadata, HASH_MATCH_MODE),
        "tournament": unique_header_flag(metadata, HASH_TOURNAMENT_MATCH),
        "clanMatch": unique_header_flag(metadata, HASH_CLAN_MATCH),
    }
    player_types = set(field_values(metadata, HASH_PLAYER_TYPE))
    carry = metadata[-(FIELD_BYTES - 1) :]
    for chunk in chunks:
        joined = carry + chunk
        player_types.update(field_values(joined, HASH_PLAYER_TYPE))
        carry = joined[-(FIELD_BYTES - 1) :]

    conflicts: list[str] = []
    if flags["fpm"] == 1 and flags["match"] != 1:
        conflicts.append("FPM header without Match Mode header")
    unexpected_player_types = sorted(player_types - {0, 1})
    if unexpected_player_types:
        conflicts.append(f"unexpected player types: {unexpected_player_types}")

    return {
        "path": str(path),
        "sha256": digest,
        "headerFlags": flags,
        "playerTypes": sorted(player_types),
        "observedServerModeSignals": observed_signals(flags, player_types),
        "conflicts": conflicts,
    }


def discover(inputs: list[str]) -> list[pathlib.Path]:
    paths: set[pathlib.Path] = set()
    for item in inputs:
        path = pathlib.Path(item)
        if path.is_file() and path.suffix.lower() == ".wicdemo":
            paths.add(path)
        elif path.is_dir():
            for root, _directories, filenames in os.walk(path, followlinks=True):
                paths.update(
                    pathlib.Path(root, filename)
                    for filename in filenames
                    if pathlib.Path(filename).suffix.lower() == ".wicdemo"
                )
    return sorted(paths, key=lambda path: str(path).casefold())


def build_report(results: list[dict[str, Any]]) -> dict[str, Any]:
    signal_sets = Counter(
        tuple(item["observedServerModeSignals"])
        for item in results
        if "observedServerModeSignals" in item
    )
    flag_sets = Counter(
        tuple((name, value) for name, value in item["headerFlags"].items())
        for item in results
        if "headerFlags" in item
    )
    player_types = Counter(
        tuple(item["playerTypes"]) for item in results if "playerTypes" in item
    )
    return {
        "method": {
            "fewPlayerMode": "myFPMModeFlag=1",
            "matchMode": "myMatchModeFlag=1 and myFPMModeFlag!=1",
            "bots": "at least one PlayerEntersGame/roster myType=1; may coexist with FPM",
            "ranked": "not currently serialized; all-clear flags remain unknown",
        },
        "summary": {
            "replays": len(results),
            "observedSignalSets": [
                {"signals": list(signals), "count": count}
                for signals, count in sorted(
                    signal_sets.items(), key=lambda item: (-item[1], item[0])
                )
            ],
            "headerFlagSets": [
                {"flags": dict(flags), "count": count}
                for flags, count in sorted(
                    flag_sets.items(), key=lambda item: (-item[1], item[0])
                )
            ],
            "playerTypeSets": [
                {"values": list(values), "count": count}
                for values, count in sorted(
                    player_types.items(), key=lambda item: (-item[1], item[0])
                )
            ],
            "conflicts": sum(bool(item.get("conflicts")) for item in results),
            "errors": sum("error" in item for item in results),
        },
        "replays": results,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inputs", nargs="+", help="Replay files or corpus directories")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--json", dest="json_path", help="Write the full JSON report")
    args = parser.parse_args()
    paths = discover(args.inputs)
    if not paths:
        parser.error("no .wicdemo files found")
    if args.jobs < 1:
        parser.error("--jobs must be positive")

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(audit_replay, map(str, paths)))
    report = build_report(results)
    if args.json_path:
        pathlib.Path(args.json_path).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report["summary"], indent=2))
    return 1 if report["summary"]["errors"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
