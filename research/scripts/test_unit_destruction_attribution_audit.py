import unittest
import struct
from types import SimpleNamespace

from unit_destruction_attribution_audit import (
    KILLER_UNIT_SENTINEL,
    analyse_blink_lifecycles,
    analyse_blower_lifecycles,
    building_resident_damage_candidates,
    bridge_destruction_candidates,
    build_clock_samples,
    classify_destruction,
    decode_blink_trailing_fields,
    decode_trailing_bool_field,
    decode_unit_frame,
    damaging_cloud_candidates,
    events_in_window,
    exact_target_normal_actors,
    exact_tactical_aid_cause,
    cluster_multi_player_deaths,
    link_taunts,
    positive_score_candidate_windows,
    preceding_positive_honors_gap,
    primary_chain_end_offset,
    projectile_spatial_event,
    replay_map_name,
    preceding_same_tick_blast_candidates,
    support_impact_causes,
    summarise,
    timeline_time_at_offset,
    unit_create_fortification_id,
)
from wic_bintag import name_hash
from wic_ice import CloudTypeDefinition, UnitTypeParasites


class UnitDestructionAttributionAuditTests(unittest.TestCase):
    def test_cloud_candidate_requires_fresh_geometry_and_sufficient_damage(self):
        definition = CloudTypeDefinition(
            index=5,
            support_id=0x12345678,
            support_name="TacticalNuke_US",
            support_section="US",
            cloud_key_hash=0x87654321,
            time_to_live=1.0,
            health_change=-150000,
            health_change_interval=0.5,
            radius=80.0,
            initial_logic_delay=0.0,
            affect_friendly=True,
            affect_enemy=True,
            affects_infantry=True,
            affects_vehicle=True,
            affects_tanks=True,
            affects_copters=True,
            affects_misc=True,
            affects_buildings=True,
            infantry_damage_multiplier=1.0,
            ground_damage_multiplier=1.0,
            heavy_armor_damage_multiplier=1.0,
            air_damage_multiplier=1.0,
            building_damage_multiplier=1.0,
        )
        unit = UnitTypeParasites(
            name="TestTank",
            type_hash=0x11111111,
            meta_type=1,
            meta_type_name="GROUND",
            unit_category=2,
            unit_category_name="TANKS",
            max_health=1000,
            max_speed=10.0,
            max_speed_multiplier=1.0,
            parasite_type_hashes=frozenset(),
        )
        raw_time_bits = struct.unpack("<I", struct.pack("<f", 10.0))[0]
        cloud = {
            "_definition": definition,
            "_rawTime": 10.0,
            "eventIndex": 10,
            "offset": 100,
            "rawEventTime": 10.0,
            "rawTimeBits": raw_time_bits,
            "timeToLiveSeconds": 1.0,
            "team": 1,
            "position": [0.0, 0.0, 0.0],
        }
        destruction = {
            "_rawTime": 10.0,
            "rawTimeBits": raw_time_bits,
            "eventIndex": 20,
            "unitTypeId": unit.type_hash,
            "victimTeam": 2,
            "victimBuildingOccupancy": None,
            "victimPositionAtDestruction": [79.9, 0.0, 0.0],
            "victimLastFrame": {
                "eventIndex": 19,
                "rawEventTime": 10.0,
                "rawTimeBits": raw_time_bits,
                "encoding": "compact",
            },
        }

        candidate = damaging_cloud_candidates(
            destruction, [cloud], {unit.type_hash: unit}
        )[0]

        self.assertTrue(candidate["maximumHealthLethal"])
        self.assertTrue(candidate["definitelyInsideRadiusByFreshFrame"])
        self.assertTrue(candidate["binarySufficientCloudCandidate"])
        self.assertEqual(candidate["causalAttribution"], "candidateOnly")

        stale = damaging_cloud_candidates(
            {
                **destruction,
                "victimLastFrame": {
                    **destruction["victimLastFrame"],
                    "rawTimeBits": raw_time_bits - 1,
                },
            },
            [cloud],
            {unit.type_hash: unit},
        )[0]
        self.assertFalse(stale["binarySufficientCloudCandidate"])

        snapshot = damaging_cloud_candidates(
            destruction,
            [{**cloud, "timeToLiveSeconds": 0.5}],
            {unit.type_hash: unit},
        )[0]
        self.assertFalse(snapshot["freshCreationLifetime"])
        self.assertFalse(snapshot["binarySufficientCloudCandidate"])

    def test_building_resident_candidates_require_exact_building_time_and_order(self):
        destruction = {
            "eventIndex": 20,
            "rawTimeBits": 0x41200000,
            "victimBuildingOccupancy": {"buildingId": 37},
            "killerUnitId": KILLER_UNIT_SENTINEL,
            "syntheticTerminalDirection": True,
        }
        exact = {
            "eventIndex": 18,
            "rawTimeBits": 0x41200000,
            "buildingId": 37,
            "state": 2,
        }
        candidates = [
            exact,
            {**exact, "buildingId": 38},
            {**exact, "rawTimeBits": 0x41200001},
            {**exact, "eventIndex": 20},
            {**exact, "eventIndex": 21},
        ]

        self.assertEqual(
            building_resident_damage_candidates(destruction, candidates), [exact]
        )
        self.assertEqual(
            building_resident_damage_candidates(
                destruction,
                [exact, {**exact, "eventIndex": 19, "state": 3}],
            ),
            [],
        )
        self.assertEqual(
            building_resident_damage_candidates(
                {**destruction, "killerUnitId": 41}, [exact]
            ),
            [],
        )
        self.assertEqual(
            building_resident_damage_candidates(
                {**destruction, "syntheticTerminalDirection": False}, [exact]
            ),
            [],
        )
        self.assertEqual(
            building_resident_damage_candidates(
                {**destruction, "victimBuildingOccupancy": None}, candidates
            ),
            [],
        )

    def test_reads_only_the_exact_optional_unit_create_fortification_field(self):
        body = [SimpleNamespace(hash=0, i32=-1, u32=0xFFFFFFFF) for _ in range(15)]
        body[14] = SimpleNamespace(hash=name_hash("aFortificationId"), i32=37, u32=37)

        self.assertEqual(unit_create_fortification_id(body), 37)
        body[14] = SimpleNamespace(
            hash=name_hash("aFortificationId"), i32=-1, u32=0xFFFFFFFF
        )
        self.assertIsNone(unit_create_fortification_id(body))
        body[14] = SimpleNamespace(hash=0, i32=37, u32=37)
        self.assertIsNone(unit_create_fortification_id(body))
        self.assertIsNone(unit_create_fortification_id(body[:14]))

    def test_cuts_the_duplicated_summary_at_the_envelope_clock_reset(self):
        def envelope(time_seconds):
            return (
                struct.pack("<II", name_hash("Event"), 0x00000015)
                + b"\x06"
                + struct.pack("<IfI", 21, time_seconds, name_hash("UnitCreate"))
            )

        primary = b"".join(envelope(float(index)) for index in range(6))
        summary = b"".join(envelope(float(index)) for index in range(6))

        self.assertEqual(primary_chain_end_offset(primary + summary), len(primary))
        self.assertEqual(primary_chain_end_offset(primary), len(primary))

    def test_decodes_compact_unit_remove_boolean_field_exactly(self):
        field_hash = name_hash("aIsToBeReplacedBySpecialistFlag")
        encoded = struct.pack("<IIBIB", field_hash, 14, 3, 14, 1)

        self.assertIs(decode_trailing_bool_field(encoded, field_hash), True)
        self.assertIs(
            decode_trailing_bool_field(encoded[:-1] + b"\x00", field_hash), False
        )
        self.assertIsNone(decode_trailing_bool_field(encoded, field_hash + 1))
        self.assertIsNone(decode_trailing_bool_field(encoded[:-1], field_hash))

    def test_decodes_blink_bool_and_following_float_fields_exactly(self):
        def float_field(name, value):
            return struct.pack("<IIBIf", name_hash(name), 17, 2, 17, value)

        encoded = struct.pack("<IIBIB", name_hash("aBlinkOnOffFlag"), 14, 3, 14, 1)
        encoded += float_field("aBlinkTime", 6.0)
        encoded += float_field("aBlinkFreq", 0.5)

        self.assertEqual(decode_blink_trailing_fields(encoded), (True, 6.0, 0.5))
        self.assertIsNone(decode_blink_trailing_fields(encoded + b"\x00"))

    def test_blower_outcomes_require_the_same_lifecycle_and_remain_candidates(self):
        blower_events = [
            {
                "_rawTime": 10.0,
                "rawEventTime": 10.0,
                "eventIndex": 10,
                "unitId": 7,
                "unitCreationOffset": 100,
                "playerId": 2,
                "team": 1,
            },
            {
                "_rawTime": 20.0,
                "rawEventTime": 20.0,
                "eventIndex": 20,
                "unitId": 7,
                "unitCreationOffset": 200,
                "playerId": 3,
                "team": 2,
            },
        ]
        terminals = {
            100: {
                "reason": "UnitDestroy",
                "eventIndex": 15,
                "rawEventTime": 10.72,
            },
            200: {
                "reason": "UnitRemove",
                "eventIndex": 25,
                "rawEventTime": 20.5,
            },
        }
        destructions = [
            {
                "victimCreation": {"offset": 100},
                "killerUnitId": KILLER_UNIT_SENTINEL,
                "killerPlayerId": None,
            }
        ]

        metrics, outcomes = analyse_blower_lifecycles(
            blower_events, terminals, destructions
        )

        self.assertEqual(metrics["eventsWithActiveLifecycle"], 2)
        self.assertEqual(metrics["thenUnitDestroy"], 1)
        self.assertEqual(metrics["thenUnitDestroyKiller512"], 1)
        self.assertEqual(metrics["thenUnitRemove"], 1)
        self.assertEqual(metrics["terminalWithin0.75s"], 2)
        self.assertEqual(outcomes[0]["terminalDeltaSeconds"], 0.72)
        self.assertEqual(outcomes[0]["causalAttribution"], "candidateOnly")

    def test_reads_exact_replay_map_and_keeps_bridge_matches_candidate_only(self):
        metadata = bytearray(47)
        metadata.extend(b"maps/ustown1/ustown1.ice\0")
        self.assertEqual(replay_map_name(bytes(metadata)), "maps/ustown1/ustown1.ice")

        destruction = {
            "_rawTime": 10.0,
            "eventIndex": 20,
            "victimPositionAtDestruction": [100.0, 9.0, 100.0],
            "victimLastFrame": {
                "rawEventTime": 10.0,
                "rawTimeBits": struct.unpack("<I", struct.pack("<f", 10.0))[0],
            },
            "syntheticTerminalDirection": True,
        }
        bridge = {
            "_rawTime": 10.1,
            "rawEventTime": 10.1,
            "eventIndex": 25,
            "position": [103.0, 2.0, 104.0],
            "killBoundsXZ": [95.0, 95.0, 105.0, 105.0],
            "state": 3,
        }
        candidates = bridge_destruction_candidates(
            destruction, [bridge], window_seconds=0.11
        )

        self.assertEqual(candidates[0]["horizontalDistanceFromBridge"], 5.0)
        self.assertIs(candidates[0]["insideExactServerKillBounds"], True)
        self.assertIs(candidates[0]["victimLastFrameSameRawTick"], True)
        self.assertIs(candidates[0]["syntheticTerminalDirection"], True)
        self.assertEqual(candidates[0]["eventIndexDeltaFromBridgeDestruction"], -5)
        self.assertEqual(candidates[0]["causalAttribution"], "candidateOnly")
        self.assertEqual(
            bridge_destruction_candidates(destruction, [bridge], window_seconds=0.05),
            [],
        )

    def test_blink_outcomes_keep_removal_and_lethal_destruction_distinct(self):
        blinks = [
            {
                "_rawTime": 10.0,
                "rawEventTime": 10.0,
                "eventIndex": 10,
                "unitCreationOffset": 100,
                "blinkOn": True,
                "blinkTimeSeconds": 6.0,
            },
            {
                "_rawTime": 20.0,
                "rawEventTime": 20.0,
                "eventIndex": 20,
                "unitCreationOffset": 200,
                "blinkOn": True,
                "blinkTimeSeconds": 6.0,
            },
            {
                "_rawTime": 30.0,
                "rawEventTime": 30.0,
                "eventIndex": 30,
                "unitCreationOffset": 300,
                "blinkOn": False,
                "blinkTimeSeconds": 6.0,
            },
        ]
        terminals = {
            100: {
                "reason": "UnitRemove",
                "eventIndex": 16,
                "rawEventTime": 16.0,
            },
            200: {
                "reason": "UnitDestroy",
                "eventIndex": 22,
                "rawEventTime": 22.0,
            },
        }
        destructions = [
            {
                "victimCreation": {"offset": 200},
                "killerUnitId": KILLER_UNIT_SENTINEL,
                "killerPlayerId": None,
            }
        ]

        metrics, outcomes = analyse_blink_lifecycles(blinks, terminals, destructions)

        self.assertEqual(metrics["onThenUnitRemove"], 1)
        self.assertEqual(metrics["onThenUnitRemoveAtOrAfterAdvertisedTime"], 1)
        self.assertEqual(metrics["onThenUnitDestroy"], 1)
        self.assertEqual(metrics["onThenUnitDestroyBeforeAdvertisedTime"], 1)
        self.assertEqual(metrics["onThenUnitDestroyKiller512"], 1)
        self.assertEqual(metrics["offEvents"], 1)
        self.assertEqual(
            [row["terminal"]["reason"] for row in outcomes],
            ["UnitRemove", "UnitDestroy"],
        )

    def test_score_and_honors_helpers_retain_only_bounded_positive_candidates(self):
        scores = [
            {"_rawTime": 9.5, "eventIndex": 5, "playerId": 1, "delta": 10},
            {"_rawTime": 10.0, "eventIndex": 9, "playerId": 2, "delta": 5},
            {"_rawTime": 10.0, "eventIndex": 11, "playerId": 3, "delta": 5},
            {"_rawTime": 10.0, "eventIndex": 12, "playerId": 4, "delta": 0},
            {"_rawTime": 10.0, "eventIndex": 13, "playerId": None, "delta": 5},
        ]
        windows = positive_score_candidate_windows(scores, 10.0, 10)

        self.assertEqual(
            [event["playerId"] for event in windows["sameTickBefore"]], [2]
        )
        self.assertEqual([event["playerId"] for event in windows["sameTickAfter"]], [3])
        self.assertEqual(
            [event["playerId"] for event in windows["preceding0.25s"]], [2]
        )
        self.assertEqual(
            [event["playerId"] for event in windows["preceding1s"]], [1, 2]
        )

        honors = [
            {"_rawTime": 10.0, "eventIndex": 4, "delta": 1.0},
            {"_rawTime": 10.0, "eventIndex": 8, "delta": -1.0},
            {"_rawTime": 10.0, "eventIndex": 9, "delta": 2.0},
            {"_rawTime": 10.0, "eventIndex": 11, "delta": 3.0},
        ]
        self.assertEqual(preceding_positive_honors_gap(honors, 10.0, 10), 1)
        self.assertIsNone(preceding_positive_honors_gap(honors, 11.0, 10))

    def test_exact_target_normal_actors_require_target_lifecycle_and_prior_order(self):
        destruction = {
            "_rawTime": 10.0,
            "eventIndex": 20,
            "unitId": 41,
            "victimCreation": {"offset": 100},
        }
        projectile = {
            "_rawTime": 9.75,
            "eventIndex": 15,
            "targetUnitId": 41,
            "targetUnitCreationOffset": 100,
            "firingPlayerId": 3,
            "firingTeam": 1,
        }

        self.assertEqual(
            exact_target_normal_actors(destruction, [projectile]), {(3, 1)}
        )
        self.assertEqual(
            exact_target_normal_actors(
                destruction, [{**projectile, "targetUnitCreationOffset": 99}]
            ),
            set(),
        )
        self.assertEqual(
            exact_target_normal_actors(
                destruction, [{**projectile, "eventIndex": 21, "_rawTime": 10.0}]
            ),
            set(),
        )

    def test_support_impact_causes_require_order_projection_and_one_team(self):
        projectile = {
            "_rawTime": 8.0,
            "eventIndex": 8,
            "supportId": 0x11111111,
            "position": [1.0, 2.0, 3.0],
            "vector": [2.0, 0.0, -2.0],
        }
        explosion = {
            "_rawTime": 9.0,
            "eventIndex": 9,
            "position": [3.0, 2.0, 1.0],
        }
        deployment = {
            "_rawTime": 7.0,
            "eventIndex": 7,
            "supportId": 0x11111111,
            "team": 2,
        }

        self.assertEqual(
            support_impact_causes(
                [projectile], [explosion], [deployment], {0x11111111}, 0.0
            ),
            {(0x11111111, 2)},
        )
        self.assertEqual(
            support_impact_causes(
                [projectile],
                [explosion],
                [deployment, {**deployment, "eventIndex": 6, "team": 1}],
                {0x11111111},
                0.0,
            ),
            set(),
        )

    def test_blast_candidates_require_prior_order_and_radius_containment(self):
        explosions = [
            {"eventIndex": 9, "distanceToVictim": 5.0, "radius": 5.0},
            {"eventIndex": 10, "distanceToVictim": 1.0, "radius": 5.0},
            {"eventIndex": 8, "distanceToVictim": 5.1, "radius": 5.0},
        ]

        self.assertEqual(
            preceding_same_tick_blast_candidates(explosions, 10), [explosions[0]]
        )

    def test_exact_tactical_aid_cause_requires_target_lifecycle_and_one_team(self):
        destruction = {
            "_rawTime": 10.0,
            "eventIndex": 20,
            "unitId": 41,
            "killerUnitId": KILLER_UNIT_SENTINEL,
            "victimCreation": {"offset": 100},
        }
        projectile = {
            "_rawTime": 9.0,
            "eventIndex": 15,
            "supportId": 0x11111111,
            "targetUnitId": 41,
            "targetUnitCreationOffset": 100,
        }
        deployment = {
            "_rawTime": 8.0,
            "eventIndex": 10,
            "supportId": 0x11111111,
            "team": 3,
        }

        self.assertEqual(
            exact_tactical_aid_cause(
                destruction, [projectile], [deployment], {0x11111111}
            ),
            (0x11111111, 3),
        )
        conflicting = {**deployment, "eventIndex": 11, "team": 1}
        self.assertIsNone(
            exact_tactical_aid_cause(
                destruction,
                [projectile],
                [deployment, conflicting],
                {0x11111111},
            )
        )
        self.assertIsNone(
            exact_tactical_aid_cause(
                destruction,
                [{**projectile, "targetUnitCreationOffset": 99}],
                [deployment],
                {0x11111111},
            )
        )

    def test_multi_player_clusters_count_every_death_and_distinct_player(self):
        deaths = [
            {"_rawTime": 1.0, "eventIndex": 1, "victimPlayerId": 2},
            {"_rawTime": 1.1, "eventIndex": 2, "victimPlayerId": 2},
            {"_rawTime": 1.2, "eventIndex": 3, "victimPlayerId": 7},
            {"_rawTime": 4.0, "eventIndex": 4, "victimPlayerId": 9},
        ]

        self.assertEqual(cluster_multi_player_deaths(deaths, 0.25), (1, 3, 2))
        self.assertEqual(cluster_multi_player_deaths(deaths, 0.05), (0, 0, 0))

    def test_projects_a_projectile_without_promoting_the_ballistics_hypothesis(self):
        event = {
            "_rawTime": 9.5,
            "rawEventTime": 9.5,
            "offset": 1,
            "position": [1.0, 2.0, 3.0],
            "vector": [2.0, 0.0, -2.0],
        }
        projected = projectile_spatial_event(event, 10.0, [2.0, 2.0, 2.0])

        self.assertEqual(projected["constantVectorProjectedPosition"], [2.0, 2.0, 2.0])
        self.assertEqual(projected["constantVectorDistanceToVictim"], 0.0)
        self.assertEqual(
            projected["constantVectorSemantics"],
            "researchHypothesisNotValidatedBallistics",
        )

    def test_decodes_compact_and_full_unit_frame_identity_and_position(self):
        def wrapped(field_name, payload):
            total = 13 + len(payload)
            return (
                struct.pack("<II", name_hash(field_name), total)
                + b"\x06"
                + struct.pack("<I", total)
                + payload
            )

        compact_header = 95 | (2 << 12)
        compact_payload = struct.pack(
            "<HHHHHHHH", compact_header, 420, 840, 1260, 0, 0, 0, 0
        ) + (b"\0" * 10)
        compact = wrapped("UnitFrameDataCompact", compact_payload)
        envelope = SimpleNamespace(body_start=0, body_end=len(compact))
        self.assertEqual(
            decode_unit_frame(compact, envelope),
            {
                "encoding": "compact",
                "unitId": 95,
                "position": [10.0, 20.0, 30.0],
                "childCount": 2,
            },
        )

        full_payload = (
            struct.pack("<fffffffH", 1.5, 2.5, 3.5, 0.0, 0.0, 0.0, 1.0, 41) + b"\0"
        )
        full = wrapped("UnitFrameData", full_payload)
        envelope = SimpleNamespace(body_start=0, body_end=len(full))
        self.assertEqual(
            decode_unit_frame(full, envelope),
            {
                "encoding": "full",
                "unitId": 41,
                "position": [1.5, 2.5, 3.5],
                "childCount": 0,
            },
        )

    def test_links_exact_taunt_and_same_tick_victim_only_as_candidate(self):
        catalogue = [0x11111111, 0x22222222]
        taunts = [
            {
                "rawEventTime": 12.5,
                "offset": 100,
                "playerFrom": 3,
                "playerTaunted": 7,
                "taIndex": 1,
                "supportUpgradeLevel": 2,
            }
        ]
        destructions = [
            {"rawEventTime": 12.5, "victimPlayerId": 7, "unitId": 41},
            {"rawEventTime": 12.5, "victimPlayerId": 8, "unitId": 42},
            {"rawEventTime": 12.6, "victimPlayerId": 7, "unitId": 43},
        ]

        linked = link_taunts(
            catalogue,
            taunts,
            destructions,
            {0x22222222: "Tankbuster_NATO"},
        )

        self.assertEqual(linked[0]["supportId"], 0x22222222)
        self.assertEqual(linked[0]["supportName"], "Tankbuster_NATO")
        self.assertEqual(linked[0]["attribution"]["class"], "exact")
        self.assertEqual(
            linked[0]["attribution"]["scope"], "damageThresholdNotUnitKill"
        )
        self.assertEqual(
            linked[0]["destructionCandidateBasis"],
            "sameRawTimestampAndVictimPlayer",
        )
        self.assertEqual(
            [item["unitId"] for item in linked[0]["sameTickVictimDestructions"]],
            [41],
        )
        self.assertEqual(linked[0]["destructionAttribution"], "candidate")

    def test_rejects_ta_index_outside_replay_catalogue(self):
        linked = link_taunts(
            [0x11111111],
            [
                {
                    "rawEventTime": 1.0,
                    "offset": 10,
                    "playerFrom": 1,
                    "playerTaunted": 2,
                    "taIndex": 9,
                    "supportUpgradeLevel": 1,
                }
            ],
            [],
            {},
        )

        self.assertIsNone(linked[0]["supportId"])
        self.assertEqual(linked[0]["attribution"]["class"], "invalid")
        self.assertEqual(linked[0]["destructionAttribution"], "none")

    def test_summary_uses_compact_support_counts_without_event_rows(self):
        summary = summarise(
            [
                {
                    "metrics": {"sendTATaunts": 2},
                    "bySupportId": {
                        "0x22222222": {
                            "taunts": 2,
                            "withSameTickVictimDestruction": 1,
                            "sameTickVictimDestructions": 3,
                        }
                    },
                }
            ]
        )

        self.assertEqual(summary["sendTATaunts"], 2)
        self.assertEqual(summary["bySupportId"]["0x22222222"]["taunts"], 2)
        self.assertEqual(
            summary["bySupportId"]["0x22222222"]["sameTickVictimDestructions"],
            3,
        )

    def test_clock_timeline_matches_schema_v10_interpolation_and_jitter(self):
        samples = build_clock_samples(
            [(100, 1200.0), (200, 1199.0), (300, 1199.2), (400, 1198.0)]
        )

        self.assertEqual([sample.elapsed_seconds for sample in samples], [0, 1, 1, 2])
        self.assertEqual(timeline_time_at_offset(samples, 50), 0)
        self.assertEqual(timeline_time_at_offset(samples, 150), 0.5)
        self.assertEqual(timeline_time_at_offset(samples, 250), 1)
        self.assertEqual(timeline_time_at_offset(samples, 450), 2)

    def test_clock_reset_begins_a_new_phase(self):
        samples = build_clock_samples(
            [(10, 600.0), (20, 590.0), (30, 900.0), (40, 890.0)]
        )

        self.assertEqual(
            [sample.elapsed_seconds for sample in samples], [0, 10, 10, 20]
        )

    def test_temporal_window_includes_boundaries_and_order_tolerance(self):
        events = [
            {"_rawTime": 9.5, "offset": 1},
            {"_rawTime": 10.0, "offset": 2},
            {"_rawTime": 10.25, "offset": 3},
            {"_rawTime": 10.251, "offset": 4},
        ]

        selected = events_in_window(events, death_time=10.0, lookback=0.5)

        self.assertEqual([event["offset"] for event in selected], [1, 2, 3])

    def test_sentinel_never_resolves_through_historical_owner(self):
        destruction = {
            "killerUnitId": KILLER_UNIT_SENTINEL,
            "killerPlayerId": None,
            "historicalKillerPlayerIds": [7],
        }

        self.assertEqual(classify_destruction(destruction, 0, []), "noCandidate")
        self.assertEqual(classify_destruction(destruction, 1, []), "candidateOnly")
        self.assertEqual(classify_destruction(destruction, 2, []), "ambiguous")

    def test_active_and_historical_killers_remain_distinct(self):
        active = {
            "killerUnitId": 41,
            "killerPlayerId": 3,
            "historicalKillerPlayerIds": [3],
        }
        historical = {
            "killerUnitId": 41,
            "killerPlayerId": None,
            "historicalKillerPlayerIds": [3],
        }

        self.assertEqual(classify_destruction(active, 9, []), "exactActiveKillerUnit")
        self.assertEqual(
            classify_destruction(historical, 0, []), "historicalOwnerCandidate"
        )

    def test_historical_killer_projectiles_preserve_evidence_strength(self):
        destruction = {
            "unitId": 95,
            "killerUnitId": 41,
            "killerPlayerId": None,
            "historicalKillerPlayerIds": [3],
        }

        self.assertEqual(
            classify_destruction(destruction, 1, [{"firingUnitId": 41}]),
            "historicalOwnerProjectileCandidate",
        )
        self.assertEqual(
            classify_destruction(
                destruction,
                1,
                [{"firingUnitId": 41, "targetUnitId": 95}],
            ),
            "historicalOwnerExactTargetProjectileCandidate",
        )


if __name__ == "__main__":
    unittest.main()
