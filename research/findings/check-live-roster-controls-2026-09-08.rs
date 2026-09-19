//! Read-only control: use the actual production timeline and playback extractors.
#[path = "../../../src/playback.rs"]
#[allow(dead_code)]
mod playback;
fn main() {
    let input = std::env::args().nth(1).unwrap();
    let rows: Vec<serde_json::Value> =
        serde_json::from_reader(std::fs::File::open(input).unwrap()).unwrap();
    for r in rows {
        let path = std::path::Path::new(r["path"].as_str().unwrap());
        let p = wic_replay_parser::parser::WicReplayParser::new(path).unwrap();
        let timeline = p.parse_with_timeline().timeline;
        let live = playback::load(path).unwrap();
        println!(
            "{}",
            serde_json::json!({"input":r,"sessions":timeline.participant_sessions,"teams":live.score_teams,"scores":live.score_samples})
        );
    }
}
