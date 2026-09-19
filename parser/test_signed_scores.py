import struct
import unittest
from unittest.mock import patch

from wic_replay_parser import (
    WICReplayParserV4,
    _BINTAG_SEP,
    _HASH_APOS,
    _HASH_SCORE,
    _HASH_SCORE_ROLE0,
    _HASH_SCORE_ROLE3,
    _HASH_TOTAL_SCORE,
)


def field(tag, value):
    return tag + _BINTAG_SEP + b"\0" + _BINTAG_SEP + struct.pack("<i", value)


class SignedScoreTests(unittest.TestCase):
    def test_negative_final_and_role_scores(self):
        data = field(_HASH_SCORE, -11).ljust(47, b"\0")
        data += _BINTAG_SEP + struct.pack("<I", 5)
        data += field(_HASH_APOS, 4)
        data += field(_HASH_SCORE_ROLE0, 10)
        data += field(_HASH_SCORE_ROLE3, -12)
        data += field(_HASH_TOTAL_SCORE, -12)
        data = data.ljust(400, b"\0")
        parser = WICReplayParserV4("synthetic.wicdemo")
        with patch.object(parser, "get_full_decompressed_data", return_value=data):
            self.assertEqual(parser._extract_scores_by_hash(), {5: -11})
            summary = parser.extract_player_end_summaries()[4]
            self.assertEqual(summary[0], "infantry")
            self.assertEqual(summary[4], -12)
            self.assertEqual(summary[12], -12)
        negative_only = field(_HASH_APOS, 4) + field(_HASH_SCORE_ROLE3, -12)
        with patch.object(
            parser,
            "get_full_decompressed_data",
            return_value=negative_only.ljust(300, b"\0"),
        ):
            self.assertEqual(parser.extract_player_end_summaries()[4][0], "air")
