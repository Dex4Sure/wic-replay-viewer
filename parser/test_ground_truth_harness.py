import json
import tempfile
import unittest
from contextlib import ExitStack, redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch

import test_ground_truth as harness


def expected() -> dict:
    return {
        "game_info": {
            "map_name": "map",
            "map_display_name": "Map",
            "server_name": "Server",
            "date_time": "Date",
            "game_mode": "Domination",
        },
        "winner": "USA",
        "winner_domination_pct": 0.6,
        "loser_domination_pct": 0.4,
        "incomplete": False,
        "recorder": "Alice",
        "player_scores": [10],
        "players": [
            {
                "id": 1,
                "name": "Alice",
                "team": 1,
                "faction": "USA",
                "score": 10,
                "role": "air",
            }
        ],
    }


def actual() -> dict:
    item = expected()
    return {
        "gameInfo": {
            "mapName": item["game_info"]["map_name"],
            "mapDisplayName": item["game_info"]["map_display_name"],
            "serverName": item["game_info"]["server_name"],
            "dateTime": item["game_info"]["date_time"],
            "gameMode": item["game_info"]["game_mode"],
        },
        "winner": item["winner"],
        "winnerDominationPct": item["winner_domination_pct"],
        "loserDominationPct": item["loser_domination_pct"],
        "incomplete": item["incomplete"],
        "recorder": item["recorder"],
        "playerScores": item["player_scores"],
        "players": item["players"],
    }


class GroundTruthHarnessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.binary = self.root / "parser"
        self.binary.touch()

    def tearDown(self):
        self.temp.cleanup()

    def run_harness(self, fixtures, *, require_all=False, find=None, parse=None):
        truth = self.root / "truth.json"
        truth.write_text(json.dumps(fixtures), encoding="utf-8")
        output = StringIO()
        with ExitStack() as stack:
            stack.enter_context(patch.object(harness, "GROUND_TRUTH_FILE", truth))
            stack.enter_context(patch.object(harness, "RUST_PARSER", self.binary))
            stack.enter_context(patch.object(harness, "REQUIRE_ALL", require_all))
            if find is not None:
                stack.enter_context(
                    patch.object(harness, "find_replay", side_effect=find)
                )
            if parse is not None:
                stack.enter_context(
                    patch.object(harness, "parse_replay", side_effect=parse)
                )
            stack.enter_context(redirect_stdout(output))
            result = harness.run_tests()
        return result, output.getvalue()

    def test_missing_binary_fails_before_reading_fixtures(self):
        harness.RUST_PARSER = self.root / "missing"
        output = StringIO()
        with redirect_stdout(output):
            self.assertEqual(harness.run_tests(), 1)
        self.assertIn("build the Rust parser first", output.getvalue())

    def test_zero_executed_fixtures_fails_closed(self):
        result, output = self.run_harness(
            {"missing": expected()},
            find=lambda _name: (_ for _ in ()).throw(FileNotFoundError("gone")),
        )
        self.assertEqual(result, 1)
        self.assertIn("no ground-truth fixtures ran", output)

    def test_partial_execution_requires_explicit_non_strict_mode(self):
        fixtures = {"good": expected(), "missing": expected()}

        def find(name):
            if name == "good":
                return self.root / name
            raise FileNotFoundError("gone")

        self.assertEqual(
            self.run_harness(fixtures, find=find, parse=lambda _path: actual())[0], 0
        )
        self.assertEqual(
            self.run_harness(
                fixtures, require_all=True, find=find, parse=lambda _path: actual()
            )[0],
            1,
        )

    def test_parser_errors_and_semantic_differences_fail(self):
        fixtures = {"replay": expected()}

        def find(name):
            return self.root / name

        self.assertEqual(
            self.run_harness(
                fixtures,
                find=find,
                parse=lambda _path: (_ for _ in ()).throw(RuntimeError("bad replay")),
            )[0],
            1,
        )
        changed = actual()
        changed["winner"] = "USSR"
        result, output = self.run_harness(
            fixtures, find=find, parse=lambda _path: changed
        )
        self.assertEqual(result, 1)
        self.assertIn("winner", output)


if __name__ == "__main__":
    unittest.main()
