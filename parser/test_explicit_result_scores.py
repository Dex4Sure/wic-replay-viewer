"""Own-slot score attribution and incomplete-recording fallback controls."""

import unittest

from test_player_identity import (
    envelope,
    gameplay_clock,
    match_end,
    u32_field,
)
from wic_replay_parser import WICReplayParserV4, _HASH_APOS, _HASH_SCORE


def score(slot, value, time, unsigned_slot=True):
    body = bytearray(u32_field(_HASH_APOS, slot))
    body[8] = int(unsigned_slot)
    body += u32_field(_HASH_SCORE, value & 0xFFFFFFFF)
    return envelope(bytes([41, 3, 220, 13]), time, body)


class ExplicitScoresTests(unittest.TestCase):
    def parser(self, data):
        parser = WICReplayParserV4("synthetic.wicdemo")
        parser.full_data = data
        parser.metadata_chunk = b""
        return parser

    def test_final_update_signed_values_reset_and_post_result_boundary(self):
        data = gameplay_clock(0.0)
        data += score(15, 809, 1.0) + score(0, 12, 2.0)
        data += score(15, 810, 3.0) + score(4, -11, 4.0)
        data += score(0, 0, 5.0) + match_end(6.0) + score(15, 0, 7.0)
        parser = self.parser(data)
        self.assertEqual(
            {slot: value for _, slot, value in parser._explicit_match_scores()},
            {15: 810, 0: 0, 4: -11},
        )

    def test_incomplete_chain_does_not_replace_summary(self):
        parser = self.parser(gameplay_clock(0.0) + score(15, 100, 1.0))
        self.assertEqual(parser._explicit_match_scores(), [])

    def test_wrong_field_types_and_invalid_slots_are_not_scores(self):
        parser = self.parser(
            gameplay_clock(0.0)
            + score(15, 100, 1.0, unsigned_slot=False)
            + score(16, 200, 2.0)
            + match_end(3.0)
        )
        self.assertEqual(parser._explicit_match_scores(), [])
