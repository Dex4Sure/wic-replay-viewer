//! Comparison-only probe. Compile as a parser example in the Flatpak SDK cache.
//! Includes unchanged production parser source to inspect byte-offset evidence.
#![allow(dead_code)]
#[path = "../../../src/roster.rs"]
mod roster;
mod parser {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/parser.rs"));

    #[derive(Serialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct Segment {
        team: Option<u32>,
        start: usize,
        end: Option<usize>,
        source: String,
        units: usize,
        roles: usize,
        nonzero_scores: usize,
        unit_teams: std::collections::BTreeSet<u32>,
        first_activity: Option<usize>,
        last_activity: Option<usize>,
    }
    impl Segment {
        fn new(team: Option<u32>, start: usize, source: &str) -> Self {
            Self {
                team,
                start,
                end: None,
                source: source.into(),
                units: 0,
                roles: 0,
                nonzero_scores: 0,
                unit_teams: Default::default(),
                first_activity: None,
                last_activity: None,
            }
        }
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Session {
        slot: u32,
        name: Option<String>,
        start: usize,
        end: Option<usize>,
        start_time: f32,
        end_time: Option<f32>,
        explicit_entry: bool,
        segments: Vec<Segment>,
        last_score: Option<(usize, f32, i32)>,
    }
    impl Session {
        fn new(slot: u32, name: Option<String>, start: usize, time: f32, explicit: bool) -> Self {
            Self {
                slot,
                name,
                start,
                end: None,
                start_time: time,
                end_time: None,
                explicit_entry: explicit,
                segments: vec![Segment::new(None, start, "unknown")],
                last_score: None,
            }
        }
        fn team(&mut self, team: u32, offset: usize, source: &str) {
            if self.segments.last().unwrap().team == Some(team) {
                return;
            }
            self.segments.last_mut().unwrap().end = Some(offset);
            self.segments.push(Segment::new(Some(team), offset, source));
        }
    }
    pub fn inspect(path: &std::path::Path) -> Result<serde_json::Value, String> {
        let p = WicReplayParser::new(path)?;
        let replay = p.parse();
        let hidden = p.abandoned_lobby_duplicate_slots(&replay.players);
        let evidence = p.player_result_evidence(&replay.players);
        let mut current = serde_json::to_value(&replay.players).unwrap();
        let rows = current.as_array_mut().unwrap();
        rows.retain(|q| !hidden.contains(&(q["id"].as_u64().unwrap() as u32)));
        for e in &evidence {
            if let Some(score) = &e.score_before_leave {
                let q = rows.iter_mut().find(|q| q["id"] == e.player_id).unwrap();
                q["score"] = serde_json::json!(score.score);
                q["scoreBeforeLeave"] = serde_json::to_value(score).unwrap();
                for field in [
                    "scoreInfantry",
                    "scoreSupport",
                    "scoreArmor",
                    "scoreAir",
                    "scoreCapturing",
                    "scoreFortification",
                    "scoreTransportation",
                    "scoreRepair",
                    "scoreBridgeLaying",
                    "scoreUnitDamage",
                    "scoreTacticalAid",
                    "scoreTotal",
                ] {
                    q[field] = serde_json::Value::Null;
                }
            }
        }
        let overflow = super::roster::overflow_departure_ids(rows.iter().map(|r| {
            (
                r["id"].as_u64().unwrap() as u32,
                r["team"].as_u64().map(|t| t as u32),
                r["leftAtSeconds"].as_f64(),
            )
        }));
        rows.retain(|r| !overflow.contains(&(r["id"].as_u64().unwrap() as u32)));
        let data = &p.full_data;
        let timeline = p.envelope_timeline();
        let start = p.extract_clock_timeline().0.first().map(|s| s.offset);
        let mut sessions = Vec::<Session>::new();
        let mut active = [None; 16];
        let all = (0..16).collect();
        let mut seeds: Vec<_> = p.extract_resolved_slot_names(&all).into_iter().collect();
        seeds.sort_by_key(|s| s.0);
        for (slot, name) in seeds {
            active[slot as usize] = Some(sessions.len());
            sessions.push(Session::new(slot, Some(name), 0, 0.0, false));
        }
        let mut result = None;
        let mut events = 0;
        let mut unattributed = 0;
        for &offset in &timeline.starts {
            let pos = offset + ENVELOPE_MESSAGE_OFFSET;
            let Some(env) = event_envelope(data, pos) else {
                continue;
            };
            let tag = &data[pos..pos + 4];
            let time = env.raw_time_seconds;
            if tag == HASH_TEAM_WINS {
                result = Some((offset, time));
                break;
            }
            events += 1;
            if tag == HASH_PLAYER_ENTERS_GAME {
                if let Some((_, slot, name)) = parse_player_entry(data, pos) {
                    let previous = active[slot as usize];
                    let same = previous.is_some_and(|i| name.is_some() && sessions[i].name == name);
                    if !same {
                        if let Some(i) = previous {
                            sessions[i].end = Some(offset);
                            sessions[i].end_time = Some(time);
                            sessions[i].segments.last_mut().unwrap().end = Some(offset);
                        }
                        active[slot as usize] = Some(sessions.len());
                        sessions.push(Session::new(slot, name, offset, time, true));
                    }
                    let s = &mut sessions[active[slot as usize].unwrap()];
                    s.explicit_entry = true;
                    // Same bounded entry-team field used by the live scoreboard.
                    if data.get(pos + 21..pos + 25) == Some(&[168, 1, 50, 4]) {
                        if let Some(team) = read_bintag_u32(data, pos + 21) {
                            s.team(team, offset, "entry");
                        }
                    }
                }
                continue;
            }
            let slot = if tag == HASH_SET_SCORE {
                read_expected_u32(data, pos + 4, &HASH_APOS)
            } else if tag == HASH_UNIT_CREATE {
                read_expected_u32(data, pos + 21, &HASH_APLAYER)
            } else if [
                HASH_PLAYER_LEAVES_GAME,
                HASH_PLAYER_JOINED_TEAM,
                HASH_SPECTATOR_JOINED_TEAM,
                HASH_PLAYER_SET_ROLE,
            ]
            .iter()
            .any(|h| tag == h)
            {
                read_expected_u32(data, pos + 4, &HASH_ASLOT)
            } else {
                continue;
            };
            let Some(slot) = slot.filter(|s| *s < 16) else {
                continue;
            };
            let Some(i) = active[slot as usize] else {
                unattributed += 1;
                continue;
            };
            let s = &mut sessions[i];
            if tag == HASH_PLAYER_LEAVES_GAME {
                s.end = Some(env.end);
                s.end_time = Some(time);
                s.segments.last_mut().unwrap().end = Some(env.end);
                active[slot as usize] = None;
                continue;
            }
            if tag == HASH_PLAYER_JOINED_TEAM || tag == HASH_SPECTATOR_JOINED_TEAM {
                if let Some(team) = read_expected_u32(data, pos + 21, &HASH_ATEAM) {
                    s.team(
                        if tag == HASH_SPECTATOR_JOINED_TEAM {
                            0
                        } else {
                            team
                        },
                        offset,
                        if tag == HASH_SPECTATOR_JOINED_TEAM {
                            "spectator"
                        } else {
                            "joined"
                        },
                    );
                }
                continue;
            }
            let gameplay = start.is_some_and(|st| offset >= st);
            let segment = s.segments.last_mut().unwrap();
            let mut activity = false;
            if tag == HASH_SET_SCORE && pos + 38 <= env.end {
                if let Some(score) = read_expected_i32(data, pos + 21, &HASH_SCORE) {
                    s.last_score = Some((offset, time, score));
                    if gameplay && score != 0 {
                        segment.nonzero_scores += 1;
                        activity = true;
                    }
                }
            } else if tag == HASH_UNIT_CREATE && pos + 55 <= env.end && gameplay {
                if let Some(team) = read_expected_u32(data, pos + 38, &HASH_ATEAM) {
                    segment.units += 1;
                    segment.unit_teams.insert(team);
                    activity = true;
                }
            } else if tag == HASH_PLAYER_SET_ROLE && pos + 38 <= env.end && gameplay {
                if read_expected_u32(data, pos + 21, &HASH_AROLE_ID)
                    .and_then(role_name)
                    .is_some_and(|r| r != "fewPlayer")
                {
                    segment.roles += 1;
                    activity = true;
                }
            }
            if activity {
                segment.first_activity.get_or_insert(offset);
                segment.last_activity = Some(offset);
            }
        }
        Ok(
            serde_json::json!({"current":current,"raw":replay,"hidden":hidden,"evidence":evidence,
            "start":start,"result":result,"chainEvents":events,"unattributedEvents":unattributed,"sessions":sessions}),
        )
    }
}
fn main() {
    use std::{
        collections::VecDeque,
        io::Write,
        sync::{Arc, Mutex},
    };
    let input = std::env::args().nth(1).expect("input JSON manifest");
    let entries: Vec<serde_json::Value> =
        serde_json::from_reader(std::fs::File::open(input).unwrap()).unwrap();
    let queue = Arc::new(Mutex::new(VecDeque::from(entries)));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let queue = queue.clone();
            scope.spawn(move || {
                loop {
                    let Some(item) = queue.lock().unwrap().pop_front() else {
                        break;
                    };
                    let path = item["path"].as_str().unwrap();
                    let result = match parser::inspect(std::path::Path::new(path)) {
                        Ok(v) => serde_json::json!({"input":item,"data":v}),
                        Err(e) => serde_json::json!({"input":item,"error":e}),
                    };
                    let stdout = std::io::stdout();
                    let mut out = stdout.lock();
                    serde_json::to_writer(&mut out, &result).unwrap();
                    writeln!(out).unwrap();
                }
            });
        }
    });
}
