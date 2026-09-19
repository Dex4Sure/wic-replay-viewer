#!/usr/bin/env python3
"""Tests for tactical-aid definition/localization recovery."""

from __future__ import annotations

import unittest

from ta_support_catalogue import parse_support_localization, presentation_class


class SupportLocalizationTests(unittest.TestCase):
    def test_extracts_internal_and_gui_names(self) -> None:
        data = (
            b"\xef\xbb\xbfSupportWeaponDatabase.US.TacticalNuke_US.myGuiName\t"
            b"Tactical Nuke\r\n"
            b"SupportWeaponDatabase.US.TacticalNuke_US.myGuiDescription\tBoom\r\n"
        )
        result = parse_support_localization(data)
        self.assertEqual(result["TacticalNuke_US"].section, "US")
        self.assertEqual(result["TacticalNuke_US"].gui_names, {"Tactical Nuke"})

    def test_rejects_conflicting_sections(self) -> None:
        data = (
            b"SupportWeaponDatabase.US.Shared.myGuiName\tFirst\n"
            b"SupportWeaponDatabase.NATO.Shared.myGuiName\tSecond\n"
        )
        with self.assertRaisesRegex(ValueError, "multiple sections"):
            parse_support_localization(data)

    def test_preserves_multiple_gui_names_as_ambiguity(self) -> None:
        data = (
            b"SupportWeaponDatabase.US.Example.myGuiName\tFirst\n"
            b"SupportWeaponDatabase.US.Example.myGuiName\tSecond\n"
        )
        result = parse_support_localization(data)
        self.assertEqual(result["Example"].gui_names, {"First", "Second"})

    def test_classifies_parent_child_and_special_definitions(self) -> None:
        data = (
            b"SupportWeaponDatabase.US.Parent.myGuiName\tParent\n"
            b"SupportWeaponDatabase.US.CHILD_Parent_1.myGuiName\tChild\n"
            b"SupportWeaponDatabase.SpecialAbilities.SPECIAL_ArtilleryMarker."
            b"myGuiName\tArtillery\n"
        )
        result = parse_support_localization(data)
        self.assertEqual(presentation_class(result["Parent"]), "topLevelFactionAid")
        self.assertEqual(presentation_class(result["CHILD_Parent_1"]), "childEffect")
        self.assertEqual(
            presentation_class(result["SPECIAL_ArtilleryMarker"]), "specialAbility"
        )


if __name__ == "__main__":
    unittest.main()
