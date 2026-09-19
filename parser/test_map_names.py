#!/usr/bin/env python3
"""Regression tests for internal-to-public map-name translation."""

import re
import unittest
from pathlib import Path

from wic_replay_parser import MAP_NAMES


class MapNameTests(unittest.TestCase):
    def test_every_public_name_has_a_recognized_mode_prefix(self) -> None:
        for raw_path, display_name in MAP_NAMES.items():
            with self.subTest(raw_path=raw_path):
                self.assertTrue(
                    display_name.startswith(("do_", "as_", "tw_")),
                    f"{display_name!r} has no recognized game-mode prefix",
                )

    def test_python_and_rust_tables_are_identical(self) -> None:
        rust_source = (
            Path(__file__).parent / "rust_parser" / "src" / "parser.rs"
        ).read_text()
        rust_names = dict(
            re.findall(r'^\s*"(maps/[^"]+)" => "([^"]+)",$', rust_source, re.MULTILINE)
        )
        self.assertEqual(rust_names, MAP_NAMES)

    def test_russia1_is_radar_tug_of_war(self) -> None:
        """The shipped russia1 localization names this map tw_Radar."""
        self.assertEqual(MAP_NAMES["maps/russia1/russia1.ice"], "tw_Radar")

    def test_verified_corrections_match_game_names(self) -> None:
        self.assertEqual(MAP_NAMES["maps/usdesert2/usdesert2.ice"], "as_AirBase")
        self.assertEqual(
            MAP_NAMES["maps/caspian_border_chepoint1/caspian_border_chepoint1.ice"],
            "do_Caspianborder",
        )
        self.assertEqual(MAP_NAMES["maps/usfarmland4/usfarmland4.ice"], "tw_Highway")
        self.assertEqual(MAP_NAMES["maps/usfarmland2/usfarmland2.ice"], "tw_Wasteland")

    def test_revision_paths_use_the_canonical_public_name(self) -> None:
        self.assertEqual(MAP_NAMES["maps/airport_03/airport_03.ice"], "do_Airport")
        self.assertEqual(MAP_NAMES["maps/airport_v2/airport_v2.ice"], "do_Airport")
        self.assertEqual(MAP_NAMES["maps/do_wakebeta2/do_wakebeta2.ice"], "do_Wake")
        obsolete_numbered_names = {
            "do_Airport2",
            "do_Airport3",
            "do_Farmland2",
            "do_Farmland4",
            "do_WakeBeta2",
        }
        self.assertTrue(obsolete_numbered_names.isdisjoint(MAP_NAMES.values()))

    def test_other_community_aliases_are_preserved(self) -> None:
        self.assertEqual(
            MAP_NAMES["maps/bllack_forest/bllack_forest.ice"], "do_BlackForest"
        )
        self.assertEqual(MAP_NAMES["maps/helgoland/helgoland.ice"], "do_Helgoland")

    def test_community_wake_fallback_is_preserved(self) -> None:
        self.assertEqual(MAP_NAMES["maps/do_wake/do_wake.ice"], "do_Wake")


if __name__ == "__main__":
    unittest.main()
