use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Bump when parser output, timeline schema, or cached detail JSON changes.
/// Summary-only migrations can mark summaries stale while preserving lazy details.
pub const PARSER_CACHE_KEY: &str = "wic-replay-parser@cf194232abab4436d5f921f0e6ca48d41fc9f5e0/timeline-v18/detail-v48/replay-name-v1";
pub const DATABASE_SCHEMA_VERSION: i32 = 13;

pub(crate) fn human_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            output.push_str(&rest[start..]);
            rest = "";
            break;
        };
        rest = &rest[start + end + 1..];
    }
    output.push_str(rest);
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFingerprint {
    pub size: u64,
    pub modified_ns: i64,
}

impl FileFingerprint {
    pub fn read(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("Not a file: {}", path.display()));
        }
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let modified_ns = modified
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .min(i64::MAX as u128) as i64;
        Ok(Self {
            size: metadata.len(),
            modified_ns,
        })
    }
}

/// A selected result occupant, preserving the name/faction association for search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPlayer {
    pub name: String,
    pub faction: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaySummary {
    pub path: PathBuf,
    pub file_name: String,
    pub replay_name: Option<String>,
    pub server_name: String,
    pub fingerprint: FileFingerprint,
    pub cache_key: String,
    pub map_name: String,
    pub map_display_name: String,
    pub game_mode: String,
    pub server_modes: String,
    pub format: String,
    pub date_time: String,
    /// Match length: how long the game ran.
    pub duration_seconds: Option<f32>,
    /// Replay length: how long the recording runs. Larger than
    /// `duration_seconds` whenever the recording caught lobby time or kept
    /// running past the match, which is the normal case.
    pub recording_seconds: Option<f32>,
    pub winner: Option<String>,
    pub player_count: u32,
    pub player_names: String,
    pub search_players: Vec<SearchPlayer>,
    pub factions: String,
    pub recorder: Option<String>,
    pub recorder_faction: Option<String>,
    pub incomplete: bool,
    pub parse_error: Option<String>,
    pub imported_at: i64,
}

impl ReplaySummary {
    pub fn stale(&self) -> bool {
        self.cache_key != PARSER_CACHE_KEY
    }

    pub fn title(&self) -> &str {
        if self.map_display_name.is_empty() {
            &self.file_name
        } else {
            &self.map_display_name
        }
    }
}

pub fn sort_summaries(rows: &mut [ReplaySummary]) {
    rows.sort_by(|left, right| {
        right
            .date_time
            .cmp(&left.date_time)
            .then_with(|| {
                left.file_name
                    .to_lowercase()
                    .cmp(&right.file_name.to_lowercase())
            })
            .then_with(|| left.path.cmp(&right.path))
    });
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub discovered: usize,
    pub pending: usize,
    pub processed: usize,
    pub imported: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> ReplaySummary {
        ReplaySummary {
            path: PathBuf::from("/tmp/example.wicdemo"),
            file_name: "example.wicdemo".to_owned(),
            replay_name: Some("Older in-game title".to_owned()),
            server_name: "Example server".to_owned(),
            fingerprint: FileFingerprint {
                size: 12,
                modified_ns: 34,
            },
            cache_key: PARSER_CACHE_KEY.to_owned(),
            map_name: "do_Hometown".to_owned(),
            map_display_name: "Hometown".to_owned(),
            game_mode: "Domination".to_owned(),
            server_modes: "FPM · Bots".to_owned(),
            format: "1vs1".to_owned(),
            date_time: "2009-01-02".to_owned(),
            duration_seconds: Some(600.0),
            recording_seconds: Some(660.0),
            winner: Some("NATO".to_owned()),
            player_count: 2,
            search_players: Vec::new(),
            player_names: "Alpha, Bravo".to_owned(),
            factions: "NATO, USSR".to_owned(),
            recorder: Some("Alpha".to_owned()),
            recorder_faction: Some("USSR".to_owned()),
            incomplete: false,
            parse_error: None,
            imported_at: 1,
        }
    }

    #[test]
    fn cache_key_controls_staleness() {
        let mut row = summary();
        assert!(!row.stale());
        row.cache_key = "old".to_owned();
        assert!(row.stale());
    }

    #[test]
    fn human_text_removes_game_markup_and_normalizes_spacing() {
        assert_eq!(
            human_text("  <#06C>WiCGate</>  Ranked\n Server "),
            "WiCGate Ranked Server"
        );
    }
}
