/// World in Conflict Replay Parser v6.0 (Rust) — CLI entry point
// Use the package library rather than compiling parser.rs into the binary a
// second time. This keeps the CLI and library on one implementation and makes
// the parser unit suite execute once per workspace test run.
use wic_replay_parser::parser;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use serde::Serialize;

#[cfg(feature = "cli")]
extern crate glob;

fn collect_replay_files(args: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        if args[i] == "--dir" {
            if i + 1 < args.len() {
                let dir = Path::new(&args[i + 1]);
                if dir.is_dir() {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("wicdemo"))
                            {
                                files.push(path);
                            }
                        }
                    }
                } else {
                    eprintln!("Error: '{}' is not a directory", args[i + 1]);
                }
                i += 2;
                continue;
            } else {
                eprintln!("Error: --dir requires a path argument");
                i += 1;
                continue;
            }
        }

        let arg = &args[i];
        if arg.contains('*') || arg.contains('?') {
            if let Ok(paths) = glob::glob(arg) {
                for entry in paths.flatten() {
                    if entry
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("wicdemo"))
                    {
                        files.push(entry);
                    }
                }
            }
        } else {
            let path = PathBuf::from(arg);
            if path.exists() {
                files.push(path);
            } else {
                eprintln!("Error: file not found: {arg}");
            }
        }
        i += 1;
    }

    files.sort();
    let mut seen = HashSet::new();
    files.retain(|path| path.canonicalize().is_ok_and(|path| seen.insert(path)));
    files
}

