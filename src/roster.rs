//! Result presentation only: retain parser evidence and historical participants.

/// Prefer the replay end screen for Overview and library summaries. Historical
/// parser output and timeline identities retain their existing contracts.
pub fn prepare_results(
    parser: &wic_replay_parser::parser::WicReplayParser,
    replay: &mut wic_replay_parser::parser::ReplayData,
) -> (
    Vec<u32>,
    Vec<wic_replay_parser::parser::PlayerResultEvidence>,
) {
    if let Some(players) = parser.final_screen_players() {
        if !has_opposing_sides(players.iter()) && has_opposing_sides(replay.players.iter()) {
            let hidden = parser.abandoned_lobby_duplicate_slots(&replay.players);
            if has_opposing_sides(replay.players.iter().filter(|p| !hidden.contains(&p.id))) {
                return (hidden, parser.player_result_evidence(&replay.players));
            }
        }
        replay.players = players;
        replay.player_scores = replay
            .players
            .iter()
            .filter_map(|p| p.score)
            .filter(|s| *s != 0)
            .collect();
        replay.player_scores.sort_unstable_by(|a, b| b.cmp(a));
        return (Vec::new(), Vec::new());
    }
    (
        parser.abandoned_lobby_duplicate_slots(&replay.players),
        parser.player_result_evidence(&replay.players),
    )
}

// USA and NATO are alternative allied factions, not opposing sides.
fn has_opposing_sides<'a>(
    players: impl IntoIterator<Item = &'a wic_replay_parser::parser::Player>,
) -> bool {
    let mut allied = false;
    let mut soviet = false;
    for player in players {
        allied |= matches!(player.team, Some(1 | 2));
        soviet |= player.team == Some(3);
    }
    allied && soviet
}

/// Remove only confirmed departures when a playing team exceeds eight rows.
/// Tied departure times use slot ID for stable cache/import ordering.
pub fn overflow_departure_ids(
    players: impl IntoIterator<Item = (u32, Option<u32>, Option<f64>)>,
) -> Vec<u32> {
    let players: Vec<_> = players.into_iter().collect();
    let mut hidden = Vec::new();
    for team in 1..=3 {
        let count = players.iter().filter(|(_, t, _)| *t == Some(team)).count();
        let excess = count.saturating_sub(8);
        if excess == 0 {
            continue;
        }
        let mut departed: Vec<_> = players
            .iter()
            .filter_map(|(id, t, left)| {
                if *t != Some(team) {
                    return None;
                }
                left.filter(|time| time.is_finite() && *time >= 0.0)
                    .map(|time| (*id, time))
            })
            .collect();
        departed.sort_by(|(id_a, time_a), (id_b, time_b)| {
            time_a.total_cmp(time_b).then(id_a.cmp(id_b))
        });
        hidden.extend(departed.into_iter().take(excess).map(|(id, _)| id));
    }
    hidden
}

#[cfg(test)]
mod tests {
    use super::overflow_departure_ids;

