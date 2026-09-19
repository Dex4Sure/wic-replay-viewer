"""Tests for the research replay-state decoder."""

from __future__ import annotations

import pathlib
import struct
import tempfile
import unittest
import zlib

from replay_state_reconstruction import (
    MAX_COORD,
    MIN_COORD,
    TYPE_REAL,
    TYPE_VECTOR2,
    X_COORD,
    Z_COORD,
    decode_replay,
    decode_compact_position,
    decode_unit_frame,
    dxt1_dds_to_png,
    playfield_bounds,
)
from wic_bintag import ENVELOPE_HASH, Envelope, name_hash

SEP = b"\x11\x00\x00\x00"


def scalar(name: str, value: int | float, flag: int = 1) -> bytes:
    raw = struct.pack("<f", value) if flag == 2 else struct.pack("<I", value)
    return struct.pack("<I", name_hash(name)) + SEP + bytes([flag]) + SEP + raw


def vector(name: str, values: tuple[float, float, float]) -> bytes:
    return b"".join(
        scalar(f"{name}.{axis}", value, 2) for axis, value in zip("xyz", values)
    )


def envelope(message: str, time_seconds: float, body: bytes) -> bytes:
    total = 21 + len(body)
    return (
        struct.pack("<II", ENVELOPE_HASH, 0x15)
        + b"\x06"
        + struct.pack("<IfI", total, time_seconds, name_hash(message))
        + body
    )


def frame_body(field: str, payload: bytes) -> bytes:
    total = 13 + len(payload)
    return (
        struct.pack("<II", name_hash(field), total)
        + b"\x06"
        + struct.pack("<I", total)
        + payload
    )


def compact_payload(unit_id: int, position=(42, 84, 126), children=()) -> bytes:
    header = unit_id | (len(children) << 12)
    parent = struct.pack("<8H", header, *position, 20000, 20001, 20002, 20003)
    child_data = b"".join(
        bytes([(index + 1) | (0x80 if flag else 0)]) + struct.pack("<HH", first, second)
        for index, flag, first, second in children
    )
    return parent + child_data


class ReplayStateTests(unittest.TestCase):
    def test_decodes_dxt1_overview_without_external_image_packages(self):
        dds = bytearray(128)
        dds[:4] = b"DDS "
        dds[4:8] = struct.pack("<I", 124)
        dds[12:20] = struct.pack("<II", 4, 4)
        dds[84:88] = b"DXT1"
        dds.extend(struct.pack("<HHI", 0xF800, 0x001F, 0))

        png = dxt1_dds_to_png(bytes(dds))

        self.assertTrue(png.startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertIn(b"IHDR", png)
        self.assertIn(b"IEND", png)

    def test_reads_exact_map_playfield_bounds(self):
        def vector2(field: int, x: str, z: str) -> bytes:
            result = struct.pack("<III", TYPE_VECTOR2, field, 2)
            for component, value in ((X_COORD, x), (Z_COORD, z)):
                encoded = value.encode()
                result += struct.pack(
                    "<IIII", TYPE_REAL, component, 0xFFFFFFFF, len(encoded)
                )
                result += encoded
            return result

        ice = (
            b"ice0010\0"
            + vector2(MIN_COORD, "123", "92")
            + vector2(MAX_COORD, "1451", "1460")
        )

        self.assertEqual(
            playfield_bounds(ice),
            {"minX": 123.0, "minZ": 92.0, "maxX": 1451.0, "maxZ": 1460.0},
        )

    def test_decodes_compact_parent_and_child_state(self):
        body = frame_body(
            "UnitFrameDataCompact",
            compact_payload(17, children=[(3, True, 20010, 19990)]),
        )
        event = Envelope(
            0, 21 + len(body), 3.5, name_hash("UnitFrame"), 21, 21 + len(body)
        )
        decoded = decode_unit_frame(envelope("UnitFrame", 3.5, body), event)

        self.assertEqual(decoded["unitId"], 17)
        self.assertEqual(
            decoded["position"],
            [decode_compact_position(value) for value in (42, 84, 126)],
        )
        self.assertEqual(decoded["childCount"], 1)
        self.assertEqual(decoded["children"][0]["index"], 3)
        self.assertTrue(decoded["children"][0]["flag"])

    def test_decodes_full_parent_and_child_state(self):
        parent = struct.pack("<7fHB", 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0, 25, 1)
        payload = parent + struct.pack("<ffBB", 0.25, -0.5, 1, 4)
        body = frame_body("UnitFrameData", payload)
        event = Envelope(
            0, 21 + len(body), 4.0, name_hash("UnitFrame"), 21, 21 + len(body)
        )
        decoded = decode_unit_frame(envelope("UnitFrame", 4.0, body), event)

        self.assertEqual(decoded["unitId"], 25)
        self.assertEqual(decoded["position"], [1.0, 2.0, 3.0])
        self.assertEqual(decoded["orientation"], [0.0, 0.0, 0.0, 1.0])
        self.assertEqual(decoded["children"][0]["index"], 4)

    def test_rejects_inconsistent_compact_child_count(self):
        body = frame_body("UnitFrameDataCompact", compact_payload(2)[:-1])
        event = Envelope(
            0, 21 + len(body), 0.0, name_hash("UnitFrame"), 21, 21 + len(body)
        )
        self.assertIsNone(decode_unit_frame(envelope("UnitFrame", 0.0, body), event))

    def test_lifecycle_generations_do_not_merge_reused_ids(self):
        def create(at: float, x: float) -> bytes:
            body = (
                scalar("aPlayer", 2)
                + scalar("aTeam", 1)
                + scalar("aUnit", 7)
                + scalar("aType", 99)
                + vector("aPosition", (x, 0.0, 1.0))
                + scalar("aHeading", 0.0, 2)
                + scalar("aCurrentHealth", 100.0, 2)
            )
            return envelope("UnitCreate", at, body)

        stream = bytearray(100)
        map_name = b"maps/test/test.ice\0"
        stream[47 : 47 + len(map_name)] = map_name
        stream.extend(create(0.0, 1.0))
        stream.extend(
            envelope(
                "UnitFrame",
                0.3,
                frame_body("UnitFrameDataCompact", compact_payload(7)),
            )
        )
        stream.extend(
            envelope("UnitDestroy", 1.0, scalar("aUnit", 7) + scalar("aKiller", 8))
        )
        stream.extend(create(2.0, 2.0))
        stream.extend(
            envelope(
                "UnitFrame",
                2.3,
                frame_body("UnitFrameDataCompact", compact_payload(7)),
            )
        )
        stream.extend(envelope("TeamWins", 2.4, b""))

        with tempfile.TemporaryDirectory() as directory:
            replay = pathlib.Path(directory) / "reuse.wicdemo"
            replay.write_bytes(bytes(19) + zlib.compress(bytes(stream)))
            document = decode_replay(replay)

        self.assertEqual(document["source"]["mapName"], "maps/test/test.ice")
        self.assertEqual(len(document["units"]), 2)
        self.assertEqual([len(unit["frames"]) for unit in document["units"]], [1, 1])
        self.assertEqual(document["units"][0]["terminal"]["type"], "destroyed")
        self.assertEqual(document["units"][1]["generation"], 2)
        self.assertIsNone(document["coverage"]["frameGaps"]["maxSeconds"])


if __name__ == "__main__":
    unittest.main()
