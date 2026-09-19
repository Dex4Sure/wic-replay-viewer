#!/usr/bin/env python3
"""Read-only inspector for World in Conflict RYS/SDF archives.

The shipped version-9/10 archives start with ``RYS`` plus a one-byte version and
an absolute directory offset. The directory block stores its uncompressed size
and a compressed-size word whose top two bits select the codec. The decompressed
directory begins with a sorted lookup table of ``(hash, record_offset)`` pairs.

This tool implements bounded listing and extraction for the plain and zlib
codecs, plus the three-stream codec 3 used by client assets. Codec 2 remains
visible in listings but is not extracted.

Examples:

    research/scripts/wic_sdf.py local/binaries/game/wic20.sdf --match support
    research/scripts/wic_sdf.py local/binaries/game/wic*.sdf --match 'tactical|artillery'
    research/scripts/wic_sdf.py local/binaries/server/wic_ds.sdf --json local/generated/wic-ds-sdf.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import struct
import zlib
from dataclasses import asdict, dataclass

SDF_SIGNATURE = b"RYS"
SDF_HEADER_SIZE = 8
BLOCK_HEADER_SIZE = 8
CODEC_SHIFT = 30
SIZE_MASK = (1 << CODEC_SHIFT) - 1
VERSION_10_AUX_HEADER_SIZE = 28
SEGMENTED_PREFIX_SIZE = 128
SUPPORTED_VERSIONS = {9, 10}


class SdfError(ValueError):
    """Raised when an SDF structure is malformed or unsupported."""


@dataclass(frozen=True)
class SdfEntry:
    """One file record recovered from the SDF directory."""

    path: str
    lookup_hash: int
    content_id: int
    uncompressed_size: int
    compressed_size: int
    codec: int
    data_offset: int
    record_offset: int


@dataclass(frozen=True)
class _SegmentedBlock:
    """Codec-3 prefix and stored lengths recovered from version-10 metadata."""

    prefix: bytes
    first_compressed_size: int
    second_compressed_size: int


def _u32(data: bytes, offset: int, label: str) -> int:
    if offset < 0 or offset + 4 > len(data):
        raise SdfError(f"{label} is outside the directory")
    return struct.unpack_from("<I", data, offset)[0]


def _c_string(data: bytes, offset: int, label: str) -> str:
    if offset < 0 or offset >= len(data):
        raise SdfError(f"{label} offset is outside the directory")
    end = data.find(b"\0", offset)
    if end < 0:
        raise SdfError(f"{label} is not NUL-terminated")
    try:
        return data[offset:end].decode("utf-8")
    except UnicodeDecodeError as error:
        raise SdfError(f"{label} is not UTF-8/ASCII") from error


def _decode_segment(payload: bytes, label: str) -> bytes:
    decoder = zlib.decompressobj()
    try:
        output = decoder.decompress(payload) + decoder.flush()
    except zlib.error as error:
        raise SdfError(f"invalid {label} zlib stream: {error}") from error
    if not decoder.eof:
        raise SdfError(f"{label} zlib stream is truncated")
    if decoder.unused_data or decoder.unconsumed_tail:
        raise SdfError(f"{label} zlib stream has trailing data")
    return output


def _decode_block(
    payload: bytes,
    size_word: int,
    expected_size: int,
    segmented: _SegmentedBlock | None = None,
) -> bytes:
    codec = size_word >> CODEC_SHIFT
    compressed_size = size_word & SIZE_MASK
    if len(payload) != compressed_size:
        raise SdfError(
            f"compressed block is truncated: expected {compressed_size}, "
            f"found {len(payload)}"
        )
    if codec == 0:
        output = payload
    elif codec == 1:
        try:
            output = zlib.decompress(payload)
        except zlib.error as error:
            raise SdfError(f"invalid zlib block: {error}") from error
    elif codec == 3:
        if segmented is None:
            raise SdfError("codec 3 metadata is unavailable")
        first_end = segmented.first_compressed_size
        second_end = first_end + segmented.second_compressed_size
        if first_end <= 0 or second_end <= first_end or second_end >= len(payload):
            raise SdfError("codec 3 segment sizes do not partition the payload")
        output = segmented.prefix + b"".join(
            (
                _decode_segment(payload[:first_end], "codec 3 segment 1"),
                _decode_segment(payload[first_end:second_end], "codec 3 segment 2"),
                _decode_segment(payload[second_end:], "codec 3 segment 3"),
            )
        )
    else:
        raise SdfError(f"codec {codec} extraction is not implemented")
    if len(output) != expected_size:
        raise SdfError(
            f"decoded size mismatch: expected {expected_size}, found {len(output)}"
        )
    return output


class SdfArchive:
    """Bounded read-only view of an SDF archive."""

    def __init__(
        self,
        path: pathlib.Path,
        version: int,
        file_size: int,
        directory_offset: int,
        directory_codec: int,
        directory: bytes,
        entries: tuple[SdfEntry, ...],
        segmented_blocks: dict[int, _SegmentedBlock],
    ) -> None:
        self.path = path
        self.version = version
        self.file_size = file_size
        self.directory_offset = directory_offset
        self.directory_codec = directory_codec
        self.directory = directory
        self.entries = entries
        self._segmented_blocks = segmented_blocks

    @classmethod
    def open(cls, path: str | pathlib.Path) -> SdfArchive:
        archive_path = pathlib.Path(path)
        file_size = archive_path.stat().st_size
        if file_size < SDF_HEADER_SIZE + BLOCK_HEADER_SIZE:
            raise SdfError("file is too small to be an SDF archive")

        with archive_path.open("rb") as handle:
            header = handle.read(SDF_HEADER_SIZE)
            if header[:3] != SDF_SIGNATURE:
                raise SdfError("missing RYS signature")
            version = header[3]
            if version not in SUPPORTED_VERSIONS:
                raise SdfError(f"unsupported SDF version {version}")
            directory_offset = struct.unpack_from("<I", header, 4)[0]
            if not SDF_HEADER_SIZE <= directory_offset <= file_size - BLOCK_HEADER_SIZE:
                raise SdfError("directory offset is outside the archive")

            handle.seek(directory_offset)
            directory_size, size_word = struct.unpack("<II", handle.read(8))
            directory_compressed_size = size_word & SIZE_MASK
            if (
                directory_compressed_size
                > file_size - directory_offset - BLOCK_HEADER_SIZE
            ):
                raise SdfError("compressed directory extends past end of archive")
            compressed = handle.read(directory_compressed_size)

        directory = _decode_block(compressed, size_word, directory_size)
        entry_count = _u32(directory, 0, "entry count")
        table_end = 4 + entry_count * 8
        if table_end > len(directory):
            raise SdfError("entry lookup table extends past the directory")

        entries: list[SdfEntry] = []
        seen_records: set[int] = set()
        for index in range(entry_count):
            lookup_hash, record_offset = struct.unpack_from(
                "<II", directory, 4 + index * 8
            )
            if record_offset in seen_records:
                raise SdfError(f"duplicate record offset 0x{record_offset:x}")
            seen_records.add(record_offset)
            if record_offset < table_end or record_offset + 20 > len(directory):
                raise SdfError(f"entry {index} record is outside the directory")

            (
                content_id,
                uncompressed_size,
                entry_size_word,
                data_offset,
                prefix_offset,
            ) = struct.unpack_from("<IIIII", directory, record_offset)
            compressed_size = entry_size_word & SIZE_MASK
            codec = entry_size_word >> CODEC_SHIFT
            if data_offset < SDF_HEADER_SIZE:
                raise SdfError(f"entry {index} data offset precedes the payload")
            if compressed_size > directory_offset - data_offset:
                raise SdfError(f"entry {index} data extends into the directory")

            prefix = _c_string(directory, prefix_offset, f"entry {index} prefix")
            basename = _c_string(directory, record_offset + 20, f"entry {index} name")
            if not basename:
                raise SdfError(f"entry {index} has an empty name")
            entry_path = f"{prefix}/{basename}" if prefix else basename
            entries.append(
                SdfEntry(
                    path=entry_path,
                    lookup_hash=lookup_hash,
                    content_id=content_id,
                    uncompressed_size=uncompressed_size,
                    compressed_size=compressed_size,
                    codec=codec,
                    data_offset=data_offset,
                    record_offset=record_offset,
                )
            )

        segmented_blocks: dict[int, _SegmentedBlock] = {}
        codec_3_indexes = [
            index for index, entry in enumerate(entries) if entry.codec == 3
        ]
        if codec_3_indexes:
            if version != 10:
                raise SdfError("codec 3 entries require version-10 auxiliary metadata")
            auxiliary_offset = (
                directory_offset + BLOCK_HEADER_SIZE + directory_compressed_size
            )
            segmented_blocks = _read_segmented_blocks(
                archive_path,
                file_size,
                auxiliary_offset,
                entries,
                codec_3_indexes,
            )

        return cls(
            path=archive_path,
            version=version,
            file_size=file_size,
            directory_offset=directory_offset,
            directory_codec=size_word >> CODEC_SHIFT,
            directory=directory,
            entries=tuple(entries),
            segmented_blocks=segmented_blocks,
        )

    def read_entry(self, entry: SdfEntry) -> bytes:
        """Return one decoded file without modifying the source archive."""
        with self.path.open("rb") as handle:
            handle.seek(entry.data_offset)
            payload = handle.read(entry.compressed_size)
        return _decode_block(
            payload,
            (entry.codec << CODEC_SHIFT) | entry.compressed_size,
            entry.uncompressed_size,
            self._segmented_blocks.get(entry.record_offset),
        )


def _read_segmented_blocks(
    path: pathlib.Path,
    file_size: int,
    auxiliary_offset: int,
    entries: list[SdfEntry],
    codec_3_indexes: list[int],
) -> dict[int, _SegmentedBlock]:
    """Read codec-3 descriptors from the two version-10 auxiliary blocks."""

    if auxiliary_offset + VERSION_10_AUX_HEADER_SIZE > file_size:
        raise SdfError("version-10 auxiliary header extends past end of archive")
    with path.open("rb") as handle:
        handle.seek(auxiliary_offset)
        header = handle.read(VERSION_10_AUX_HEADER_SIZE)
        (
            entry_count,
            _archive_value_1,
            descriptor_size,
            descriptor_size_word,
            _archive_value_2,
            table_size,
            table_size_word,
        ) = struct.unpack("<7I", header)
        if entry_count != len(entries):
            raise SdfError(
                "version-10 auxiliary entry count mismatch: "
                f"expected {len(entries)}, found {entry_count}"
            )

        descriptor_compressed_size = descriptor_size_word & SIZE_MASK
        table_compressed_size = table_size_word & SIZE_MASK
        blocks_end = (
            auxiliary_offset
            + VERSION_10_AUX_HEADER_SIZE
            + descriptor_compressed_size
            + table_compressed_size
        )
        if blocks_end > file_size:
            raise SdfError("version-10 auxiliary blocks extend past end of archive")
        descriptor_payload = handle.read(descriptor_compressed_size)
        table_payload = handle.read(table_compressed_size)

    descriptors = _decode_block(
        descriptor_payload, descriptor_size_word, descriptor_size
    )
    segment_table = _decode_block(table_payload, table_size_word, table_size)
    expected_table_size = len(entries) * 8
    if len(segment_table) != expected_table_size:
        raise SdfError(
            "version-10 segment table size mismatch: "
            f"expected {expected_table_size}, found {len(segment_table)}"
        )

    result: dict[int, _SegmentedBlock] = {}
    for index in codec_3_indexes:
        kind, descriptor_span, descriptor_offset = struct.unpack_from(
            "<HHI", segment_table, index * 8
        )
        if kind != 1:
            raise SdfError(f"codec 3 entry {index} has descriptor kind {kind}")
        if descriptor_span < 12 or descriptor_offset + descriptor_span > len(
            descriptors
        ):
            raise SdfError(f"codec 3 entry {index} descriptor is outside metadata")
        prefix_offset, first_size, second_size = struct.unpack_from(
            "<III", descriptors, descriptor_offset
        )
        if prefix_offset + SEGMENTED_PREFIX_SIZE > len(descriptors):
            raise SdfError(f"codec 3 entry {index} prefix is outside metadata")
        entry = entries[index]
        if first_size <= 0 or second_size <= 0:
            raise SdfError(f"codec 3 entry {index} has an empty stored segment")
        if first_size + second_size >= entry.compressed_size:
            raise SdfError(f"codec 3 entry {index} segment sizes exceed its payload")
        result[entry.record_offset] = _SegmentedBlock(
            prefix=descriptors[prefix_offset : prefix_offset + SEGMENTED_PREFIX_SIZE],
            first_compressed_size=first_size,
            second_compressed_size=second_size,
        )
    return result


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def archive_report(
    archive: SdfArchive,
    pattern: re.Pattern[str] | None,
    include_all: bool,
    include_hash: bool,
) -> dict:
    matches = [
        entry
        for entry in archive.entries
        if include_all or (pattern is not None and pattern.search(entry.path))
    ]
    codecs: dict[str, int] = {}
    for entry in archive.entries:
        key = str(entry.codec)
        codecs[key] = codecs.get(key, 0) + 1
    report = {
        "path": str(archive.path),
        "version": archive.version,
        "fileSize": archive.file_size,
        "directoryOffset": archive.directory_offset,
        "directorySize": len(archive.directory),
        "directoryCodec": archive.directory_codec,
        "entryCount": len(archive.entries),
        "entryCodecs": dict(sorted(codecs.items())),
        "matches": [asdict(entry) for entry in matches],
    }
    if include_hash:
        report["sha256"] = sha256(archive.path)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archives", nargs="+", help="SDF archives to inspect")
    parser.add_argument(
        "--match", help="case-insensitive entry-path regular expression"
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="include every entry (otherwise only --match results are emitted)",
    )
    parser.add_argument("--hash", action="store_true", help="calculate archive SHA-256")
    parser.add_argument("--json", help="write the complete report to this path")
    args = parser.parse_args()

    if args.list and args.match:
        parser.error("--list and --match are mutually exclusive")
    pattern = re.compile(args.match, re.IGNORECASE) if args.match else None

    reports = []
    failed = False
    for path in args.archives:
        try:
            archive = SdfArchive.open(path)
            reports.append(archive_report(archive, pattern, args.list, args.hash))
        except (OSError, SdfError) as error:
            reports.append({"path": path, "error": f"{type(error).__name__}: {error}"})
            failed = True

    output = {"schemaVersion": 1, "archives": reports}
    rendered = json.dumps(output, indent=2) + "\n"
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(rendered)
    else:
        print(rendered, end="")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
