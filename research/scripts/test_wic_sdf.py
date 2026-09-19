#!/usr/bin/env python3
"""Tests for the bounded World in Conflict SDF reader."""

from __future__ import annotations

import pathlib
import struct
import tempfile
import unittest
import zlib

from wic_sdf import CODEC_SHIFT, SdfArchive, SdfError


def fixture_bytes(
    content: bytes = b"support definition\n",
    prefix: str = "units/support",
    basename: str = "example.ice",
) -> bytes:
    compressed_content = zlib.compress(content, level=9)
    record_offset = 12
    basename_bytes = basename.encode() + b"\0"
    prefix_offset = record_offset + 20 + len(basename_bytes)
    record = struct.pack(
        "<IIIII",
        0x12345678,
        len(content),
        (1 << CODEC_SHIFT) | len(compressed_content),
        8,
        prefix_offset,
    )
    directory = (
        struct.pack("<III", 1, 0xAABBCCDD, record_offset)
        + record
        + basename_bytes
        + prefix.encode()
        + b"\0"
    )
    compressed_directory = zlib.compress(directory, level=9)
    directory_offset = 8 + len(compressed_content)
    return (
        b"RYS\x0a"
        + struct.pack("<I", directory_offset)
        + compressed_content
        + struct.pack(
            "<II",
            len(directory),
            (1 << CODEC_SHIFT) | len(compressed_directory),
        )
        + compressed_directory
    )


def segmented_fixture_bytes(
    descriptor_kind: int = 1,
    auxiliary_entry_count: int = 1,
    first_size: int | None = None,
    second_size: int | None = None,
) -> tuple[bytes, bytes]:
    prefix = b"DDS " + bytes(range(124))
    body_parts = (b"first segment\n", b"second segment\n" * 2, b"third segment\n" * 3)
    compressed_parts = tuple(zlib.compress(part, level=9) for part in body_parts)
    payload = b"".join(compressed_parts)
    expected = prefix + b"".join(body_parts)

    basename = b"overviewmap.dds\0"
    path_prefix = b"maps/example\0"
    record_offset = 12
    prefix_offset = record_offset + 20 + len(basename)
    directory = (
        struct.pack("<III", 1, 0xAABBCCDD, record_offset)
        + struct.pack(
            "<IIIII",
            0x87654321,
            len(expected),
            (3 << CODEC_SHIFT) | len(payload),
            8,
            prefix_offset,
        )
        + basename
        + path_prefix
    )
    compressed_directory = zlib.compress(directory, level=9)
    directory_offset = 8 + len(payload)

    descriptor = (
        struct.pack(
            "<III",
            12,
            len(compressed_parts[0]) if first_size is None else first_size,
            len(compressed_parts[1]) if second_size is None else second_size,
        )
        + prefix
    )
    compressed_descriptor = zlib.compress(descriptor, level=9)
    segment_table = struct.pack("<HHI", descriptor_kind, len(descriptor), 0)
    compressed_segment_table = zlib.compress(segment_table, level=9)
    auxiliary_header = struct.pack(
        "<7I",
        auxiliary_entry_count,
        0,
        len(descriptor),
        (1 << CODEC_SHIFT) | len(compressed_descriptor),
        0,
        len(segment_table),
        (1 << CODEC_SHIFT) | len(compressed_segment_table),
    )
    archive = (
        b"RYS\x0a"
        + struct.pack("<I", directory_offset)
        + payload
        + struct.pack(
            "<II",
            len(directory),
            (1 << CODEC_SHIFT) | len(compressed_directory),
        )
        + compressed_directory
        + auxiliary_header
        + compressed_descriptor
        + compressed_segment_table
    )
    return archive, expected


class SdfArchiveTests(unittest.TestCase):
    def write_fixture(self, root: str, data: bytes) -> pathlib.Path:
        path = pathlib.Path(root) / "fixture.sdf"
        path.write_bytes(data)
        return path

    def test_lists_and_extracts_a_zlib_entry(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            path = self.write_fixture(root, fixture_bytes())
            archive = SdfArchive.open(path)

            self.assertEqual(archive.version, 10)
            self.assertEqual(len(archive.entries), 1)
            entry = archive.entries[0]
            self.assertEqual(entry.path, "units/support/example.ice")
            self.assertEqual(entry.lookup_hash, 0xAABBCCDD)
            self.assertEqual(entry.content_id, 0x12345678)
            self.assertEqual(archive.read_entry(entry), b"support definition\n")

    def test_extracts_a_three_stream_codec_3_entry(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data, expected = segmented_fixture_bytes()
            path = self.write_fixture(root, data)
            archive = SdfArchive.open(path)

            self.assertEqual(len(archive.entries), 1)
            self.assertEqual(archive.entries[0].codec, 3)
            self.assertEqual(archive.read_entry(archive.entries[0]), expected)

    def test_rejects_a_bad_signature(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data = bytearray(fixture_bytes())
            data[:3] = b"BAD"
            path = self.write_fixture(root, bytes(data))
            with self.assertRaisesRegex(SdfError, "RYS signature"):
                SdfArchive.open(path)

    def test_rejects_an_entry_that_overlaps_the_directory(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data = bytearray(fixture_bytes())
            directory_offset = struct.unpack_from("<I", data, 4)[0]
            directory_size, size_word = struct.unpack_from(
                "<II", data, directory_offset
            )
            compressed_size = size_word & ((1 << CODEC_SHIFT) - 1)
            directory = bytearray(
                zlib.decompress(
                    data[directory_offset + 8 : directory_offset + 8 + compressed_size]
                )
            )
            struct.pack_into("<I", directory, 12 + 8, 0x3FFFFFFF)
            replacement = zlib.compress(directory, level=9)
            rebuilt = (
                data[:directory_offset]
                + struct.pack(
                    "<II",
                    directory_size,
                    (1 << CODEC_SHIFT) | len(replacement),
                )
                + replacement
            )
            path = self.write_fixture(root, bytes(rebuilt))
            with self.assertRaisesRegex(SdfError, "extends into the directory"):
                SdfArchive.open(path)

    def test_rejects_a_codec_3_descriptor_of_the_wrong_kind(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data, _ = segmented_fixture_bytes(descriptor_kind=0)
            path = self.write_fixture(root, data)
            with self.assertRaisesRegex(SdfError, "descriptor kind"):
                SdfArchive.open(path)

    def test_rejects_an_auxiliary_entry_count_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data, _ = segmented_fixture_bytes(auxiliary_entry_count=2)
            path = self.write_fixture(root, data)
            with self.assertRaisesRegex(SdfError, "auxiliary entry count mismatch"):
                SdfArchive.open(path)

    def test_rejects_codec_3_segments_that_overrun_the_payload(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data, _ = segmented_fixture_bytes(first_size=0x3FFFFFF0)
            path = self.write_fixture(root, data)
            with self.assertRaisesRegex(SdfError, "segment sizes exceed its payload"):
                SdfArchive.open(path)

    def test_rejects_an_empty_codec_3_segment(self) -> None:
        with tempfile.TemporaryDirectory() as root:
            data, _ = segmented_fixture_bytes(second_size=0)
            path = self.write_fixture(root, data)
            with self.assertRaisesRegex(SdfError, "empty stored segment"):
                SdfArchive.open(path)


if __name__ == "__main__":
    unittest.main()
