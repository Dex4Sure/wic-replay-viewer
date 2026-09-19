use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use wic_replay_parser::parser;

use crate::model::human_text;
use crate::server_mode;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailView {
    pub overview: Overview,
    pub score_participants: Vec<ParticipantSessionDocument>,
    pub timeline_schema: u32,
    pub duration_seconds: f32,
    pub phases: Vec<PhaseView>,
    pub domination_samples: Vec<TimelineValueSampleView>,
    pub domination_anchor_faction: Option<String>,
    pub coverage_chat: String,
    pub coverage_tactical_aid: String,
    pub recorder_view: String,
    pub timeline_rows: Vec<TimelineRow>,
    pub chat_rows: Vec<ChatRow>,
    pub tactical_aid_rows: Vec<TacticalAidRow>,
    pub tactical_aid_summary: TacticalAidSummaryView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub map_name: String,
    pub map_display_name: String,
    pub server_name: String,
    pub date_time: String,
    pub game_mode: String,
    pub server_modes: Vec<String>,
    pub format: Option<String>,
    /// Match time elapsed, which exceeds the recording length for a late join.
    pub duration_seconds: Option<f32>,
    pub timing: MatchTimingView,
    pub match_ending: String,
    pub winner_domination_pct: Option<f64>,
    pub loser_domination_pct: Option<f64>,
    pub domination_shares: Option<Vec<DominationShareView>>,
    pub domination_anchor: Option<String>,
    pub winner: Option<String>,
    pub recorder: Option<String>,
    pub incomplete: bool,
    pub players: Vec<PlayerView>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTimingView {
    #[serde(default)]
    pub captured_match_start: bool,
    /// Real length of the replay file, from the `Event` envelope clock.
    #[serde(default)]
    pub recording_seconds: f32,
    /// Gameplay observed between the first and last countdown sample.
    pub observed_gameplay_seconds: Option<f32>,
    pub match_elapsed_seconds: Option<f32>,
    pub round_length_seconds: Option<f32>,
    #[serde(default)]
    pub round_length_exact: bool,
    pub joined_at_remaining_seconds: Option<f32>,
    pub final_remaining_seconds: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DominationShareView {
    pub faction: String,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerView {
    pub id: u32,
    pub name: String,
    pub team: Option<u32>,
    pub faction: Option<String>,
    pub role: Option<String>,
    pub score: Option<i32>,
    pub left_at_seconds: Option<f64>,
    pub score_before_leave: Option<parser::ScoreBeforeLeave>,
    pub score_infantry: Option<i32>,
    pub score_support: Option<i32>,
    pub score_armor: Option<i32>,
    pub score_air: Option<i32>,
    pub score_capturing: Option<i32>,
    pub score_fortification: Option<i32>,
    pub score_transportation: Option<i32>,
    pub score_repair: Option<i32>,
    pub score_bridge_laying: Option<i32>,
    pub score_unit_damage: Option<i32>,
    pub score_tactical_aid: Option<i32>,
    pub score_total: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseView {
    pub index: u32,
    pub start_seconds: f32,
    pub end_seconds: f32,
    pub initial_clock_seconds: f32,
    pub final_clock_seconds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineValueSampleView {
    pub time_seconds: f32,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineRow {
    pub time_seconds: f32,
    pub kind: &'static str,
    pub description: String,
    pub players: Vec<TimelinePlayer>,
    pub command_point_id: Option<u32>,
    pub command_point_team: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePlayer {
    pub name: String,
    pub faction: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRow {
    pub time_label: String,
    pub stage: &'static str,
    pub player: String,
    pub channel: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalAidRow {
    pub time_seconds: f32,
    pub support: String,
    pub player: String,
    pub faction: String,
    pub player_attribution: &'static str,
    pub honors_cost: Option<f32>,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalAidSummaryView {
    pub player: String,
    pub total_placements: u32,
    pub supports: Vec<TacticalAidSupportView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalAidSupportView {
    pub support: String,
    pub faction: String,
    pub placement_count: u32,
    pub observed_costs: Vec<f32>,
}

impl DetailView {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let document: DetailDocument = serde_json::from_str(json)
            .map_err(|error| format!("Cannot decode cached replay detail: {error}"))?;

        Ok(Self::from_document(document))
    }

    pub fn from_parser_document(document: parser::ReplayWithTimeline) -> Self {
        Self::from_document(document.into())
    }

    /// Cache selected result rows, historical timeline, and fallback roster evidence.
    pub fn cache_parser_document(
        document: parser::ReplayWithTimeline,
        hidden_player_ids: Vec<u32>,
        player_result_evidence: Vec<parser::PlayerResultEvidence>,
    ) -> Result<(Self, String), String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct CachedDocument<'a> {
            #[serde(flatten)]
            document: &'a parser::ReplayWithTimeline,
            hidden_player_ids: &'a [u32],
            player_result_evidence: &'a [parser::PlayerResultEvidence],
        }
        let json = serde_json::to_string(&CachedDocument {
            document: &document,
            hidden_player_ids: &hidden_player_ids,
            player_result_evidence: &player_result_evidence,
        })
        .map_err(|error| format!("Cannot serialize replay detail: {error}"))?;
        let mut projected = DetailDocument::from(document);
        projected.hidden_player_ids = hidden_player_ids;
        projected.player_result_evidence = player_result_evidence;
        Ok((Self::from_document(projected), json))
    }

    fn from_document(mut document: DetailDocument) -> Self {
        document
            .replay
            .players
            .retain(|player| !document.hidden_player_ids.contains(&player.id));
        for player in &mut document.replay.players {
            if let Some(evidence) = document
                .player_result_evidence
                .iter()
                .find(|e| e.player_id == player.id)
                && let Some(score) = &evidence.score_before_leave
            {
                player.score = Some(score.score);
                player.score_before_leave = Some(score.clone());
                // End-summary zeros no longer describe this departed person's
                // earlier activity; category totals are unavailable, not zero.
                player.score_infantry = None;
                player.score_support = None;
                player.score_armor = None;
                player.score_air = None;
                player.score_capturing = None;
                player.score_fortification = None;
                player.score_transportation = None;
                player.score_repair = None;
                player.score_bridge_laying = None;
                player.score_unit_damage = None;
                player.score_tactical_aid = None;
                player.score_total = None;
            }
        }
        let overflow = crate::roster::overflow_departure_ids(
            document
                .replay
                .players
                .iter()
                .map(|p| (p.id, p.team, p.left_at_seconds.map(f64::from))),
        );
        document
            .replay
            .players
            .retain(|p| !overflow.contains(&p.id));
        let server_modes = server_mode_labels(&document.replay.server_classification);
        let overview = Overview {
            map_name: document.replay.game_info.map_name,
            map_display_name: document.replay.game_info.map_display_name,
            server_name: human_text(&document.replay.game_info.server_name),
            date_time: document.replay.game_info.date_time,
            game_mode: document.replay.game_info.game_mode,
            server_modes: server_modes.clone(),
            format: server_mode::matchup(document.replay.players.iter().map(|player| player.team)),
            duration_seconds: document.replay.duration_seconds,
            timing: document.replay.timing,
            match_ending: document.replay.match_ending,
            winner_domination_pct: document.replay.winner_domination_pct,
            loser_domination_pct: document.replay.loser_domination_pct,
            domination_shares: document.replay.domination_shares,
            domination_anchor: document.replay.domination_anchor,
            winner: document.replay.winner,
            recorder: document.replay.recorder,
            incomplete: document.replay.incomplete,
            players: document
                .replay
                .players
                .into_iter()
                .map(|player| PlayerView {
                    id: player.id,
                    name: player.name,
                    team: player.team,
                    faction: player.faction,
                    role: player.role,
                    score: player.score,
                    left_at_seconds: player.left_at_seconds.map(f64::from),
                    score_before_leave: player.score_before_leave,
                    score_infantry: player.score_infantry,
                    score_support: player.score_support,
                    score_armor: player.score_armor,
                    score_air: player.score_air,
                    score_capturing: player.score_capturing,
                    score_fortification: player.score_fortification,
                    score_transportation: player.score_transportation,
                    score_repair: player.score_repair,
                    score_bridge_laying: player.score_bridge_laying,
                    score_unit_damage: player.score_unit_damage,
                    score_tactical_aid: player.score_tactical_aid,
                    score_total: player.score_total,
                })
                .collect(),
        };

        let mut static_player_names: HashMap<u32, String> = document
            .timeline
            .participants
            .iter()
            .filter_map(|participant| {
                participant
                    .player_name
                    .as_ref()
                    .map(|name| (participant.player_id, name.clone()))
            })
            .collect();
        for player in &overview.players {
            static_player_names
                .entry(player.id)
                .or_insert_with(|| player.name.clone());
        }
        let player_identities = PlayerIdentityDirectory::new(
            static_player_names,
            &document.timeline.participant_sessions,
        );

        let recorder_player_id = document.timeline.recorder_tactical_aid_usage.player_id;
        let recorder_view = recorder_view_label(&document.timeline.events, recorder_player_id);
        let recorder_links = link_recorder_costs(&document.timeline.events);
        let tactical_aid_rows = project_tactical_aid_rows(
            &document.timeline.events,
            &recorder_links,
            &player_identities,
        );
        let player_factions =
            PlayerFactionDirectory::new(&document.timeline.events, &overview.players);
        let mut timeline_rows = Vec::with_capacity(document.timeline.events.len());
        let mut chat_rows = Vec::new();

        for message in document.timeline.pre_match_chat {
            chat_rows.push(ChatRow {
                time_label: timestamp_label(message.time_seconds),
                stage: "Pre-match",
                player: identity(
                    message.player_id,
                    None,
                    message.player_name.as_deref(),
                    &player_identities,
                ),
                channel: message.channel.label().to_owned(),
                message: message.message,
            });
        }

        for (event_index, event) in document.timeline.events.into_iter().enumerate() {
            let time_seconds = event.time_seconds();
            let kind = event.kind();
            let description = event.description(&player_identities);
            let (command_point_id, command_point_team) = match &event {
                TimelineEventDocument::CommandPointOwnerChanged {
                    command_point_id,
                    team,
                    ..
                } => (Some(*command_point_id), Some(*team)),
                _ => (None, None),
            };
            if let TimelineEventDocument::ChatMessage {
                player_id,
                player_name,
                message,
                channel,
                ..
            } = &event
            {
                chat_rows.push(ChatRow {
                    time_label: timestamp_label(time_seconds),
                    stage: "Match",
                    player: identity(
                        *player_id,
                        Some(time_seconds),
                        player_name.as_deref(),
                        &player_identities,
                    ),
                    channel: channel.label().to_owned(),
                    message: message.clone(),
                });
            }
            let include_in_timeline = match &event {
                TimelineEventDocument::TacticalAidUsed { .. } => {
                    !recorder_links.matched_purchases.contains(&event_index)
                }
                TimelineEventDocument::TacticalAidMarker { .. }
                | TimelineEventDocument::TacticalAidDeployed { .. } => false,
                TimelineEventDocument::UnitDestroyed {
                    unit_type_id,
                    player_id,
                    team,
                    killer_player_id,
                    killer_team,
                    ..
                } => unit_destruction_is_identifiable(
                    *unit_type_id,
                    *player_id,
                    *team,
                    *killer_player_id,
                    *killer_team,
                ),
                _ => true,
            };
            if include_in_timeline {
                timeline_rows.push(TimelineRow {
                    time_seconds,
                    kind,
                    description,
                    players: event.named_players(&player_identities, &player_factions),
                    command_point_id,
                    command_point_team,
                });
            }
        }

        timeline_rows.extend(tactical_aid_rows.iter().map(|row| {
            TimelineRow {
                time_seconds: row.time_seconds,
                kind: "Tactical aid",
                players: (row.player_attribution == "Exact player")
                    .then(|| TimelinePlayer {
                        name: row.player.clone(),
                        faction: fighting_faction(&row.faction),
                    })
                    .into_iter()
                    .collect(),
                command_point_id: None,
                command_point_team: None,
                description: if row.player_attribution == "Exact player" {
                    format!("{} deployed {} ({})", row.player, row.support, row.faction)
                } else {
                    format!(
                        "{} deployed {} (player unavailable)",
                        row.player, row.support
                    )
                },
            }
        }));
        timeline_rows.sort_by(|left, right| left.time_seconds.total_cmp(&right.time_seconds));

        for message in document.timeline.post_match_chat {
            chat_rows.push(ChatRow {
                time_label: timestamp_label(message.time_seconds),
                stage: "Post-match",
                player: identity(
                    message.player_id,
                    None,
                    message.player_name.as_deref(),
                    &player_identities,
                ),
                channel: message.channel.label().to_owned(),
                message: message.message,
            });
        }

        let mut support_summaries: BTreeMap<(String, String), TacticalAidSupportView> =
            BTreeMap::new();
        for row in &tactical_aid_rows {
            let summary = support_summaries
                .entry((row.faction.clone(), row.support.clone()))
                .or_insert_with(|| TacticalAidSupportView {
                    support: row.support.clone(),
                    faction: row.faction.clone(),
                    placement_count: 0,
                    observed_costs: Vec::new(),
                });
            summary.placement_count += 1;
            if let Some(cost) = row.honors_cost {
                summary.observed_costs.push(cost);
            }
        }

        let tactical_aid_summary = TacticalAidSummaryView {
            player: document
                .timeline
                .recorder_tactical_aid_usage
                .player_id
                .map(|id| identity(id, None, None, &player_identities))
                .unwrap_or_else(|| "Unknown recorder".to_owned()),
            total_placements: u32::try_from(tactical_aid_rows.len()).unwrap_or(u32::MAX),
            supports: support_summaries.into_values().collect(),
        };

        let coverage_tactical_aid = if matches!(
            document.timeline.coverage.tactical_aid_deployments.as_str(),
            "bothFactionsTeamOnly" | "bothFactionsWithValidatedUnitDropPlayers"
        ) && document.timeline.coverage.tactical_aid_markers
            == "visibleFactionWithPlayer"
        {
            "Both factions; exact player where proven".to_owned()
        } else {
            document.timeline.coverage.tactical_aid.clone()
        };

        Self {
            score_participants: document.timeline.participant_sessions,
            overview,
            timeline_schema: document.timeline.schema_version,
            duration_seconds: document.timeline.duration_seconds,
            domination_samples: document.timeline.domination_samples,
            domination_anchor_faction: document.timeline.domination_anchor_faction,
            phases: document
                .timeline
                .phases
                .into_iter()
                .map(|phase| PhaseView {
                    index: phase.index,
                    start_seconds: phase.start_seconds,
                    end_seconds: phase.end_seconds,
                    initial_clock_seconds: phase.initial_clock_seconds,
                    final_clock_seconds: phase.final_clock_seconds,
                })
                .collect(),
            coverage_chat: document.timeline.coverage.chat,
            coverage_tactical_aid,
            recorder_view,
            timeline_rows,
            chat_rows,
            tactical_aid_rows,
            tactical_aid_summary,
        }
    }
}

#[derive(Debug, Default)]
struct PlayerIdentityDirectory {
    static_names: HashMap<u32, String>,
    sessions: HashMap<u32, Vec<ParticipantSessionDocument>>,
}

impl PlayerIdentityDirectory {
    fn new(
        static_names: HashMap<u32, String>,
        participant_sessions: &[ParticipantSessionDocument],
    ) -> Self {
        let mut sessions: HashMap<u32, Vec<ParticipantSessionDocument>> = HashMap::new();
        for session in participant_sessions {
            sessions
                .entry(session.player_id)
                .or_default()
                .push(session.clone());
        }
        for slot_sessions in sessions.values_mut() {
            slot_sessions.sort_by_key(|session| session.session_index);
        }
        Self {
            static_names,
            sessions,
        }
    }

    fn name_at(&self, player_id: u32, time_seconds: Option<f32>) -> Option<&str> {
        let Some(time_seconds) = time_seconds else {
            return self.static_names.get(&player_id).map(String::as_str);
        };
        let Some(sessions) = self.sessions.get(&player_id) else {
            return self.static_names.get(&player_id).map(String::as_str);
        };
        let active_end = sessions.partition_point(|session| session.start_seconds <= time_seconds);
        sessions
            .get(active_end.checked_sub(1)?)
            .and_then(|session| {
                session
                    .end_seconds
                    .is_none_or(|end_seconds| time_seconds < end_seconds)
                    .then_some(session.player_name.as_deref())
                    .flatten()
            })
    }
}

/// Which side a player slot was on at a given moment.
///
/// The recorded team changes are the evidence: a slot's side is whatever it was
/// last assigned at or before the event. Before a slot's first recorded
/// assignment it has no side, so an event there stays neutral rather than
/// borrowing the side the slot later held — which matters because slots are
/// reused, and the final roster entry can belong to a different occupant.
///
/// The roster is a fallback for one case only: a slot the recording never saw
/// change teams at all, which is the normal shape of a mid-join recording.
struct PlayerFactionDirectory {
    assignments: HashMap<u32, Vec<(f32, u32)>>,
    roster: HashMap<u32, u32>,
}

impl PlayerFactionDirectory {
    fn new(events: &[TimelineEventDocument], players: &[PlayerView]) -> Self {
        let mut assignments: HashMap<u32, Vec<(f32, u32)>> = HashMap::new();
        for event in events {
            if let TimelineEventDocument::PlayerJoinedTeam {
                time_seconds,
                player_id,
                team,
            } = event
            {
                assignments
                    .entry(*player_id)
                    .or_default()
                    .push((*time_seconds, *team));
            }
        }
        for slot in assignments.values_mut() {
            slot.sort_by(|left, right| left.0.total_cmp(&right.0));
        }

        let roster = players
            .iter()
            .filter_map(|player| player.team.map(|team| (player.id, team)))
            .collect();

        Self {
            assignments,
            roster,
        }
    }

    fn faction_at(&self, player_id: u32, time_seconds: f32) -> Option<&'static str> {
        if let Some(slot) = self.assignments.get(&player_id) {
            let active_end = slot.partition_point(|(assigned_at, _)| *assigned_at <= time_seconds);
            return slot
                .get(active_end.checked_sub(1)?)
                .and_then(|(_, team)| faction_of_team(*team));
        }
        self.roster
            .get(&player_id)
            .copied()
            .and_then(faction_of_team)
    }
}

fn identity(
    player_id: u32,
    time_seconds: Option<f32>,
    event_name: Option<&str>,
    player_identities: &PlayerIdentityDirectory,
) -> String {
    event_name
        .or_else(|| player_identities.name_at(player_id, time_seconds))
        .map_or_else(|| "Unknown player".to_owned(), str::to_owned)
}

fn support_label(support_id: u32, support_name: Option<&str>) -> String {
    let Some(name) = support_name else {
        return format!("Tactical aid 0x{support_id:08x}");
    };
    let base_name = name
        .strip_suffix("_NATO_British")
        .or_else(|| name.strip_suffix("_USSR"))
        .or_else(|| name.strip_suffix("_NATO"))
        .or_else(|| name.strip_suffix("_US"))
        .unwrap_or(name);
    match base_name {
        "RadarScan" => "Aerial Recon".to_owned(),
        "Paratrooper" => "Airborne Infantry".to_owned(),
        "Repair_Jeep" => "Airdropped Transport".to_owned(),
        "Light_Tank" => "Airdropped Light Tank".to_owned(),
        "BridgeRepairer" => "Repair Bridge".to_owned(),
        "Napalmstrike" => "Napalm Strike".to_owned(),
        "Tankbuster" => "Tank Buster".to_owned(),
        "Bunkerbuster" => "Laser Guided Bomb".to_owned(),
        "AntiAirstrike" => "Air-to-Air Strike".to_owned(),
        "GasAttack" => "Chemical Strike".to_owned(),
        "HeavyAirSupport" => "Heavy Air Support".to_owned(),
        "LightArtilleryBarrage" => "Light Artillery Barrage".to_owned(),
        "Artillery" => "Precision Artillery".to_owned(),
        "HeavyArtilleryBarrage" => "Heavy Artillery Barrage".to_owned(),
        "ClusterBomb" | "Airstrike" => "Airstrike".to_owned(),
        "DaisyCutter" if name.ends_with("_USSR") => "Fuel Air Bomb".to_owned(),
        "DaisyCutter" => "Daisy Cutter Bomb".to_owned(),
        "CarpetBombing" => "Carpet Bombing".to_owned(),
        "TacticalNuke" => "Nuclear Strike".to_owned(),
        _ => human_identifier(name),
    }
}

fn support_faction(support_name: &str) -> &'static str {
    if support_name.ends_with("_NATO_British") || support_name.ends_with("_NATO") {
        "NATO"
    } else if support_name.ends_with("_USSR") {
        "USSR"
    } else if support_name.ends_with("_US") {
        "USA"
    } else {
        "Invalid faction data"
    }
}

type SupportEffectKey = (u32, u32, u32, u32);

fn support_effect_key(support_id: u32, position: [f32; 3]) -> SupportEffectKey {
    (
        support_id,
        position[0].to_bits(),
        position[1].to_bits(),
        position[2].to_bits(),
    )
}

fn deployment_time(time_seconds: f32, age_seconds: f32) -> f32 {
    (time_seconds - age_seconds).max(0.0)
}

fn marker_row(
    marker_index: usize,
    event: &TimelineEventDocument,
    faction: String,
    recorder_links: &RecorderMarkerLinks,
    player_identities: &PlayerIdentityDirectory,
) -> TacticalAidRow {
    let TimelineEventDocument::TacticalAidMarker {
        time_seconds,
        support_id,
        support_name: Some(support_name),
        position,
        player_id,
        ..
    } = event
    else {
        unreachable!("marker projection accepts named markers only");
    };
    TacticalAidRow {
        time_seconds: *time_seconds,
        support: support_label(*support_id, Some(support_name)),
        player: identity(*player_id, Some(*time_seconds), None, player_identities),
        faction,
        player_attribution: "Exact player",
        honors_cost: recorder_links.marker_costs.get(&marker_index).copied(),
        position: *position,
    }
}

/// The actor for a deployment whose exact player was never proven.
///
/// Naming the side is the honest reading: a strike is always some player's, and
/// the recording does prove which side fired it, where "Unknown player" reads as
/// a gap in the data. It is phrased as a collective so it cannot be mistaken for
/// a player called `USA`. Any other value is invalid canonical parser data and is
/// filtered before presentation.
fn team_actor(faction: &str) -> String {
    match faction {
        "USA" | "NATO" | "USSR" => format!("Team {faction}"),
        _ => "Invalid faction data".to_owned(),
    }
}

fn deployment_row(
    event: &TimelineEventDocument,
    player_identities: &PlayerIdentityDirectory,
) -> TacticalAidRow {
    let TimelineEventDocument::TacticalAidDeployed {
        time_seconds,
        support_id,
        support_name,
        position,
        team,
        age_seconds,
        player_id,
        player_attribution,
        ..
    } = event
    else {
        unreachable!("deployment projection accepts deployment events only");
    };
    let exact_player_id = (*player_attribution
        == Some(TacticalAidPlayerAttributionDocument::UnitSpawnOwnership))
    .then_some(*player_id)
    .flatten();
    let faction = team_label(*team);
    TacticalAidRow {
        time_seconds: deployment_time(*time_seconds, *age_seconds),
        support: support_label(*support_id, Some(support_name)),
        player: exact_player_id
            .map(|id| identity(id, Some(*time_seconds), None, player_identities))
            .unwrap_or_else(|| team_actor(faction)),
        faction: faction.to_owned(),
        player_attribution: if exact_player_id.is_some() {
            "Exact player"
        } else {
            "Team only"
        },
        honors_cost: None,
        position: *position,
    }
}

/// Project raw player markers and both-faction deployment effects into one table.
///
/// Equal exact support/position groups are paired in replay order. The deployment
/// proves the faction and the marker proves the player. A group with unequal
/// cardinality remains deployment-only so the presentation never guesses which
/// player-bearing marker belongs to which effect. Marker-only groups are retained.
fn project_tactical_aid_rows(
    events: &[TimelineEventDocument],
    recorder_links: &RecorderMarkerLinks,
    player_identities: &PlayerIdentityDirectory,
) -> Vec<TacticalAidRow> {
    let mut markers: HashMap<SupportEffectKey, Vec<usize>> = HashMap::new();
    let mut deployments: HashMap<SupportEffectKey, Vec<usize>> = HashMap::new();
    // Stable key traversal keeps equal-time rows deterministic across repeated
    // projections and across the platform-specific standard-library hash seeds.
    let mut keys = BTreeSet::new();

    for (index, event) in events.iter().enumerate() {
        match event {
            TimelineEventDocument::TacticalAidMarker {
                support_id,
                support_name: Some(_),
                position,
                ..
            } => {
                let key = support_effect_key(*support_id, *position);
                keys.insert(key);
                markers.entry(key).or_default().push(index);
            }
            TimelineEventDocument::TacticalAidDeployed {
                support_id,
                position,
                ..
            } => {
                let key = support_effect_key(*support_id, *position);
                keys.insert(key);
                deployments.entry(key).or_default().push(index);
            }
            _ => {}
        }
    }

    let mut rows = Vec::new();
    for key in keys {
        let marker_indexes = markers.remove(&key).unwrap_or_default();
        let deployment_indexes = deployments.remove(&key).unwrap_or_default();
        if !marker_indexes.is_empty() && marker_indexes.len() == deployment_indexes.len() {
            for (marker_index, deployment_index) in
                marker_indexes.into_iter().zip(deployment_indexes)
            {
                let faction = match &events[deployment_index] {
                    TimelineEventDocument::TacticalAidDeployed { team, .. } => {
                        team_label(*team).to_owned()
                    }
                    _ => unreachable!(),
                };
                rows.push(marker_row(
                    marker_index,
                    &events[marker_index],
                    faction,
                    recorder_links,
                    player_identities,
                ));
            }
        } else if !deployment_indexes.is_empty() {
            rows.extend(
                deployment_indexes
                    .into_iter()
                    .map(|index| deployment_row(&events[index], player_identities)),
            );
        } else {
            rows.extend(marker_indexes.into_iter().map(|index| {
                let faction = match &events[index] {
                    TimelineEventDocument::TacticalAidMarker {
                        support_name: Some(support_name),
                        ..
                    } => support_faction(support_name).to_owned(),
                    _ => unreachable!(),
                };
                marker_row(
                    index,
                    &events[index],
                    faction,
                    recorder_links,
                    player_identities,
                )
            }));
        }
    }
    rows.sort_by(|left, right| left.time_seconds.total_cmp(&right.time_seconds));
    // The canonical parser rejects deployment teams outside USA/NATO/USSR and
    // resolves marker names from a faction-qualified allowlist. Keep malformed,
    // stale, or future cached documents from exposing an impossible faction row.
    rows.retain(|row| fighting_faction(&row.faction).is_some());
    rows
}

fn recorder_view_label(
    events: &[TimelineEventDocument],
    recorder_player_id: Option<u32>,
) -> String {
    let Some(recorder_player_id) = recorder_player_id else {
        return "Unknown POV".to_owned();
    };
    let modes: HashSet<String> = events
        .iter()
        .filter_map(|event| match event {
            TimelineEventDocument::SpectatorViewChanged {
                player_id,
                team,
                view,
                ..
            } if *player_id == recorder_player_id => Some(view.label(*team)),
            _ => None,
        })
        .collect();
    let played = events.iter().any(|event| {
        matches!(event,
        TimelineEventDocument::PlayerJoinedTeam { player_id, team, .. }
            if *player_id == recorder_player_id && faction_of_team(*team).is_some())
    });
    if played && !modes.is_empty() {
        return "Player / spectator POV".to_owned();
    }
    match modes.len() {
        0 => "Player POV".to_owned(),
        1 => format!(
            "Spectator · {}",
            modes.into_iter().next().unwrap_or_default()
        ),
        _ => "Spectator · mixed views".to_owned(),
    }
}

type SupportPlacementKey = (u32, u32, u32, u32, u32);

#[derive(Default)]
struct RecorderMarkerLinks {
    marker_costs: HashMap<usize, f32>,
    matched_purchases: HashSet<usize>,
}

fn support_placement_key(
    support_id: u32,
    position: [f32; 3],
    player_id: u32,
) -> SupportPlacementKey {
    (
        support_id,
        position[0].to_bits(),
        position[1].to_bits(),
        position[2].to_bits(),
        player_id,
    )
}

/// Conservatively link recorder purchases to player-bearing marker effects.
///
/// Within an exact support/position/player group, replay stream order makes the
/// candidate sets nested. Walking purchases newest-first yields a link only when
/// exactly one still-unassigned later marker exists. Multiple candidates leave the
/// connected older portion unlinked rather than choosing the nearest timestamp.
fn link_recorder_costs(events: &[TimelineEventDocument]) -> RecorderMarkerLinks {
    let mut purchases: HashMap<SupportPlacementKey, Vec<(usize, f32)>> = HashMap::new();
    let mut markers: HashMap<SupportPlacementKey, Vec<usize>> = HashMap::new();

    for (event_index, event) in events.iter().enumerate() {
        match event {
            TimelineEventDocument::TacticalAidUsed {
                support_id,
                honors_cost,
                position,
                player_id: Some(player_id),
                ..
            } => purchases
                .entry(support_placement_key(*support_id, *position, *player_id))
                .or_default()
                .push((event_index, *honors_cost)),
            TimelineEventDocument::TacticalAidMarker {
                support_id,
                support_name: Some(_),
                position,
                player_id,
                ..
            } => markers
                .entry(support_placement_key(*support_id, *position, *player_id))
                .or_default()
                .push(event_index),
            _ => {}
        }
    }

    let mut links = RecorderMarkerLinks::default();
    for (key, mut grouped_purchases) in purchases {
        let Some(mut available_markers) = markers.remove(&key) else {
            continue;
        };
        grouped_purchases.sort_unstable_by_key(|(event_index, _)| *event_index);
        available_markers.sort_unstable();

        let Some(last_marker_index) = available_markers.last().copied() else {
            continue;
        };
        let purchase_end = grouped_purchases
            .partition_point(|(purchase_index, _)| *purchase_index <= last_marker_index);
        grouped_purchases.truncate(purchase_end);
        let Some(first_purchase_index) = grouped_purchases
            .first()
            .map(|(event_index, _)| *event_index)
        else {
            continue;
        };
        let marker_start =
            available_markers.partition_point(|marker_index| *marker_index < first_purchase_index);
        let available_markers = &available_markers[marker_start..];
        if grouped_purchases.len() != available_markers.len() {
            continue;
        }

        let mut group_links = Vec::with_capacity(grouped_purchases.len());
        for (position, ((purchase_index, honors_cost), marker_index)) in grouped_purchases
            .into_iter()
            .zip(available_markers.iter().copied())
            .enumerate()
        {
            let has_unique_later_marker = marker_index >= purchase_index
                && (position == 0 || available_markers[position - 1] < purchase_index);
            if !has_unique_later_marker {
                group_links.clear();
                break;
            }
            group_links.push((purchase_index, marker_index, honors_cost));
        }
        for (purchase_index, marker_index, honors_cost) in group_links {
            links.marker_costs.insert(marker_index, honors_cost);
            links.matched_purchases.insert(purchase_index);
        }
    }
    links
}

/// The tactical-aid projection labels a row with whatever it could prove, which
/// includes the placeholders `support_faction` and `team_label` fall back to.
/// Keep those out of the row's faction so an unproven side stays neutral.
fn fighting_faction(label: &str) -> Option<String> {
    matches!(label, "USA" | "NATO" | "USSR").then(|| label.to_owned())
}

/// The faction name for a fighting side, or `None` for spectators and for any
/// team value the recording does not resolve to one.
const fn faction_of_team(team: u32) -> Option<&'static str> {
    match team {
        1 => Some("USA"),
        2 => Some("NATO"),
        3 => Some("USSR"),
        _ => None,
    }
}

/// An event timestamp as `M:SS.s`.
///
/// Timestamps run to the length of a recording, where raw seconds stop being
/// readable well before the end of a round. Tenths are kept because the events
/// resolve to them and simultaneous ones need to stay distinguishable.
///
/// The whole value is converted to tenths before it is split, so a timestamp
/// just short of a minute cannot round into a `0:60.0`.
fn timestamp_label(seconds: f32) -> String {
    let tenths = (f64::from(seconds.max(0.0)) * 10.0).round() as u64;
    let minutes = tenths / 600;
    let seconds = (tenths % 600) / 10;
    let remainder = tenths % 10;
    format!("{minutes}:{seconds:02}.{remainder}")
}

fn team_label(team: u32) -> &'static str {
    match team {
        0 => "spectators",
        1 => "USA",
        2 => "NATO",
        3 => "USSR",
        _ => "Invalid faction data",
    }
}

fn command_point_description(team: u32) -> String {
    if team == 0 {
        "Command point became neutral".to_owned()
    } else {
        format!("{} captured a command point", team_label(team))
    }
}

/// Human-readable categories for the nine multiplayer command-point fortifications.
/// IDs are Adler-32 hashes of the shipped `*_fortification_*` unit type names.
fn fortification_type_label(unit_type_id: u32) -> Option<&'static str> {
    match unit_type_id {
        0x77d7_09c0 | 0x85aa_0a4a | 0x884f_0a65 => Some("anti-air fortification"),
        0x822e_0a32 | 0x908b_0abc | 0x934b_0ad7 => Some("anti-tank fortification"),
        0x972d_0af7 | 0xa69e_0b81 | 0xa994_0b9c => Some("machine-gun fortification"),
        _ => None,
    }
}

fn fortification_destroyed_description(unit_type_id: u32, actor: Option<&str>) -> Option<String> {
    let label = fortification_type_label(unit_type_id)?;
    let article = if label.starts_with("anti-") {
        "an"
    } else {
        "a"
    };
    Some(match actor {
        Some(actor) => format!("{actor} destroyed {article} {label}"),
        None => {
            let article = if article == "an" { "An" } else { "A" };
            format!("{article} {label} was destroyed")
        }
    })
}

fn unit_destruction_is_identifiable(
    unit_type_id: Option<u32>,
    player_id: Option<u32>,
    team: Option<u32>,
    killer_player_id: Option<u32>,
    killer_team: Option<u32>,
) -> bool {
    player_id.is_some()
        || team.is_some()
        || killer_player_id.is_some()
        || killer_team.is_some()
        || unit_type_id.is_some_and(|id| fortification_type_label(id).is_some())
}

fn human_identifier(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_lowercase = false;
    for character in value.chars() {
        if character == '_' {
            output.push(' ');
            previous_lowercase = false;
        } else {
            if character.is_uppercase() && previous_lowercase {
                output.push(' ');
            }
            output.push(character);
            previous_lowercase = character.is_lowercase();
        }
    }
    output
}

fn server_mode_labels(classification: &ServerClassificationDocument) -> Vec<String> {
    server_mode::labels(
        classification.few_player_mode,
        classification.match_mode,
        classification.has_bots,
        classification.clan_match,
        classification.tournament_match,
        classification.ranked,
    )
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DetailDocument {
    #[serde(default)]
    hidden_player_ids: Vec<u32>,
    #[serde(default)]
    player_result_evidence: Vec<parser::PlayerResultEvidence>,
    replay: ReplayDocument,
    timeline: TimelineDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplayDocument {
    game_info: GameInfoDocument,
    #[serde(default)]
    server_classification: ServerClassificationDocument,
    players: Vec<PlayerDocument>,
    duration_seconds: Option<f32>,
    #[serde(default)]
    timing: MatchTimingView,
    #[serde(default = "unknown_ending")]
    match_ending: String,
    winner_domination_pct: Option<f64>,
    loser_domination_pct: Option<f64>,
    #[serde(default)]
    domination_shares: Option<Vec<DominationShareView>>,
    #[serde(default)]
    domination_anchor: Option<String>,
    winner: Option<String>,
    incomplete: bool,
    recorder: Option<String>,
}

fn unknown_ending() -> String {
    "unknown".to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerClassificationDocument {
    few_player_mode: Option<bool>,
    match_mode: Option<bool>,
    has_bots: Option<bool>,
    clan_match: Option<bool>,
    tournament_match: Option<bool>,
    ranked: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameInfoDocument {
    map_name: String,
    map_display_name: String,
    server_name: String,
    date_time: String,
    game_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerDocument {
    id: u32,
    name: String,
    team: Option<u32>,
    faction: Option<String>,
    role: Option<String>,
    score: Option<i32>,
    #[serde(default)]
    score_before_leave: Option<parser::ScoreBeforeLeave>,
    // Match the parser width before widening for the public view. Reading its
    // shortest JSON decimal directly as f64 changes the recorded timestamp.
    #[serde(default)]
    left_at_seconds: Option<f32>,
    score_infantry: Option<i32>,
    score_support: Option<i32>,
    score_armor: Option<i32>,
    score_air: Option<i32>,
    score_capturing: Option<i32>,
    score_fortification: Option<i32>,
    score_transportation: Option<i32>,
    score_repair: Option<i32>,
    score_bridge_laying: Option<i32>,
    score_unit_damage: Option<i32>,
    score_tactical_aid: Option<i32>,
    score_total: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineDocument {
    schema_version: u32,
    /// Length of the recording: the axis every timeline timestamp lives on.
    duration_seconds: f32,
    phases: Vec<PhaseDocument>,
    #[serde(default)]
    domination_samples: Vec<TimelineValueSampleView>,
    #[serde(default)]
    domination_anchor_faction: Option<String>,
    participants: Vec<ParticipantDocument>,
    #[serde(default)]
    participant_sessions: Vec<ParticipantSessionDocument>,
    events: Vec<TimelineEventDocument>,
    pre_match_chat: Vec<PreMatchChatDocument>,
    post_match_chat: Vec<PostMatchChatDocument>,
    coverage: CoverageDocument,
    recorder_tactical_aid_usage: TacticalAidSummaryDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhaseDocument {
    index: u32,
    start_seconds: f32,
    end_seconds: f32,
    initial_clock_seconds: f32,
    final_clock_seconds: f32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParticipantDocument {
    player_id: u32,
    player_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantSessionDocument {
    player_id: u32,
    session_index: u32,
    player_name: Option<String>,
    start_seconds: f32,
    end_seconds: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
    dead_code,
    reason = "`seconds_before_match` is retained as the parser's other reading of the same moment; the view reads the recording axis"
)]
struct PreMatchChatDocument {
    time_seconds: f32,
    seconds_before_match: f32,
    player_id: u32,
    player_name: Option<String>,
    message: String,
    channel: ChatChannelDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
    dead_code,
    reason = "`seconds_after_match` is retained as the parser's other reading of the same moment; the view reads the recording axis"
)]
struct PostMatchChatDocument {
    time_seconds: f32,
    seconds_after_match: f32,
    player_id: u32,
    player_name: Option<String>,
    message: String,
    channel: ChatChannelDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverageDocument {
    chat: String,
    tactical_aid: String,
    tactical_aid_markers: String,
    tactical_aid_deployments: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TacticalAidSummaryDocument {
    player_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ChatChannelDocument {
    All,
    Team,
}

impl ChatChannelDocument {
    const fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Team => "Team",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SpectatorViewDocument {
    None,
    OneTeam,
    AllTeams,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TacticalAidPlayerAttributionDocument {
    UnitSpawnOwnership,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum UnitDestructionCauseDocument {
    #[default]
    Unknown,
    Unit,
    TacticalAid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[allow(
    dead_code,
    reason = "raw parser context IDs are retained while prose uses only the proven context kind"
)]
enum UnitDestructionContextDocument {
    BuildingCollapse { building_id: u32 },
    DestroyedWithContainer { container_unit_id: u32 },
}

impl UnitDestructionContextDocument {
    const fn suffix(self) -> &'static str {
        match self {
            Self::BuildingCollapse { .. } => " when its occupied building collapsed",
            Self::DestroyedWithContainer { .. } => " with its transport",
        }
    }
}

impl SpectatorViewDocument {
    fn label(self, team: u32) -> String {
        match self {
            Self::None => "no team".to_owned(),
            Self::OneTeam => team_label(team).to_owned(),
            Self::AllTeams => "all teams".to_owned(),
            Self::Unknown => "an unknown mode".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[allow(
    dead_code,
    reason = "raw parser fields are intentionally retained while the view projection hides IDs"
)]
enum TimelineEventDocument {
    PlayerEntered {
        time_seconds: f32,
        player_id: u32,
    },
    PlayerLeft {
        time_seconds: f32,
        player_id: u32,
    },
    PlayerJoinedTeam {
        time_seconds: f32,
        player_id: u32,
        team: u32,
    },
    SpectatorViewChanged {
        time_seconds: f32,
        player_id: u32,
        team: u32,
        spectator_los: u32,
        view: SpectatorViewDocument,
    },
    PlayerSetRole {
        time_seconds: f32,
        player_id: u32,
        role_id: u32,
        role: Option<String>,
    },
    CommandPointOwnerChanged {
        time_seconds: f32,
        command_point_id: u32,
        team: u32,
    },
    UnitDestroyed {
        time_seconds: f32,
        unit_id: u32,
        unit_type_id: Option<u32>,
        player_id: Option<u32>,
        team: Option<u32>,
        killer_unit_id: Option<u32>,
        killer_player_id: Option<u32>,
        killer_team: Option<u32>,
        #[serde(default)]
        cause: UnitDestructionCauseDocument,
        #[serde(default)]
        destruction_context: Option<UnitDestructionContextDocument>,
        tactical_aid_support_id: Option<u32>,
        tactical_aid_support_name: Option<String>,
    },
    TacticalAidTransferred {
        time_seconds: f32,
        from_player_id: u32,
        to_player_id: u32,
        amount: u32,
    },
    TacticalAidDamageThreshold {
        time_seconds: f32,
        player_id: u32,
        target_player_id: u32,
        ta_index: u32,
        support_id: Option<u32>,
        support_name: Option<String>,
        upgrade_level: u32,
    },
    TacticalAidUsed {
        time_seconds: f32,
        support_id: u32,
        support_name: Option<String>,
        honors_cost: f32,
        position: [f32; 3],
        player_id: Option<u32>,
    },
    TacticalAidMarker {
        time_seconds: f32,
        event_id: u32,
        support_id: u32,
        support_name: Option<String>,
        position: [f32; 3],
        player_id: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        duration_seconds: f32,
    },
    TacticalAidDeployed {
        time_seconds: f32,
        support_id: u32,
        support_name: String,
        position: [f32; 3],
        team: u32,
        upgrade_level: u32,
        direction: [f32; 3],
        age_seconds: f32,
        player_id: Option<u32>,
        player_attribution: Option<TacticalAidPlayerAttributionDocument>,
    },
    ChatMessage {
        time_seconds: f32,
        player_id: u32,
        player_name: Option<String>,
        message: String,
        channel: ChatChannelDocument,
    },
    VoteStarted {
        time_seconds: f32,
        player_id: u32,
        vote_id: u32,
        float_value: f32,
        int_value: i32,
        uint_value: u32,
    },
    TeamWon {
        time_seconds: f32,
        team: u32,
        win_type: u32,
    },
}

impl From<parser::ReplayWithTimeline> for DetailDocument {
    fn from(document: parser::ReplayWithTimeline) -> Self {
        let replay = document.replay;
        let timeline = document.timeline;
        Self {
            hidden_player_ids: Vec::new(),
            player_result_evidence: Vec::new(),
            replay: ReplayDocument {
                game_info: GameInfoDocument {
                    map_name: replay.game_info.map_name,
                    map_display_name: replay.game_info.map_display_name,
                    server_name: replay.game_info.server_name,
                    date_time: replay.game_info.date_time,
                    game_mode: replay.game_info.game_mode,
                },
                server_classification: ServerClassificationDocument {
                    few_player_mode: replay.server_classification.few_player_mode,
                    match_mode: replay.server_classification.match_mode,
                    has_bots: replay.server_classification.has_bots,
                    clan_match: replay.server_classification.clan_match,
                    tournament_match: replay.server_classification.tournament_match,
                    ranked: replay.server_classification.ranked,
                },
                players: replay
                    .players
                    .into_iter()
                    .map(|player| PlayerDocument {
                        id: player.id,
                        name: player.name,
                        team: player.team,
                        faction: player.faction,
                        role: player.role,
                        score: player.score,
                        score_before_leave: None,
                        left_at_seconds: player.left_at_seconds,
                        score_infantry: player.score_infantry,
                        score_support: player.score_support,
                        score_armor: player.score_armor,
                        score_air: player.score_air,
                        score_capturing: player.score_capturing,
                        score_fortification: player.score_fortification,
                        score_transportation: player.score_transportation,
                        score_repair: player.score_repair,
                        score_bridge_laying: player.score_bridge_laying,
                        score_unit_damage: player.score_unit_damage,
                        score_tactical_aid: player.score_tactical_aid,
                        score_total: player.score_total,
                    })
                    .collect(),
                duration_seconds: replay.duration_seconds,
                timing: MatchTimingView {
                    captured_match_start: replay.timing.captured_match_start,
                    recording_seconds: replay.timing.recording_seconds,
                    observed_gameplay_seconds: replay.timing.observed_gameplay_seconds,
                    match_elapsed_seconds: replay.timing.match_elapsed_seconds,
                    round_length_seconds: replay.timing.round_length_seconds,
                    round_length_exact: replay.timing.round_length_exact,
                    joined_at_remaining_seconds: replay.timing.joined_at_remaining_seconds,
                    final_remaining_seconds: replay.timing.final_remaining_seconds,
                },
                match_ending: match replay.match_ending {
                    parser::MatchEnding::TotalDomination => "totalDomination",
                    parser::MatchEnding::Timeout => "timeout",
                    parser::MatchEnding::Forfeit => "forfeit",
                    parser::MatchEnding::Unknown => "unknown",
                }
                .to_owned(),
                winner_domination_pct: replay.winner_domination_pct.map(json_normalized_f64),
                loser_domination_pct: replay.loser_domination_pct.map(json_normalized_f64),
                domination_shares: replay.domination_shares.map(|shares| {
                    shares
                        .into_iter()
                        .map(|share| DominationShareView {
                            faction: share.faction,
                            pct: json_normalized_f64(share.pct),
                        })
                        .collect()
                }),
                domination_anchor: replay.domination_anchor.map(|anchor| {
                    match anchor {
                        parser::DominationAnchor::PovTeam => "povTeam",
                        parser::DominationAnchor::WinnerInferred => "winnerInferred",
                    }
                    .to_owned()
                }),
                winner: replay.winner,
                incomplete: replay.incomplete,
                recorder: replay.recorder,
            },
            timeline: TimelineDocument {
                schema_version: timeline.schema_version,
                duration_seconds: timeline.duration_seconds,
                domination_samples: timeline
                    .domination_samples
                    .into_iter()
                    .map(|sample| TimelineValueSampleView {
                        time_seconds: sample.time_seconds,
                        value: sample.value,
                    })
                    .collect(),
                domination_anchor_faction: timeline.domination_anchor_faction,
                phases: timeline
                    .phases
                    .into_iter()
                    .map(|phase| PhaseDocument {
                        index: phase.index,
                        start_seconds: phase.start_seconds,
                        end_seconds: phase.end_seconds,
                        initial_clock_seconds: phase.initial_clock_seconds,
                        final_clock_seconds: phase.final_clock_seconds,
                    })
                    .collect(),
                participants: timeline
                    .participants
                    .into_iter()
                    .map(|participant| ParticipantDocument {
                        player_id: participant.player_id,
                        player_name: participant.player_name,
                    })
                    .collect(),
                participant_sessions: timeline
                    .participant_sessions
                    .into_iter()
                    .map(|session| ParticipantSessionDocument {
                        player_id: session.player_id,
                        session_index: session.session_index,
                        player_name: session.player_name,
                        start_seconds: session.start_seconds,
                        end_seconds: session.end_seconds,
                    })
                    .collect(),
                events: timeline.events.into_iter().map(Into::into).collect(),
                pre_match_chat: timeline
                    .pre_match_chat
                    .into_iter()
                    .map(|message| PreMatchChatDocument {
                        time_seconds: message.time_seconds,
                        seconds_before_match: message.seconds_before_match,
                        player_id: message.player_id,
                        player_name: message.player_name,
                        message: message.message,
                        channel: message.channel.into(),
                    })
                    .collect(),
                post_match_chat: timeline
                    .post_match_chat
                    .into_iter()
                    .map(|message| PostMatchChatDocument {
                        time_seconds: message.time_seconds,
                        seconds_after_match: message.seconds_after_match,
                        player_id: message.player_id,
                        player_name: message.player_name,
                        message: message.message,
                        channel: message.channel.into(),
                    })
                    .collect(),
                coverage: CoverageDocument {
                    chat: timeline.coverage.chat.to_owned(),
                    tactical_aid: timeline.coverage.tactical_aid.to_owned(),
                    tactical_aid_markers: timeline.coverage.tactical_aid_markers.to_owned(),
                    tactical_aid_deployments: timeline.coverage.tactical_aid_deployments.to_owned(),
                },
                recorder_tactical_aid_usage: TacticalAidSummaryDocument {
                    player_id: timeline.recorder_tactical_aid_usage.player_id,
                },
            },
        }
    }
}

/// Preserve the exact floating-point value exposed by the legacy JSON cache
/// route. `serde_json`'s shortest decimal representation can round a computed
/// `f64` to an adjacent value when it is parsed back.
fn json_normalized_f64(value: f64) -> f64 {
    serde_json::to_string(&value)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(value)
}

impl From<parser::ChatChannel> for ChatChannelDocument {
    fn from(channel: parser::ChatChannel) -> Self {
        match channel {
            parser::ChatChannel::All => Self::All,
            parser::ChatChannel::Team => Self::Team,
        }
    }
}

impl From<parser::SpectatorView> for SpectatorViewDocument {
    fn from(view: parser::SpectatorView) -> Self {
        match view {
            parser::SpectatorView::None => Self::None,
            parser::SpectatorView::OneTeam => Self::OneTeam,
            parser::SpectatorView::AllTeams => Self::AllTeams,
            parser::SpectatorView::Unknown => Self::Unknown,
        }
    }
}

impl From<parser::TacticalAidPlayerAttribution> for TacticalAidPlayerAttributionDocument {
    fn from(attribution: parser::TacticalAidPlayerAttribution) -> Self {
        match attribution {
            parser::TacticalAidPlayerAttribution::UnitSpawnOwnership => Self::UnitSpawnOwnership,
        }
    }
}

impl From<parser::UnitDestructionCause> for UnitDestructionCauseDocument {
    fn from(cause: parser::UnitDestructionCause) -> Self {
        match cause {
            parser::UnitDestructionCause::Unknown => Self::Unknown,
            parser::UnitDestructionCause::Unit => Self::Unit,
            parser::UnitDestructionCause::TacticalAid => Self::TacticalAid,
        }
    }
}

impl From<parser::UnitDestructionContext> for UnitDestructionContextDocument {
    fn from(context: parser::UnitDestructionContext) -> Self {
        match context {
            parser::UnitDestructionContext::BuildingCollapse { building_id } => {
                Self::BuildingCollapse { building_id }
            }
            parser::UnitDestructionContext::DestroyedWithContainer { container_unit_id } => {
                Self::DestroyedWithContainer { container_unit_id }
            }
        }
    }
}

impl From<parser::TimelineEvent> for TimelineEventDocument {
    fn from(event: parser::TimelineEvent) -> Self {
        match event {
            parser::TimelineEvent::PlayerEntered {
                time_seconds,
                player_id,
            } => Self::PlayerEntered {
                time_seconds,
                player_id,
            },
            parser::TimelineEvent::PlayerLeft {
                time_seconds,
                player_id,
            } => Self::PlayerLeft {
                time_seconds,
                player_id,
            },
            parser::TimelineEvent::PlayerJoinedTeam {
                time_seconds,
                player_id,
                team,
            } => Self::PlayerJoinedTeam {
                time_seconds,
                player_id,
                team,
            },
            parser::TimelineEvent::SpectatorViewChanged {
                time_seconds,
                player_id,
                team,
                spectator_los,
                view,
            } => Self::SpectatorViewChanged {
                time_seconds,
                player_id,
                team,
                spectator_los,
                view: view.into(),
            },
            parser::TimelineEvent::PlayerSetRole {
                time_seconds,
                player_id,
                role_id,
                role,
            } => Self::PlayerSetRole {
                time_seconds,
                player_id,
                role_id,
                role,
            },
            parser::TimelineEvent::CommandPointOwnerChanged {
                time_seconds,
                command_point_id,
                team,
            } => Self::CommandPointOwnerChanged {
                time_seconds,
                command_point_id,
                team,
            },
            parser::TimelineEvent::UnitDestroyed {
                time_seconds,
                unit_id,
                unit_type_id,
                player_id,
                team,
                killer_unit_id,
                killer_player_id,
                killer_team,
                cause,
                destruction_context,
                tactical_aid_support_id,
                tactical_aid_support_name,
            } => Self::UnitDestroyed {
                time_seconds,
                unit_id,
                unit_type_id,
                player_id,
                team,
                killer_unit_id,
                killer_player_id,
                killer_team,
                cause: cause.into(),
                destruction_context: destruction_context.map(Into::into),
                tactical_aid_support_id,
                tactical_aid_support_name,
            },
            parser::TimelineEvent::TacticalAidTransferred {
                time_seconds,
                from_player_id,
                to_player_id,
                amount,
            } => Self::TacticalAidTransferred {
                time_seconds,
                from_player_id,
                to_player_id,
                amount,
            },
            parser::TimelineEvent::TacticalAidDamageThreshold {
                time_seconds,
                player_id,
                target_player_id,
                ta_index,
                support_id,
                support_name,
                upgrade_level,
            } => Self::TacticalAidDamageThreshold {
                time_seconds,
                player_id,
                target_player_id,
                ta_index,
                support_id,
                support_name,
                upgrade_level,
            },
            parser::TimelineEvent::TacticalAidUsed {
                time_seconds,
                support_id,
                support_name,
                honors_cost,
                position,
                player_id,
            } => Self::TacticalAidUsed {
                time_seconds,
                support_id,
                support_name,
                honors_cost,
                position,
                player_id,
            },
            parser::TimelineEvent::TacticalAidMarker {
                time_seconds,
                event_id,
                support_id,
                support_name,
                position,
                player_id,
                upgrade_level,
                direction,
                duration_seconds,
            } => Self::TacticalAidMarker {
                time_seconds,
                event_id,
                support_id,
                support_name,
                position,
                player_id,
                upgrade_level,
                direction,
                duration_seconds,
            },
            parser::TimelineEvent::TacticalAidDeployed {
                time_seconds,
                support_id,
                support_name,
                position,
                team,
                upgrade_level,
                direction,
                age_seconds,
                player_id,
                player_attribution,
            } => Self::TacticalAidDeployed {
                time_seconds,
                support_id,
                support_name,
                position,
                team,
                upgrade_level,
                direction,
                age_seconds,
                player_id,
                player_attribution: player_attribution.map(Into::into),
            },
            parser::TimelineEvent::ChatMessage {
                time_seconds,
                player_id,
                player_name,
                message,
                channel,
            } => Self::ChatMessage {
                time_seconds,
                player_id,
                player_name,
                message,
                channel: channel.into(),
            },
            parser::TimelineEvent::VoteStarted {
                time_seconds,
                player_id,
                vote_id,
                float_value,
                int_value,
                uint_value,
            } => Self::VoteStarted {
                time_seconds,
                player_id,
                vote_id,
                float_value,
                int_value,
                uint_value,
            },
            parser::TimelineEvent::TeamWon {
                time_seconds,
                team,
                win_type,
            } => Self::TeamWon {
                time_seconds,
                team,
                win_type,
            },
        }
    }
}

impl TimelineEventDocument {
    fn time_seconds(&self) -> f32 {
        match self {
            Self::PlayerEntered { time_seconds, .. }
            | Self::PlayerLeft { time_seconds, .. }
            | Self::PlayerJoinedTeam { time_seconds, .. }
            | Self::SpectatorViewChanged { time_seconds, .. }
            | Self::PlayerSetRole { time_seconds, .. }
            | Self::CommandPointOwnerChanged { time_seconds, .. }
            | Self::UnitDestroyed { time_seconds, .. }
            | Self::TacticalAidTransferred { time_seconds, .. }
            | Self::TacticalAidDamageThreshold { time_seconds, .. }
            | Self::TacticalAidUsed { time_seconds, .. }
            | Self::TacticalAidMarker { time_seconds, .. }
            | Self::TacticalAidDeployed { time_seconds, .. }
            | Self::ChatMessage { time_seconds, .. }
            | Self::VoteStarted { time_seconds, .. }
            | Self::TeamWon { time_seconds, .. } => *time_seconds,
        }
    }

    const fn kind(&self) -> &'static str {
        match self {
            Self::PlayerEntered { .. } => "Player entered",
            Self::PlayerLeft { .. } => "Player left",
            Self::PlayerJoinedTeam { .. } => "Team change",
            Self::SpectatorViewChanged { .. } => "Spectator",
            Self::PlayerSetRole { .. } => "Role change",
            Self::CommandPointOwnerChanged { .. } => "Command point",
            Self::UnitDestroyed {
                cause: UnitDestructionCauseDocument::Unknown,
                killer_player_id: None,
                killer_team: None,
                destruction_context: None,
                ..
            } => "Unit lost",
            Self::UnitDestroyed { .. } => "Unit destroyed",
            Self::TacticalAidTransferred { .. } => "TA transfer",
            Self::TacticalAidDamageThreshold { .. } => "TA damage",
            Self::TacticalAidUsed { .. } => "Tactical aid",
            Self::TacticalAidMarker { .. } => "Tactical aid",
            Self::TacticalAidDeployed { .. } => "Tactical aid",
            Self::ChatMessage { .. } => "Chat",
            Self::VoteStarted { .. } => "Vote",
            Self::TeamWon { .. } => "Match result",
        }
    }

    /// Every player explicitly named by this event, paired with that player's
    /// side at the event time. This is independent of the event actor/row side:
    /// a kill, transfer, or damage event can name players from both factions.
    fn named_players(
        &self,
        player_identities: &PlayerIdentityDirectory,
        player_factions: &PlayerFactionDirectory,
    ) -> Vec<TimelinePlayer> {
        let time_seconds = self.time_seconds();
        let mut participants: Vec<(u32, Option<u32>, Option<&str>)> = match self {
            Self::PlayerEntered { player_id, .. }
            | Self::PlayerLeft { player_id, .. }
            | Self::PlayerSetRole { player_id, .. }
            | Self::VoteStarted { player_id, .. }
            | Self::SpectatorViewChanged { player_id, .. }
            | Self::TacticalAidMarker { player_id, .. } => vec![(*player_id, None, None)],
            Self::PlayerJoinedTeam {
                player_id, team, ..
            } => vec![(*player_id, Some(*team), None)],
            Self::UnitDestroyed {
                player_id,
                team,
                killer_player_id,
                killer_team,
                ..
            } => [
                player_id.map(|id| (id, *team, None)),
                killer_player_id.map(|id| (id, *killer_team, None)),
            ]
            .into_iter()
            .flatten()
            .collect(),
            Self::TacticalAidTransferred {
                from_player_id,
                to_player_id,
                ..
            } => vec![(*from_player_id, None, None), (*to_player_id, None, None)],
            Self::TacticalAidDamageThreshold {
                player_id,
                target_player_id,
                ..
            } => vec![(*player_id, None, None), (*target_player_id, None, None)],
            Self::TacticalAidUsed {
                player_id: Some(player_id),
                ..
            } => vec![(*player_id, None, None)],
            Self::TacticalAidDeployed {
                player_id: Some(player_id),
                team,
                ..
            } => vec![(*player_id, Some(*team), None)],
            Self::ChatMessage {
                player_id,
                player_name,
                ..
            } => vec![(*player_id, None, player_name.as_deref())],
            Self::CommandPointOwnerChanged { .. }
            | Self::TacticalAidUsed { .. }
            | Self::TacticalAidDeployed { .. }
            | Self::TeamWon { .. } => Vec::new(),
        };

        participants
            .drain(..)
            .filter_map(|(player_id, explicit_team, event_name)| {
                let name = identity(player_id, Some(time_seconds), event_name, player_identities);
                (name != "Unknown player").then(|| TimelinePlayer {
                    name,
                    faction: explicit_team
                        .and_then(faction_of_team)
                        .or_else(|| player_factions.faction_at(player_id, time_seconds))
                        .map(str::to_owned),
                })
            })
            .collect()
    }

    fn description(&self, player_identities: &PlayerIdentityDirectory) -> String {
        let time_seconds = Some(self.time_seconds());
        match self {
            Self::PlayerEntered { player_id, .. } => {
                format!(
                    "{} entered the game",
                    identity(*player_id, time_seconds, None, player_identities)
                )
            }
            Self::PlayerLeft { player_id, .. } => {
                format!(
                    "{} left the game",
                    identity(*player_id, time_seconds, None, player_identities)
                )
            }
            Self::PlayerJoinedTeam {
                player_id, team, ..
            } => format!(
                "{} joined {}",
                identity(*player_id, time_seconds, None, player_identities),
                team_label(*team)
            ),
            Self::SpectatorViewChanged {
                player_id,
                team,
                view,
                ..
            } => format!(
                "{} switched spectator view to {}",
                identity(*player_id, time_seconds, None, player_identities),
                view.label(*team)
            ),
            Self::PlayerSetRole {
                player_id, role, ..
            } => format!(
                "{} selected {}",
                identity(*player_id, time_seconds, None, player_identities),
                role.as_deref()
                    .map_or_else(|| "a new role".to_owned(), human_identifier)
            ),
            Self::CommandPointOwnerChanged { team, .. } => command_point_description(*team),
            Self::UnitDestroyed {
                unit_type_id,
                player_id,
                team,
                killer_player_id,
                killer_team,
                cause,
                destruction_context,
                tactical_aid_support_id,
                tactical_aid_support_name,
                ..
            } => {
                let owner = player_id.and_then(|id| player_identities.name_at(id, time_seconds));
                let killer =
                    killer_player_id.and_then(|id| player_identities.name_at(id, time_seconds));
                let context_suffix = destruction_context.map_or("", |context| context.suffix());
                if let Some(owner_name) = owner
                    && killer == Some(owner_name)
                {
                    return format!(
                        "{owner_name} destroyed one of their own units{context_suffix}"
                    );
                }
                let actor = killer
                    .map(str::to_owned)
                    .or_else(|| killer_team.map(|value| format!("{} forces", team_label(value))));
                if owner.is_none()
                    && team.is_none()
                    && let Some(description) = unit_type_id.and_then(|unit_type_id| {
                        fortification_destroyed_description(unit_type_id, actor.as_deref())
                    })
                {
                    return description;
                }
                if *cause == UnitDestructionCauseDocument::TacticalAid {
                    let aid = tactical_aid_support_id.map_or_else(
                        || "tactical aid".to_owned(),
                        |support_id| {
                            support_label(support_id, tactical_aid_support_name.as_deref())
                        },
                    );
                    let actor = killer_team
                        .map(|team| format!("{} {aid}", team_label(team)))
                        .unwrap_or(aid);
                    return match (owner, team) {
                        (Some(owner), _) => {
                            format!("{actor} destroyed one of {owner}'s units{context_suffix}")
                        }
                        (None, Some(team)) => {
                            format!(
                                "{actor} destroyed a {} unit{context_suffix}",
                                team_label(*team)
                            )
                        }
                        (None, None) => format!("{actor} destroyed a unit{context_suffix}"),
                    };
                }
                match (actor, owner, team) {
                    (Some(actor), Some(owner), _) => {
                        format!("{actor} destroyed one of {owner}'s units{context_suffix}")
                    }
                    (Some(actor), None, Some(team)) => {
                        format!(
                            "{actor} destroyed a {} unit{context_suffix}",
                            team_label(*team)
                        )
                    }
                    (Some(actor), None, None) => {
                        format!("{actor} destroyed a unit{context_suffix}")
                    }
                    (None, Some(owner), _) if destruction_context.is_some() => {
                        format!("One of {owner}'s units was destroyed{context_suffix}")
                    }
                    (None, None, Some(team)) if destruction_context.is_some() => {
                        format!("A {} unit was destroyed{context_suffix}", team_label(*team))
                    }
                    (None, None, None) if destruction_context.is_some() => {
                        format!("A unit was destroyed{context_suffix}")
                    }
                    (None, Some(owner), _) => format!("{owner} lost a unit"),
                    (None, None, Some(team)) => format!("{} lost a unit", team_label(*team)),
                    (None, None, None) => "A unit was lost".to_owned(),
                }
            }
            Self::TacticalAidTransferred {
                from_player_id,
                to_player_id,
                amount,
                ..
            } => format!(
                "{} transferred {amount} TA to {}",
                identity(*from_player_id, time_seconds, None, player_identities),
                identity(*to_player_id, time_seconds, None, player_identities)
            ),
            Self::TacticalAidDamageThreshold {
                player_id,
                target_player_id,
                support_id,
                support_name,
                ..
            } => {
                let support = support_id.map_or_else(
                    || "tactical aid".to_owned(),
                    |support_id| support_label(support_id, support_name.as_deref()),
                );
                format!(
                    "{} used {support} against {}",
                    identity(*player_id, time_seconds, None, player_identities),
                    identity(*target_player_id, time_seconds, None, player_identities,)
                )
            }
            Self::TacticalAidUsed {
                support_id,
                support_name,
                player_id,
                ..
            } => format!(
                "{} used {}",
                player_id
                    .map(|id| identity(id, time_seconds, None, player_identities))
                    .unwrap_or_else(|| "Unknown player".to_owned()),
                support_label(*support_id, support_name.as_deref())
            ),
            Self::TacticalAidMarker {
                support_id,
                support_name,
                player_id,
                ..
            } => format!(
                "{} deployed {}",
                identity(*player_id, time_seconds, None, player_identities),
                support_label(*support_id, support_name.as_deref())
            ),
            Self::TacticalAidDeployed {
                support_id,
                support_name,
                team,
                player_id,
                player_attribution,
                ..
            } => match (player_id, player_attribution) {
                (
                    Some(player_id),
                    Some(TacticalAidPlayerAttributionDocument::UnitSpawnOwnership),
                ) => format!(
                    "{} deployed {} (unit-spawn ownership)",
                    identity(*player_id, time_seconds, None, player_identities),
                    support_label(*support_id, Some(support_name))
                ),
                _ => format!(
                    "{} deployed {} (player unavailable)",
                    team_label(*team),
                    support_label(*support_id, Some(support_name))
                ),
            },
            Self::ChatMessage {
                player_id,
                player_name,
                message,
                channel,
                ..
            } => format!(
                "{} [{}]: {message}",
                identity(
                    *player_id,
                    time_seconds,
                    player_name.as_deref(),
                    player_identities,
                ),
                channel.label()
            ),
            Self::VoteStarted { player_id, .. } => format!(
                "{} started a vote",
                identity(*player_id, time_seconds, None, player_identities)
            ),
            Self::TeamWon { team: 0, .. } => "Match ended without a valid winner".to_owned(),
            Self::TeamWon { team, .. } => format!("{} won the match", team_label(*team)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(coverage))]
    fn private_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(
            std::env::var_os(name).unwrap_or_else(|| panic!("private test requires {name}")),
        )
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private replay fixture"]
    fn configured_private_replay_typed_projection_matches_json_projection() {
        let path = private_path("WIC_REPLAY_VIEWER_SMOKE");
        let parser = parser::WicReplayParser::new(&path).expect("configured replay");
        let document = parser.parse_with_timeline();
        let json = serde_json::to_string(&document).expect("parser JSON");
        let legacy = DetailView::from_json(&json).expect("JSON projection");
        let typed = DetailView::from_parser_document(document);

        assert_eq!(
            serde_json::to_value(typed).expect("typed view JSON"),
            serde_json::to_value(legacy).expect("legacy view JSON")
        );
    }

    fn joined(player_id: u32, time_seconds: f32, team: u32) -> TimelineEventDocument {
        TimelineEventDocument::PlayerJoinedTeam {
            time_seconds,
            player_id,
            team,
        }
    }

    #[test]
    fn match_result_does_not_present_the_spectator_sentinel_as_a_winner() {
        let player_identities = PlayerIdentityDirectory::new(HashMap::new(), &[]);
        let result = |team| TimelineEventDocument::TeamWon {
            time_seconds: 1.0,
            team,
            win_type: 0,
        };

        assert_eq!(
            result(0).description(&player_identities),
            "Match ended without a valid winner"
        );
        assert_eq!(
            result(2).description(&player_identities),
            "NATO won the match"
        );
    }

    #[test]
    fn resolves_a_slot_side_from_the_assignment_in_force_at_that_moment() {
        let assignments = [joined(4, 10.0, 1), joined(4, 200.0, 3)];
        let factions = PlayerFactionDirectory::new(&assignments, &[]);

        // Before the slot's first assignment it has no side; a later occupant's
        // team is not evidence about an earlier one.
        assert_eq!(factions.faction_at(4, 5.0), None);
        assert_eq!(factions.faction_at(4, 10.0), Some("USA"));
        assert_eq!(factions.faction_at(4, 199.0), Some("USA"));
        assert_eq!(factions.faction_at(4, 200.0), Some("USSR"));
        // A slot the recording never saw change teams falls back to the roster.
        assert_eq!(factions.faction_at(8, 50.0), None);
    }

    #[test]
    fn falls_back_to_the_roster_for_a_slot_with_no_recorded_team_change() {
        let players = [PlayerView {
            id: 8,
            name: "Mid-join observer".to_owned(),
            team: Some(2),
            faction: Some("NATO".to_owned()),
            role: None,
            score: None,
            score_before_leave: None,
            left_at_seconds: None,
            score_infantry: None,
            score_support: None,
            score_armor: None,
            score_air: None,
            score_capturing: None,
            score_fortification: None,
            score_transportation: None,
            score_repair: None,
            score_bridge_laying: None,
            score_unit_damage: None,
            score_tactical_aid: None,
            score_total: None,
        }];
        let factions = PlayerFactionDirectory::new(&[], &players);

        assert_eq!(factions.faction_at(8, 50.0), Some("NATO"));
    }

    #[test]
    fn reads_event_timestamps_as_minutes_and_seconds() {
        assert_eq!(timestamp_label(0.0), "0:00.0");
        assert_eq!(timestamp_label(7.25), "0:07.3");
        assert_eq!(timestamp_label(65.0), "1:05.0");
        assert_eq!(timestamp_label(812.4), "13:32.4");
        assert_eq!(timestamp_label(3600.0), "60:00.0");
        // A value just short of a minute must not round into a "0:60.0".
        assert_eq!(timestamp_label(59.97), "1:00.0");
        // Negative input cannot occur but must not underflow the split.
        assert_eq!(timestamp_label(-1.0), "0:00.0");
    }

    #[test]
    fn names_the_side_as_the_actor_when_the_exact_player_is_unproven() {
        assert_eq!(team_actor("USA"), "Team USA");
        assert_eq!(team_actor("NATO"), "Team NATO");
        assert_eq!(team_actor("USSR"), "Team USSR");
        assert_eq!(team_actor("spectators"), "Invalid faction data");
        assert_eq!(team_actor("Invalid faction data"), "Invalid faction data");
    }

    #[test]
    fn keeps_an_unproven_tactical_aid_faction_neutral() {
        assert_eq!(fighting_faction("USSR"), Some("USSR".to_owned()));
        assert_eq!(fighting_faction("Invalid faction data"), None);
        assert_eq!(fighting_faction("spectators"), None);
    }

    #[test]
    fn suppresses_invalid_tactical_aid_factions_from_view_rows() {
        let events = vec![TimelineEventDocument::TacticalAidDeployed {
            time_seconds: 1.0,
            support_id: 0x1d19_047b,
            support_name: "GasAttack_US".to_owned(),
            position: [1.0, 2.0, 3.0],
            team: 99,
            upgrade_level: 0,
            direction: [0.0, 0.0, 1.0],
            age_seconds: 0.0,
            player_id: None,
            player_attribution: None,
        }];
        let rows = project_tactical_aid_rows(
            &events,
            &RecorderMarkerLinks::default(),
            &PlayerIdentityDirectory::new(HashMap::new(), &[]),
        );

        assert!(rows.is_empty());
    }

    #[test]
    fn server_mode_labels_preserve_fpm_bots_outlier() {
        let classification = ServerClassificationDocument {
            few_player_mode: Some(true),
            has_bots: Some(true),
            ranked: None,
            ..ServerClassificationDocument::default()
        };

        assert_eq!(server_mode_labels(&classification), vec!["FPM", "Bots"]);
        assert!(!server_mode_labels(&classification).contains(&"Ranked".to_owned()));
    }

    #[test]
    fn matchup_counts_zero_score_players_in_every_server_mode() {
        for clan_match in [false, true] {
            let json = serde_json::json!({
                "replay": {
                    "gameInfo": {
                        "mapName": "do_Hometown", "mapDisplayName": "Hometown",
                        "serverName": "S", "dateTime": "2009-01-01", "gameMode": "Domination"
                    },
                    "serverClassification": {"matchMode": true, "clanMatch": clan_match},
                    "players": [
                        {"id": 0, "name": "Observer", "team": 0, "score": 0},
                        {"id": 1, "name": "USA scoring", "team": 1, "score": 10},
                        {"id": 2, "name": "USA zero", "team": 1, "score": 0},
                        {"id": 3, "name": "USSR scoring", "team": 3, "score": 20},
                        {"id": 4, "name": "USSR zero", "team": 3, "score": 0}
                    ],
                    "incomplete": false
                },
                "timeline": {"schemaVersion": 18, "durationSeconds": 100.0, "phases": [], "participants": [],
                    "events": [], "preMatchChat": [], "postMatchChat": [],
                    "dominationSamples": [],
                    "coverage": {"chat": "none", "tacticalAid": "none", "tacticalAidMarkers": "none", "tacticalAidDeployments": "none"},
                    "recorderTacticalAidUsage": {"playerId": null, "totalPlacements": 0, "supports": []}}
            });
            let detail = DetailView::from_json(&json.to_string()).expect("cached roster");
            assert_eq!(detail.overview.format.as_deref(), Some("2vs2"));
            assert_eq!(detail.overview.players.len(), 5);
            assert_eq!(detail.overview.players[2].score, Some(0));
            assert_eq!(detail.overview.players[4].score, Some(0));
        }
    }

    #[test]
    fn loads_a_cached_document_written_before_match_timing_existed() {
        let json = r#"{
            "replay": {
                "gameInfo": {
                    "mapName": "do_Hometown", "mapDisplayName": "Hometown",
                    "serverName": "S", "dateTime": "2009-01-01",
                    "gameMode": "Domination"
                },
                "players": [],
                "durationSeconds": 100.0,
                "winnerDominationPct": null,
                "loserDominationPct": null,
                "winner": null,
                "incomplete": true,
                "recorder": null
            },
            "timeline": {"schemaVersion": 11, "durationSeconds": 100.0,
                         "phases": [], "participants": [], "participantSessions": [],
                         "events": [], "preMatchChat": [], "postMatchChat": [],
                         "dominationSamples": [],
                         "coverage": {"chat": "none", "tacticalAid": "none",
                                      "tacticalAidMarkers": "none",
                                      "tacticalAidDeployments": "none"},
                         "recorderTacticalAidUsage": {"playerId": null,
                                                      "totalPlacements": 0,
                                                      "supports": []}}
        }"#;

        let detail = DetailView::from_json(json).expect("decode legacy cached detail");
        assert_eq!(detail.overview.match_ending, "unknown");
        assert!(detail.overview.domination_shares.is_none());
        assert_eq!(detail.overview.timing.match_elapsed_seconds, None);
    }

    #[test]
    fn caches_result_provenance_without_reclassifying_departures() {
        let json = r#"{
            "replay": {
                "gameInfo": {
                    "mapName": "do_Hometown", "mapDisplayName": "Hometown",
                    "serverName": "S", "dateTime": "2009-01-01",
                    "gameMode": "Domination"
                },
                "players": [],
                "durationSeconds": 100.0,
                "winnerDominationPct": null,
                "loserDominationPct": null,
                "winner": null,
                "incomplete": true,
                "recorder": null
            },
            "timeline": {"schemaVersion": 11, "durationSeconds": 100.0,
                         "phases": [], "participants": [], "participantSessions": [],
                         "events": [], "preMatchChat": [], "postMatchChat": [],
                         "dominationSamples": [],
                         "coverage": {"chat": "none", "tacticalAid": "none",
                                      "tacticalAidMarkers": "none",
                                      "tacticalAidDeployments": "none"},
                         "recorderTacticalAidUsage": {"playerId": null,
                                                      "totalPlacements": 0,
                                                      "supports": []}}
        }"#;

        let mut raw: serde_json::Value = serde_json::from_str(json).unwrap();
        raw["replay"]["players"] = serde_json::json!([
            {"id": 1, "name": "Departed", "team": 3, "faction": "USSR", "score": 0, "scoreTotal": 0},
            {"id": 2, "name": "Air", "team": 3, "faction": "USSR", "score": 0}
        ]);
        // The parser writes the shortest f32 decimal. Cache loading must recover
        // that recorded value before converting it to the public f64 timestamp.
        let recorded_departure = 64.565_674_f32;
        raw["replay"]["players"][0]["leftAtSeconds"] =
            serde_json::from_str(&serde_json::to_string(&recorded_departure).unwrap()).unwrap();
        let legacy = DetailView::from_json(&raw.to_string()).unwrap();
        assert_eq!(
            legacy.overview.players[0].left_at_seconds,
            Some(f64::from(recorded_departure))
        );
        assert_eq!(legacy.overview.players[1].left_at_seconds, None);
        assert_eq!(legacy.overview.players[0].score, Some(0));
        assert!(legacy.overview.players[0].score_before_leave.is_none());
        // Withdrawn role metadata must be ignored even in legacy cached documents.
        raw["playerResultEvidence"] = serde_json::json!([
            {"playerId": 1, "role": null, "scoreBeforeLeave": {"score": 150, "observedAtSeconds": 40.0, "leftAtSeconds": 41.0, "lastRecordedRole": {"role": "support", "observedAtSeconds": 10.0}}},
            {"playerId": 2, "role": "air", "scoreBeforeLeave": null}
        ]);
        let detail = DetailView::from_json(&raw.to_string()).unwrap();
        let departed = &detail.overview.players[0];
        assert_eq!(departed.team, Some(3));
        assert_eq!(departed.score, Some(150));
        assert_eq!(departed.role, None);
        assert_eq!(departed.score_total, None);
        assert_eq!(
            departed
                .score_before_leave
                .as_ref()
                .unwrap()
                .left_at_seconds,
            41.0
        );
        assert_eq!(detail.overview.players[1].role, None);
        assert_eq!(detail.overview.players[1].score, Some(0));

        // Cached raw results retain all participants; only the projection trims
        // confirmed departures from an overflowing team.
        raw["playerResultEvidence"] = serde_json::json!([]);
        raw["replay"]["players"] = serde_json::Value::Array((0..9).map(|id| serde_json::json!({
            "id": id, "name": format!("Player {id}"), "team": 3, "faction": "USSR",
            "score": 10, "leftAtSeconds": if id == 1 { Some(10.0) } else if id == 2 { Some(20.0) } else { None }
        })).collect());
        let detail = DetailView::from_json(&raw.to_string()).unwrap();
        assert_eq!(raw["replay"]["players"].as_array().unwrap().len(), 9);
        assert_eq!(detail.overview.players.len(), 8);
        assert!(!detail.overview.players.iter().any(|p| p.id == 1));
        assert_eq!(
            detail
                .overview
                .players
                .iter()
                .find(|p| p.id == 2)
                .unwrap()
                .left_at_seconds,
            Some(20.0)
        );
        raw["replay"]["players"].as_array_mut().unwrap().pop();
        let smaller = DetailView::from_json(&raw.to_string()).unwrap();
        assert_eq!(smaller.overview.players.len(), 8);
        assert!(smaller.overview.players.iter().any(|p| p.id == 1));
    }

    #[test]
    fn builds_virtualized_view_rows_from_parser_json() {
        let json = r#"{
            "replay": {
                "gameInfo": {
                    "mapName": "do_Hometown",
                    "mapDisplayName": "Hometown",
                    "serverName": "<#06C>WiCGate</> Ranked Server",
                    "dateTime": "2009-01-01",
                    "gameMode": "Domination"
                },
                "players": [{
                    "id": 3, "name": "Alpha", "team": 1,
                    "faction": "NATO", "role": "Air", "score": 100,
                    "scoreInfantry": 7, "scoreSupport": 8,
                    "scoreArmor": 9, "scoreAir": 74,
                    "scoreCapturing": 11, "scoreFortification": 12,
                    "scoreTransportation": 13, "scoreRepair": 14,
                    "scoreBridgeLaying": 15, "scoreUnitDamage": 16,
                    "scoreTacticalAid": 17, "scoreTotal": 98
                }],
                "durationSeconds": 60.0,
                "winnerDominationPct": 0.75,
                "loserDominationPct": 0.25,
                "timing": {
                    "capturedMatchStart": false,
                    "recordingSeconds": 1010.5,
                    "observedGameplaySeconds": 900.0,
                    "matchElapsedSeconds": 1199.05,
                    "roundLengthSeconds": 1200.0,
                    "roundLengthExact": false,
                    "joinedAtRemainingSeconds": 926.6,
                    "finalRemainingSeconds": 0.95
                },
                "matchEnding": "timeout",
                "dominationShares": [
                    {"faction": "USA", "pct": 0.75},
                    {"faction": "USSR", "pct": 0.25}
                ],
                "dominationAnchor": "povTeam",
                "winner": "NATO",
                "incomplete": false,
                "recorder": "Alpha"
            },
            "timeline": {
                "schemaVersion": 8,
                "durationSeconds": 60.0,
                "phases": [{
                    "index": 0, "startSeconds": 0.0, "endSeconds": 60.0,
                    "initialClockSeconds": 60.0, "finalClockSeconds": 0.0
                }],
                "dominationSamples": [
                    {"timeSeconds": 2.0, "value": 0.5},
                    {"timeSeconds": 7.0, "value": 0.61}
                ],
                "dominationAnchorFaction": "USA",
                "participants": [
                    {"playerId": 3, "playerName": "Alpha"},
                    {"playerId": 8, "playerName": "Bravo"}
                ],
                "events": [
                    {"type": "chatMessage", "timeSeconds": 2.0, "playerId": 3,
                     "playerName": "Alpha", "message": "hi", "channel": "all"},
                    {"type": "tacticalAidUsed", "timeSeconds": 3.0,
                     "supportId": 1102579402, "supportName": "CarpetBombing_USSR",
                     "honorsCost": 6.0,
                     "position": [1.0, 2.0, 3.0], "playerId": 3},
                    {"type": "tacticalAidMarker", "timeSeconds": 3.01,
                     "eventId": 10, "supportId": 1102579402,
                     "supportName": "CarpetBombing_USSR",
                     "position": [1.0, 2.0, 3.0], "playerId": 3,
                     "upgradeLevel": 0, "direction": [0.0, 0.0, 1.0],
                     "durationSeconds": 1.0},
                    {"type": "tacticalAidDeployed", "timeSeconds": 4.01,
                     "supportId": 1102579402, "supportName": "CarpetBombing_USSR",
                     "position": [1.0, 2.0, 3.0], "team": 3,
                     "upgradeLevel": 0, "direction": [0.0, 0.0, 1.0],
                     "ageSeconds": 1.0},
                    {"type": "tacticalAidMarker", "timeSeconds": 3.5,
                     "eventId": 11, "supportId": 1127679774,
                     "supportName": "HeavyAirSupport_US",
                     "position": [4.0, 5.0, 6.0], "playerId": 8,
                     "upgradeLevel": 0, "direction": [0.0, 1.0, 0.0],
                     "durationSeconds": 1.0},
                    {"type": "tacticalAidDeployed", "timeSeconds": 4.5,
                     "supportId": 1127679774, "supportName": "HeavyAirSupport_US",
                     "position": [4.0, 5.0, 6.0], "team": 1,
                     "upgradeLevel": 0, "direction": [0.0, 1.0, 0.0],
                     "ageSeconds": 1.0},
                    {"type": "tacticalAidDeployed", "timeSeconds": 4.7,
                     "supportId": 990316133, "supportName": "TacticalNuke_USSR",
                     "position": [7.0, 8.0, 9.0], "team": 3,
                     "upgradeLevel": 0, "direction": [0.0, 1.0, 0.0],
                     "ageSeconds": 1.0},
                    {"type": "tacticalAidMarker", "timeSeconds": 3.6,
                     "eventId": 12, "supportId": 3605269606,
                     "supportName": null,
                     "position": [4.0, 5.0, 6.0], "playerId": 8,
                     "upgradeLevel": 0, "direction": [0.0, 1.0, 0.0],
                     "durationSeconds": 1.0},
                    {"type": "unitDestroyed", "timeSeconds": 4.0,
                     "unitId": 101, "unitTypeId": 305419896, "playerId": 3,
                     "team": 2, "killerUnitId": 202, "killerPlayerId": 8,
                    "killerTeam": 3}
                ],
                "infantrySoldierDeaths": [{
                    "timeSeconds": 4.0, "unitId": 102,
                    "unitTypeId": 244777779, "playerId": 3, "team": 2,
                    "killerUnitId": 202, "killerPlayerId": 8, "killerTeam": 3
                }],
                "preMatchChat": [{
                    "timeSeconds": 0.5, "secondsBeforeMatch": 1.5,
                    "playerId": 3, "playerName": "Alpha",
                    "message": "ready?", "channel": "all"
                }],
                "postMatchChat": [{
                    "timeSeconds": 9.25, "secondsAfterMatch": 4.25,
                    "playerId": 8, "playerName": "Bravo",
                    "message": "gg", "channel": "all"
                }],
                "coverage": {
                    "chat": "visibleToRecorder",
                    "tacticalAid": "recorderOnly",
                    "tacticalAidMarkers": "visibleFactionWithPlayer",
                    "tacticalAidDeployments": "bothFactionsTeamOnly"
                },
                "recorderTacticalAidUsage": {
                    "playerId": 3,
                    "totalPlacements": 1,
                    "supports": [{"supportId": 9, "supportName": null,
                                  "placementCount": 1, "observedCosts": [6.0]}]
                }
            }
        }"#;

        let detail = DetailView::from_json(json).expect("detail");
        assert_eq!(detail.overview.players.len(), 1);
        let player = &detail.overview.players[0];
        assert_eq!(player.score_infantry, Some(7));
        assert_eq!(player.score_support, Some(8));
        assert_eq!(player.score_armor, Some(9));
        assert_eq!(player.score_air, Some(74));
        assert_eq!(player.score_capturing, Some(11));
        assert_eq!(player.score_fortification, Some(12));
        assert_eq!(player.score_transportation, Some(13));
        assert_eq!(player.score_repair, Some(14));
        assert_eq!(player.score_bridge_laying, Some(15));
        assert_eq!(player.score_unit_damage, Some(16));
        assert_eq!(player.score_tactical_aid, Some(17));
        assert_eq!(player.score_total, Some(98));
        assert_eq!(detail.overview.winner_domination_pct, Some(0.75));
        assert_eq!(detail.overview.loser_domination_pct, Some(0.25));
        assert_eq!(detail.overview.match_ending, "timeout");
        assert_eq!(
            detail.domination_samples,
            vec![
                TimelineValueSampleView {
                    time_seconds: 2.0,
                    value: 0.5,
                },
                TimelineValueSampleView {
                    time_seconds: 7.0,
                    value: 0.61,
                },
            ]
        );
        assert_eq!(detail.domination_anchor_faction.as_deref(), Some("USA"));
        // A mid-match join reports true match time, longer than the recording.
        assert!(!detail.overview.timing.captured_match_start);
        assert_eq!(detail.overview.timing.recording_seconds, 1010.5);
        assert_eq!(
            detail.overview.timing.observed_gameplay_seconds,
            Some(900.0)
        );
        assert_eq!(detail.overview.timing.match_elapsed_seconds, Some(1199.05));
        assert_eq!(
            detail.overview.timing.joined_at_remaining_seconds,
            Some(926.6)
        );
        let shares = detail
            .overview
            .domination_shares
            .as_ref()
            .expect("faction-anchored shares");
        assert_eq!(shares[0].faction, "USA");
        assert_eq!(shares[1].faction, "USSR");
        assert_eq!(
            detail.overview.domination_anchor.as_deref(),
            Some("povTeam")
        );
        // Schema-v16 infantrySoldierDeaths are parser facts, not timeline rows.
        assert_eq!(detail.timeline_rows.len(), 5);
        assert_eq!(detail.overview.server_name, "WiCGate Ranked Server");
        assert_eq!(
            detail.timeline_rows[4].description,
            "Bravo destroyed one of Alpha's units"
        );
        assert_eq!(detail.timeline_rows[4].players.len(), 2);
        assert_eq!(detail.timeline_rows[4].players[0].name, "Alpha");
        assert_eq!(
            detail.timeline_rows[4].players[0].faction.as_deref(),
            Some("NATO")
        );
        assert_eq!(detail.timeline_rows[4].players[1].name, "Bravo");
        assert_eq!(
            detail.timeline_rows[4].players[1].faction.as_deref(),
            Some("USSR")
        );
        assert!(!detail.timeline_rows[4].description.contains('#'));
        assert!(!detail.timeline_rows[4].description.contains("101"));
        // Every chat stage reads its own position on the recording clock, so a
        // pre- or post-match message sits on the same axis as the timeline.
        assert_eq!(
            detail
                .chat_rows
                .iter()
                .map(|row| (row.stage, row.time_label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Pre-match", "0:00.5"),
                ("Match", "0:02.0"),
                ("Post-match", "0:09.3"),
            ]
        );
        assert_eq!(detail.chat_rows[1].player, "Alpha");
        assert_eq!(detail.tactical_aid_rows.len(), 3);
        assert_eq!(detail.tactical_aid_rows[0].player, "Alpha");
        assert_eq!(detail.tactical_aid_rows[0].faction, "USSR");
        assert_eq!(detail.tactical_aid_rows[0].support, "Carpet Bombing");
        assert_eq!(detail.tactical_aid_rows[0].honors_cost, Some(6.0));
        assert_eq!(detail.tactical_aid_rows[1].player, "Bravo");
        assert_eq!(detail.tactical_aid_rows[1].faction, "USA");
        assert_eq!(detail.tactical_aid_rows[1].honors_cost, None);
        assert_eq!(detail.tactical_aid_rows[2].player, "Team USSR");
        assert_eq!(detail.tactical_aid_rows[2].faction, "USSR");
        assert_eq!(detail.tactical_aid_summary.total_placements, 3);
        assert_eq!(detail.timeline_schema, 8);
        assert_eq!(detail.recorder_view, "Player POV");
    }

    #[test]
    fn ambiguous_recorder_cost_links_remain_unpriced() {
        let events = vec![
            TimelineEventDocument::TacticalAidUsed {
                time_seconds: 1.0,
                support_id: 1,
                support_name: Some("Airstrike_US".to_owned()),
                honors_cost: 10.0,
                position: [1.0, 2.0, 3.0],
                player_id: Some(4),
            },
            TimelineEventDocument::TacticalAidMarker {
                time_seconds: 1.1,
                event_id: 1,
                support_id: 1,
                support_name: Some("Airstrike_US".to_owned()),
                position: [1.0, 2.0, 3.0],
                player_id: 4,
                upgrade_level: 0,
                direction: [0.0; 3],
                duration_seconds: 0.0,
            },
            TimelineEventDocument::TacticalAidMarker {
                time_seconds: 1.2,
                event_id: 2,
                support_id: 1,
                support_name: Some("Airstrike_US".to_owned()),
                position: [1.0, 2.0, 3.0],
                player_id: 4,
                upgrade_level: 0,
                direction: [0.0; 3],
                duration_seconds: 0.0,
            },
        ];

        let links = link_recorder_costs(&events);
        assert!(links.marker_costs.is_empty());
        assert!(links.matched_purchases.is_empty());
    }

    #[test]
    fn unit_spawn_attribution_resolves_a_deployment_player() {
        let event: TimelineEventDocument = serde_json::from_str(
            r#"{
                "type": "tacticalAidDeployed",
                "timeSeconds": 35.0,
                "supportId": 711198103,
                "supportName": "Paratrooper_US",
                "position": [100.0, 0.0, 200.0],
                "team": 1,
                "upgradeLevel": 0,
                "direction": [0.0, 0.0, 1.0],
                "ageSeconds": 5.0,
                "playerId": 8,
                "playerAttribution": "unitSpawnOwnership"
            }"#,
        )
        .expect("schema-v9 deployment");
        let player_identities =
            PlayerIdentityDirectory::new(HashMap::from([(8, "Bravo".to_owned())]), &[]);

        let row = deployment_row(&event, &player_identities);

        assert_eq!(row.time_seconds, 30.0);
        assert_eq!(row.player, "Bravo");
        assert_eq!(row.player_attribution, "Exact player");
        assert_eq!(row.faction, "USA");
    }

    #[test]
    fn incomplete_many_to_one_cost_links_remain_unpriced() {
        let purchase = |time_seconds, honors_cost| TimelineEventDocument::TacticalAidUsed {
            time_seconds,
            support_id: 1,
            support_name: Some("Airstrike_US".to_owned()),
            honors_cost,
            position: [1.0, 2.0, 3.0],
            player_id: Some(4),
        };
        let events = vec![
            purchase(1.0, 10.0),
            purchase(1.1, 8.0),
            TimelineEventDocument::TacticalAidMarker {
                time_seconds: 1.2,
                event_id: 1,
                support_id: 1,
                support_name: Some("Airstrike_US".to_owned()),
                position: [1.0, 2.0, 3.0],
                player_id: 4,
                upgrade_level: 0,
                direction: [0.0; 3],
                duration_seconds: 0.0,
            },
        ];

        let links = link_recorder_costs(&events);
        assert!(links.marker_costs.is_empty());
        assert!(links.matched_purchases.is_empty());
    }

    #[test]
    fn unknown_support_labels_retain_the_raw_id() {
        assert_eq!(support_label(0x1234_abcd, None), "Tactical aid 0x1234abcd");
    }

    #[test]
    fn temporal_sessions_replace_reused_slots_without_leaking_static_names_into_gaps() {
        let sessions = vec![
            ParticipantSessionDocument {
                player_id: 1,
                session_index: 0,
                player_name: Some("[-HH-]JonnySky".to_owned()),
                start_seconds: 0.0,
                end_seconds: Some(10.0),
            },
            ParticipantSessionDocument {
                player_id: 1,
                session_index: 1,
                player_name: Some("[WHO]LtDan73".to_owned()),
                start_seconds: 20.0,
                end_seconds: None,
            },
        ];
        let identities = PlayerIdentityDirectory::new(
            HashMap::from([
                (1, "[-HH-]JonnySky".to_owned()),
                (2, "StablePlayer".to_owned()),
            ]),
            &sessions,
        );

        assert_eq!(identity(1, Some(5.0), None, &identities), "[-HH-]JonnySky");
        assert_eq!(identity(1, Some(15.0), None, &identities), "Unknown player");
        assert_eq!(
            identity(1, Some(402.673), None, &identities),
            "[WHO]LtDan73"
        );
        assert_eq!(
            identity(2, Some(402.673), None, &identities),
            "StablePlayer"
        );
    }

    #[test]
    fn tactical_aid_damage_threshold_names_both_players_without_claiming_a_kill() {
        let player_identities = PlayerIdentityDirectory::new(
            HashMap::from([(1, "Target".to_owned()), (11, "Actor".to_owned())]),
            &[],
        );
        let known = TimelineEventDocument::TacticalAidDamageThreshold {
            time_seconds: 183.4,
            player_id: 11,
            target_player_id: 1,
            ta_index: 60,
            support_id: Some(0x2f92_05b5),
            support_name: Some("Tankbuster_NATO".to_owned()),
            upgrade_level: 1,
        };
        let unknown = TimelineEventDocument::TacticalAidDamageThreshold {
            time_seconds: 200.0,
            player_id: 11,
            target_player_id: 1,
            ta_index: 999,
            support_id: None,
            support_name: None,
            upgrade_level: 0,
        };

        assert_eq!(known.kind(), "TA damage");
        assert_eq!(
            known.description(&player_identities),
            "Actor used Tank Buster against Target"
        );
        assert_eq!(
            unknown.description(&player_identities),
            "Actor used tactical aid against Target"
        );
        assert!(!known.description(&player_identities).contains("destroy"));
        assert!(!known.description(&player_identities).contains("kill"));
    }

    #[test]
    fn unit_destruction_distinguishes_self_fire_from_a_neutral_loss() {
        let player_identities =
            PlayerIdentityDirectory::new(HashMap::from([(3, "Alpha".to_owned())]), &[]);
        let event = |killer_player_id: Option<u32>| TimelineEventDocument::UnitDestroyed {
            time_seconds: 10.0,
            unit_id: 41,
            unit_type_id: Some(0x1234_5678),
            player_id: Some(3),
            team: Some(1),
            killer_unit_id: killer_player_id.map(|_| 42),
            killer_player_id,
            killer_team: killer_player_id.map(|_| 1),
            cause: killer_player_id.map_or(UnitDestructionCauseDocument::Unknown, |_| {
                UnitDestructionCauseDocument::Unit
            }),
            destruction_context: None,
            tactical_aid_support_id: None,
            tactical_aid_support_name: None,
        };

        assert_eq!(
            event(Some(3)).description(&player_identities),
            "Alpha destroyed one of their own units"
        );
        assert_eq!(
            event(None).description(&player_identities),
            "Alpha lost a unit"
        );
        assert_eq!(event(None).kind(), "Unit lost");
        assert_eq!(event(Some(3)).kind(), "Unit destroyed");
    }

    #[test]
    fn tactical_aid_destruction_names_the_team_and_aid() {
        let player_identities =
            PlayerIdentityDirectory::new(HashMap::from([(3, "Alpha".to_owned())]), &[]);
        let event = TimelineEventDocument::UnitDestroyed {
            time_seconds: 10.0,
            unit_id: 41,
            unit_type_id: Some(0x1234_5678),
            player_id: Some(3),
            team: Some(1),
            killer_unit_id: Some(512),
            killer_player_id: None,
            killer_team: Some(3),
            cause: UnitDestructionCauseDocument::TacticalAid,
            destruction_context: None,
            tactical_aid_support_id: Some(0xaffe_0b0a),
            tactical_aid_support_name: Some("HeavyAirSupport_USSR".to_owned()),
        };

        assert_eq!(
            event.description(&player_identities),
            "USSR Heavy Air Support destroyed one of Alpha's units"
        );
    }

    #[test]
    fn unit_destruction_context_explains_a_loss_without_inventing_an_actor() {
        let player_identities =
            PlayerIdentityDirectory::new(HashMap::from([(3, "Alpha".to_owned())]), &[]);
        let event = |destruction_context| TimelineEventDocument::UnitDestroyed {
            time_seconds: 10.0,
            unit_id: 41,
            unit_type_id: Some(0x1234_5678),
            player_id: Some(3),
            team: Some(1),
            killer_unit_id: Some(512),
            killer_player_id: None,
            killer_team: None,
            cause: UnitDestructionCauseDocument::Unknown,
            destruction_context: Some(destruction_context),
            tactical_aid_support_id: None,
            tactical_aid_support_name: None,
        };

        let building = event(UnitDestructionContextDocument::BuildingCollapse { building_id: 900 });
        assert_eq!(building.kind(), "Unit destroyed");
        assert_eq!(
            building.description(&player_identities),
            "One of Alpha's units was destroyed when its occupied building collapsed"
        );

        let container = event(UnitDestructionContextDocument::DestroyedWithContainer {
            container_unit_id: 50,
        });
        assert_eq!(container.kind(), "Unit destroyed");
        assert_eq!(
            container.description(&player_identities),
            "One of Alpha's units was destroyed with its transport"
        );
    }

    #[test]
    fn command_point_team_zero_is_neutral_not_spectators() {
        assert_eq!(command_point_description(0), "Command point became neutral");
        assert_eq!(
            command_point_description(2),
            "NATO captured a command point"
        );
    }

    #[test]
    fn shipped_fortification_ids_have_specific_destruction_labels() {
        for unit_type_id in [0x77d7_09c0, 0x85aa_0a4a, 0x884f_0a65] {
            assert_eq!(
                fortification_destroyed_description(unit_type_id, None).as_deref(),
                Some("An anti-air fortification was destroyed")
            );
        }
        for unit_type_id in [0x822e_0a32, 0x908b_0abc, 0x934b_0ad7] {
            assert_eq!(
                fortification_destroyed_description(unit_type_id, None).as_deref(),
                Some("An anti-tank fortification was destroyed")
            );
        }
        for unit_type_id in [0x972d_0af7, 0xa69e_0b81, 0xa994_0b9c] {
            assert_eq!(
                fortification_destroyed_description(unit_type_id, Some("NATO forces")).as_deref(),
                Some("NATO forces destroyed a machine-gun fortification")
            );
        }
        assert_eq!(fortification_destroyed_description(0x1234_5678, None), None);
    }

    #[test]
    fn unidentified_unit_destructions_are_omitted_without_hiding_fortifications() {
        assert!(!unit_destruction_is_identifiable(
            Some(0x1234_5678),
            None,
            None,
            None,
            None
        ));
        assert!(unit_destruction_is_identifiable(
            Some(0x884f_0a65),
            None,
            None,
            None,
            None
        ));
        assert!(unit_destruction_is_identifiable(
            None,
            Some(3),
            Some(2),
            None,
            None
        ));
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private tactical-aid replay fixture"]
    fn configured_private_replay_projects_both_faction_tactical_aid() {
        let path = private_path("WIC_REPLAY_VIEWER_TA_REVIEW");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path).expect("review replay");
        let raw = serde_json::to_value(parser.parse_with_timeline()).expect("raw JSON");
        let raw_events = raw["timeline"]["events"]
            .as_array()
            .expect("timeline events");
        let expected_deployments = raw_events
            .iter()
            .filter(|event| event["type"] == "tacticalAidDeployed")
            .count();
        let recorder_purchases = raw_events
            .iter()
            .filter(|event| event["type"] == "tacticalAidUsed")
            .count();
        let detail = DetailView::from_json(&serde_json::to_string(&raw).expect("detail JSON"))
            .expect("detail projection");

        assert!(detail.tactical_aid_rows.len() >= expected_deployments);
        assert_eq!(
            detail
                .tactical_aid_rows
                .iter()
                .filter(|row| row.honors_cost.is_some())
                .count(),
            recorder_purchases
        );
        assert!(detail.tactical_aid_rows.iter().any(|row| {
            row.player_attribution == "Team only" && row.player.starts_with("Team ")
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private slot-reuse replay fixture"]
    fn configured_private_reused_slot_resolves_later_tactical_aid_occupant() {
        let path = private_path("WIC_REPLAY_VIEWER_IDENTITY_REUSE");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path)
            .expect("slot-reuse review replay");
        let raw = serde_json::to_string(&parser.parse_with_timeline()).expect("raw JSON");
        let detail = DetailView::from_json(&raw).expect("detail projection");

        let player = detail
            .overview
            .players
            .iter()
            .find(|p| p.id == 8)
            .expect("slot 8");
        assert_eq!(
            (
                player.name.as_str(),
                player.team,
                player.score,
                player.role.as_deref()
            ),
            ("Todesengel22", Some(3), Some(247), Some("support"))
        );
        let player = detail
            .overview
            .players
            .iter()
            .find(|p| p.id == 11)
            .expect("slot 11");
        assert_eq!(
            (
                player.name.as_str(),
                player.team,
                player.score,
                player.role.as_deref()
            ),
            ("bTd^westurkey", Some(3), Some(673), Some("armor"))
        );

        for (time_seconds, support) in [
            (433.549, "Heavy Air Support"),
            (1100.437, "Precision Artillery"),
        ] {
            let row = detail
                .tactical_aid_rows
                .iter()
                .find(|row| {
                    (row.time_seconds - time_seconds).abs() <= 0.001 && row.support == support
                })
                .expect("review tactical-aid row");
            assert_eq!(row.player, "[WHO]LtDan73");
        }
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private roster-reuse replay fixture"]
    fn configured_private_pre_match_slot_reuse_labels_final_roles() {
        let path = private_path("WIC_REPLAY_VIEWER_ROSTER_REUSE");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path)
            .expect("pre-match slot-reuse review replay");
        let detail = DetailView::from_parser_document(parser.parse_with_timeline());

        for (player_id, expected_name, expected_role) in [
            (3, "-=/LK/=-THE_MONTY", "support"),
            (6, "Inc.G^Dönerbude", "infantry"),
            (7, "[Lем.s]Hand of Devil", "air"),
        ] {
            let player = detail
                .overview
                .players
                .iter()
                .find(|player| player.id == player_id)
                .expect("review player row");
            assert_eq!(player.name, expected_name);
            assert_eq!(player.role.as_deref(), Some(expected_role));
        }
    }

    #[test]
    fn recorder_view_preserves_spectators_and_distinguishes_recorded_team_changes() {
        let spectator = |time_seconds, view, team| TimelineEventDocument::SpectatorViewChanged {
            time_seconds,
            player_id: 7,
            team,
            spectator_los: 2,
            view,
        };
        let all = || spectator(0.0, SpectatorViewDocument::AllTeams, 0);
        let one = || spectator(10.0, SpectatorViewDocument::OneTeam, 3);
        let join = |player_id| TimelineEventDocument::PlayerJoinedTeam {
            time_seconds: 5.0,
            player_id,
            team: 3,
        };
        assert_eq!(recorder_view_label(&[], None), "Unknown POV");
        assert_eq!(recorder_view_label(&[], Some(7)), "Player POV");
        assert_eq!(
            recorder_view_label(&[all()], Some(7)),
            "Spectator · all teams"
        );
        assert_eq!(recorder_view_label(&[one()], Some(7)), "Spectator · USSR");
        assert_eq!(
            recorder_view_label(&[all(), one()], Some(7)),
            "Spectator · mixed views"
        );
        assert_eq!(
            recorder_view_label(&[all(), join(8)], Some(7)),
            "Spectator · all teams"
        );
        assert_eq!(
            recorder_view_label(&[all(), join(7)], Some(7)),
            "Player / spectator POV"
        );
        assert_eq!(
            recorder_view_label(&[join(7), one()], Some(7)),
            "Player / spectator POV"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires private event-backed result fixtures"]
    fn configured_private_event_backed_results() {
        for (variable, scores) in [
            ("WIC_REPLAY_VIEWER_RESULT_333", vec![(3, 150), (15, 408)]),
            (
                "WIC_REPLAY_VIEWER_RESULT_358",
                vec![(4, 358), (6, 167), (15, 265)],
            ),
            ("WIC_REPLAY_VIEWER_RESULT_DEMO69", vec![(12, 457)]),
            ("WIC_REPLAY_VIEWER_RESULT_DEMO66", vec![(4, 180)]),
            ("WIC_REPLAY_VIEWER_RESULT_DEMO33", vec![(1, 215)]),
            ("WIC_REPLAY_VIEWER_RESULT_DEMO34", vec![]),
            ("WIC_REPLAY_VIEWER_RESULT_DEMO30", vec![]),
        ] {
            let parser = parser::WicReplayParser::new(&private_path(variable)).unwrap();
            let document = parser.parse_with_timeline();
            let raw: serde_json::Value =
                serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
            let evidence = parser.player_result_evidence(&document.replay.players);
            assert_eq!(
                evidence
                    .iter()
                    .filter_map(|e| e
                        .score_before_leave
                        .as_ref()
                        .map(|s| (e.player_id, s.score)))
                    .collect::<Vec<_>>(),
                scores,
                "{variable}"
            );
            let hidden = parser.abandoned_lobby_duplicate_slots(&document.replay.players);
            assert!(hidden.is_empty());
            let (detail, json) =
                DetailView::cache_parser_document(document, hidden, evidence).unwrap();
            let stored: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(stored["replay"], raw["replay"]);
            assert_eq!(stored["timeline"], raw["timeline"]);
            assert_eq!(
                serde_json::to_value(&detail).unwrap(),
                serde_json::to_value(DetailView::from_json(&json).unwrap()).unwrap()
            );
            assert_eq!(
                detail.overview.players.len(),
                raw["replay"]["players"].as_array().unwrap().len()
            );
            for player in &detail.overview.players {
                let old = raw["replay"]["players"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["id"] == player.id)
                    .unwrap();
                assert_eq!(old["name"], player.name);
                assert_eq!(old["team"], serde_json::to_value(player.team).unwrap());
                assert_eq!(old["role"], serde_json::to_value(&player.role).unwrap());
                if let Some((_, score)) = scores.iter().find(|(id, _)| *id == player.id) {
                    assert_eq!(player.score, Some(*score));
                    assert_eq!(player.score_before_leave.as_ref().unwrap().score, *score);
                    assert_eq!(player.score_total, None);
                    assert_eq!(player.score_unit_damage, None);
                } else {
                    assert_eq!(old["score"], serde_json::to_value(player.score).unwrap());
                    assert!(player.score_before_leave.is_none());
                }
            }
        }
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private lobby role spectator fixture"]
    fn configured_private_lobby_role_spectator() {
        // SHA-256: 04b34a91b7ffa57273e37924504c5d494199340e8dade4170a1967aaa55033b1
        let path = private_path("WIC_REPLAY_VIEWER_LOBBY_ROLE_SPECTATOR");
        let parser = parser::WicReplayParser::new(&path).unwrap();
        let document = parser.parse_with_timeline();
        let player = document.replay.players.iter().find(|p| p.id == 8).unwrap();
        assert_eq!(player.name, "[v]ÅÞÞL£");
        assert_eq!(player.team, Some(0));
        assert_eq!(player.score, Some(0));
        let detail = DetailView::from_parser_document(document);
        assert_eq!(detail.recorder_view, "Spectator · all teams");
        assert_eq!(
            detail
                .overview
                .players
                .iter()
                .find(|p| p.id == 8)
                .unwrap()
                .faction
                .as_deref(),
            Some("Spectator")
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private abandoned lobby duplicate fixture"]
    fn configured_private_abandoned_lobby_duplicate() {
        // SHA-256: 921ced0bb9ea8d04371616ff20c7c4f18aebbd9c545a05600b9e0561ef02c18d
        let path = private_path("WIC_REPLAY_VIEWER_ABANDONED_DUPLICATE");
        let parser = parser::WicReplayParser::new(&path).unwrap();
        let document = parser.parse_with_timeline();
        let raw_count = document.replay.players.len();
        let hidden = parser.abandoned_lobby_duplicate_slots(&document.replay.players);
        assert_eq!(hidden, vec![3]);
        assert_eq!(
            document
                .replay
                .players
                .iter()
                .filter(|p| p.name == "Shiny^Hunter UK")
                .count(),
            2
        );
        let (detail, json) =
            DetailView::cache_parser_document(document, hidden, Vec::new()).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            raw["replay"]["players"].as_array().unwrap().len(),
            raw_count
        );
        let cached = DetailView::from_json(&json).unwrap();
        assert_eq!(
            serde_json::to_value(&detail).unwrap(),
            serde_json::to_value(&cached).unwrap()
        );
        let hunters: Vec<_> = detail
            .overview
            .players
            .iter()
            .filter(|p| p.name == "Shiny^Hunter UK")
            .collect();
        assert_eq!(hunters.len(), 1);
        assert_eq!(hunters[0].id, 8);
        assert_eq!(hunters[0].score, Some(1024));
        assert_eq!(hunters[0].faction.as_deref(), Some("USSR"));
        assert_eq!(detail.overview.players.len(), raw_count - 1);
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires private genuine spectator replay fixtures"]
    fn configured_private_genuine_spectator_views_remain_intact() {
        for (variable, expected, count) in [
            // SHA-256: d4a40ad3de3f0fa747b2157e1841a08328ee14e0e2cf617cd1fb6ba336403f22
            (
                "WIC_REPLAY_VIEWER_TRUE_SPECTATOR_REVIEW",
                "Spectator · all teams",
                1,
            ),
            // SHA-256: c7f8d9f8413922008149a4f4d1e4567fb20176f573499ee7c867b06eca361013
            (
                "WIC_REPLAY_VIEWER_MIXED_SPECTATOR_REVIEW",
                "Spectator · mixed views",
                4,
            ),
        ] {
            let path = private_path(variable);
            let parser =
                wic_replay_parser::parser::WicReplayParser::new(&path).expect("spectator replay");
            let document = parser.parse_with_timeline();
            let recorder = document
                .timeline
                .recorder_tactical_aid_usage
                .player_id
                .expect("recorder");
            assert_eq!(document.timeline.events.iter().filter(|event| matches!(event,
                parser::TimelineEvent::SpectatorViewChanged { player_id, .. } if *player_id == recorder)).count(), count);
            let detail = DetailView::from_parser_document(document);
            assert_eq!(detail.recorder_view, expected);
        }
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private demo263 spectator replay fixture"]
    fn configured_private_demo263_zero_score_spectators() {
        // SHA-256: 96a4dec09c046aefd3c7fe2a9bc077f1e9c214c5bde79c3a1afe6b2b899b225b
        let path = private_path("WIC_REPLAY_VIEWER_ZERO_SCORE_SPECTATORS");
        let parser = parser::WicReplayParser::new(&path).expect("demo263");
        let document = parser.parse_with_timeline();
        for (slot, name, team, score, role) in [
            (0, "[MAGOG]Kruelgor", 3, 2968, Some("armor")),
            (6, "[CSiE]kickapoo149", 1, 2640, Some("infantry")),
            (1, "Default", 0, 0, None),
            (7, "[CSiE]JMann9", 0, 0, None),
        ] {
            let player = document
                .replay
                .players
                .iter()
                .find(|p| p.id == slot)
                .unwrap();
            assert_eq!(player.name, name);
            assert_eq!(player.team, Some(team));
            assert_eq!(player.score, Some(score));
            assert_eq!(player.role.as_deref(), role);
        }
        assert_eq!(document.replay.players.len(), 8);
        let detail = DetailView::from_parser_document(document);
        assert_eq!(
            detail
                .overview
                .players
                .iter()
                .filter(|p| p.team == Some(0))
                .count(),
            6
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private demo08 recorder replay fixture"]
    fn configured_private_demo08_recorder_is_player_after_lobby_spectating() {
        // SHA-256: e3fb409649d034796f4f73443b07eede142f7ca6112a33f4baea1a17679cc52c
        // Slot 7 spectates at 1.056s, joins USSR at 47.174s; gameplay starts at 282.891s.
        let path = private_path("WIC_REPLAY_VIEWER_RECORDER_LOBBY_REVIEW");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path).expect("demo08");
        let document = parser.parse_with_timeline();
        assert_eq!(
            document.timeline.recorder_tactical_aid_usage.player_id,
            Some(7)
        );
        assert!(!document.timeline.events.iter().any(|event| matches!(
            event,
            parser::TimelineEvent::SpectatorViewChanged { player_id: 7, .. }
        )));
        let detail = DetailView::from_parser_document(document);
        assert_eq!(detail.recorder_view, "Player POV");
        assert!(
            detail
                .overview
                .players
                .iter()
                .any(|player| player.id == 7 && player.team == Some(3))
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private spectator replay fixture"]
    fn configured_private_one_team_lobby_spectator_joins_playing_team() {
        let path = private_path("WIC_REPLAY_VIEWER_SPECTATOR_REVIEW");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path)
            .expect("spectator review replay");
        let raw = serde_json::to_string(&parser.parse_with_timeline()).expect("raw JSON");
        let detail = DetailView::from_json(&raw).expect("detail projection");
        let factions: HashSet<&str> = detail
            .tactical_aid_rows
            .iter()
            .map(|row| row.faction.as_str())
            .collect();

        assert_eq!(detail.recorder_view, "Player POV");
        assert!(factions.contains("USA"));
        assert!(factions.contains("USSR"));
        assert!(
            detail
                .tactical_aid_rows
                .iter()
                .any(|row| row.player_attribution == "Exact player")
        );
        assert!(
            detail
                .tactical_aid_rows
                .iter()
                .any(|row| row.player_attribution == "Team only")
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private all-team spectator replay fixture"]
    fn configured_private_all_team_lobby_spectator_joins_playing_team() {
        let path = private_path("WIC_REPLAY_VIEWER_ALL_TEAM_SPECTATOR_REVIEW");
        let parser = wic_replay_parser::parser::WicReplayParser::new(&path)
            .expect("all-team spectator review replay");
        let raw = serde_json::to_string(&parser.parse_with_timeline()).expect("raw JSON");
        let detail = DetailView::from_json(&raw).expect("detail projection");
        let factions: HashSet<&str> = detail
            .tactical_aid_rows
            .iter()
            .map(|row| row.faction.as_str())
            .collect();

        assert_eq!(detail.recorder_view, "Player POV");
        assert!(factions.len() >= 2);
        assert!(
            detail
                .tactical_aid_rows
                .iter()
                .any(|row| row.player_attribution == "Exact player")
        );
        assert!(
            detail
                .tactical_aid_rows
                .iter()
                .any(|row| row.player_attribution == "Team only")
        );
    }
}
