//! Lazy, evidence-preserving map playback extraction.
//!
//! The canonical timeline intentionally excludes the high-volume `UnitFrame`
//! stream. The desktop viewer requests this projection only when its Replay tab
//! is opened. Positions are authoritative serialized checkpoints; consumers must
//! not imply simulated motion between them.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use wic_replay_parser::parser::{
    acquire_replay_work, decompress_replay_stream, is_infantry_soldier_type, read_replay_file,
};

const MAX_PLAYBACK_UNITS: usize = 65_536;
const MAX_PLAYBACK_FRAMES: usize = 1_000_000;
const MAX_PLAYBACK_OBJECTIVES: usize = 16_384;
const MAX_PLAYBACK_OBJECTIVE_CHANGES: usize = 1_000_000;
const MAX_NUCLEAR_EFFECTS: usize = 16_384;
const CREATE_CLOUD: u32 = hash(b"CreateCloud");
const SPAWN_EXPLOSION: u32 = hash(b"SpawnExplosion");
const SPAWN_EXPLOSION_WITH_CRATER: u32 = hash(b"SpawnExplosionWithCrater");
const MAX_AREA_EFFECTS: usize = 131_072;

#[derive(Clone, Copy)]
struct PlaybackLimits {
    score_samples: usize,
    units: usize,
    frames: usize,
    objectives: usize,
    objective_changes: usize,
}

impl Default for PlaybackLimits {
    fn default() -> Self {
        Self {
            score_samples: MAX_SCORE_SAMPLES,
            units: MAX_PLAYBACK_UNITS,
            frames: MAX_PLAYBACK_FRAMES,
            objectives: MAX_PLAYBACK_OBJECTIVES,
            objective_changes: MAX_PLAYBACK_OBJECTIVE_CHANGES,
        }
    }
}

const PLAYER_ENTERS_GAME: u32 = hash(b"PlayerEntersGame");
const ENTRY_TEAM: u32 = hash(b"team");
const PLAYER_JOINED_TEAM: u32 = hash(b"PlayerJoinedTeam");
const SPECTATOR_JOINED_TEAM: u32 = hash(b"SpectatorJoinedTeam");
const A_SLOT: u32 = hash(b"aSlot");
const SET_SCORE: u32 = hash(b"SetScore");
const A_POS: u32 = hash(b"aPos");
const A_PLAYER_SCORE: u32 = hash(b"aPlayerScore");
const MAX_SCORE_SAMPLES: usize = 1_000_000;

const EVENT: u32 = hash(b"Event");
const UNIT_CREATE: u32 = hash(b"UnitCreate");
const UNIT_FRAME: u32 = hash(b"UnitFrame");
const UNIT_HEALTH: u32 = hash(b"UnitHealth");
const UNIT_DESTROY: u32 = hash(b"UnitDestroy");
const UNIT_REMOVE: u32 = hash(b"UnitRemove");
const UNIT_SET_TEAM: u32 = hash(b"UnitSetTeam");
const ADD_COMMAND_POINT: u32 = hash(b"AddCommandPoint");
const ADD_PERIMETER_POINT: u32 = hash(b"AddPerimeterPoint");
const SET_COMMAND_POINT_OWNER: u32 = hash(b"SetCommandPointOwner");
const SET_PERIMETER_POINT_OWNER: u32 = hash(b"SetPerimeterPointOwner");
const FRAME_COMPACT: u32 = hash(b"UnitFrameDataCompact");
const FRAME_FULL: u32 = hash(b"UnitFrameData");
const A_UNIT: u32 = hash(b"aUnit");
const A_TEAM: u32 = hash(b"aTeam");
const A_TYPE: u32 = hash(b"aType");
const A_HEALTH: u32 = hash(b"aHealth");
const A_CURRENT_HEALTH: u32 = hash(b"aCurrentHealth");
const A_NAME: u32 = hash(b"aName");
const A_PARENT: u32 = hash(b"aParent");
const A_POSITION_X: u32 = hash(b"aPosition.x");
const A_POSITION_Y: u32 = hash(b"aPosition.y");
const A_POSITION_Z: u32 = hash(b"aPosition.z");

fn is_infantry_member(unit_type_id: Option<u32>) -> bool {
    unit_type_id.is_some_and(is_infantry_soldier_type)
}

const fn hash(bytes: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    let mut index = 0;
    while index < bytes.len() {
        a = (a + bytes[index] as u32) % 65_521;
        b = (b + a) % 65_521;
        index += 1;
    }
    (b << 16) | a
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackView {
    pub area_effects: Vec<AreaEffect>,
    pub nuclear_effects: Vec<NuclearEffect>,
    pub duration_seconds: f32,
    pub score_samples: Vec<ScoreSample>,
    pub score_teams: Vec<ScoreTeamChange>,
    pub units: Vec<PlaybackUnit>,
    pub objectives: Vec<PlaybackObjective>,
    pub objective_changes: Vec<ObjectiveChange>,
    pub frame_count: usize,
    pub malformed_frames: usize,
    pub semantics: &'static str,
}

/// Recorded footprint; generic explosions intentionally carry no TA attribution.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaEffect {
    pub time_seconds: f32,
    pub position: [f32; 3],
    pub radius: f32,
    pub duration_seconds: f32,
    pub kind: &'static str,
}

