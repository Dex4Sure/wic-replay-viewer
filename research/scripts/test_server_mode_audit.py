import struct
import tempfile
import unittest
from pathlib import Path

from server_mode_audit import (
    HASH_FPM_MODE,
    HASH_MATCH_MODE,
    discover,
    field_values,
    observed_signals,
    unique_header_flag,
)
from wic_bintag import FIELD_SEP


def encoded_field(field_hash: int, value: int, flag: int = 0) -> bytes:
    return (
        struct.pack("<I", field_hash)
        + FIELD_SEP
        + bytes([flag])
        + FIELD_SEP
        + struct.pack("<I", value)
    )


class ServerModeAuditTests(unittest.TestCase):
    def test_field_values_requires_structural_separators(self):
        data = (
            encoded_field(HASH_MATCH_MODE, 1)
            + struct.pack("<I", HASH_MATCH_MODE)
            + b"x" * 13
        )
        self.assertEqual(field_values(data, HASH_MATCH_MODE), [1])

    def test_unique_header_flag_rejects_missing_invalid_and_conflicting_values(self):
        self.assertIsNone(unique_header_flag(b"", HASH_FPM_MODE))
        self.assertIsNone(
            unique_header_flag(encoded_field(HASH_FPM_MODE, 2), HASH_FPM_MODE)
        )
        duplicate = encoded_field(HASH_FPM_MODE, 0) + encoded_field(HASH_FPM_MODE, 1)
        self.assertIsNone(unique_header_flag(duplicate, HASH_FPM_MODE))

    def test_observed_signals_preserve_fpm_and_bots_overlap(self):
        clear = {"fpm": 0, "match": 0, "tournament": 0, "clanMatch": 0}
        self.assertEqual(observed_signals(clear, {0}), ["unknown"])
        self.assertEqual(observed_signals({**clear, "match": 1}, {0}), ["matchMode"])
        self.assertEqual(
            observed_signals({**clear, "fpm": 1, "match": 1}, {0, 1}),
            ["fewPlayerMode", "bots"],
        )

    def test_discover_follows_replay_root_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence = root / "evidence"
            evidence.mkdir()
            replay = evidence / "sample.wicdemo"
            replay.write_bytes(b"")
            link = root / "linked"
            link.symlink_to(evidence, target_is_directory=True)
            self.assertEqual(discover([str(link)]), [link / replay.name])


if __name__ == "__main__":
    unittest.main()
