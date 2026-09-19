"""Replay end-screen table boundaries and identity replacement controls."""

import struct
import unittest
import zlib

from test_player_identity import envelope, match_end, player_leave, utf16_field
from wic_replay_parser import WICReplayParserV4, _HASH_PLAYER_ENTRY_NAME


def field(name, value, flag=1):
    return struct.pack(
        "<IIBII", zlib.adler32(name.encode()), 17, flag, 17, value & 0xFFFFFFFF
    )


def entry(slot, name):
    return envelope(
        bytes.fromhex("5906d535"),
        0.0,
        field("aSlot", slot)
        + field("team", 3, 0)
        + utf16_field(_HASH_PLAYER_ENTRY_NAME, name)
        + field("readyFlag", 1)
        + field("myType", 0),
    )


def summary(slot, score):
    names = [
        "aPos",
        "aRoleId",
        "aTotalScore",
        "aCapturingScore",
        "aFortificationScore",
        "aTransportationScore",
        "aRepairScore",
        "aBridgeLayingScore",
        "aUnitDamageScore",
        "aTacticalAidScore",
        "aScoreRole0",
        "aScoreRole1",
        "aScoreRole2",
        "aScoreRole3",
    ]
    values = [slot, 0x0AE8026E, score] + [0] * 10 + [score]
    return envelope(
        bytes.fromhex("6f06313a"),
        2.0,
        b"".join(
            field(n, v, 0 if i in (1, 2) else 1)
            for i, (n, v) in enumerate(zip(names, values))
        ),
    )


class FinalScreenTests(unittest.TestCase):
    def parse(self, data):
        p = WICReplayParserV4("synthetic.wicdemo")
        p.full_data = data
        return p.final_screen_players()

    def test_replacement_and_departure_use_final_screen_rows_and_signed_total(self):
        data = entry(0, "Old") + player_leave(0, 0.0) + entry(0, "New")
        data += entry(1, "Left") + player_leave(1, 1.0)
        rows = self.parse(data + summary(0, -12) + match_end(2.0) + entry(0, "Later"))
        self.assertEqual(
            [(p.name, p.score, p.score_total, p.role) for p in rows],
            [("New", -12, -12, "air")],
        )

    def test_incomplete_and_ambiguous_tables_preserve_fallback(self):
        for data in [
            entry(0, "A") + summary(0, 1),
            entry(0, "A") + match_end(2.0),
            summary(0, 1) + match_end(2.0),
            entry(0, "A") + entry(1, "B") + summary(0, 1) + match_end(2.0),
            entry(0, "A") + summary(0, 1) + summary(0, 2) + match_end(2.0),
        ]:
            self.assertIsNone(self.parse(data))

    def test_explicit_spectator_and_unknown_departure(self):
        data = player_leave(5, 0.0) + entry(0, "A")
        data += envelope(
            bytes.fromhex("96072e4c"), 1.0, field("aSlot", 0) + field("aTeam", 3)
        )
        rows = self.parse(data + summary(0, 0) + match_end(2.0))
        self.assertEqual(rows[0].team, 0)
        self.assertIsNone(rows[0].role)

    def test_intact_table_does_not_reuse_name_after_unnamed_reentry(self):
        unnamed = envelope(
            bytes.fromhex("5906d535"),
            1.0,
            field("aSlot", 0)
            + field("team", 3, 0)
            + field("readyFlag", 1)
            + field("myType", 0),
        )
        p = WICReplayParserV4("synthetic.wicdemo")
        p.full_data = entry(0, "Old") + player_leave(0, 0.0) + unnamed
        p.full_data += summary(0, 91) + match_end(2.0)
        self.assertIsNotNone(p._final_screen_score_table())
        self.assertIsNone(p.final_screen_players())

    def test_damaged_chunks_cannot_be_hidden_by_aligned_event_splices(self):
        before = entry(0, "Old") + b"".join(
            envelope(bytes.fromhex("12345678"), 1.0, b"") for _ in range(6)
        )
        result = summary(0, 91) + match_end(2.0)
        data = before + result + entry(0, "After result")
        damaged = bytearray(zlib.compress(entry(0, "Unseen replacement")))
        damaged[-1] ^= 1
        for case, split in [
            ("before table", len(before)),
            ("inside table", len(before) + 100),
            ("after result", len(before) + len(result)),
        ]:
            with self.subTest(case=case):
                p = WICReplayParserV4("synthetic.wicdemo")
                p.raw_data = bytes(19) + zlib.compress(data[:split]) + damaged
                p.raw_data += zlib.compress(data[split:])
                self.assertEqual(p.get_full_decompressed_data(), data)
                self.assertEqual(p.decompression_gaps, [split])
                self.assertEqual(
                    p._final_screen_score_table() is not None, case != "inside table"
                )
                self.assertEqual(
                    p.final_screen_players() is not None, case == "after result"
                )

    def test_broken_event_chain_keeps_table_separate_from_identity(self):
        p = WICReplayParserV4("synthetic.wicdemo")
        p.full_data = entry(0, "Old") + b"".join(
            envelope(bytes.fromhex("12345678"), 1.0, b"") for _ in range(6)
        )
        p.full_data += b"broken envelope" + summary(0, 91) + match_end(2.0)
        self.assertIsNotNone(p._final_screen_score_table())
        self.assertIsNone(p.final_screen_players())
