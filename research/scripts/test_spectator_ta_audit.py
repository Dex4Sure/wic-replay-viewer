#!/usr/bin/env python3
"""Tests for spectator tactical-aid coverage decoding."""

from __future__ import annotations

import unittest

from spectator_ta_audit import decode_spectator_view, support_faction


class SpectatorViewTests(unittest.TestCase):
    def test_one_team_view_retains_selected_faction(self) -> None:
        view = decode_spectator_view(3, 1)
        self.assertEqual(view.mode, "oneTeam")
        self.assertEqual(view.team, 3)
        self.assertEqual(view.raw_los, 1)

    def test_all_team_view_does_not_claim_one_faction(self) -> None:
        view = decode_spectator_view(0, 2)
        self.assertEqual(view.mode, "allTeams")
        self.assertIsNone(view.team)

    def test_unknown_values_remain_unknown(self) -> None:
        self.assertEqual(decode_spectator_view(2, 7).mode, "unknown")

    def test_top_level_name_suffix_maps_to_faction(self) -> None:
        self.assertEqual(support_faction("Airstrike_US"), 1)
        self.assertEqual(support_faction("TacticalNuke_NATO_British"), 2)
        self.assertEqual(support_faction("DaisyCutter_USSR"), 3)
        self.assertIsNone(support_faction("SPECIAL_ArtilleryMarker"))


if __name__ == "__main__":
    unittest.main()
