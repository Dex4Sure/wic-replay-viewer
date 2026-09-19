"""Compare baseline Overview against event snapshots and gameplay participants.

Read-only analysis of compare-event-rosters-2026-09-08.rs output. No proposed
changes are applied to the parser, viewer, library database, or raw replays.
"""

import argparse
import collections
import json
from pathlib import Path

BROKEN = "972959c61587f57dd9cce5d7116c4f0e6ac1ed3c3b65fc55ece9b92860ca0447"
PLAYING = (1, 2, 3)


def display_team(player):
    names = {"USA": 1, "NATO": 2, "USSR": 3, "Spectator": 0}
    return names.get(
        player.get("faction"),
        player.get("team") if player.get("team") in PLAYING else 0,
    )


def overlaps(start, end, lo, hi):
    return start < hi and (end is None or end > lo)


def compare(data):
    current = data["current"]
    lo = data["start"]
    result = data["result"]
    if lo is None or result is None:
        return {"blocked": "missing gameplay clock or primary-chain result"}
    hi = result[0]
    if hi <= lo:
        return {"blocked": "result precedes observed gameplay"}
    sessions = []
    for source in data["sessions"]:
        if not overlaps(source["start"], source["end"], lo, hi):
            continue
        segments = [
            s for s in source["segments"] if overlaps(s["start"], s["end"], lo, hi)
        ]
        playing = sorted({s["team"] for s in segments if s["team"] in PLAYING})
        activity = [s for s in segments if s["firstActivity"] is not None]
        units = sum(s["units"] for s in activity)
        roles = sum(s["roles"] for s in activity)
        nonzero = sum(s["nonzeroScores"] for s in activity)
        unit_teams = sorted(
            {t for s in activity for t in s["unitTeams"] if t in PLAYING}
        )
        source = dict(
            source,
            playingTeams=playing,
            units=units,
            roles=roles,
            nonzeroScores=nonzero,
            unitTeams=unit_teams,
            activity=bool(activity),
            gameplaySegments=segments,
        )
        sessions.append(source)
    baseline = sorted(
        (p["id"], p["name"], display_team(p)) for p in current if display_team(p)
    )
    snapshot = []
    participants = []
    for s in sessions:
        if s["end"] is None or s["end"] >= hi:
            team = s["segments"][-1]["team"]
            if team in PLAYING and s["name"]:
                snapshot.append((s["slot"], s["name"], team))
        if s["activity"] and s["name"]:
            for team in s["playingTeams"]:
                participants.append((s["slot"], s["name"], team))
    snapshot = sorted(set(snapshot))
    participants = sorted(set(participants))
    reasons = collections.Counter()
    candidates = []
    per_slot = collections.defaultdict(list)
    for s in sessions:
        per_slot[s["slot"]].append(s)
    # Proposed corrections are separate from the baseline and require full start.
    # They are hypotheses for review, not automatic mutations or ground truth.
    for p in current:
        own = [s for s in per_slot[p["id"]] if s["name"] == p["name"]]
        slot = per_slot[p["id"]]
        if display_team(p) and not own:
            reasons["baseline name absent from gameplay sessions"] += 1
        if display_team(p) and own and not any(s["activity"] for s in own):
            reasons["playing row without observed gameplay activity"] += 1
        if (
            display_team(p)
            and own
            and all(s["end"] is not None and s["end"] < hi for s in own)
        ):
            reasons["departed participant omitted by end snapshot"] += 1
        if display_team(p) and own and all(not s["playingTeams"] for s in own):
            reasons["baseline faction lacks gameplay team event"] += 1
        if any(len(s["playingTeams"]) > 1 for s in slot):
            reasons["multiple playing factions in one session"] += 1
        if len({s["name"] for s in slot if s["activity"]}) > 1:
            reasons["multiple gameplay occupants in one slot"] += 1
        if not data["raw"]["timing"]["capturedMatchStart"]:
            continue
        active = [s for s in slot if s["activity"]]
        # Only compare unique sessions. Reconnects and unnamed sessions abstain.
        if len(slot) != 1 or not slot[0]["name"] or not slot[0]["explicitEntry"]:
            continue
        s = slot[0]
        if s["name"] != p["name"]:
            if (
                active
                and s["units"]
                and len(s["playingTeams"]) == 1
                and s["unitTeams"] == s["playingTeams"]
            ):
                candidates.append(
                    {
                        "kind": "identity-review",
                        "slot": p["id"],
                        "before": p,
                        "proposedName": s["name"],
                        "session": s,
                        "limitation": "final score attribution not established",
                    }
                )
            continue
        if (
            s["units"]
            and len(s["playingTeams"]) == 1
            and s["unitTeams"] == s["playingTeams"]
        ):
            team = s["playingTeams"][0]
            if display_team(p) != team:
                candidates.append(
                    {
                        "kind": "team-review",
                        "slot": p["id"],
                        "before": p,
                        "proposedTeam": team,
                        "session": s,
                        "limitation": "historical participation versus final spectator state requires review",
                    }
                )
        elif (
            display_team(p)
            and not s["activity"]
            and p.get("role") is None
            and all(v == 0 for k, v in p.items() if k.startswith("score"))
            and s["segments"][-1]["team"] == 0
            and s["segments"][-1]["source"] == "spectator"
        ):
            candidates.append(
                {
                    "kind": "spectator-review",
                    "slot": p["id"],
                    "before": p,
                    "proposedTeam": 0,
                    "session": s,
                    "limitation": "no observed activity does not prove absence; inspect pregame units",
                }
            )
    current_keys = {(p["id"], p["name"]) for p in current}
    missing = [
        s
        for s in sessions
        if s["name"]
        and s["activity"]
        and s["playingTeams"]
        and (s["slot"], s["name"]) not in current_keys
    ]
    scores = []
    for slot, name, team in snapshot:
        old = next((p for p in current if p["id"] == slot and p["name"] == name), None)
        ss = [
            s
            for s in sessions
            if s["slot"] == slot
            and s["name"] == name
            and (s["end"] is None or s["end"] >= hi)
        ]
        if old and len(ss) == 1 and ss[0]["lastScore"]:
            observed = ss[0]["lastScore"]
            if old["score"] != observed[2]:
                scores.append(
                    {
                        "slot": slot,
                        "name": name,
                        "overview": old["score"],
                        "lastLive": observed,
                        "hasRecovery": "scoreBeforeLeave" in old,
                    }
                )
    return {
        "baseline": baseline,
        "snapshot": snapshot,
        "participants": participants,
        "snapshotRemoved": sorted(set(baseline) - set(snapshot)),
        "snapshotAdded": sorted(set(snapshot) - set(baseline)),
        "participantsRemoved": sorted(set(baseline) - set(participants)),
        "participantsAdded": sorted(set(participants) - set(baseline)),
        "reasons": dict(reasons),
        "candidates": candidates,
        "missingParticipants": missing,
        "scoreDifferences": scores,
        "lateCapture": not data["raw"]["timing"]["capturedMatchStart"],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    counts = collections.Counter()
    reasons = collections.Counter()
    cases = []
    acceptance_issues = []
    for line in args.input.open():
        row = json.loads(line)
        item = row["input"]
        weight = len(item["aliases"])
        counts["unique inputs"] += 1
        counts["paths"] += weight
        if "error" in row:
            counts["rejected unique"] += 1
            counts["rejected paths"] += weight
            if any(e is None for e in item["expectedErrors"]):
                acceptance_issues.append(item["sha256"])
            continue
        if any(e is not None for e in item["expectedErrors"]):
            acceptance_issues.append(item["sha256"])
        counts["accepted unique"] += 1
        counts["accepted paths"] += weight
        c = compare(row["data"])
        c.update(input=item, knownBroken=item["sha256"] == BROKEN)
        if c["knownBroken"]:
            counts["known broken accepted unique"] += 1
        if "blocked" in c:
            counts["blocked unique"] += 1
        else:
            for key in [
                "snapshotRemoved",
                "snapshotAdded",
                "participantsRemoved",
                "participantsAdded",
                "scoreDifferences",
                "missingParticipants",
            ]:
                counts[key + " rows unique"] += len(c[key])
                if c[key]:
                    counts[key + " replays unique"] += 1
            if c["baseline"] != c["snapshot"]:
                counts["snapshot differs unique"] += 1
            if c["baseline"] != c["participants"]:
                counts["participants differs unique"] += 1
            if c["lateCapture"]:
                counts["late capture unique"] += 1
            reasons.update(c["reasons"])
            for candidate in c["candidates"]:
                counts[candidate["kind"] + " rows unique"] += 1
        cases.append(c)
    result = dict(
        counts=dict(counts),
        reasons=dict(reasons),
        acceptanceIssues=acceptance_issues,
        cases=cases,
    )
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2))
    print(json.dumps({k: v for k, v in result.items() if k != "cases"}, indent=2))


if __name__ == "__main__":
    main()
