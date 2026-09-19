#!/usr/bin/env python3
"""Audit tactical-aid marker coverage under each recorder spectator mode.

``SpectatorJoinedTeam`` serializes the spectator's selected faction in ``aTeam``
and the line-of-sight mode in ``aSpectatorLos``. Client binary tracing establishes
the payload order; shipped UI observations and corpus values identify LOS 1 as one
selected faction and LOS 2 as all factions. This tool measures which top-level TA
markers the recorder actually received under those modes without treating absent
network-visible records as server-global facts.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import multiprocessing
import os
import pathlib
from collections import Counter
from dataclasses import dataclass

from wic_bintag import decompress, fields, name_hash, recorder_slot, walk

MSG_PLAYER_JOINED_TEAM = name_hash("PlayerJoinedTeam")
MSG_SPECTATOR_JOINED_TEAM = name_hash("SpectatorJoinedTeam")
MSG_UNIT_CREATE = name_hash("UnitCreate")
MSG_SUPPORT_THING_FEEDBACK = name_hash("SupportThingFeedback")
MSG_SUPPORT_THING_SPAWNED_DELAYED = name_hash("SupportThingSpawnedDelayed")
MSG_SUPPORT_THING_MARKER = name_hash("SupportThingMarker")


@dataclass(frozen=True)
class SpectatorView:
    mode: str
    team: int | None
    raw_los: int


def decode_spectator_view(team: int, spectator_los: int) -> SpectatorView:
    """Decode the two spectator modes proven by the client and corpus."""
    if spectator_los == 1 and team in {1, 2, 3}:
        return SpectatorView("oneTeam", team, spectator_los)
    if spectator_los == 2:
        return SpectatorView("allTeams", None, spectator_los)
    if spectator_los == 0:
        return SpectatorView("none", None, spectator_los)
    return SpectatorView("unknown", team if team in {1, 2, 3} else None, spectator_los)


def support_faction(internal_name: str) -> int | None:
    """Return the faction encoded by a top-level support definition name."""
    if internal_name.endswith("_NATO_British") or internal_name.endswith("_NATO"):
        return 2
    if internal_name.endswith("_USSR"):
        return 3
    if internal_name.endswith("_US"):
        return 1
    return None


def load_top_level_supports(path: pathlib.Path) -> dict[int, int]:
    report = json.loads(path.read_text())
    result = {}
    for item in report.get("supports", []):
        if item.get("presentationClass") != "topLevelFactionAid":
            continue
        faction = support_faction(item["internalName"])
        if faction is None:
            raise ValueError(
                f"top-level support has no faction suffix: {item['internalName']}"
            )
        result[item["supportId"]] = faction
    if not result:
        raise ValueError("support catalogue contains no top-level faction aids")
    return result


def analyse_replay(path: str, top_level_supports: dict[int, int]) -> dict:
    replay_path = pathlib.Path(path)
    data = decompress(replay_path)
    recorder = recorder_slot(data)
    player_teams: dict[int, int] = {}
    recorder_view: SpectatorView | None = None
    view_changes = []
    marker_factions = Counter()
    marker_players = Counter()
    marker_views = Counter()
    marker_view_factions: dict[str, Counter] = {}
    effect_views = Counter()
    effect_view_factions: dict[str, Counter] = {}
    effect_kinds = Counter()
    effect_kind_view_factions: dict[str, dict[str, Counter]] = {}
    observed_player_factions: set[int] = set()
    spawned_team_matches = 0
    spawned_team_mismatches = 0
    spawned_team_unknown = 0
    marker_player_team_matches = 0
    marker_player_team_mismatches = 0
    marker_player_team_unknown = 0
    one_team_view_faction_mismatches = 0

    for event_index, envelope in enumerate(walk(data)):
        if envelope.message == MSG_PLAYER_JOINED_TEAM:
            body, _ = fields(data, envelope)
            if len(body) >= 2 and body[0].u32 <= 15 and body[1].u32 in {1, 2, 3}:
                player_teams[body[0].u32] = body[1].u32
                observed_player_factions.add(body[1].u32)
            continue

        if envelope.message == MSG_SPECTATOR_JOINED_TEAM:
            body, _ = fields(data, envelope)
            if len(body) < 3 or body[0].u32 > 15:
                continue
            player_id = body[0].u32
            player_teams[player_id] = 0
            if player_id == recorder:
                recorder_view = decode_spectator_view(body[1].u32, body[2].u32)
                view_changes.append(
                    {
                        "eventIndex": event_index,
                        "offset": envelope.offset,
                        "rawEventTime": round(envelope.time, 6),
                        "team": recorder_view.team,
                        "mode": recorder_view.mode,
                        "rawSpectatorLos": recorder_view.raw_los,
                    }
                )
            continue

        if envelope.message == MSG_UNIT_CREATE:
            body, _ = fields(data, envelope)
            if len(body) >= 3 and body[1].u32 <= 15 and body[2].u32 in {1, 2, 3}:
                player_teams[body[1].u32] = body[2].u32
                observed_player_factions.add(body[2].u32)
            continue

        if envelope.message in {
            MSG_SUPPORT_THING_FEEDBACK,
            MSG_SUPPORT_THING_SPAWNED_DELAYED,
        }:
            body, _ = fields(data, envelope)
            if body:
                support_faction_id = top_level_supports.get(body[0].u32)
                if support_faction_id is not None:
                    view_mode = (
                        recorder_view.mode
                        if recorder_view is not None
                        else "notSpectating"
                    )
                    kind = (
                        "feedback"
                        if envelope.message == MSG_SUPPORT_THING_FEEDBACK
                        else "spawnedDelayed"
                    )
                    effect_views[view_mode] += 1
                    effect_view_factions.setdefault(view_mode, Counter())[
                        support_faction_id
                    ] += 1
                    effect_kinds[kind] += 1
                    effect_kind_view_factions.setdefault(kind, {}).setdefault(
                        view_mode, Counter()
                    )[support_faction_id] += 1
                    if kind == "spawnedDelayed":
                        if len(body) < 5 or body[4].u32 not in {1, 2, 3}:
                            spawned_team_unknown += 1
                        elif body[4].u32 == support_faction_id:
                            spawned_team_matches += 1
                        else:
                            spawned_team_mismatches += 1
            continue

        if envelope.message != MSG_SUPPORT_THING_MARKER:
            continue
        body, _ = fields(data, envelope)
        if len(body) < 6 or body[5].u32 > 15:
            continue
        support_id = body[1].u32
        support_team = top_level_supports.get(support_id)
        if support_team is None:
            continue
        player_id = body[5].u32
        marker_factions[support_team] += 1
        marker_players[player_id] += 1
        player_team = player_teams.get(player_id)
        if player_team is None or player_team == 0:
            marker_player_team_unknown += 1
        elif player_team == support_team:
            marker_player_team_matches += 1
        else:
            marker_player_team_mismatches += 1

        view_mode = recorder_view.mode if recorder_view is not None else "notSpectating"
        marker_views[view_mode] += 1
        marker_view_factions.setdefault(view_mode, Counter())[support_team] += 1
        if (
            recorder_view is not None
            and recorder_view.mode == "oneTeam"
            and recorder_view.team != support_team
        ):
            one_team_view_faction_mismatches += 1

    return {
        "path": str(replay_path),
        "recorderPlayerId": recorder,
        "recorderSpectatorViewChanges": view_changes,
        "topLevelMarkers": sum(marker_factions.values()),
        "markerFactionCounts": {
            str(team): count for team, count in sorted(marker_factions.items())
        },
        "markerPlayerCounts": {
            str(player): count for player, count in sorted(marker_players.items())
        },
        "markerCountsByRecorderView": dict(sorted(marker_views.items())),
        "markerFactionCountsByRecorderView": {
            mode: {str(team): count for team, count in sorted(counts.items())}
            for mode, counts in sorted(marker_view_factions.items())
        },
        "topLevelEffectCountsByRecorderView": dict(sorted(effect_views.items())),
        "topLevelEffectFactionCountsByRecorderView": {
            mode: {str(team): count for team, count in sorted(counts.items())}
            for mode, counts in sorted(effect_view_factions.items())
        },
        "topLevelEffectKinds": dict(sorted(effect_kinds.items())),
        "topLevelEffectKindFactionCountsByRecorderView": {
            kind: {
                mode: {str(team): count for team, count in sorted(counts.items())}
                for mode, counts in sorted(by_view.items())
            }
            for kind, by_view in sorted(effect_kind_view_factions.items())
        },
        "observedPlayerFactions": sorted(observed_player_factions),
        "spawnedTeamMatchesSupportFaction": spawned_team_matches,
        "spawnedTeamMismatchesSupportFaction": spawned_team_mismatches,
        "spawnedTeamUnknown": spawned_team_unknown,
        "markerPlayerTeamMatches": marker_player_team_matches,
        "markerPlayerTeamMismatches": marker_player_team_mismatches,
        "markerPlayerTeamUnknown": marker_player_team_unknown,
        "oneTeamViewFactionMismatches": one_team_view_faction_mismatches,
    }


def analyse_replay_safe(arguments: tuple[str, dict[int, int]]) -> dict:
    path, top_level_supports = arguments
    try:
        return analyse_replay(path, top_level_supports)
    except (OSError, ValueError) as error:
        return {"path": path, "status": "failed", "error": str(error)}


def summarise(results: list[dict]) -> dict:
    totals = Counter()
    los_pairs = Counter()
    samples: dict[str, list[str]] = {"oneTeam": [], "allTeams": [], "mixed": []}
    for result in results:
        if result.get("status") == "failed":
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        changes = result["recorderSpectatorViewChanges"]
        if changes:
            totals["recorderSpectatorReplays"] += 1
        modes = {change["mode"] for change in changes}
        for change in changes:
            los_pairs[(change["team"], change["rawSpectatorLos"])] += 1
        for mode, count in result["markerCountsByRecorderView"].items():
            totals[f"topLevelMarkersView_{mode}"] += count
        for mode, count in result["topLevelEffectCountsByRecorderView"].items():
            totals[f"topLevelEffectsView_{mode}"] += count
        for kind, count in result["topLevelEffectKinds"].items():
            totals[f"topLevelEffects_{kind}"] += count
        spawned_views = result["topLevelEffectKindFactionCountsByRecorderView"].get(
            "spawnedDelayed", {}
        )
        spawned_factions = {
            faction for counts in spawned_views.values() for faction in counts
        }
        if len(spawned_factions) > 1:
            totals["replaysWithBothFactionDeployments"] += 1
        for mode, counts in spawned_views.items():
            totals[f"replaysWithDeploymentsView_{mode}"] += 1
            if len(counts) > 1:
                totals[f"replaysWithBothFactionDeploymentsView_{mode}"] += 1
        if len(result["markerFactionCounts"]) > 1:
            totals["replaysWithMultipleMarkerFactions"] += 1
        totals["topLevelMarkers"] += result["topLevelMarkers"]
        totals["markerPlayerTeamMatches"] += result["markerPlayerTeamMatches"]
        totals["markerPlayerTeamMismatches"] += result["markerPlayerTeamMismatches"]
        totals["markerPlayerTeamUnknown"] += result["markerPlayerTeamUnknown"]
        totals["spawnedTeamMatchesSupportFaction"] += result[
            "spawnedTeamMatchesSupportFaction"
        ]
        totals["spawnedTeamMismatchesSupportFaction"] += result[
            "spawnedTeamMismatchesSupportFaction"
        ]
        totals["spawnedTeamUnknown"] += result["spawnedTeamUnknown"]
        totals["oneTeamViewFactionMismatches"] += result["oneTeamViewFactionMismatches"]
        if result["topLevelMarkers"]:
            for mode in ("oneTeam", "allTeams"):
                if mode in modes and len(samples[mode]) < 8:
                    samples[mode].append(result["path"])
            if {"oneTeam", "allTeams"}.issubset(modes) and len(samples["mixed"]) < 8:
                samples["mixed"].append(result["path"])

    return {
        "schemaVersion": 1,
        **dict(sorted(totals.items())),
        "spectatorTeamLosPairs": {
            f"team={team},los={los}": count
            for (team, los), count in sorted(
                los_pairs.items(), key=lambda item: (str(item[0][0]), item[0][1])
            )
        },
        "sampleReplays": samples,
    }


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def replay_paths(roots: list[str]) -> list[pathlib.Path]:
    paths = []
    seen: set[pathlib.Path] = set()
    for root in roots:
        node = pathlib.Path(root)
        candidates = sorted(node.rglob("*.wicdemo")) if node.is_dir() else [node]
        for candidate in candidates:
            resolved = candidate.resolve()
            if resolved not in seen:
                seen.add(resolved)
                paths.append(candidate)
    return paths


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", help="replay files or directories")
    parser.add_argument(
        "--catalogue",
        default="local/generated/ta-support-catalogue.json",
        help="derived exact support catalogue JSON",
    )
    parser.add_argument("--json", help="write summary and per-replay rows here")
    parser.add_argument(
        "--jobs",
        type=positive_int,
        default=min(4, os.cpu_count() or 1),
        help="parallel replay workers (default: auto, capped at 4)",
    )
    args = parser.parse_args()

    try:
        supports = load_top_level_supports(pathlib.Path(args.catalogue))
    except (OSError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))
    paths = replay_paths(args.roots)
    if not paths:
        parser.error("no replay files found")
    work = [(str(path), supports) for path in paths]
    if args.jobs == 1:
        results = [analyse_replay_safe(item) for item in work]
    else:
        start_method = (
            "fork" if "fork" in multiprocessing.get_all_start_methods() else "spawn"
        )
        with concurrent.futures.ProcessPoolExecutor(
            max_workers=args.jobs,
            mp_context=multiprocessing.get_context(start_method),
        ) as executor:
            results = list(executor.map(analyse_replay_safe, work))

    summary = summarise(results)
    print(json.dumps(summary, indent=2))
    if args.json:
        target = pathlib.Path(args.json)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(
            json.dumps({"summary": summary, "replays": results}, indent=2) + "\n"
        )
    return 1 if summary.get("replaysFailed", 0) else 0


if __name__ == "__main__":
    raise SystemExit(main())
