"""Tests for offline runtime-oracle comparison."""

from __future__ import annotations

import json
import pathlib
import tempfile
import unittest

from replay_state_oracle import compare_capture, expected_parent_frames
from replay_state_reconstruction import (
    decode_compact_orientation,
    decode_compact_position,
)


class ReplayStateOracleTests(unittest.TestCase):
    def test_compares_exact_runtime_parent_bits(self):
        export = {
            "contract": "researchReplayState",
            "source": {"replaySha256": "a" * 64},
            "units": [
                {
                    "unitId": 7,
                    "frames": [
                        {
                            "offset": 123,
                            "encoding": "compact",
                            "timeBits": 0,
                            "raw": [7, 42, 84, 126, 20000, 20001, 20002, 20003],
                        }
                    ],
                }
            ],
        }
        expected = expected_parent_frames(export)[0]
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            export_path = root / "export.json"
            capture_path = root / "capture.jsonl"
            export_path.write_text(json.dumps(export))
            capture_path.write_text(
                json.dumps({"type": "captureHeader", "binarySha256": "b" * 64})
                + "\n"
                + json.dumps(
                    {
                        "type": "unitFrame",
                        "sequence": 0,
                        "unitId": 7,
                        "stateBits": expected["stateBits"],
                    }
                )
                + "\n"
            )
            result = compare_capture(export_path, capture_path, 0, 0)

        self.assertEqual(result["verdict"], "exactParentCheckpointMatch")
        self.assertEqual(result["mismatchCount"], 0)

    def test_binary_derived_compact_values_are_float32(self):
        self.assertEqual(decode_compact_position(42), 0.9880952835083008)
        self.assertEqual(decode_compact_orientation(20000), -9.999999747378752e-05)


if __name__ == "__main__":
    unittest.main()
