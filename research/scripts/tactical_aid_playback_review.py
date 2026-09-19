#!/usr/bin/env python3
"""Prepare and validate the tactical-aid manual playback checklist."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import struct
import subprocess

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_PARSER = REPO_ROOT / "target/release/wic_replay_parser"
DEFAULT_CHECKLIST = (
    REPO_ROOT / "research/findings/tactical-aid-manual-review-2026-08-17.md"
)
DEFAULT_REPLAYS = {
    "A": REPO_ROOT / "local/replays/main/WicTracker/downloads/1000__demo07.wicdemo",
    "B": REPO_ROOT / "local/replays/main/WicTracker/downloads/744__demo08.wicdemo",
}
EXPECTED_PROJECTED_ROWS = {"A": 47, "B": 111}
ROW_PATTERN = re.compile(r"^\| \[(?P<status>.)\] \| (?P<key>[AB]) \|")


def _float_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def _effect_key(event: dict) -> tuple[int, int, int, int]:
    return (
        event["supportId"],
        *(_float_bits(value) for value in event["position"]),
    )


def parse_checklist(path: pathlib.Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text().splitlines(), 1):
        match = ROW_PATTERN.match(line)
        if not match:
            continue
        columns = [column.strip() for column in line.strip().strip("|").split("|")]
        time_seconds = float(columns[2].split()[0])
        player_match = re.fullmatch(r"`(.+)` \((\d+)\)", columns[3])
        if player_match is None:
            raise ValueError(f"invalid player column at {path}:{line_number}")
        rows.append(
            {
                "line": line_number,
                "status": columns[0][1],
                "replay": columns[1],
                "timeSeconds": time_seconds,
                "seekHint": columns[2].split("`", 2)[1],
                "playerName": player_match.group(1),
                "playerId": int(player_match.group(2)),
                "supportIdHex": columns[4].strip("`"),
                "supportId": int(columns[4].strip("`"), 16),
                "supportName": columns[5].strip("`"),
                "viewerLabel": columns[6],
            }
        )
    if len(rows) != 18:
        raise ValueError(f"expected 18 family rows in {path}, found {len(rows)}")
    return rows


def parse_replay(parser_path: pathlib.Path, replay_path: pathlib.Path) -> dict:
    completed = subprocess.run(
        [str(parser_path), "--timeline-json", str(replay_path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout)


def projection_summary(document: dict) -> dict:
    events = document["timeline"]["events"]
    grouped_markers: dict[tuple, list[dict]] = {}
    grouped_deployments: dict[tuple, list[dict]] = {}
    for event in events:
        if event["type"] == "tacticalAidMarker" and event.get("supportName"):
            grouped_markers.setdefault(_effect_key(event), []).append(event)
        elif event["type"] == "tacticalAidDeployed":
            grouped_deployments.setdefault(_effect_key(event), []).append(event)

    projected = []
    for key in grouped_markers.keys() | grouped_deployments.keys():
        markers = grouped_markers.get(key, [])
        deployments = grouped_deployments.get(key, [])
        if markers and len(markers) == len(deployments):
            projected.extend({"source": "pairedMarker", **event} for event in markers)
        elif deployments:
            projected.extend({"source": "deployment", **event} for event in deployments)
        else:
            projected.extend({"source": "markerOnly", **event} for event in markers)

    exact_rows = sum(
        event["source"] != "deployment"
        or event.get("playerAttribution") == "unitSpawnOwnership"
        for event in projected
    )
    return {
        "projectedRows": len(projected),
        "exactPlayerRows": exact_rows,
        "teamOnlyRows": len(projected) - exact_rows,
        "recorderPurchases": sum(
            event["type"] == "tacticalAidUsed" for event in events
        ),
        "deployments": sum(event["type"] == "tacticalAidDeployed" for event in events),
        "namedMarkers": sum(
            event["type"] == "tacticalAidMarker"
            and event.get("supportName") is not None
            for event in events
        ),
        "unnamedMarkersExcluded": sum(
            event["type"] == "tacticalAidMarker" and event.get("supportName") is None
            for event in events
        ),
    }


def player_name_at(timeline: dict, player_id: int, time_seconds: float) -> str | None:
    """Resolve one slot at an event time without leaking a previous occupant."""
    sessions = [
        session
        for session in timeline.get("participantSessions", [])
        if session["playerId"] == player_id
    ]
    if sessions:
        active = [
            session
            for session in sessions
            if session["startSeconds"] <= time_seconds
            and (session["endSeconds"] is None or time_seconds < session["endSeconds"])
        ]
        if not active:
            return None
        return max(active, key=lambda session: session["sessionIndex"])["playerName"]
    return next(
        (
            participant["playerName"]
            for participant in timeline["participants"]
            if participant["playerId"] == player_id
        ),
        None,
    )


def validate_rows(rows: list[dict], documents: dict[str, dict]) -> list[dict]:
    evidence = []
    for row in rows:
        document = documents[row["replay"]]
        timeline = document["timeline"]
        candidates = [
            event
            for event in timeline["events"]
            if event["type"] == "tacticalAidMarker"
            and abs(event["timeSeconds"] - row["timeSeconds"]) <= 0.001
            and event["supportId"] == row["supportId"]
            and event["playerId"] == row["playerId"]
        ]
        if len(candidates) != 1:
            raise ValueError(
                f"checklist line {row['line']} matched {len(candidates)} marker records"
            )
        marker = candidates[0]
        if marker["supportName"] != row["supportName"]:
            raise ValueError(f"support name mismatch at checklist line {row['line']}")
        resolved_name = player_name_at(timeline, row["playerId"], marker["timeSeconds"])
        if resolved_name != row["playerName"]:
            raise ValueError(
                f"participant name mismatch at checklist line {row['line']}: "
                f"expected {row['playerName']!r}, resolved {resolved_name!r}"
            )
        evidence.append(
            {
                **row,
                "serializedMarker": marker,
                "automatedStatus": "passed",
                "visualStatus": ("pending" if row["status"] == " " else row["status"]),
            }
        )
    return evidence


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--parser", type=pathlib.Path, default=DEFAULT_PARSER)
    parser.add_argument("--checklist", type=pathlib.Path, default=DEFAULT_CHECKLIST)
    parser.add_argument("--replay-a", type=pathlib.Path, default=DEFAULT_REPLAYS["A"])
    parser.add_argument("--replay-b", type=pathlib.Path, default=DEFAULT_REPLAYS["B"])
    parser.add_argument("--json", type=pathlib.Path)
    args = parser.parse_args()

    if not args.parser.is_file():
        parser.error(f"parser binary is unavailable: {args.parser}")
    replay_paths = {"A": args.replay_a, "B": args.replay_b}
    documents = {
        key: parse_replay(args.parser, replay_path)
        for key, replay_path in replay_paths.items()
    }
    summaries = {
        key: projection_summary(document) for key, document in documents.items()
    }
    for key, expected_rows in EXPECTED_PROJECTED_ROWS.items():
        actual = summaries[key]["projectedRows"]
        if actual != expected_rows:
            raise ValueError(
                f"replay {key} projects {actual} rows, expected {expected_rows}"
            )

    report = {
        "schemaVersion": 1,
        "timelineSchemaVersions": {
            key: document["timeline"]["schemaVersion"]
            for key, document in documents.items()
        },
        "replays": {
            key: {"path": str(path), **summaries[key]}
            for key, path in replay_paths.items()
        },
        "familyChecks": validate_rows(parse_checklist(args.checklist), documents),
        "automatedStatus": "passed",
        "visualStatus": "pendingUserPlayback",
    }
    rendered = json.dumps(report, indent=2) + "\n"
    print(rendered, end="")
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(rendered)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
