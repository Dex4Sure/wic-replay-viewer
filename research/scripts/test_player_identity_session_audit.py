from __future__ import annotations

import struct
import unittest

from player_identity_session_audit import (
    FIELD_PLAYER_ENTRY_NAME,
    decode_player_entry,
)
from wic_bintag import (
    ENVELOPE_FLAG,
    ENVELOPE_HASH,
    ENVELOPE_HEADER_BYTES,
    ENVELOPE_TYPE,
    FIELD_SEP,
    Envelope,
    name_hash,
)


def fixed_field(name: str, flag: int, value: int) -> bytes:
    return (
        struct.pack("<I", name_hash(name))
        + FIELD_SEP
        + bytes([flag])
        + FIELD_SEP
        + struct.pack("<I", value)
    )


def player_entry(slot: int, name: str) -> tuple[bytes, Envelope]:
    encoded = name.encode("utf-16-le") + b"\0\0"
    total = 13 + len(encoded)
    body = (
        fixed_field("aSlot", 1, slot)
        + fixed_field("unknown", 0, 0)
        + struct.pack("<II", FIELD_PLAYER_ENTRY_NAME, total)
        + bytes([5])
        + struct.pack("<I", total)
        + encoded
    )
    envelope_total = ENVELOPE_HEADER_BYTES + len(body)
    data = (
        struct.pack("<I", ENVELOPE_HASH)
        + ENVELOPE_TYPE
        + bytes([ENVELOPE_FLAG])
        + struct.pack("<IfI", envelope_total, 1.0, name_hash("PlayerEntersGame"))
        + body
    )
    return data, Envelope(
        0, envelope_total, 1.0, name_hash("PlayerEntersGame"), 21, envelope_total
    )


class PlayerIdentitySessionAuditTests(unittest.TestCase):
    def test_decodes_name_bearing_player_entry(self) -> None:
        data, envelope = player_entry(1, "[WHO]LtDan73")
        self.assertEqual(decode_player_entry(data, envelope), (1, "[WHO]LtDan73"))

    def test_rejects_out_of_range_slot(self) -> None:
        data, envelope = player_entry(16, "replacement")
        self.assertIsNone(decode_player_entry(data, envelope))

    def test_rejects_non_terminated_name(self) -> None:
        data, envelope = player_entry(1, "replacement")
        damaged = data[:-2] + b"xx"
        self.assertIsNone(decode_player_entry(damaged, envelope))


if __name__ == "__main__":
    unittest.main()
