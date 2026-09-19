//! Run as a viewer crate example in the Flatpak SDK cache.
#[path = "../src/roster.rs"]
mod roster;
fn main() {
    use std::io::Write;
    use wic_replay_parser::parser::WicReplayParser;
    let entries: Vec<serde_json::Value> =
        serde_json::from_reader(std::fs::File::open(std::env::args().nth(1).unwrap()).unwrap())
            .unwrap();
    use rayon::prelude::*;
    entries.par_iter().for_each(|item| {
        let path = std::path::Path::new(item["path"].as_str().unwrap());
        let output = match WicReplayParser::new(path) {
            Err(e) => serde_json::json!({"input":item,"error":e}),
            Ok(p) => {
                let mut replay = p.parse();
                let (hidden, evidence) = roster::prepare_results(&p, &mut replay);
                replay.players.retain(|p| !hidden.contains(&p.id));
                for player in &mut replay.players {
                    if let Some(score) = evidence
                        .iter()
                        .find(|e| e.player_id == player.id)
                        .and_then(|e| e.score_before_leave.as_ref())
                    {
                        player.score = Some(score.score);
                    }
                }
                let overflow = roster::overflow_departure_ids(
                    replay
                        .players
                        .iter()
                        .map(|p| (p.id, p.team, p.left_at_seconds.map(f64::from))),
                );
                replay.players.retain(|p| !overflow.contains(&p.id));
                serde_json::json!({"input":item,"players":replay.players})
            }
        };
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        serde_json::to_writer(&mut out, &output).unwrap();
        writeln!(out).unwrap();
    });
}