    #[test]
    fn final_screen_projection_bypasses_recovery_and_survives_detail_cache() {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;
        use wic_replay_parser::parser::WicReplayParser;
        fn hash(s: &str) -> [u8; 4] {
            let (mut a, mut b) = (1u32, 0u32);
            for c in s.bytes() {
                a = (a + u32::from(c)) % 65521;
                b = (b + a) % 65521;
            }
            ((b << 16) | a).to_le_bytes()
        }
        fn field(name: &str, flag: u8, bytes: &[u8]) -> Vec<u8> {
            let size = (13 + bytes.len()) as u32;
            [
                hash(name).as_slice(),
                &size.to_le_bytes(),
                &[flag],
                &size.to_le_bytes(),
                bytes,
            ]
            .concat()
        }
        fn event(name: &str, body: Vec<u8>) -> Vec<u8> {
            [
                hash("Event").as_slice(),
                &21u32.to_le_bytes(),
                &[6],
                &((21 + body.len()) as u32).to_le_bytes(),
                &0f32.to_le_bytes(),
                &hash(name),
                &body,
            ]
            .concat()
        }
        let mut entry = field("aSlot", 1, &0u32.to_le_bytes());
        entry.extend(field("team", 0, &3u32.to_le_bytes()));
        entry.extend(field("name", 5, &[b'A', 0, 0, 0]));
        entry.extend(field("readyFlag", 1, &1u32.to_le_bytes()));
        entry.extend(field("myType", 1, &0u32.to_le_bytes()));
        let mut summary = Vec::new();
        for (i, name) in [
            "aPos",
            "aRoleId",
            "aTotalScore",
            "aCapturingScore",
            "aFortificationScore",
            "aTransportationScore",
            "aRepairScore",
            "aBridgeLayingScore",
            "aUnitDamageScore",
            "aTacticalAidScore",
            "aScoreRole0",
            "aScoreRole1",
            "aScoreRole2",
            "aScoreRole3",
        ]
        .iter()
        .enumerate()
        {
            summary.extend(field(
                name,
                if i == 1 || i == 2 { 0 } else { 1 },
                &(if i == 2 { 7i32 } else { 0 }).to_le_bytes(),
            ));
        }
        for (complete, opposing_fallback) in [(true, false), (false, false), (true, true)] {
            let mut data = event("PlayerEntersGame", entry.clone());
            if complete {
                data.extend(event("SetScoreAtGameEnd", summary.clone()));
            }
            data.extend(event("TeamWins", field("aTeam", 1, &3u32.to_le_bytes())));
            let mut compressed = ZlibEncoder::new(vec![0; 19], Compression::best());
            compressed.write_all(&data).unwrap();
            let p = WicReplayParser::from_bytes(&compressed.finish().unwrap()).unwrap();
            let mut doc = p.parse_with_timeline();
            if opposing_fallback {
                doc.replay.players = p.final_screen_players().unwrap();
                doc.replay.players[0].score = Some(99);
                doc.replay.players[0].team = Some(3);
                let mut opponent = doc.replay.players[0].clone();
                opponent.id = 1;
                opponent.name = "Departed opponent".into();
                opponent.team = Some(1);
                opponent.faction = Some("USA".into());
                doc.replay.players.push(opponent);
            }
            let (hidden, evidence) = super::prepare_results(&p, &mut doc.replay);
            let (detail, json) =
                crate::detail::DetailView::cache_parser_document(doc, hidden, evidence).unwrap();
            let cached = crate::detail::DetailView::from_json(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&detail).unwrap(),
                serde_json::to_value(cached).unwrap()
            );
            if opposing_fallback {
                assert_eq!(detail.overview.players.len(), 2);
                assert_eq!(detail.overview.players[0].score, Some(99));
            } else if complete {
                assert_eq!(detail.overview.players[0].score, Some(7));
                assert!(detail.overview.players[0].score_before_leave.is_none());
            }
        }
    }

    #[test]
    fn preserves_small_teams_and_removes_only_earliest_confirmed_departures() {
        let mut players: Vec<_> = (0..8).map(|id| (id, Some(3), None)).collect();
        players[1].2 = Some(50.0);
        players[2].2 = Some(20.0);
        assert!(overflow_departure_ids(players.clone()).is_empty());
        players.push((8, Some(3), None));
        assert_eq!(overflow_departure_ids(players.clone()), vec![2]);
        players.push((9, Some(3), None));
        assert_eq!(overflow_departure_ids(players.clone()), vec![2, 1]);
        players.push((10, Some(3), None));
        assert_eq!(overflow_departure_ids(players), vec![2, 1]);
    }

    #[test]
    fn treats_factions_separately_and_ignores_invalid_departures_and_spectators() {
        let mut players: Vec<_> = (0..9).map(|id| (id, Some(1), Some(f64::NAN))).collect();
        players.extend((9..18).map(|id| (id, Some(0), Some(0.0))));
        players.extend((18..27).map(|id| (id, None, Some(0.0))));
        assert!(overflow_departure_ids(players.clone()).is_empty());
        players[3].2 = Some(1.0);
        players[2].2 = Some(1.0);
        assert_eq!(overflow_departure_ids(players), vec![2]);
    }
}
