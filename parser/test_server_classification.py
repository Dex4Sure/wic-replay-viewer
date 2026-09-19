import struct
import unittest

from wic_replay_parser import (
    WICReplayParserV4,
    _BINTAG_SEP,
    _HASH_MY_FPM_MODE_FLAG,
    _HASH_MY_IS_CLAN_MATCH_FLAG,
    _HASH_MY_IS_TOURNAMENT_MATCH_FLAG,
    _HASH_MY_MATCH_MODE_FLAG,
    _HASH_MY_TYPE,
)


def field(field_hash: bytes, value: int) -> bytes:
    return field_hash + _BINTAG_SEP + b"\0" + _BINTAG_SEP + struct.pack("<I", value)


def parser_with(metadata: bytes, full_data: bytes) -> WICReplayParserV4:
    parser = WICReplayParserV4("fixture.wicdemo")
    parser.metadata_chunk = metadata
    parser.full_data = full_data
    return parser


class ServerClassificationTests(unittest.TestCase):
    def test_fpm_and_bots_are_independent(self):
        metadata = b"".join(
            [
                field(_HASH_MY_FPM_MODE_FLAG, 1),
                field(_HASH_MY_MATCH_MODE_FLAG, 1),
                field(_HASH_MY_IS_TOURNAMENT_MATCH_FLAG, 0),
                field(_HASH_MY_IS_CLAN_MATCH_FLAG, 0),
            ]
        )
        raw, classification = parser_with(
            metadata, metadata + field(_HASH_MY_TYPE, 1)
        ).extract_server_classification()

        self.assertTrue(raw.few_player_mode_flag)
        self.assertTrue(raw.match_mode_flag)
        self.assertTrue(classification.few_player_mode)
        self.assertFalse(classification.match_mode)
        self.assertTrue(classification.has_bots)
        self.assertIsNone(classification.ranked)

    def test_plain_match_mode_and_human_roster(self):
        metadata = b"".join(
            [
                field(_HASH_MY_FPM_MODE_FLAG, 0),
                field(_HASH_MY_MATCH_MODE_FLAG, 1),
                field(_HASH_MY_IS_TOURNAMENT_MATCH_FLAG, 0),
                field(_HASH_MY_IS_CLAN_MATCH_FLAG, 1),
            ]
        )
        _, classification = parser_with(
            metadata, metadata + field(_HASH_MY_TYPE, 0)
        ).extract_server_classification()

        self.assertTrue(classification.match_mode)
        self.assertFalse(classification.has_bots)
        self.assertTrue(classification.clan_match)

    def test_missing_evidence_remains_unknown(self):
        _, classification = parser_with(b"", b"").extract_server_classification()

        self.assertIsNone(classification.few_player_mode)
        self.assertIsNone(classification.match_mode)
        self.assertIsNone(classification.has_bots)
        self.assertIsNone(classification.ranked)


if __name__ == "__main__":
    unittest.main()
