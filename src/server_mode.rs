use std::collections::BTreeMap;

const CLAN_MATCH: &str = "Clan Match";

/// Display labels for one replay's server classification, most specific first.
///
/// The residual — every mode positively ruled out — is labelled `Ranked`. That is
/// an inference, not a replay-side fact: the demo header does not serialize the
/// dedicated server's `RankedFlag`, so the parser correctly reports
/// `serverClassification.ranked` as null and the conclusion lives here instead.
/// It is an elimination argument over the five modes the replay does evidence,
/// closed by operator ground truth that unranked public servers were not a
/// category that got run. See the workspace finding
/// `findings/ranked-server-mode-hypothesis-2026-08-22.md`.
///
/// Two properties of that argument shape the code below. It runs one way only —
/// the dedicated server permits Ranked alongside Match Mode, clan, and tournament
/// servers, so a labelled replay is not thereby unranked — and it requires every
/// mode to be *positively* false. A missing input means the evidence was not
/// recovered, never that the mode was absent, so a partial classification stays
/// unlabelled rather than being promoted to `Ranked`.
pub fn labels(
    few_player_mode: Option<bool>,
    match_mode: Option<bool>,
    has_bots: Option<bool>,
    clan_match: Option<bool>,
    tournament_match: Option<bool>,
    ranked: Option<bool>,
) -> Vec<&'static str> {
    let fpm = few_player_mode == Some(true);
    let bots = has_bots == Some(true);

    if fpm {
        return if bots {
            vec!["FPM", "Bots"]
        } else {
            vec!["FPM"]
        };
    }
    if clan_match == Some(true) {
        return vec![CLAN_MATCH];
    }
    if tournament_match == Some(true) {
        return vec!["Tournament Match"];
    }
    if bots {
        return vec!["Bots"];
    }
    if match_mode == Some(true) {
        return vec!["Match Mode"];
    }
    if ranked == Some(true) {
        return vec!["Ranked"];
    }
    if [
        few_player_mode,
        match_mode,
        has_bots,
        clan_match,
        tournament_match,
    ]
    .iter()
    .all(|flag| *flag == Some(false))
    {
        return vec!["Ranked"];
    }

    Vec::new()
}

/// Count the selected result roster by team, independently of score or server mode.
/// Spectators do not count; an unknown team prevents a definitive format.
pub fn matchup(teams: impl IntoIterator<Item = Option<u32>>) -> Option<String> {
    let mut team_counts = BTreeMap::new();
    for team in teams {
        match team {
            Some(0) => {}
            Some(team) => *team_counts.entry(team).or_insert(0usize) += 1,
            None => return None,
        }
    }

    (team_counts.len() == 2).then(|| {
        team_counts
            .values()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("vs")
    })
}

#[cfg(test)]
mod tests {
    use super::{labels, matchup};

    #[test]
    fn specializes_match_mode_labels() {
        assert_eq!(
            labels(None, Some(true), None, Some(true), None, None),
            vec!["Clan Match"]
        );
        assert_eq!(
            labels(None, Some(true), Some(true), None, None, None),
            vec!["Bots"]
        );
        assert_eq!(
            labels(None, Some(true), None, None, Some(true), None),
            vec!["Tournament Match"]
        );
    }

    #[test]
    fn preserves_fpm_with_bots_as_a_combination() {
        assert_eq!(
            labels(Some(true), Some(true), Some(true), None, None, None),
            vec!["FPM", "Bots"]
        );
    }

    #[test]
    fn leaves_all_unknown_properties_unlabelled() {
        assert!(labels(None, None, None, None, None, None).is_empty());
    }

    #[test]
    fn labels_the_fully_ruled_out_residual_as_ranked() {
        assert_eq!(
            labels(
                Some(false),
                Some(false),
                Some(false),
                Some(false),
                Some(false),
                None
            ),
            vec!["Ranked"]
        );
    }

    #[test]
    fn leaves_the_residual_unlabelled_when_one_mode_is_unproven() {
        // Missing evidence is not proof the mode was absent, so the elimination
        // argument does not close and nothing is claimed.
        for index in 0..5 {
            let mut flags = [Some(false); 5];
            flags[index] = None;
            assert!(
                labels(flags[0], flags[1], flags[2], flags[3], flags[4], None).is_empty(),
                "input {index} unknown should leave the replay unlabelled"
            );
        }
    }

    #[test]
    fn resolves_two_team_player_counts_as_a_separate_format() {
        assert_eq!(
            matchup([Some(1), Some(1), Some(3), Some(3)]),
            Some("2vs2".to_owned())
        );
        assert_eq!(
            matchup([Some(2), Some(3), Some(3), Some(3)]),
            Some("1vs3".to_owned())
        );
    }

    #[test]
    fn excludes_spectators_and_falls_back_for_uncertain_teams() {
        assert_eq!(
            matchup([Some(0), Some(1), Some(1), Some(3), Some(3)]),
            Some("2vs2".to_owned())
        );
        assert_eq!(matchup([Some(1), Some(3), None]), None);
        assert_eq!(matchup([Some(1), Some(1)]), None);
        assert_eq!(matchup([Some(1), Some(2), Some(3)]), None);
        assert_eq!(matchup([]), None);
    }
}
