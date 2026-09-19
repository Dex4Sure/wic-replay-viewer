import tempfile
import unittest
from pathlib import Path

from wic_replay_parser import WICReplayParserV4


class ReplayFileValidationTests(unittest.TestCase):
    def test_accepts_the_wicdemo_extension_case_insensitively(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.WICDEMO"
            path.write_bytes(b"replay")
            self.assertEqual(WICReplayParserV4(str(path)).read_file(), b"replay")

    def test_rejects_an_existing_non_wicdemo_path(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sample.dat"
            path.write_bytes(b"replay")
            with self.assertRaisesRegex(ValueError, r"must end in \.wicdemo"):
                WICReplayParserV4(str(path)).read_file()


if __name__ == "__main__":
    unittest.main()
