#!/usr/bin/env python3
"""Decode and validate shipped map overview DDS entries without extracting art."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import struct
from collections import Counter, defaultdict

from wic_sdf import SdfArchive, SdfEntry, SdfError

EXPECTED_ENTRY_COUNT = 32
DDS_HEADER_SIZE = 128
DDSD_MIPMAPCOUNT = 0x00020000


def _dxt1_level_size(width: int, height: int) -> int:
    return max(1, (width + 3) // 4) * max(1, (height + 3) // 4) * 8


def inspect_dds(data: bytes) -> dict[str, int | str]:
    """Validate the bounded DDS/DXT1 form used by overview maps."""

    if len(data) < DDS_HEADER_SIZE or data[:4] != b"DDS ":
        raise SdfError("decoded overview is missing the DDS header")
    header_size, flags, height, width = struct.unpack_from("<4I", data, 4)
    header_mipmap_count = struct.unpack_from("<I", data, 28)[0]
    pixel_format_size = struct.unpack_from("<I", data, 76)[0]
    fourcc = data[84:88]
    if header_size != 124:
        raise SdfError(f"DDS header size is {header_size}, expected 124")
    if pixel_format_size != 32:
        raise SdfError(f"DDS pixel-format size is {pixel_format_size}, expected 32")
    if width == 0 or height == 0:
        raise SdfError("DDS dimensions must be nonzero")
    if fourcc != b"DXT1":
        raise SdfError(f"DDS format is {fourcc!r}, expected DXT1")

    if flags & DDSD_MIPMAPCOUNT:
        if header_mipmap_count == 0:
            raise SdfError("DDS declares mipmaps but has a zero mipmap count")
        mipmap_count = header_mipmap_count
    else:
        mipmap_count = 1
    expected_payload_size = 0
    level_width = width
    level_height = height
    for _level in range(mipmap_count):
        expected_payload_size += _dxt1_level_size(level_width, level_height)
        level_width = max(1, level_width // 2)
        level_height = max(1, level_height // 2)
    if DDS_HEADER_SIZE + expected_payload_size != len(data):
        raise SdfError(
            "DDS byte count does not match its dimensions/format/mipmaps: "
            f"expected {DDS_HEADER_SIZE + expected_payload_size}, found {len(data)}"
        )
    return {
        "width": width,
        "height": height,
        "format": fourcc.decode("ascii"),
        "mipmapCount": mipmap_count,
        "headerMipmapCount": header_mipmap_count,
        "pixelPayloadSize": expected_payload_size,
    }


def is_overview(entry: SdfEntry) -> bool:
    parts = pathlib.PurePosixPath(entry.path).parts
    return len(parts) == 3 and parts[0] == "maps" and parts[2] == "overviewmap.dds"


def audit(archive_paths: list[pathlib.Path]) -> dict:
    records: list[dict] = []
    for archive_path in archive_paths:
        archive = SdfArchive.open(archive_path)
        for entry in archive.entries:
            if not is_overview(entry):
                continue
            decoded = archive.read_entry(entry)
            dds = inspect_dds(decoded)
            records.append(
                {
                    "mapName": pathlib.PurePosixPath(entry.path).parts[1],
                    "entryPath": entry.path,
                    "sourceArchive": str(archive_path),
                    "contentId": entry.content_id,
                    "codec": entry.codec,
                    "compressedSize": entry.compressed_size,
                    "uncompressedSize": entry.uncompressed_size,
                    "decodedSha256": hashlib.sha256(decoded).hexdigest(),
                    **dds,
                }
            )

    records.sort(key=lambda record: (record["sourceArchive"], record["entryPath"]))
    by_map: defaultdict[str, list[dict]] = defaultdict(list)
    for record in records:
        by_map[record["mapName"]].append(record)
    duplicates = {
        map_name: [
            {
                "sourceArchive": record["sourceArchive"],
                "decodedSha256": record["decodedSha256"],
                "uncompressedSize": record["uncompressedSize"],
                "width": record["width"],
                "height": record["height"],
                "mipmapCount": record["mipmapCount"],
            }
            for record in map_records
        ]
        for map_name, map_records in sorted(by_map.items())
        if len(map_records) > 1
    }
    size_classes = Counter(record["uncompressedSize"] for record in records)
    return {
        "schemaVersion": 1,
        "entryCount": len(records),
        "uniqueMapCount": len(by_map),
        "sizeClasses": {
            str(size): count for size, count in sorted(size_classes.items())
        },
        "duplicates": duplicates,
        "entries": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="+", type=pathlib.Path)
    parser.add_argument("--json", required=True, type=pathlib.Path)
    parser.add_argument("--expected-count", type=int, default=EXPECTED_ENTRY_COUNT)
    args = parser.parse_args()

    try:
        report = audit(args.archives)
        if report["entryCount"] != args.expected_count:
            raise SdfError(
                f"expected {args.expected_count} overview entries, "
                f"found {report['entryCount']}"
            )
    except (OSError, SdfError) as error:
        parser.exit(1, f"{type(error).__name__}: {error}\n")

    args.json.parent.mkdir(parents=True, exist_ok=True)
    args.json.write_text(json.dumps(report, indent=2) + "\n")
    print(
        f"validated {report['entryCount']} entries across "
        f"{report['uniqueMapCount']} map names"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
