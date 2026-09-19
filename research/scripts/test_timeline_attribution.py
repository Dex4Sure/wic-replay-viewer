#!/usr/bin/env python3
"""Focused tests for conservative timeline attribution joins."""

from __future__ import annotations

import unittest

from timeline_attribution import (
    SupportMarker,
    SupportMarkerStop,
    SupportPurchase,
    SupportSpawn,
    link_marker_stops,
    marker_support_type_stats,
    match_purchase_marker_candidates,
    match_purchase_spawn_candidates,
)


def purchase(
    offset: int,
    support_type_id: int = 1,
    position: tuple[float, float, float] = (1.0, 2.0, 3.0),
) -> SupportPurchase:
    return SupportPurchase(0, offset, float(offset), support_type_id, position, 7, -5.0)


def spawn(
    offset: int,
    support_type_id: int = 1,
    position: tuple[float, float, float] = (1.0, 2.0, 3.0),
) -> SupportSpawn:
    return SupportSpawn(
        1,
        offset,
        float(offset),
        support_type_id,
        position,
        3,
        0,
        (0.0, 0.0, 1.0),
        0.0,
    )


class CandidateMatchingTests(unittest.TestCase):
    def test_marker_support_fingerprint_is_compact_and_exact(self) -> None:
        markers = [
            SupportMarker(
                1,
                11,
                11.0,
                42,
                0x12345678,
                (0.0, 0.0, 0.0),
                3,
                1,
                (0.0, 0.0, 0.0),
                20.0,
            ),
            SupportMarker(
                2,
                12,
                12.0,
                43,
                0x12345678,
                (1.0, 2.0, 3.0),
                4,
                1,
                (0.0, 1.0, 0.0),
                25.0,
            ),
        ]
        result = marker_support_type_stats(markers)["0x12345678"]
        self.assertEqual(result["deployments"], 2)
        self.assertEqual(result["playerSlotCounts"], {"3": 1, "4": 1})
        self.assertEqual(result["upgradeLevelCounts"], {"1": 2})
        self.assertEqual(
            result["durationSecondsRange"], {"minimum": 20.0, "maximum": 25.0}
        )
        self.assertEqual(result["zeroDirection"], 1)
        self.assertEqual(result["nonzeroDirection"], 1)
        self.assertEqual(result["worldOriginPosition"], 1)

    def test_singleton_component_is_proposed(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10)], [spawn(11)], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "unique")
        self.assertEqual(
            results[0]["proposedAttribution"]["validationState"],
            "researchCandidate",
        )

    def test_multiple_spawn_candidates_remain_ambiguous(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10)], [spawn(11), spawn(12)], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "ambiguous")
        self.assertNotIn("proposedAttribution", results[0])

    def test_reverse_collision_remains_ambiguous(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10), purchase(11)], [spawn(12)], position_epsilon=0.05
        )
        self.assertEqual(
            [item["status"] for item in results], ["ambiguous", "ambiguous"]
        )

    def test_spawn_before_purchase_is_not_a_candidate(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10)], [spawn(9)], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "unmatched")

    def test_support_type_must_match(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10, support_type_id=1)],
            [spawn(11, support_type_id=2)],
            position_epsilon=0.05,
        )
        self.assertEqual(results[0]["status"], "unmatched")

    def test_position_outside_epsilon_does_not_match(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10)],
            [spawn(11, position=(1.1, 2.0, 3.0))],
            position_epsilon=0.05,
        )
        self.assertEqual(results[0]["status"], "unmatched")

    def test_unique_link_without_recorder_does_not_propose_player(self) -> None:
        item = purchase(10)
        without_recorder = SupportPurchase(
            item.event_index,
            item.offset,
            item.raw_event_time,
            item.support_type_id,
            item.position,
            None,
            item.honors_delta,
        )
        results = match_purchase_spawn_candidates(
            [without_recorder], [spawn(11)], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "unique")
        self.assertNotIn("proposedAttribution", results[0])

    def test_unique_link_without_negative_ledger_does_not_propose_player(self) -> None:
        item = purchase(10)
        without_ledger = SupportPurchase(
            item.event_index,
            item.offset,
            item.raw_event_time,
            item.support_type_id,
            item.position,
            item.recorder_player_id,
            None,
        )
        results = match_purchase_spawn_candidates(
            [without_ledger], [spawn(11)], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "unique")
        self.assertNotIn("proposedAttribution", results[0])

    def test_marker_event_id_is_preserved_on_candidate(self) -> None:
        marker = SupportMarker(
            1,
            11,
            11.0,
            42,
            1,
            (1.0, 2.0, 3.0),
            3,
            0,
            (0.0, 0.0, 1.0),
            20.0,
        )
        results = match_purchase_marker_candidates(
            [purchase(10)], [marker], position_epsilon=0.05
        )
        self.assertEqual(results[0]["status"], "unique")
        self.assertEqual(results[0]["candidates"][0]["effectEventId"], 42)
        self.assertEqual(results[0]["candidates"][0]["effectPlayerId"], 3)
        self.assertFalse(results[0]["attributionCrossCheck"]["agrees"])
        self.assertNotIn("proposedAttribution", results[0])

    def test_marker_stop_inherits_exact_player_from_active_event_id(self) -> None:
        marker = SupportMarker(
            1,
            11,
            11.0,
            42,
            1,
            (1.0, 2.0, 3.0),
            7,
            0,
            (0.0, 0.0, 1.0),
            20.0,
        )
        stop = SupportMarkerStop(2, 12, 12.0, 42)
        results = link_marker_stops([marker], [stop])
        self.assertEqual(results[0]["attribution"]["class"], "exact")
        self.assertEqual(results[0]["attribution"]["playerId"], 7)

    def test_marker_stop_without_active_start_remains_unmatched(self) -> None:
        stop = SupportMarkerStop(2, 12, 12.0, 42)
        results = link_marker_stops([], [stop])
        self.assertEqual(results[0]["attribution"]["class"], "unmatched")
        self.assertIsNone(results[0]["attribution"]["playerId"])

    def test_global_constraints_resolve_a_forced_assignment(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10), purchase(12)],
            [spawn(11), spawn(13)],
            position_epsilon=0.05,
        )
        self.assertEqual([item["status"] for item in results], ["unique", "unique"])
        self.assertEqual(
            [
                item["candidates"][item["selectedCandidateIndex"]]["effectOffset"]
                for item in results
            ],
            [11, 13],
        )

    def test_multiple_complete_matchings_remain_ambiguous(self) -> None:
        results = match_purchase_spawn_candidates(
            [purchase(10), purchase(10)],
            [spawn(11), spawn(12)],
            position_epsilon=0.05,
        )
        self.assertEqual(
            [item["status"] for item in results], ["ambiguous", "ambiguous"]
        )


if __name__ == "__main__":
    unittest.main()