fn collect_replay_files_recursive(roots: &[String]) -> Vec<PathBuf> {
    fn visit(
        node: &Path,
        files: &mut Vec<PathBuf>,
        seen_files: &mut HashSet<PathBuf>,
        seen_dirs: &mut HashSet<PathBuf>,
    ) {
        if node.is_file() {
            if node
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("wicdemo"))
                && let Ok(canonical) = node.canonicalize()
                && seen_files.insert(canonical)
            {
                files.push(node.to_path_buf());
            }
            return;
        }
        if !node.is_dir() {
            return;
        }
        let Ok(canonical) = node.canonicalize() else {
            return;
        };
        if !seen_dirs.insert(canonical) {
            return;
        }
        let Ok(entries) = fs::read_dir(node) else {
            return;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        children.sort();
        for child in children {
            visit(&child, files, seen_files, seen_dirs);
        }
    }

    let mut files = Vec::new();
    let mut seen_files = HashSet::new();
    let mut seen_dirs = HashSet::new();
    for root in roots {
        visit(Path::new(root), &mut files, &mut seen_files, &mut seen_dirs);
    }
    files.sort();
    files
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusSupportUsage {
    support_id: u32,
    support_name: Option<String>,
    observed_costs: Vec<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusParsedSummary {
    path: String,
    server_name: String,
    date_time: String,
    mode: String,
    schema_version: u32,
    duration_seconds: f32,
    phases: usize,
    recorder: Option<String>,
    timeline_participants: u32,
    timeline_participants_without_name: u32,
    timeline_duplicate_participants: u32,
    timeline_player_references_without_participant: u32,
    timeline_participant_name_conflicts: u32,
    chat_participant_name_conflicts: u32,
    coverage_chat: String,
    coverage_tactical_aid: String,
    coverage_tactical_aid_markers: String,
    coverage_tactical_aid_deployments: String,
    tactical_aid_uses: u32,
    tactical_aid_uses_without_player: u32,
    tactical_aid_uses_at_time_zero: u32,
    tactical_aid_uses_past_duration: u32,
    tactical_aid_uses_at_world_origin: u32,
    tactical_aid_summary_total: u32,
    tactical_aid_summary_support_total: u32,
    tactical_aid_markers: u32,
    tactical_aid_markers_with_invalid_player: u32,
    tactical_aid_markers_without_participant_name: u32,
    tactical_aid_markers_past_duration: u32,
    tactical_aid_deployments: u32,
    tactical_aid_deployments_with_player: u32,
    tactical_aid_deployments_with_invalid_player: u32,
    tactical_aid_deployments_without_participant_name: u32,
    tactical_aid_deployments_with_unit_spawn_attribution: u32,
    tactical_aid_deployments_with_invalid_team: u32,
    tactical_aid_deployments_past_duration: u32,
    spectator_view_changes: u32,
    spectator_view_changes_with_unknown_los: u32,
    unit_destructions: u32,
    unit_caused_unit_destructions: u32,
    tactical_aid_unit_destructions: u32,
    unknown_unit_destructions: u32,
    building_collapse_unit_destructions: u32,
    building_collapse_unknown_unit_destructions: u32,
    destroyed_with_container_unit_destructions: u32,
    destroyed_with_container_unknown_unit_destructions: u32,
    destroyed_with_container_tactical_aid_unit_destructions: u32,
    contextual_unit_destructions_with_player: u32,
    infantry_soldier_deaths: u32,
    infantry_soldier_deaths_past_duration: u32,
    supports: Box<[CorpusSupportUsage]>,
    chat_messages: u32,
    pre_match_chat_messages: u32,
    post_match_chat_messages: u32,
    chat_all: u32,
    chat_team: u32,
    chat_messages_with_invalid_player: u32,
    chat_messages_without_player_name: u32,
    chat_player_names_outside_replay_roster: u32,
    chat_player_name_conflicts: u32,
    chat_messages_past_duration: u32,
}

#[derive(Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum CorpusReplaySummary {
    Parsed(Box<CorpusParsedSummary>),
    Rejected { path: String, error: String },
}

fn audit_chat_player_name(
    player_id: u32,
    player_name: Option<&str>,
    replay_player_names: &HashMap<u32, &str>,
    without_name: &mut u32,
    outside_replay_roster: &mut u32,
    conflicts: &mut u32,
) {
    match (player_name, replay_player_names.get(&player_id).copied()) {
        (None, _) => *without_name = without_name.saturating_add(1),
        (Some(_), None) => {
            *outside_replay_roster = outside_replay_roster.saturating_add(1);
        }
        (Some(timeline_name), Some(replay_name)) if timeline_name != replay_name => {
            *conflicts = conflicts.saturating_add(1);
        }
        _ => {}
    }
}

fn corpus_summary(path: &Path) -> CorpusReplaySummary {
    let display_path = path.display().to_string();
    let parser = match parser::WicReplayParser::new(path) {
        Ok(parser) => parser,
        Err(error) => {
            return CorpusReplaySummary::Rejected {
                path: display_path,
                error,
            };
        }
    };
    let replay = parser.parse();
    let timeline = parser.parse_timeline();
    let mut tactical_aid_uses = 0u32;
    let mut tactical_aid_uses_without_player = 0u32;
    let mut tactical_aid_uses_at_time_zero = 0u32;
    let mut tactical_aid_uses_past_duration = 0u32;
    let mut tactical_aid_uses_at_world_origin = 0u32;
    let mut tactical_aid_markers = 0u32;
    let mut tactical_aid_markers_with_invalid_player = 0u32;
    let mut tactical_aid_markers_without_participant_name = 0u32;
    let mut tactical_aid_markers_past_duration = 0u32;
    let mut chat_messages = 0u32;
    let mut chat_all = 0u32;
    let mut chat_team = 0u32;
    let mut chat_messages_with_invalid_player = 0u32;
    let mut chat_messages_without_player_name = 0u32;
    let mut chat_player_names_outside_replay_roster = 0u32;
    let mut chat_player_name_conflicts = 0u32;
    let mut chat_participant_name_conflicts = 0u32;
    let mut chat_messages_past_duration = 0u32;
    let mut tactical_aid_deployments = 0u32;
    let mut tactical_aid_deployments_with_player = 0u32;
    let mut tactical_aid_deployments_with_invalid_player = 0u32;
    let mut tactical_aid_deployments_without_participant_name = 0u32;
    let mut tactical_aid_deployments_with_unit_spawn_attribution = 0u32;
    let mut tactical_aid_deployments_with_invalid_team = 0u32;
    let mut tactical_aid_deployments_past_duration = 0u32;
    let mut spectator_view_changes = 0u32;
    let mut spectator_view_changes_with_unknown_los = 0u32;
    let mut unit_destructions = 0u32;
    let mut unit_caused_unit_destructions = 0u32;
    let mut tactical_aid_unit_destructions = 0u32;
    let mut unknown_unit_destructions = 0u32;
    let mut building_collapse_unit_destructions = 0u32;
    let mut building_collapse_unknown_unit_destructions = 0u32;
    let mut destroyed_with_container_unit_destructions = 0u32;
    let mut destroyed_with_container_unknown_unit_destructions = 0u32;
    let mut destroyed_with_container_tactical_aid_unit_destructions = 0u32;
    let mut contextual_unit_destructions_with_player = 0u32;
    let replay_player_names: HashMap<u32, &str> = replay
        .players
        .iter()
        .map(|player| (player.id, player.name.as_str()))
        .collect();
    let participant_names: HashMap<u32, Option<&str>> = timeline
        .participants
        .iter()
        .map(|participant| (participant.player_id, participant.player_name.as_deref()))
        .collect();
    let participant_ids: HashSet<u32> = participant_names.keys().copied().collect();
    let timeline_duplicate_participants = timeline
        .participants
        .len()
        .saturating_sub(participant_names.len()) as u32;
    let timeline_participants_without_name = timeline
        .participants
        .iter()
        .filter(|participant| participant.player_name.is_none())
        .count() as u32;
    let timeline_participant_name_conflicts = timeline
        .participants
        .iter()
        .filter(|participant| {
            participant
                .player_name
                .as_deref()
                .zip(replay_player_names.get(&participant.player_id).copied())
                .is_some_and(|(timeline_name, replay_name)| timeline_name != replay_name)
        })
        .count() as u32;
    let mut timeline_player_references = HashSet::new();
    for event in &timeline.events {
        timeline_player_references.extend(event.referenced_player_ids().into_iter().flatten());
    }
    for death in &timeline.infantry_soldier_deaths {
        timeline_player_references.extend(
            [death.player_id, death.killer_player_id]
                .into_iter()
                .flatten(),
        );
    }
    timeline_player_references.extend(
        timeline
            .pre_match_chat
            .iter()
            .map(|message| message.player_id),
    );
    timeline_player_references.extend(
        timeline
            .post_match_chat
            .iter()
            .map(|message| message.player_id),
    );
    timeline_player_references.extend(timeline.recorder_tactical_aid_usage.player_id);
    let timeline_player_references_without_participant = timeline_player_references
        .difference(&participant_ids)
        .count() as u32;

    for event in &timeline.events {
        match event {
            parser::TimelineEvent::TacticalAidUsed {
                time_seconds,
                position,
                player_id,
                ..
            } => {
                tactical_aid_uses = tactical_aid_uses.saturating_add(1);
                if player_id.is_none() {
                    tactical_aid_uses_without_player =
                        tactical_aid_uses_without_player.saturating_add(1);
                }
                if *time_seconds <= 0.0 {
                    tactical_aid_uses_at_time_zero =
                        tactical_aid_uses_at_time_zero.saturating_add(1);
                }
                if *time_seconds > timeline.duration_seconds + 1.0 {
                    tactical_aid_uses_past_duration =
                        tactical_aid_uses_past_duration.saturating_add(1);
                }
                if *position == [0.0, 0.0, 0.0] {
                    tactical_aid_uses_at_world_origin =
                        tactical_aid_uses_at_world_origin.saturating_add(1);
                }
            }
            parser::TimelineEvent::TacticalAidMarker {
                time_seconds,
                player_id,
                ..
            } => {
                tactical_aid_markers = tactical_aid_markers.saturating_add(1);
                if *player_id > 15 {
                    tactical_aid_markers_with_invalid_player =
                        tactical_aid_markers_with_invalid_player.saturating_add(1);
                }
                if participant_names
                    .get(player_id)
                    .copied()
                    .flatten()
                    .is_none()
                {
                    tactical_aid_markers_without_participant_name =
                        tactical_aid_markers_without_participant_name.saturating_add(1);
                }
                if *time_seconds > timeline.duration_seconds + 0.001 {
                    tactical_aid_markers_past_duration =
                        tactical_aid_markers_past_duration.saturating_add(1);
                }
            }
            parser::TimelineEvent::TacticalAidDeployed {
                time_seconds,
                team,
                player_id,
                player_attribution,
                ..
            } => {
                tactical_aid_deployments = tactical_aid_deployments.saturating_add(1);
                if let Some(player_id) = player_id {
                    tactical_aid_deployments_with_player =
                        tactical_aid_deployments_with_player.saturating_add(1);
                    if *player_id > 15 {
                        tactical_aid_deployments_with_invalid_player =
                            tactical_aid_deployments_with_invalid_player.saturating_add(1);
                    }
                    if participant_names
                        .get(player_id)
                        .copied()
                        .flatten()
                        .is_none()
                    {
                        tactical_aid_deployments_without_participant_name =
                            tactical_aid_deployments_without_participant_name.saturating_add(1);
                    }
                }
                if *player_attribution
                    == Some(parser::TacticalAidPlayerAttribution::UnitSpawnOwnership)
                {
                    tactical_aid_deployments_with_unit_spawn_attribution =
                        tactical_aid_deployments_with_unit_spawn_attribution.saturating_add(1);
                }
                if !(1..=3).contains(team) {
                    tactical_aid_deployments_with_invalid_team =
                        tactical_aid_deployments_with_invalid_team.saturating_add(1);
                }
                if *time_seconds > timeline.duration_seconds + 0.001 {
                    tactical_aid_deployments_past_duration =
                        tactical_aid_deployments_past_duration.saturating_add(1);
                }
            }
            parser::TimelineEvent::SpectatorViewChanged { view, .. } => {
                spectator_view_changes = spectator_view_changes.saturating_add(1);
                if *view == parser::SpectatorView::Unknown {
                    spectator_view_changes_with_unknown_los =
                        spectator_view_changes_with_unknown_los.saturating_add(1);
                }
            }
            parser::TimelineEvent::UnitDestroyed {
                player_id,
                cause,
                destruction_context,
                ..
            } => {
                unit_destructions = unit_destructions.saturating_add(1);
                match cause {
                    parser::UnitDestructionCause::Unit => {
                        unit_caused_unit_destructions =
                            unit_caused_unit_destructions.saturating_add(1);
                    }
                    parser::UnitDestructionCause::TacticalAid => {
                        tactical_aid_unit_destructions =
                            tactical_aid_unit_destructions.saturating_add(1);
                    }
                    parser::UnitDestructionCause::Unknown => {
                        unknown_unit_destructions = unknown_unit_destructions.saturating_add(1);
                    }
                }
                if destruction_context.is_some() && player_id.is_some() {
                    contextual_unit_destructions_with_player =
                        contextual_unit_destructions_with_player.saturating_add(1);
                }
                match destruction_context {
                    Some(parser::UnitDestructionContext::BuildingCollapse { .. }) => {
                        building_collapse_unit_destructions =
                            building_collapse_unit_destructions.saturating_add(1);
                        if *cause == parser::UnitDestructionCause::Unknown {
                            building_collapse_unknown_unit_destructions =
                                building_collapse_unknown_unit_destructions.saturating_add(1);
                        }
                    }
                    Some(parser::UnitDestructionContext::DestroyedWithContainer { .. }) => {
                        destroyed_with_container_unit_destructions =
                            destroyed_with_container_unit_destructions.saturating_add(1);
                        match cause {
                            parser::UnitDestructionCause::Unknown => {
                                destroyed_with_container_unknown_unit_destructions =
                                    destroyed_with_container_unknown_unit_destructions
                                        .saturating_add(1);
                            }
                            parser::UnitDestructionCause::TacticalAid => {
                                destroyed_with_container_tactical_aid_unit_destructions =
                                    destroyed_with_container_tactical_aid_unit_destructions
                                        .saturating_add(1);
                            }
                            parser::UnitDestructionCause::Unit => {}
                        }
                    }
                    None => {}
                }
            }
            parser::TimelineEvent::ChatMessage {
                time_seconds,
                player_id,
                player_name,
                channel,
                ..
            } => {
                chat_messages = chat_messages.saturating_add(1);
                if *player_id > 15 {
                    chat_messages_with_invalid_player =
                        chat_messages_with_invalid_player.saturating_add(1);
                }
                audit_chat_player_name(
                    *player_id,
                    player_name.as_deref(),
                    &replay_player_names,
                    &mut chat_messages_without_player_name,
                    &mut chat_player_names_outside_replay_roster,
                    &mut chat_player_name_conflicts,
                );
                if player_name.as_deref() != participant_names.get(player_id).copied().flatten() {
                    chat_participant_name_conflicts =
                        chat_participant_name_conflicts.saturating_add(1);
                }
                if *time_seconds > timeline.duration_seconds + 0.001 {
                    chat_messages_past_duration = chat_messages_past_duration.saturating_add(1);
                }
                match channel {
                    parser::ChatChannel::All => chat_all = chat_all.saturating_add(1),
                    parser::ChatChannel::Team => chat_team = chat_team.saturating_add(1),
                }
            }
            _ => {}
        }
    }
    let infantry_soldier_deaths_past_duration = timeline
        .infantry_soldier_deaths
        .iter()
        .filter(|death| death.time_seconds > timeline.duration_seconds + 0.001)
        .count() as u32;
    for message in &timeline.pre_match_chat {
        if message.player_id > 15 {
            chat_messages_with_invalid_player = chat_messages_with_invalid_player.saturating_add(1);
        }
        audit_chat_player_name(
            message.player_id,
            message.player_name.as_deref(),
            &replay_player_names,
            &mut chat_messages_without_player_name,
            &mut chat_player_names_outside_replay_roster,
            &mut chat_player_name_conflicts,
        );
        if message.player_name.as_deref()
            != participant_names.get(&message.player_id).copied().flatten()
        {
            chat_participant_name_conflicts = chat_participant_name_conflicts.saturating_add(1);
        }
        match message.channel {
            parser::ChatChannel::All => chat_all = chat_all.saturating_add(1),
            parser::ChatChannel::Team => chat_team = chat_team.saturating_add(1),
        }
    }
    for message in &timeline.post_match_chat {
        if message.player_id > 15 {
            chat_messages_with_invalid_player = chat_messages_with_invalid_player.saturating_add(1);
        }
        audit_chat_player_name(
            message.player_id,
            message.player_name.as_deref(),
            &replay_player_names,
            &mut chat_messages_without_player_name,
            &mut chat_player_names_outside_replay_roster,
            &mut chat_player_name_conflicts,
        );
        if message.player_name.as_deref()
            != participant_names.get(&message.player_id).copied().flatten()
        {
            chat_participant_name_conflicts = chat_participant_name_conflicts.saturating_add(1);
        }
        match message.channel {
            parser::ChatChannel::All => chat_all = chat_all.saturating_add(1),
            parser::ChatChannel::Team => chat_team = chat_team.saturating_add(1),
        }
    }

    let tactical_aid_summary_support_total = timeline
        .recorder_tactical_aid_usage
        .supports
        .iter()
        .fold(0u32, |total, usage| {
            total.saturating_add(usage.placement_count)
        });
    let supports: Vec<CorpusSupportUsage> = timeline
        .recorder_tactical_aid_usage
        .supports
        .into_iter()
        .map(|usage| CorpusSupportUsage {
            support_id: usage.support_id,
            support_name: usage.support_name,
            observed_costs: usage.observed_costs,
        })
        .collect();

    CorpusReplaySummary::Parsed(Box::new(CorpusParsedSummary {
        path: display_path,
        server_name: replay.game_info.server_name,
        date_time: replay.game_info.date_time,
        mode: replay.game_info.game_mode,
        schema_version: timeline.schema_version,
        duration_seconds: timeline.duration_seconds,
        phases: timeline.phases.len(),
        recorder: replay.recorder,
        timeline_participants: timeline.participants.len() as u32,
        timeline_participants_without_name,
        timeline_duplicate_participants,
        timeline_player_references_without_participant,
        timeline_participant_name_conflicts,
        chat_participant_name_conflicts,
        coverage_chat: timeline.coverage.chat.to_string(),
        coverage_tactical_aid: timeline.coverage.tactical_aid.to_string(),
        coverage_tactical_aid_markers: timeline.coverage.tactical_aid_markers.to_string(),
        coverage_tactical_aid_deployments: timeline.coverage.tactical_aid_deployments.to_string(),
        tactical_aid_uses,
        tactical_aid_uses_without_player,
        tactical_aid_uses_at_time_zero,
        tactical_aid_uses_past_duration,
        tactical_aid_uses_at_world_origin,
        tactical_aid_summary_total: timeline.recorder_tactical_aid_usage.total_placements,
        tactical_aid_summary_support_total,
        tactical_aid_markers,
        tactical_aid_markers_with_invalid_player,
        tactical_aid_markers_without_participant_name,
        tactical_aid_markers_past_duration,
        tactical_aid_deployments,
        tactical_aid_deployments_with_player,
        tactical_aid_deployments_with_invalid_player,
        tactical_aid_deployments_without_participant_name,
        tactical_aid_deployments_with_unit_spawn_attribution,
        tactical_aid_deployments_with_invalid_team,
        tactical_aid_deployments_past_duration,
        spectator_view_changes,
        spectator_view_changes_with_unknown_los,
        unit_destructions,
        unit_caused_unit_destructions,
        tactical_aid_unit_destructions,
        unknown_unit_destructions,
        building_collapse_unit_destructions,
        building_collapse_unknown_unit_destructions,
        destroyed_with_container_unit_destructions,
        destroyed_with_container_unknown_unit_destructions,
        destroyed_with_container_tactical_aid_unit_destructions,
        contextual_unit_destructions_with_player,
        infantry_soldier_deaths: timeline.infantry_soldier_deaths.len() as u32,
        infantry_soldier_deaths_past_duration,
        supports: supports.into_boxed_slice(),
        chat_messages,
        pre_match_chat_messages: timeline.pre_match_chat.len() as u32,
        post_match_chat_messages: timeline.post_match_chat.len() as u32,
        chat_all,
        chat_team,
        chat_messages_with_invalid_player,
        chat_messages_without_player_name,
        chat_player_names_outside_replay_roster,
        chat_player_name_conflicts,
        chat_messages_past_duration,
    }))
}

fn default_corpus_jobs() -> usize {
    parser::available_replay_workers()
}

fn parse_corpus_options(args: &[String]) -> Result<(Vec<String>, usize), String> {
    let mut roots = Vec::new();
    let mut jobs = default_corpus_jobs();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--corpus-summary-json" => index += 1,
            "--jobs" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--jobs requires a positive integer".to_string())?;
                jobs = value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--jobs requires a positive integer".to_string())?;
                index += 2;
            }
            argument if argument.starts_with("--jobs=") => {
                jobs = argument[7..]
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| "--jobs requires a positive integer".to_string())?;
                index += 1;
            }
            argument if argument.starts_with('-') => {
                return Err(format!("unsupported corpus option: {argument}"));
            }
            root => {
                roots.push(root.to_string());
                index += 1;
            }
        }
    }

    Ok((roots, jobs))
}

