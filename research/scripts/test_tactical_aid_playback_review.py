import unittest

from tactical_aid_playback_review import player_name_at, projection_summary


class TacticalAidPlaybackReviewTests(unittest.TestCase):
    def test_player_name_resolution_prefers_time_bounded_sessions(self):
        timeline = {
            "participants": [{"playerId": 1, "playerName": "JonnySky"}],
            "participantSessions": [
                {
                    "playerId": 1,
                    "sessionIndex": 0,
                    "playerName": "JonnySky",
                    "startSeconds": 0.0,
                    "endSeconds": 10.0,
                },
                {
                    "playerId": 1,
                    "sessionIndex": 1,
                    "playerName": "LtDan73",
                    "startSeconds": 20.0,
                    "endSeconds": None,
                },
            ],
        }

        self.assertEqual(player_name_at(timeline, 1, 5.0), "JonnySky")
        self.assertIsNone(player_name_at(timeline, 1, 15.0))
        self.assertEqual(player_name_at(timeline, 1, 402.673), "LtDan73")

    def test_projection_pairs_equal_groups_and_keeps_marker_only_rows(self):
        marker = {
            "type": "tacticalAidMarker",
            "supportId": 1,
            "supportName": "Aid_US",
            "position": [1.0, 2.0, 3.0],
            "playerId": 4,
        }
        deployment = {
            "type": "tacticalAidDeployed",
            "supportId": 1,
            "position": [1.0, 2.0, 3.0],
            "playerId": None,
            "playerAttribution": None,
        }
        marker_only = {
            "type": "tacticalAidMarker",
            "supportId": 2,
            "supportName": "Other_US",
            "position": [4.0, 5.0, 6.0],
            "playerId": 5,
        }
        document = {"timeline": {"events": [marker, deployment, marker_only]}}

        summary = projection_summary(document)

        self.assertEqual(summary["projectedRows"], 2)
        self.assertEqual(summary["exactPlayerRows"], 2)
        self.assertEqual(summary["teamOnlyRows"], 0)

    def test_unequal_group_remains_deployment_only_and_team_only(self):
        marker = {
            "type": "tacticalAidMarker",
            "supportId": 1,
            "supportName": "Aid_US",
            "position": [1.0, 2.0, 3.0],
            "playerId": 4,
        }
        deployments = [
            {
                "type": "tacticalAidDeployed",
                "supportId": 1,
                "position": [1.0, 2.0, 3.0],
                "playerId": None,
                "playerAttribution": None,
            }
            for _ in range(2)
        ]

        summary = projection_summary({"timeline": {"events": [marker, *deployments]}})

        self.assertEqual(summary["projectedRows"], 2)
        self.assertEqual(summary["exactPlayerRows"], 0)
        self.assertEqual(summary["teamOnlyRows"], 2)

    def test_unit_spawn_ownership_is_exact_on_deployment_row(self):
        deployment = {
            "type": "tacticalAidDeployed",
            "supportId": 1,
            "position": [1.0, 2.0, 3.0],
            "playerId": 7,
            "playerAttribution": "unitSpawnOwnership",
        }

        summary = projection_summary({"timeline": {"events": [deployment]}})

        self.assertEqual(summary["exactPlayerRows"], 1)
        self.assertEqual(summary["teamOnlyRows"], 0)


if __name__ == "__main__":
    unittest.main()