fn fixed_effect_fields(data: &[u8], envelope: Envelope, tags: &[u32], flags: &[u8]) -> bool {
    if envelope.body_end - envelope.body_start != tags.len() * 17 {
        return false;
    }
    tags.iter().enumerate().all(|(i, tag)| {
        let offset = envelope.body_start + i * 17;
        read_u32(data, offset) == Some(*tag)
            && read_u32(data, offset + 4) == Some(17)
            && read_u32(data, offset + 9) == Some(17)
            && data.get(offset + 8) == flags.get(i)
    })
}

fn area_effect(data: &[u8], envelope: Envelope) -> Option<AreaEffect> {
    let (kind, radius, duration_seconds) = if envelope.message == CREATE_CLOUD {
        let tags = [
            A_TYPE,
            A_POSITION_X,
            A_POSITION_Y,
            A_POSITION_Z,
            hash(b"aHeading"),
            hash(b"aTimeToLive"),
            A_TEAM,
        ];
        if !fixed_effect_fields(data, envelope, &tags, &[1, 2, 2, 2, 2, 2, 0])
            || field_f32(data, envelope, hash(b"aTimeToLive"))? != 25.0
            || !matches!(field_u32(data, envelope, A_TEAM)?, 1..=3)
        {
            return None;
        }
        // Stock b35 support-cloud indices, radii and lifetimes recovered by wic_ice.
        match field_u32(data, envelope, A_TYPE)? {
            0 | 10 | 20 => ("napalm", 14.0, 25.0),
            2 | 12 | 22 => ("chemical", 50.0, 25.0),
            3 | 13 | 23 => ("chemical", 65.0, 25.0),
            _ => return None,
        }
    } else {
        let tags = [
            A_POSITION_X,
            A_POSITION_Y,
            A_POSITION_Z,
            hash(b"aRadius"),
            hash(b"aStrength"),
            hash(b"anExplosionForce"),
            hash(b"aForestDestroyRadius"),
            hash(b"aCraterHitEffectIndex"),
        ];
        let count = if envelope.message == SPAWN_EXPLOSION {
            7
        } else {
            8
        };
        if !fixed_effect_fields(
            data,
            envelope,
            &tags[..count],
            &[2, 2, 2, 2, 2, 2, 2, 1][..count],
        ) {
            return None;
        }
        let radius = field_f32(data, envelope, hash(b"aRadius"))?;
        if radius <= 0.0 {
            return None;
        }
        ("explosion", radius, 1.5)
    };
    Some(AreaEffect {
        time_seconds: envelope.time,
        position: position(data, envelope)?,
        radius,
        duration_seconds,
        kind,
    })
}

/// A recorded nuclear effect creation, not a countdown-derived impact or issuer join.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NuclearEffect {
    pub time_seconds: f32,
    pub position: [f32; 3],
}

