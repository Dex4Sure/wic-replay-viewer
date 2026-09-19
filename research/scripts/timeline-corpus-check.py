#!/usr/bin/env python3
"""Run the Rust timeline parser across a replay corpus and summarise the result.

Groups by game mode so the thinly covered Assault and Tug of War maps stay
visible next to the Domination-heavy bulk, and reports how recorder tactical-aid
activations, visible-faction player-attributed markers, and both-faction team-only
deployments land relative to the countdown clock.

    research/scripts/timeline-corpus-check.py local/replays/main --limit 400
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import subprocess
import sys

MANIFEST = "parser/rust_parser/Cargo.toml"


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def run_corpus(binary: str, roots: list[str], jobs: int | None) -> list[dict]:
    command = [binary, "--corpus-summary-json", *roots]
    if jobs is not None:
        command.extend(["--jobs", str(jobs)])
    proc = subprocess.run(
        command,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip() or "Rust corpus parser failed"
        raise RuntimeError(detail)
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError as error:
        raise RuntimeError("Rust corpus parser returned invalid JSON") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+")
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument(
        "--jobs",
        type=positive_int,
        help="native parser workers (default: auto, capped at 4)",
    )
    parser.add_argument(
        "--binary",
        default="target/release/wic_replay_parser",
    )
    parser.add_argument("--json", help="write per-replay rows here")
    args = parser.parse_args()

    if not pathlib.Path(args.binary).is_file():
        print(
            f"build the release binary first: cargo build --release --manifest-path {MANIFEST}",
            file=sys.stderr,
        )
        return 2

    inputs = args.roots
    if args.limit:
        paths: list[pathlib.Path] = []
        seen: set[pathlib.Path] = set()
        for root in args.roots:
            node = pathlib.Path(root)
            candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
            for candidate in candidates:
                resolved = candidate.resolve()
                if resolved not in seen:
                    seen.add(resolved)
                    paths.append(candidate)
        inputs = [str(path) for path in paths[: args.limit]]

    corpus = run_corpus(args.binary, inputs, args.jobs)

    by_mode: dict[str, collections.Counter] = collections.defaultdict(
        collections.Counter
    )
    schema_versions: collections.Counter = collections.Counter()
    coverage_values: collections.Counter = collections.Counter()
    server_names: collections.Counter = collections.Counter()
    unknown_server_years: collections.Counter = collections.Counter()
    chat_channels: collections.Counter = collections.Counter()
    cost_by_id: dict[str, collections.Counter] = collections.defaultdict(
        collections.Counter
    )
    rows = []
    rejected = 0

    for item in corpus:
        if item["status"] == "rejected":
            rejected += 1
            continue
        mode = item["mode"]
        schema_versions[item["schemaVersion"]] += 1
        server_names[item["serverName"]] += 1
        if item["serverName"] == "Unknown":
            date_parts = item["dateTime"].split("-")
            year = date_parts[1].strip() if len(date_parts) > 1 else "Unknown"
            unknown_server_years[year] += 1
        coverage_values[
            (
                item["coverageChat"],
                item["coverageTacticalAid"],
                item["coverageTacticalAidMarkers"],
                item["coverageTacticalAidDeployments"],
            )
        ] += 1
        stats = by_mode[mode]
        stats["replays"] += 1
        stats["ta_uses"] += item["tacticalAidUses"]
        stats["ta_markers"] += item["tacticalAidMarkers"]
        stats["ta_markers_with_invalid_player"] += item[
            "tacticalAidMarkersWithInvalidPlayer"
        ]
        stats["ta_markers_without_participant_name"] += item[
            "tacticalAidMarkersWithoutParticipantName"
        ]
        stats["ta_markers_past_duration"] += item["tacticalAidMarkersPastDuration"]
        stats["ta_deployments"] += item["tacticalAidDeployments"]
        stats["ta_deployments_with_player"] += item["tacticalAidDeploymentsWithPlayer"]
        stats["ta_deployments_with_invalid_player"] += item[
            "tacticalAidDeploymentsWithInvalidPlayer"
        ]
        stats["ta_deployments_without_participant_name"] += item[
            "tacticalAidDeploymentsWithoutParticipantName"
        ]
        stats["ta_deployments_with_unit_spawn_attribution"] += item[
            "tacticalAidDeploymentsWithUnitSpawnAttribution"
        ]
        stats["ta_deployments_with_invalid_team"] += item[
            "tacticalAidDeploymentsWithInvalidTeam"
        ]
        stats["ta_deployments_past_duration"] += item[
            "tacticalAidDeploymentsPastDuration"
        ]
        stats["spectator_view_changes"] += item["spectatorViewChanges"]
        stats["spectator_view_changes_with_unknown_los"] += item[
            "spectatorViewChangesWithUnknownLos"
        ]
        stats["timeline_participants"] += item["timelineParticipants"]
        stats["timeline_participants_without_name"] += item[
            "timelineParticipantsWithoutName"
        ]
        stats["timeline_duplicate_participants"] += item[
            "timelineDuplicateParticipants"
        ]
        stats["timeline_player_references_without_participant"] += item[
            "timelinePlayerReferencesWithoutParticipant"
        ]
        stats["timeline_participant_name_conflicts"] += item[
            "timelineParticipantNameConflicts"
        ]
        stats["chat_participant_name_conflicts"] += item["chatParticipantNameConflicts"]
        stats["chat_messages"] += item["chatMessages"]
        stats["pre_match_chat_messages"] += item["preMatchChatMessages"]
        stats["post_match_chat_messages"] += item["postMatchChatMessages"]
        if item["tacticalAidUses"]:
            stats["replays_with_ta"] += 1
        if item["tacticalAidMarkers"]:
            stats["replays_with_ta_markers"] += 1
        if item["chatMessages"]:
            stats["replays_with_chat"] += 1
        if item["preMatchChatMessages"]:
            stats["replays_with_pre_match_chat"] += 1
        if item["postMatchChatMessages"]:
            stats["replays_with_post_match_chat"] += 1
        chat_channels["all"] += item["chatAll"]
        chat_channels["team"] += item["chatTeam"]
        stats["chat_messages_with_invalid_player"] += item[
            "chatMessagesWithInvalidPlayer"
        ]
        stats["chat_messages_without_player_name"] += item[
            "chatMessagesWithoutPlayerName"
        ]
        stats["chat_player_names_outside_replay_roster"] += item[
            "chatPlayerNamesOutsideReplayRoster"
        ]
        stats["chat_player_name_conflicts"] += item["chatPlayerNameConflicts"]
        stats["chat_messages_past_duration"] += item["chatMessagesPastDuration"]
        if item["tacticalAidSummaryTotal"] != item["tacticalAidUses"]:
            stats["ta_summary_total_mismatches"] += 1
        if item["tacticalAidSummarySupportTotal"] != item["tacticalAidUses"]:
            stats["ta_summary_support_mismatches"] += 1
        stats["uses_without_player"] += item["tacticalAidUsesWithoutPlayer"]
        stats["uses_at_time_zero"] += item["tacticalAidUsesAtTimeZero"]
        stats["uses_past_duration"] += item["tacticalAidUsesPastDuration"]
        stats["priced_uses_at_world_origin"] += item["tacticalAidUsesAtWorldOrigin"]
        for support in item["supports"]:
            for cost in support["observedCosts"]:
                cost_by_id[f"{support['supportId']:08x}"][cost] += 1
        rows.append(
            {
                "path": item["path"],
                "mode": mode,
                "server_name": item["serverName"],
                "date_time": item["dateTime"],
                "schema_version": item["schemaVersion"],
                "duration": item["durationSeconds"],
                "phases": item["phases"],
                "recorder": item["recorder"],
                "ta_uses": item["tacticalAidUses"],
                "ta_markers": item["tacticalAidMarkers"],
                "ta_markers_with_invalid_player": item[
                    "tacticalAidMarkersWithInvalidPlayer"
                ],
                "ta_markers_without_participant_name": item[
                    "tacticalAidMarkersWithoutParticipantName"
                ],
                "ta_markers_past_duration": item["tacticalAidMarkersPastDuration"],
                "ta_deployments": item["tacticalAidDeployments"],
                "ta_deployments_with_player": item["tacticalAidDeploymentsWithPlayer"],
                "ta_deployments_with_invalid_player": item[
                    "tacticalAidDeploymentsWithInvalidPlayer"
                ],
                "ta_deployments_without_participant_name": item[
                    "tacticalAidDeploymentsWithoutParticipantName"
                ],
                "ta_deployments_with_unit_spawn_attribution": item[
                    "tacticalAidDeploymentsWithUnitSpawnAttribution"
                ],
                "ta_deployments_with_invalid_team": item[
                    "tacticalAidDeploymentsWithInvalidTeam"
                ],
                "ta_deployments_past_duration": item[
                    "tacticalAidDeploymentsPastDuration"
                ],
                "spectator_view_changes": item["spectatorViewChanges"],
                "spectator_view_changes_with_unknown_los": item[
                    "spectatorViewChangesWithUnknownLos"
                ],
                "timeline_participants": item["timelineParticipants"],
                "timeline_participants_without_name": item[
                    "timelineParticipantsWithoutName"
                ],
                "timeline_duplicate_participants": item[
                    "timelineDuplicateParticipants"
                ],
                "timeline_player_references_without_participant": item[
                    "timelinePlayerReferencesWithoutParticipant"
                ],
                "timeline_participant_name_conflicts": item[
                    "timelineParticipantNameConflicts"
                ],
                "chat_participant_name_conflicts": item["chatParticipantNameConflicts"],
                "chat_messages": item["chatMessages"],
                "pre_match_chat_messages": item["preMatchChatMessages"],
                "post_match_chat_messages": item["postMatchChatMessages"],
                "chat_messages_without_player_name": item[
                    "chatMessagesWithoutPlayerName"
                ],
                "chat_player_names_outside_replay_roster": item[
                    "chatPlayerNamesOutsideReplayRoster"
                ],
                "chat_player_name_conflicts": item["chatPlayerNameConflicts"],
            }
        )

    summary = {
        "replays_parsed": len(rows),
        "replays_rejected": rejected,
        "schema_versions": dict(schema_versions),
        "coverage": {
            f"chat={chat},ta={ta},taMarkers={markers},taDeployments={deployments}": count
            for (chat, ta, markers, deployments), count in sorted(
                coverage_values.items()
            )
        },
        "server_names_seen": len(server_names),
        "server_names_unknown": server_names["Unknown"],
        "unknown_server_years": dict(sorted(unknown_server_years.items())),
        "chat_channels": dict(sorted(chat_channels.items())),
        "by_mode": {mode: dict(counter) for mode, counter in sorted(by_mode.items())},
        "support_ids_seen": len(cost_by_id),
        "support_ids_with_multiple_costs": sum(
            1 for costs in cost_by_id.values() if len(costs) > 1
        ),
        "costs_per_support_id": {
            support_id: dict(sorted(costs.items()))
            for support_id, costs in sorted(cost_by_id.items())
        },
    }
    print(json.dumps(summary, indent=2))
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            json.dumps({"summary": summary, "replays": rows}, indent=2) + "\n"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