fn collect_corpus_summaries(files: &[PathBuf], requested_jobs: usize) -> Vec<CorpusReplaySummary> {
    let workers = requested_jobs.min(files.len()).max(1);
    if workers == 1 {
        return files.iter().map(|path| corpus_summary(path)).collect();
    }

    let next_index = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let next_index = &next_index;
            scope.spawn(move || {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(index) else {
                        break;
                    };
                    if sender.send((index, corpus_summary(path))).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(sender);

    let mut indexed: Vec<(usize, CorpusReplaySummary)> = receiver.into_iter().collect();
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, summary)| summary).collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("World in Conflict Replay Parser v6.0 (Rust)");
        eprintln!();
        eprintln!("Usage:");
        eprintln!("  wic_replay_parser <file.wicdemo>              Parse a single replay");
        eprintln!("  wic_replay_parser *.wicdemo                   Parse all replays (glob)");
        eprintln!("  wic_replay_parser game1.wicdemo game2.wicdemo Parse multiple files");
        eprintln!("  wic_replay_parser --dir <path>                Parse all replays in directory");
        eprintln!("  wic_replay_parser --json <file>               Emit match-result JSON");
        eprintln!("  wic_replay_parser --timeline-json <file>      Emit match + timeline JSON");
        eprintln!(
            "  wic_replay_parser --corpus-summary-json <root>... [--jobs N]  Validate recursively"
        );
        process::exit(1);
    }

    if args.iter().any(|arg| arg == "--corpus-summary-json") {
        let (roots, jobs) = match parse_corpus_options(&args) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("Error: {error}");
                process::exit(2);
            }
        };
        let files = collect_replay_files_recursive(&roots);
        if files.is_empty() {
            eprintln!("No .wicdemo files found.");
            process::exit(1);
        }
        let summaries = collect_corpus_summaries(&files, jobs);
        match serde_json::to_string(&summaries) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("Error serializing corpus summary JSON: {error}");
                process::exit(1);
            }
        }
        return;
    }

    let timeline_json = args.iter().any(|arg| arg == "--timeline-json");
    let match_json = args.iter().any(|arg| arg == "--json");
    if timeline_json && match_json {
        eprintln!("Error: --json and --timeline-json cannot be used together");
        process::exit(1);
    }
    let file_args: Vec<String> = args
        .into_iter()
        .filter(|arg| arg != "--timeline-json" && arg != "--json")
        .collect();
    let files = collect_replay_files(&file_args);

    if files.is_empty() {
        eprintln!("No .wicdemo files found.");
        process::exit(1);
    }

    let multiple = files.len() > 1;

    if timeline_json {
        let mut parsed = Vec::new();
        let mut had_error = false;
        for filepath in &files {
            match parser::WicReplayParser::new(filepath) {
                Ok(parser) => parsed.push(parser.parse_with_timeline()),
                Err(error) => {
                    had_error = true;
                    eprintln!(
                        "Error parsing {}: {error}",
                        filepath.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
            }
        }

        let json = if multiple {
            serde_json::to_string_pretty(&parsed)
        } else {
            parsed
                .first()
                .map_or_else(|| Ok("[]".to_string()), serde_json::to_string_pretty)
        };
        match json {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("Error serializing timeline JSON: {error}");
                process::exit(1);
            }
        }
        if had_error {
            process::exit(1);
        }
        return;
    }

    if match_json {
        let mut parsed = Vec::new();
        let mut had_error = false;
        for filepath in &files {
            match parser::WicReplayParser::new(filepath) {
                Ok(parser) => parsed.push(parser.parse()),
                Err(error) => {
                    had_error = true;
                    eprintln!(
                        "Error parsing {}: {error}",
                        filepath.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
            }
        }

        let json = if multiple {
            serde_json::to_string_pretty(&parsed)
        } else {
            parsed
                .first()
                .map_or_else(|| Ok("[]".to_string()), serde_json::to_string_pretty)
        };
        match json {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("Error serializing match JSON: {error}");
                process::exit(1);
            }
        }
        if had_error {
            process::exit(1);
        }
        return;
    }

    let mut had_error = false;
    for (i, filepath) in files.iter().enumerate() {
        if multiple {
            if i > 0 {
                println!();
            }
            println!(
                "===== {} =====",
                filepath.file_name().unwrap_or_default().to_string_lossy()
            );
        }

        match parser::WicReplayParser::new(filepath) {
            Ok(p) => {
                let data = p.parse();
                print!("{data}");
            }
            Err(e) => {
                had_error = true;
                eprintln!(
                    "Error parsing {}: {e}",
                    filepath.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }
    }
    if had_error {
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_corpus_jobs_without_treating_it_as_a_root() {
        let args = [
            "--corpus-summary-json".to_string(),
            "replays/main".to_string(),
            "--jobs".to_string(),
            "3".to_string(),
            "replays/old".to_string(),
        ];

        let (roots, jobs) = parse_corpus_options(&args).expect("valid corpus options");

        assert_eq!(roots, ["replays/main", "replays/old"]);
        assert_eq!(jobs, 3);
    }

    #[test]
    fn rejects_zero_corpus_jobs() {
        let args = [
            "--corpus-summary-json".to_string(),
            "replays/main".to_string(),
            "--jobs=0".to_string(),
        ];

        assert_eq!(
            parse_corpus_options(&args).unwrap_err(),
            "--jobs requires a positive integer"
        );
    }
}