fn nuclear_effect(data: &[u8], envelope: Envelope) -> Option<NuclearEffect> {
    // Stock b35 cloud catalogue: the unique 140-second nuke sound/effect cloud.
    // Validate the complete CreateCloud wire shape before interpreting its index.
    let tags = [
        A_TYPE,
        A_POSITION_X,
        A_POSITION_Y,
        A_POSITION_Z,
        hash(b"aHeading"),
        hash(b"aTimeToLive"),
        A_TEAM,
    ];
    let flags = [1, 2, 2, 2, 2, 2, 0];
    if envelope.body_end - envelope.body_start != 7 * 17 {
        return None;
    }
    for (index, tag) in tags.iter().enumerate() {
        let offset = envelope.body_start + index * 17;
        if read_u32(data, offset)? != *tag
            || read_u32(data, offset + 4)? != 17
            || read_u32(data, offset + 9)? != 17
            || data.get(offset + 8)? != &flags[index]
        {
            return None;
        }
    }
    let kind = field_u32(data, envelope, A_TYPE)?;
    if !matches!(kind, 8 | 18 | 28 | 34 | 45)
        || field_f32(data, envelope, hash(b"aTimeToLive"))? != 140.0
        || !matches!(field_u32(data, envelope, A_TEAM)?, 1..=3)
    {
        return None;
    }
    Some(NuclearEffect {
        time_seconds: envelope.time,
        position: position(data, envelope)?,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreTeamChange {
    pub time_seconds: f32,
    pub player_id: u32,
    pub team: u32,
}

/// Signed, absolute player-score observations on the recording clock.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreSample {
    pub time_seconds: f32,
    pub player_id: u32,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackUnit {
    pub unit_id: u16,
    pub generation: u32,
    pub created_seconds: f32,
    pub spawn_position: [f32; 3],
    pub team: Option<u32>,
    pub unit_type_id: Option<u32>,
    pub is_infantry_member: bool,
    pub initial_health: Option<f32>,
    pub frames: Vec<PlaybackFrame>,
    pub health: Vec<ValueChange>,
    pub teams: Vec<TeamChange>,
    pub terminal: Option<TerminalState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackFrame {
    pub time_seconds: f32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueChange {
    pub time_seconds: f32,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamChange {
    pub time_seconds: f32,
    pub team: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalState {
    pub time_seconds: f32,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackObjective {
    pub kind: &'static str,
    pub id: Option<u32>,
    pub parent_id: Option<u32>,
    pub position: Option<[f32; 3]>,
    pub team: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveChange {
    pub time_seconds: f32,
    pub kind: &'static str,
    pub id: Option<u32>,
    pub team: Option<u32>,
}

#[derive(Clone, Copy)]
struct Envelope {
    time: f32,
    message: u32,
    body_start: usize,
    body_end: usize,
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    let value = f32::from_bits(read_u32(data, offset)?);
    value.is_finite().then_some(value)
}

fn field_bits(data: &[u8], envelope: Envelope, wanted: u32) -> Option<u32> {
    let mut cursor = envelope.body_start;
    let mut found = None;
    while cursor + 17 <= envelope.body_end {
        if read_u32(data, cursor) == Some(wanted)
            && data.get(cursor + 4..cursor + 8) == Some(&[0x11, 0, 0, 0])
            && data.get(cursor + 9..cursor + 13) == Some(&[0x11, 0, 0, 0])
        {
            if found.is_some() {
                return None;
            }
            found = read_u32(data, cursor + 13);
        }
        cursor += 1;
    }
    found
}

fn field_u32(data: &[u8], envelope: Envelope, wanted: u32) -> Option<u32> {
    field_bits(data, envelope, wanted)
}

// SetScore uses an unsigned slot and a signed absolute score. Reject other types.
fn score_field(data: &[u8], envelope: Envelope, tag: u32, flag: u8) -> Option<u32> {
    let value = field_bits(data, envelope, tag)?;
    let position = (envelope.body_start..envelope.body_end.saturating_sub(16))
        .find(|offset| read_u32(data, *offset) == Some(tag))?;
    (data.get(position + 8) == Some(&flag)).then_some(value)
}

fn field_f32(data: &[u8], envelope: Envelope, wanted: u32) -> Option<f32> {
    let value = f32::from_bits(field_bits(data, envelope, wanted)?);
    value.is_finite().then_some(value)
}

fn position(data: &[u8], envelope: Envelope) -> Option<[f32; 3]> {
    Some([
        field_f32(data, envelope, A_POSITION_X)?,
        field_f32(data, envelope, A_POSITION_Y)?,
        field_f32(data, envelope, A_POSITION_Z)?,
    ])
}

fn increment_bounded(count: &mut usize, maximum: usize, label: &str) -> Result<(), String> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| format!("Replay {label} count overflows"))?;
    if *count > maximum {
        return Err(format!("Replay exceeds the maximum of {maximum} {label}"));
    }
    Ok(())
}

fn envelope_at(data: &[u8], offset: usize) -> Option<(Envelope, usize)> {
    if read_u32(data, offset)? != EVENT
        || data.get(offset + 4..offset + 8)? != [0x15, 0, 0, 0]
        || *data.get(offset + 8)? != 6
    {
        return None;
    }
    let total = read_u32(data, offset + 9)? as usize;
    let end = offset.checked_add(total)?;
    if total < 21 || end > data.len() {
        return None;
    }
    Some((
        Envelope {
            time: read_f32(data, offset + 13)?,
            message: read_u32(data, offset + 17)?,
            body_start: offset + 21,
            body_end: end,
        },
        end,
    ))
}

fn chain_anchor(data: &[u8]) -> Option<usize> {
    let tag = EVENT.to_le_bytes();
    data.windows(4)
        .position(|bytes| bytes == tag)
        .and_then(|first| {
            let mut search = first;
            loop {
                let relative = data
                    .get(search..)?
                    .windows(4)
                    .position(|bytes| bytes == tag)?;
                let candidate = search + relative;
                let mut cursor = candidate;
                let mut linked = 0;
                while linked < 6 {
                    let Some((_, next)) = envelope_at(data, cursor) else {
                        break;
                    };
                    cursor = next;
                    linked += 1;
                }
                if linked == 6 || (linked > 0 && cursor == data.len()) {
                    return Some(candidate);
                }
                search = candidate + 1;
            }
        })
}

fn frame(data: &[u8], envelope: Envelope) -> Option<(u16, [f32; 3])> {
    let field = read_u32(data, envelope.body_start)?;
    let total = read_u32(data, envelope.body_start + 4)? as usize;
    if data.get(envelope.body_start + 8) != Some(&6)
        || read_u32(data, envelope.body_start + 9)? as usize != total
        || envelope.body_start + total != envelope.body_end
    {
        return None;
    }
    let payload = envelope.body_start + 13;
    if field == FRAME_COMPACT {
        let header = u16::from_le_bytes(data.get(payload..payload + 2)?.try_into().ok()?);
        let word = |index: usize| -> Option<u16> {
            Some(u16::from_le_bytes(
                data.get(payload + index * 2..payload + index * 2 + 2)?
                    .try_into()
                    .ok()?,
            ))
        };
        let decode = |value: u16| (f32::from(value) - 0.5) * 0.023_809_524_f32;
        Some((
            header & 0x0fff,
            [decode(word(1)?), decode(word(2)?), decode(word(3)?)],
        ))
    } else if field == FRAME_FULL {
        Some((
            u16::from_le_bytes(data.get(payload + 28..payload + 30)?.try_into().ok()?),
            [
                read_f32(data, payload)?,
                read_f32(data, payload + 4)?,
                read_f32(data, payload + 8)?,
            ],
        ))
    } else {
        None
    }
}

fn load_with_frames(path: &Path, include_frames: bool) -> Result<PlaybackView, String> {
    let _work = acquire_replay_work();
    let raw = read_replay_file(path)?;
    let data = decompress_replay_stream(&raw)?;
    decode_stream(&data, include_frames, PlaybackLimits::default())
}

fn decode_stream(
    data: &[u8],
    include_frames: bool,
    limits: PlaybackLimits,
) -> Result<PlaybackView, String> {
    let mut cursor = chain_anchor(data).ok_or_else(|| "Replay has no Event chain".to_owned())?;
    let mut duration = 0.0_f32;
    let mut previous = None;
    let mut active: HashMap<u16, usize> = HashMap::new();
    let mut generations: HashMap<u16, u32> = HashMap::new();
    let mut units: Vec<PlaybackUnit> = Vec::new();
    let mut objectives = Vec::new();
    let mut objective_changes = Vec::new();
    let mut score_samples = Vec::new();
    let mut score_teams = Vec::new();
    let mut frame_count = 0;
    let mut malformed_frames = 0;
    let mut nuclear_effects = Vec::new();
    let mut area_effects = Vec::new();

    while let Some((envelope, next)) = envelope_at(data, cursor) {
        if previous.is_some_and(|time| time - envelope.time > 1.0) {
            break;
        }
        previous = Some(envelope.time);
        duration = duration.max(envelope.time);
        match envelope.message {
            CREATE_CLOUD if include_frames => {
                if let Some(effect) = area_effect(data, envelope) {
                    if area_effects.len() >= MAX_AREA_EFFECTS {
                        return Err("Replay exceeds maximum area effect records".to_owned());
                    }
                    area_effects.push(effect);
                }
                if let Some(effect) = nuclear_effect(data, envelope) {
                    if nuclear_effects.len() >= MAX_NUCLEAR_EFFECTS {
                        return Err("Replay exceeds maximum nuclear effect records".to_owned());
                    }
                    nuclear_effects.push(effect);
                }
            }
            SPAWN_EXPLOSION | SPAWN_EXPLOSION_WITH_CRATER if include_frames => {
                if let Some(effect) = area_effect(data, envelope) {
                    if area_effects.len() >= MAX_AREA_EFFECTS {
                        return Err("Replay exceeds maximum area effect records".to_owned());
                    }
                    area_effects.push(effect);
                }
            }
            PLAYER_ENTERS_GAME | PLAYER_JOINED_TEAM | SPECTATOR_JOINED_TEAM if include_frames => {
                if let (Some(player_id), Some(team)) = (
                    field_u32(data, envelope, A_SLOT).filter(|id| *id < 16),
                    field_u32(
                        data,
                        if envelope.message == PLAYER_ENTERS_GAME {
                            Envelope {
                                body_end: envelope.body_end.min(envelope.body_start + 34),
                                ..envelope
                            }
                        } else {
                            envelope
                        },
                        if envelope.message == PLAYER_ENTERS_GAME {
                            ENTRY_TEAM
                        } else {
                            A_TEAM
                        },
                    ),
                ) {
                    if score_teams.len() >= limits.score_samples {
                        return Err("Replay exceeds the maximum playback team changes".to_owned());
                    }
                    score_teams.push(ScoreTeamChange {
                        time_seconds: envelope.time,
                        player_id,
                        // Spectator aTeam is the side being left, not joined.
                        team: if envelope.message == SPECTATOR_JOINED_TEAM {
                            0
                        } else {
                            team
                        },
                    });
                }
            }
            SET_SCORE if include_frames => {
                if let (Some(player_id), Some(score)) = (
                    score_field(data, envelope, A_POS, 1).filter(|id| *id < 16),
                    score_field(data, envelope, A_PLAYER_SCORE, 0),
                ) {
                    if score_samples.len() >= limits.score_samples {
                        return Err("Replay exceeds the maximum playback score samples".to_owned());
                    }
                    score_samples.push(ScoreSample {
                        time_seconds: envelope.time,
                        player_id,
                        score: score as i32,
                    });
                }
            }

            UNIT_CREATE => {
                let Some(raw_id) = field_u32(data, envelope, A_UNIT) else {
                    cursor = next;
                    continue;
                };
                let Some(spawn_position) = position(data, envelope) else {
                    cursor = next;
                    continue;
                };
                let unit_id = raw_id as u16;
                let generation = generations.entry(unit_id).or_default();
                *generation += 1;
                if let Some(index) = active.insert(unit_id, units.len()) {
                    units[index].terminal = Some(TerminalState {
                        time_seconds: envelope.time,
                        kind: "replaced",
                    });
                }
                if units.len() >= limits.units {
                    return Err(format!(
                        "Replay exceeds the maximum of {} playback units",
                        limits.units
                    ));
                }
                let unit_type_id = field_u32(data, envelope, A_TYPE);
                units.push(PlaybackUnit {
                    unit_id,
                    generation: *generation,
                    created_seconds: envelope.time,
                    spawn_position,
                    team: field_u32(data, envelope, A_TEAM),
                    unit_type_id,
                    is_infantry_member: is_infantry_member(unit_type_id),
                    initial_health: field_f32(data, envelope, A_CURRENT_HEALTH),
                    frames: Vec::new(),
                    health: Vec::new(),
                    teams: Vec::new(),
                    terminal: None,
                });
            }
            UNIT_FRAME => {
                increment_bounded(&mut frame_count, limits.frames, "playback frames")?;
                if let Some((unit_id, point)) = frame(data, envelope) {
                    if include_frames && let Some(index) = active.get(&unit_id).copied() {
                        units[index].frames.push(PlaybackFrame {
                            time_seconds: envelope.time,
                            position: point,
                        });
                    }
                } else {
                    malformed_frames += 1;
                }
            }
            UNIT_HEALTH => {
                if let (Some(id), Some(value)) = (
                    field_u32(data, envelope, A_UNIT),
                    field_f32(data, envelope, A_HEALTH),
                ) && let Some(index) = active.get(&(id as u16)).copied()
                {
                    units[index].health.push(ValueChange {
                        time_seconds: envelope.time,
                        value,
                    });
                }
            }
            UNIT_SET_TEAM => {
                if let (Some(id), Some(team)) = (
                    field_u32(data, envelope, A_UNIT),
                    field_u32(data, envelope, A_TEAM),
                ) && let Some(index) = active.get(&(id as u16)).copied()
                {
                    units[index].teams.push(TeamChange {
                        time_seconds: envelope.time,
                        team,
                    });
                }
            }
            UNIT_DESTROY | UNIT_REMOVE => {
                if let Some(id) = field_u32(data, envelope, A_UNIT)
                    && let Some(index) = active.remove(&(id as u16))
                {
                    units[index].terminal = Some(TerminalState {
                        time_seconds: envelope.time,
                        kind: if envelope.message == UNIT_DESTROY {
                            "destroyed"
                        } else {
                            "removed"
                        },
                    });
                }
            }
            ADD_COMMAND_POINT | ADD_PERIMETER_POINT => {
                if objectives.len() >= limits.objectives {
                    return Err(format!(
                        "Replay exceeds the maximum of {} playback objectives",
                        limits.objectives
                    ));
                }
                objectives.push(PlaybackObjective {
                    kind: if envelope.message == ADD_COMMAND_POINT {
                        "commandPoint"
                    } else {
                        "perimeterPoint"
                    },
                    id: field_u32(data, envelope, A_NAME),
                    parent_id: field_u32(data, envelope, A_PARENT),
                    position: position(data, envelope),
                    team: field_u32(data, envelope, A_TEAM),
                });
            }
            SET_COMMAND_POINT_OWNER | SET_PERIMETER_POINT_OWNER => {
                if objective_changes.len() >= limits.objective_changes {
                    return Err(format!(
                        "Replay exceeds the maximum of {} playback objective changes",
                        limits.objective_changes
                    ));
                }
                objective_changes.push(ObjectiveChange {
                    time_seconds: envelope.time,
                    kind: if envelope.message == SET_COMMAND_POINT_OWNER {
                        "commandPoint"
                    } else {
                        "perimeterPoint"
                    },
                    id: field_u32(data, envelope, A_NAME),
                    team: field_u32(data, envelope, A_TEAM),
                })
            }
            _ => {}
        }
        cursor = next;
    }
    area_effects.sort_by(|a, b| a.time_seconds.total_cmp(&b.time_seconds));
    Ok(PlaybackView {
        area_effects,
        nuclear_effects,
        duration_seconds: duration,
        units,
        score_samples,
        score_teams,
        objectives,
        objective_changes,
        frame_count,
        malformed_frames,
        semantics: "authoritative checkpoints; hold-last between frames",
    })
}

pub fn load(path: &Path) -> Result<PlaybackView, String> {
    load_with_frames(path, true)
}

pub fn load_objectives(path: &Path) -> Result<Vec<PlaybackObjective>, String> {
    Ok(load_with_frames(path, false)?.objectives)
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

    fn field_u32_bytes(tag: u32, value: u32) -> Vec<u8> {
        let mut field = Vec::new();
        field.extend_from_slice(&tag.to_le_bytes());
        field.extend_from_slice(&[0x11, 0, 0, 0, 0, 0x11, 0, 0, 0]);
        field.extend_from_slice(&value.to_le_bytes());
        field
    }

    fn field_f32_bytes(tag: u32, value: f32) -> Vec<u8> {
        field_u32_bytes(tag, value.to_bits())
    }

    fn envelope(data: &mut Vec<u8>, message: u32, time: f32, body: &[u8]) {
        let total = u32::try_from(21 + body.len()).expect("fixture envelope fits");
        data.extend_from_slice(&EVENT.to_le_bytes());
        data.extend_from_slice(&[0x15, 0, 0, 0, 6]);
        data.extend_from_slice(&total.to_le_bytes());
        data.extend_from_slice(&time.to_bits().to_le_bytes());
        data.extend_from_slice(&message.to_le_bytes());
        data.extend_from_slice(body);
    }

    fn unit_body(id: u32) -> Vec<u8> {
        let mut body = field_u32_bytes(A_UNIT, id);
        body.extend(field_u32_bytes(A_TEAM, 1));
        body.extend(field_u32_bytes(A_TYPE, 0x43b5_073a));
        body.extend(field_f32_bytes(A_CURRENT_HEALTH, 1.0));
        body.extend(field_f32_bytes(A_POSITION_X, 10.0));
        body.extend(field_f32_bytes(A_POSITION_Y, 20.0));
        body.extend(field_f32_bytes(A_POSITION_Z, 30.0));
        body
    }

    #[test]
    fn nuclear_effects_require_exact_cloud_shape_and_use_recording_time() {
        let cloud = |kind: u32, ttl: f32| {
            let values = [
                kind,
                821.0_f32.to_bits(),
                38.0_f32.to_bits(),
                1098.0_f32.to_bits(),
                0.0_f32.to_bits(),
                ttl.to_bits(),
                3,
            ];
            let tags = [
                A_TYPE,
                A_POSITION_X,
                A_POSITION_Y,
                A_POSITION_Z,
                hash(b"aHeading"),
                hash(b"aTimeToLive"),
                A_TEAM,
            ];
            let flags = [1, 2, 2, 2, 2, 2, 0];
            let mut body = Vec::new();
            for i in 0..7 {
                let mut field = field_u32_bytes(tags[i], values[i]);
                field[8] = flags[i];
                body.extend(field);
            }
            body
        };
        let mut data = Vec::new();
        for kind in [8, 18, 28, 34, 45] {
            envelope(&mut data, CREATE_CLOUD, 854.822, &cloud(kind, 140.0));
        }
        for (kind, ttl) in [(9, 140.0), (18, 85.0), (18, f32::NAN)] {
            envelope(&mut data, CREATE_CLOUD, 855.0, &cloud(kind, ttl));
        }
        let mut wrong_shape = cloud(18, 140.0);
        wrong_shape[8] = 2;
        envelope(&mut data, CREATE_CLOUD, 855.0, &wrong_shape);
        wrong_shape.push(0);
        envelope(&mut data, CREATE_CLOUD, 855.0, &wrong_shape);
        let view = decode_stream(&data, true, PlaybackLimits::default()).unwrap();
        assert_eq!(view.nuclear_effects.len(), 5);
        assert_eq!(view.nuclear_effects[0].time_seconds, 854.822);
        assert_eq!(view.nuclear_effects[0].position, [821.0, 38.0, 1098.0]);
        assert!(
            decode_stream(&data, false, PlaybackLimits::default())
                .unwrap()
                .nuclear_effects
                .is_empty()
        );
    }

    #[test]
    fn area_effects_keep_recorded_radii_and_validate_cloud_identity() {
        let fields = |values: &[(u32, u8, u32)]| {
            let mut body = Vec::new();
            for &(tag, flag, bits) in values {
                let mut field = field_u32_bytes(tag, bits);
                field[8] = flag;
                body.extend(field);
            }
            body
        };
        let mut data = Vec::new();
        let coords = [
            (A_POSITION_X, 2, 100.0_f32.to_bits()),
            (A_POSITION_Y, 2, 0),
            (A_POSITION_Z, 2, 200.0_f32.to_bits()),
        ];
        for (message, radius) in [
            (SPAWN_EXPLOSION, 6.0_f32),
            (SPAWN_EXPLOSION_WITH_CRATER, 35.0),
        ] {
            let mut values = coords.to_vec();
            values.extend([
                (hash(b"aRadius"), 2, radius.to_bits()),
                (hash(b"aStrength"), 2, 250.0_f32.to_bits()),
                (hash(b"anExplosionForce"), 2, 400.0_f32.to_bits()),
                (hash(b"aForestDestroyRadius"), 2, 0),
            ]);
            if message == SPAWN_EXPLOSION_WITH_CRATER {
                values.push((hash(b"aCraterHitEffectIndex"), 1, 24));
            }
            envelope(&mut data, message, 10.0, &fields(&values));
            values[3].2 = f32::NAN.to_bits();
            envelope(&mut data, message, 10.0, &fields(&values));
            values[3].2 = 0;
            envelope(&mut data, message, 10.0, &fields(&values));
        }
        for index in [0, 10, 20, 2, 12, 22, 3, 13, 23, 8, 999] {
            let mut values = vec![(A_TYPE, 1, index)];
            values.extend(coords);
            values.extend([
                (hash(b"aHeading"), 2, 0),
                (hash(b"aTimeToLive"), 2, 25.0_f32.to_bits()),
                (A_TEAM, 0, 1),
            ]);
            envelope(&mut data, CREATE_CLOUD, 10.0, &fields(&values));
            values[5].2 = 140.0_f32.to_bits();
            envelope(&mut data, CREATE_CLOUD, 10.0, &fields(&values));
            values[0].1 = 2;
            envelope(&mut data, CREATE_CLOUD, 10.0, &fields(&values));
        }
        let view = decode_stream(&data, true, PlaybackLimits::default()).unwrap();
        assert_eq!(view.area_effects.len(), 11);
        assert_eq!(view.area_effects[0].radius, 6.0);
        assert_eq!(view.area_effects[1].radius, 35.0);
        assert_eq!(view.area_effects[2].kind, "napalm");
        assert_eq!(view.area_effects[2].radius, 14.0);
        assert_eq!(view.area_effects[5].radius, 50.0);
        assert_eq!(view.area_effects[8].radius, 65.0);
        assert!(
            decode_stream(&data, false, PlaybackLimits::default())
                .unwrap()
                .area_effects
                .is_empty()
        );
    }

    fn variable_frame(tag: u32, payload: &[u8]) -> Vec<u8> {
        let total = u32::try_from(13 + payload.len()).expect("fixture frame fits");
        let mut body = Vec::new();
        body.extend_from_slice(&tag.to_le_bytes());
        body.extend_from_slice(&total.to_le_bytes());
        body.push(6);
        body.extend_from_slice(&total.to_le_bytes());
        body.extend_from_slice(payload);
        body
    }

    fn score_body(player: u32, score: i32) -> Vec<u8> {
        let mut body = field_u32_bytes(A_POS, player);
        body[8] = 1;
        body.extend(field_u32_bytes(A_PLAYER_SCORE, score as u32));
        body
    }

    #[test]
    fn scores_preserve_signed_absolute_values_order_and_recording_boundary() {
        let mut data = Vec::new();
        envelope(&mut data, SET_SCORE, 2.0, &score_body(0, 10));
        envelope(&mut data, SET_SCORE, 3.0, &score_body(0, -2));
        envelope(&mut data, SET_SCORE, 3.0, &score_body(1, 7));
        envelope(&mut data, SET_SCORE, 3.0, &score_body(16, 999));
        let mut wrong_type = score_body(2, 9);
        wrong_type[25] = 2;
        envelope(&mut data, SET_SCORE, 3.0, &wrong_type);
        envelope(&mut data, SET_SCORE, 3.0, &score_body(2, 9)[..33]);
        envelope(&mut data, SET_SCORE, 0.0, &score_body(0, 999));
        let view = decode_stream(&data, true, PlaybackLimits::default()).unwrap();
        assert_eq!(
            view.score_samples
                .iter()
                .map(|s| (s.time_seconds, s.player_id, s.score))
                .collect::<Vec<_>>(),
            vec![(2.0, 0, 10), (3.0, 0, -2), (3.0, 1, 7)]
        );
        assert!(
            decode_stream(&data, false, PlaybackLimits::default())
                .unwrap()
                .score_samples
                .is_empty()
        );
        assert!(
            decode_stream(
                &data,
                true,
                PlaybackLimits {
                    score_samples: 1,
                    ..PlaybackLimits::default()
                }
            )
            .unwrap_err()
            .contains("score samples")
        );
    }

    #[test]
    fn scoreboard_retains_lobby_teams_and_spectator_transitions() {
        let mut data = Vec::new();
        let mut entry = field_u32_bytes(A_SLOT, 2);
        entry.extend(field_u32_bytes(ENTRY_TEAM, 2));
        // Variable name payloads must not be scanned for additional team fields.
        entry.extend(field_u32_bytes(ENTRY_TEAM, 9));
        envelope(&mut data, PLAYER_ENTERS_GAME, 0.0, &entry);
        let mut team = field_u32_bytes(A_SLOT, 2);
        team.extend(field_u32_bytes(A_TEAM, 3));
        for (message, time) in [
            (PLAYER_JOINED_TEAM, 0.0),
            (SPECTATOR_JOINED_TEAM, 1.0),
            (PLAYER_JOINED_TEAM, 2.0),
        ] {
            envelope(&mut data, message, time, &team);
        }
        for _ in 0..3 {
            envelope(&mut data, SET_SCORE, 3.0, &score_body(2, 5));
        }
        let view = decode_stream(&data, true, PlaybackLimits::default()).unwrap();
        assert_eq!(
            view.score_teams
                .iter()
                .map(|t| (t.time_seconds, t.player_id, t.team))
                .collect::<Vec<_>>(),
            vec![(0.0, 2, 2), (0.0, 2, 3), (1.0, 2, 0), (2.0, 2, 3)]
        );
        assert!(
            decode_stream(
                &data,
                true,
                PlaybackLimits {
                    score_samples: 1,
                    ..PlaybackLimits::default()
                }
            )
            .unwrap_err()
            .contains("team changes")
        );
    }

    #[test]
    fn synthetic_event_chain_reconstructs_lifecycles_frames_and_objectives() {
        let mut data = Vec::new();
        envelope(&mut data, UNIT_CREATE, 0.0, &unit_body(1));

        let mut compact = Vec::new();
        for word in [1_u16, 442, 862, 1282] {
            compact.extend_from_slice(&word.to_le_bytes());
        }
        envelope(
            &mut data,
            UNIT_FRAME,
            0.1,
            &variable_frame(FRAME_COMPACT, &compact),
        );

        let mut full = vec![0_u8; 30];
        full[0..4].copy_from_slice(&40_f32.to_bits().to_le_bytes());
        full[4..8].copy_from_slice(&50_f32.to_bits().to_le_bytes());
        full[8..12].copy_from_slice(&60_f32.to_bits().to_le_bytes());
        full[28..30].copy_from_slice(&1_u16.to_le_bytes());
        envelope(
            &mut data,
            UNIT_FRAME,
            0.2,
            &variable_frame(FRAME_FULL, &full),
        );
        envelope(&mut data, UNIT_FRAME, 0.25, &[0; 17]);

        let mut health = field_u32_bytes(A_UNIT, 1);
        health.extend(field_f32_bytes(A_HEALTH, 0.5));
        envelope(&mut data, UNIT_HEALTH, 0.3, &health);
        let mut team = field_u32_bytes(A_UNIT, 1);
        team.extend(field_u32_bytes(A_TEAM, 3));
        envelope(&mut data, UNIT_SET_TEAM, 0.4, &team);

        let mut objective = field_u32_bytes(A_NAME, 10);
        objective.extend(field_u32_bytes(A_PARENT, 9));
        objective.extend(field_u32_bytes(A_TEAM, 1));
        objective.extend(field_f32_bytes(A_POSITION_X, 1.0));
        objective.extend(field_f32_bytes(A_POSITION_Y, 2.0));
        objective.extend(field_f32_bytes(A_POSITION_Z, 3.0));
        envelope(&mut data, ADD_COMMAND_POINT, 0.5, &objective);
        envelope(&mut data, ADD_PERIMETER_POINT, 0.6, &objective);
        envelope(&mut data, SET_COMMAND_POINT_OWNER, 0.7, &objective);
        envelope(&mut data, SET_PERIMETER_POINT_OWNER, 0.8, &objective);

        envelope(&mut data, UNIT_CREATE, 0.9, &unit_body(1));
        envelope(&mut data, UNIT_DESTROY, 1.0, &field_u32_bytes(A_UNIT, 1));
        envelope(&mut data, UNIT_CREATE, 1.1, &unit_body(2));
        envelope(&mut data, UNIT_REMOVE, 1.2, &field_u32_bytes(A_UNIT, 2));
        envelope(&mut data, hash(b"Unknown"), 0.0, &[]);

        let view = decode_stream(&data, true, PlaybackLimits::default()).expect("decode stream");
        assert_eq!(view.duration_seconds, 1.2);
        assert_eq!(view.frame_count, 3);
        assert_eq!(view.malformed_frames, 1);
        assert_eq!(view.units.len(), 3);
        assert_eq!(
            view.units[0].terminal.as_ref().map(|state| state.kind),
            Some("replaced")
        );
        assert_eq!(view.units[0].frames.len(), 2);
        assert_eq!(view.units[0].health[0].value, 0.5);
        assert_eq!(view.units[0].teams[0].team, 3);
        assert_eq!(
            view.units[1].terminal.as_ref().map(|state| state.kind),
            Some("destroyed")
        );
        assert_eq!(
            view.units[2].terminal.as_ref().map(|state| state.kind),
            Some("removed")
        );
        assert_eq!(view.objectives.len(), 2);
        assert_eq!(view.objective_changes.len(), 2);

        let objectives_only =
            decode_stream(&data, false, PlaybackLimits::default()).expect("objective-only stream");
        assert!(objectives_only.units[0].frames.is_empty());
    }

    #[test]
    fn synthetic_decoder_rejects_missing_chains_and_every_injected_limit() {
        assert_eq!(
            decode_stream(&[], true, PlaybackLimits::default()).unwrap_err(),
            "Replay has no Event chain"
        );
        for (message, body, limits, label) in [
            (
                UNIT_CREATE,
                unit_body(1),
                PlaybackLimits {
                    units: 0,
                    ..PlaybackLimits::default()
                },
                "playback units",
            ),
            (
                UNIT_FRAME,
                variable_frame(FRAME_COMPACT, &[0; 8]),
                PlaybackLimits {
                    frames: 0,
                    ..PlaybackLimits::default()
                },
                "playback frames",
            ),
            (
                ADD_COMMAND_POINT,
                Vec::new(),
                PlaybackLimits {
                    objectives: 0,
                    ..PlaybackLimits::default()
                },
                "playback objectives",
            ),
            (
                SET_COMMAND_POINT_OWNER,
                Vec::new(),
                PlaybackLimits {
                    objective_changes: 0,
                    ..PlaybackLimits::default()
                },
                "playback objective changes",
            ),
        ] {
            let mut data = Vec::new();
            envelope(&mut data, message, 0.0, &body);
            let error = decode_stream(&data, true, limits).expect_err("limit must fail");
            assert!(error.contains(label), "{error}");
        }
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private replay corpus"]
    fn configured_high_envelope_replays_load_without_a_generic_event_ceiling() {
        let corpus_root = private_path("WIC_REPLAY_VIEWER_CORPUS");
        for (relative, expected_duration, expected_frames) in [
            (
                "WicTracker/downloads/1217__1_vs_1fpm.wicdemo",
                1743.3148_f32,
                491_489,
            ),
            (
                "WicTracker/downloads/600__demo01.wicdemo",
                5415.8813_f32,
                880_835,
            ),
        ] {
            let path = corpus_root.join(relative);
            let replay = load(&path).unwrap_or_else(|error| {
                panic!(
                    "cannot load high-envelope replay {}: {error}",
                    path.display()
                )
            });
            assert!((replay.duration_seconds - expected_duration).abs() < 0.01);
            assert_eq!(replay.frame_count, expected_frames);
        }
    }

    #[test]
    fn distinguishes_infantry_members_from_their_squad_parent() {
        assert!(is_infantry_member(Some(0x11d8_0374))); // NATO_Medic
        assert!(!is_infantry_member(Some(0x43b5_073a))); // NATO_Squad_Infantry
        assert!(!is_infantry_member(None));
    }

    #[test]
    fn bounded_counter_rejects_the_first_value_over_its_limit() {
        let mut count = 1;
        increment_bounded(&mut count, 2, "test events").expect("at limit");
        let error = increment_bounded(&mut count, 2, "test events").expect_err("over limit");
        assert_eq!(count, 3);
        assert!(error.contains("maximum of 2 test events"));
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires WIC_PLAYBACK_REPLAY pointing to the Quarry positive control"]
    fn playback_real_replay() {
        let path = std::env::var("WIC_PLAYBACK_REPLAY").expect("WIC_PLAYBACK_REPLAY");
        let replay = load(Path::new(&path)).expect("decode playback");
        assert_eq!(replay.units.len(), 480);
        assert_eq!(
            replay
                .units
                .iter()
                .filter(|unit| unit.is_infantry_member)
                .count(),
            245
        );
        assert_eq!(
            replay
                .units
                .iter()
                .filter(|unit| {
                    matches!(
                        unit.unit_type_id,
                        Some(
                            0x3974_0697
                                | 0x39a8_06b0
                                | 0x4381_0721
                                | 0x43b5_073a
                                | 0x4569_073c
                                | 0x459d_0755
                                | 0x4799_075d
                                | 0x52ba_07e7
                                | 0x54d8_0802
                        )
                    )
                })
                .count(),
            90
        );
        assert_eq!(replay.frame_count, 219_659);
        assert_eq!(replay.malformed_frames, 0);
        assert_eq!(replay.objectives.len(), 14);
        assert_eq!(replay.objective_changes.len(), 98);
        assert_eq!(
            load_objectives(Path::new(&path)).expect("objectives").len(),
            14
        );
    }
}
