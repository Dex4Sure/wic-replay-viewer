//! Read-only final-screen candidate; compile as a parser example.
#![allow(dead_code)]
mod parser {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/parser.rs"));
    pub fn inspect(path: &std::path::Path) -> Result<serde_json::Value, String> {
        let p = WicReplayParser::new(path)?;
        let data = &p.full_data;
        let timeline = p.envelope_timeline();
        let mut state: HashMap<u32, serde_json::Value> = HashMap::new();
        let mut summaries = HashMap::new();
        let mut errors = Vec::new();
        let mut result = None;
        let mut lan_names = 0;
        let mut summary_started = false;
        for &start in &timeline.starts {
            let pos = start + ENVELOPE_MESSAGE_OFFSET;
            let env = event_envelope(data, pos).unwrap();
            let tag = &data[pos..pos + 4];
            if tag == [0xf6, 0x05, 0x5f, 0x33] {
                lan_names += 1;
            }
            if tag == HASH_TEAM_WINS {
                result = Some(start);
                break;
            }
            if tag == [0x6f, 0x06, 0x31, 0x3a] {
                summary_started = true;
                if env.end != pos + 4 + 14 * 17 {
                    errors.push(format!("bad summary length at {pos}"));
                    continue;
                }
                let fields = [
                    HASH_APOS,
                    HASH_AROLE_ID,
                    HASH_TOTAL_SCORE,
                    HASH_CAPTURING_SCORE,
                    HASH_FORTIFICATION_SCORE,
                    HASH_TRANSPORTATION_SCORE,
                    HASH_REPAIR_SCORE,
                    HASH_BRIDGE_LAYING_SCORE,
                    HASH_UNIT_DAMAGE_SCORE,
                    HASH_TACTICAL_AID_SCORE,
                    HASH_SCORE_ROLE0,
                    HASH_SCORE_ROLE1,
                    HASH_SCORE_ROLE2,
                    HASH_SCORE_ROLE3,
                ];
                let vals: Option<Vec<u32>> = fields
                    .iter()
                    .enumerate()
                    .map(|(i, h)| read_expected_u32(data, pos + 4 + i * 17, h))
                    .collect();
                if let Some(v) = vals.filter(|v| v[0] < 16) {
                    let names = [
                        "slot",
                        "roleId",
                        "scoreTotal",
                        "scoreCapturing",
                        "scoreFortification",
                        "scoreTransportation",
                        "scoreRepair",
                        "scoreBridgeLaying",
                        "scoreUnitDamage",
                        "scoreTacticalAid",
                        "scoreInfantry",
                        "scoreSupport",
                        "scoreArmor",
                        "scoreAir",
                    ];
                    let mut row = serde_json::Map::new();
                    for (i, name) in names.iter().enumerate() {
                        row.insert(
                            name.to_string(),
                            if i < 2 {
                                serde_json::json!(v[i])
                            } else {
                                serde_json::json!(v[i] as i32)
                            },
                        );
                    }
                    row.insert(
                        "state".into(),
                        state.get(&v[0]).cloned().unwrap_or(serde_json::Value::Null),
                    );
                    summaries.insert(v[0], row);
                } else {
                    errors.push(format!("bad summary fields at {pos}"));
                }
            } else if tag == HASH_PLAYER_ENTERS_GAME {
                if summary_started {
                    errors.push("entry during summary".into());
                }
                if let Some((_, slot, name)) = parse_player_entry(data, pos) {
                    let team = read_expected_u32(data, pos + 21, &[0xa8, 0x01, 0x32, 0x04]); // Adler32("team")
                    let name_end =
                        read_event_field(data, pos + 38, env.end, &HASH_PLAYER_ENTRY_NAME, 5)
                            .map(|(_, end)| end);
                    let kind =
                        name_end.and_then(|end| read_expected_u32(data, end + 17, &HASH_MY_TYPE));
                    state.insert(slot,serde_json::json!({"name":name,"team":team,"kind":kind,"active":true,"spectator":false}));
                }
            } else if tag == HASH_PLAYER_LEAVES_GAME
                || tag == HASH_PLAYER_JOINED_TEAM
                || tag == HASH_SPECTATOR_JOINED_TEAM
            {
                if summary_started {
                    errors.push("roster event during summary".into());
                }
                if let Some(slot) = read_expected_u32(data, pos + 4, &HASH_ASLOT) {
                    if let Some(row) = state.get_mut(&slot) {
                        if tag == HASH_PLAYER_LEAVES_GAME {
                            row["active"] = false.into();
                        } else {
                            row["team"] =
                                serde_json::json!(read_expected_u32(data, pos + 21, &HASH_ATEAM));
                            row["spectator"] = (tag == HASH_SPECTATOR_JOINED_TEAM).into();
                        }
                    }
                }
            }
        }
        let mut rows: Vec<_> = summaries.into_iter().collect();
        rows.sort_by_key(|(slot, _)| *slot);
        Ok(
            serde_json::json!({"candidate":p.final_screen_players(),"result":result,"rows":rows.into_iter().map(|(_,r)|r).collect::<Vec<_>>(),"errors":errors,"state":state,"lanNames":lan_names}),
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
