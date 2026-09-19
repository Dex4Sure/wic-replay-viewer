"""Synthetic controls for the comparison-only roster policy."""

import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "comparison", Path(__file__).with_name("analyze-event-rosters-2026-09-08.py")
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def session(name="Alpha", start=0, end=None, team=1, activity=True):
    return dict(
        slot=0,
        name=name,
        start=start,
        end=end,
        startTime=0,
        endTime=None,
        explicitEntry=True,
        lastScore=[500, 5, 100],
        segments=[
            dict(
                team=team,
                start=start,
                end=end,
                source="joined",
                units=int(activity),
                roles=0,
                nonzeroScores=int(activity),
                unitTeams=[team] if activity else [],
                firstActivity=200 if activity else None,
                lastActivity=500 if activity else None,
            )
        ],
    )


def data(sessions, name="Alpha", score=100, captured=True):
    return dict(
        current=[
            dict(
                id=0,
                name=name,
                team=1,
                faction="USA",
                score=score,
                role="air" if score else None,
            )
        ],
        start=100,
        result=[1000, 10],
        sessions=sessions,
        raw=dict(timing=dict(capturedMatchStart=captured)),
    )


class ComparisonTests(unittest.TestCase):
    def test_departure_is_not_spectating(self):
        r = m.compare(data([session(end=800)]))
        self.assertEqual(r["snapshot"], [])
        self.assertEqual(r["participants"], [(0, "Alpha", 1)])
        self.assertEqual(r["candidates"], [])

    def test_reused_slot_is_not_merged_or_automatically_renamed(self):
        r = m.compare(data([session(end=500), session("Beta", start=600)]))
        self.assertEqual(len(r["participants"]), 2)
        self.assertEqual(r["candidates"], [])
        self.assertEqual(r["reasons"]["multiple gameplay occupants in one slot"], 1)

    def test_late_capture_abstains_from_corrections(self):
        r = m.compare(data([session("Beta")], captured=False))
        self.assertEqual(r["candidates"], [])

    def test_missing_team_is_not_invented(self):
        s = session(team=None)
        s["segments"][0]["unitTeams"] = [1]
        r = m.compare(data([s]))
        self.assertEqual(r["snapshot"], [])
        self.assertEqual(r["participants"], [])
        self.assertEqual(r["candidates"], [])

    def test_no_activity_alone_does_not_make_spectator(self):
        r = m.compare(data([session(activity=False)], score=0))
        self.assertEqual(r["candidates"], [])

    def test_missing_primary_result_blocks_comparison(self):
        d = data([session()])
        d["result"] = None
        self.assertIn("blocked", m.compare(d))


if __name__ == "__main__":
    unittest.main()
