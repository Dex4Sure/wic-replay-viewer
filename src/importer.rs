use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::Serialize;
use wic_replay_parser::parser::{WicReplayParser, available_replay_workers, validate_replay_path};

use crate::database::Database;
use crate::detail::DetailView;
use crate::model::{FileFingerprint, PARSER_CACHE_KEY, ReplaySummary, SearchPlayer, human_text};
use crate::server_mode;

const SUMMARY_WRITE_BATCH_SIZE: usize = 64;
const MAX_DISCOVERED_DIRECTORIES: usize = 100_000;
const MAX_DISCOVERY_ENTRIES: usize = 1_000_000;
const MAX_DISCOVERED_REPLAYS: usize = 100_000;
const MAX_DISCOVERY_PATH_BYTES: usize = 128 * 1024 * 1024;

pub fn available_workers() -> usize {
    available_replay_workers()
}

fn bounded_worker_limit(requested: usize) -> usize {
    requested.clamp(1, available_workers())
}

#[derive(Default)]
struct DiscoveryBudget {
    directories: usize,
    entries: usize,
    replays: usize,
    path_bytes: usize,
}

impl DiscoveryBudget {
    fn account_entry(&mut self) -> Result<(), String> {
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| "Replay discovery entry count overflows".to_owned())?;
        if self.entries > MAX_DISCOVERY_ENTRIES {
            return Err(format!(
                "Replay discovery exceeds the maximum of {MAX_DISCOVERY_ENTRIES} filesystem entries"
            ));
        }
        Ok(())
    }

    fn account_path(&mut self, path: &Path) -> Result<(), String> {
        self.path_bytes = self
            .path_bytes
            .checked_add(path.as_os_str().len())
            .ok_or_else(|| "Replay discovery path storage overflows".to_owned())?;
        if self.path_bytes > MAX_DISCOVERY_PATH_BYTES {
            return Err(format!(
                "Replay discovery exceeds the {} MiB aggregate path limit",
                MAX_DISCOVERY_PATH_BYTES / 1024 / 1024
            ));
        }
        Ok(())
    }

    fn account_directory(&mut self, path: &Path) -> Result<(), String> {
        self.directories = self
            .directories
            .checked_add(1)
            .ok_or_else(|| "Replay discovery directory count overflows".to_owned())?;
        if self.directories > MAX_DISCOVERED_DIRECTORIES {
            return Err(format!(
                "Replay discovery exceeds the maximum of {MAX_DISCOVERED_DIRECTORIES} directories"
            ));
        }
        self.account_path(path)
    }

    fn account_replay(&mut self, path: &Path) -> Result<(), String> {
        self.replays = self
            .replays
            .checked_add(1)
            .ok_or_else(|| "Replay discovery file count overflows".to_owned())?;
        if self.replays > MAX_DISCOVERED_REPLAYS {
            return Err(format!(
                "Replay discovery exceeds the maximum of {MAX_DISCOVERED_REPLAYS} replay files"
            ));
        }
        self.account_path(path)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BackgroundEvent {
    ImportPrepared {
        discovered: usize,
        pending: usize,
        skipped: usize,
    },
    SummariesStored(Vec<ReplaySummary>),
    ImportProblem(String),
    ImportFinished {
        imported: usize,
        failed: usize,
        skipped: usize,
        removed_paths: Vec<PathBuf>,
        cancelled: bool,
    },
}

pub fn run_import<F>(
    root: &Path,
    worker_limit: usize,
    database_path: &Path,
    emit: F,
) -> Result<(), String>
where
    F: Fn(BackgroundEvent),
{
    run_import_roots(&[root.to_path_buf()], worker_limit, database_path, emit)
}

pub fn run_import_roots<F>(
    roots: &[PathBuf],
    worker_limit: usize,
    database_path: &Path,
    emit: F,
) -> Result<(), String>
where
    F: Fn(BackgroundEvent),
{
    run_import_roots_cancellable(
        roots,
        worker_limit,
        database_path,
        Arc::new(AtomicBool::new(false)),
        emit,
    )
}

pub fn run_import_roots_cancellable<F>(
    roots: &[PathBuf],
    worker_limit: usize,
    database_path: &Path,
    cancelled: Arc<AtomicBool>,
    emit: F,
) -> Result<(), String>
where
    F: Fn(BackgroundEvent),
{
    if roots.is_empty() {
        return Err("Add at least one replay folder first".to_owned());
    }

    let mut paths = BTreeSet::new();
    let mut scanned_roots = Vec::with_capacity(roots.len());
    for root in roots {
        if cancelled.load(Ordering::Acquire) {
            emit(cancelled_event(0, 0, 0));
            return Ok(());
        }
        let root = fs::canonicalize(root)
            .map_err(|error| format!("Cannot open replay folder {}: {error}", root.display()))?;
        if !root.is_dir() {
            return Err(format!(
                "Replay import path is not a folder: {}",
                root.display()
            ));
        }
        scanned_roots.push(root.clone());
        let Some(discovered_paths) = collect_replay_paths_until(&root, &cancelled)? else {
            emit(cancelled_event(0, 0, 0));
            return Ok(());
        };
        paths.extend(discovered_paths);
    }
    let discovered_paths = paths;
    let paths = discovered_paths.iter().cloned().collect::<Vec<_>>();
    let discovered = paths.len();
    let mut database = Database::open(database_path)?;
    let current_fingerprints = database.current_summary_fingerprints()?;
    let mut pending = Vec::new();
    let mut skipped = 0;
    let mut initial_failures = 0;

    for path in paths {
        if cancelled.load(Ordering::Acquire) {
            emit(cancelled_event(0, initial_failures, skipped));
            return Ok(());
        }
        match FileFingerprint::read(&path) {
            Ok(fingerprint) => {
                if current_fingerprints.get(&path) == Some(&fingerprint) {
                    skipped += 1;
                } else {
                    pending.push((path, fingerprint));
                }
            }
            Err(error) => {
                initial_failures += 1;
                emit(BackgroundEvent::ImportProblem(error));
            }
        }
    }

    emit(BackgroundEvent::ImportPrepared {
        discovered,
        pending: pending.len(),
        skipped,
    });

    if pending.is_empty() {
        let removed_paths = database.prune_missing_summaries(&scanned_roots, &discovered_paths)?;
        emit(BackgroundEvent::ImportFinished {
            imported: 0,
            failed: initial_failures,
            skipped,
            removed_paths,
            cancelled: false,
        });
        return Ok(());
    }

    let worker_limit = bounded_worker_limit(worker_limit);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(worker_limit)
        .thread_name(|index| format!("wic-replay-parser-{index}"))
        .build()
        .map_err(|error| format!("Cannot create parser worker pool: {error}"))?;
    let channel_capacity = worker_limit.saturating_mul(2);
    let (results_sender, results_receiver) = sync_channel(channel_capacity);
    let worker_cancelled = Arc::clone(&cancelled);
    let producer = thread::Builder::new()
        .name("wic-replay-import-workers".to_owned())
        .spawn(move || {
            pool.install(|| {
                pending.into_par_iter().for_each_with(
                    results_sender,
                    |sender, (path, fingerprint)| {
                        if worker_cancelled.load(Ordering::Acquire) {
                            return;
                        }
                        let summary = parse_summary(path, fingerprint);
                        if !worker_cancelled.load(Ordering::Acquire) {
                            let _ = sender.send(summary);
                        }
                    },
                );
            });
        })
        .map_err(|error| format!("Cannot start parser workers: {error}"))?;

    let mut database = Database::open(database_path)?;
    let mut imported = 0;
    let mut failed = initial_failures;
    let mut summary_batch = Vec::with_capacity(SUMMARY_WRITE_BATCH_SIZE);
    for summary in results_receiver {
        if cancelled.load(Ordering::Acquire) {
            continue;
        }
        summary_batch.push(summary);
        if summary_batch.len() == SUMMARY_WRITE_BATCH_SIZE {
            store_summary_batch(
                &mut database,
                &mut summary_batch,
                &emit,
                &mut imported,
                &mut failed,
            );
        }
    }

    if !cancelled.load(Ordering::Acquire) {
        store_summary_batch(
            &mut database,
            &mut summary_batch,
            &emit,
            &mut imported,
            &mut failed,
        );
    }

    producer
        .join()
        .map_err(|_| "A parser worker panicked".to_owned())?;
    let was_cancelled = cancelled.load(Ordering::Acquire);
    let removed_paths = if was_cancelled {
        Vec::new()
    } else {
        database.prune_missing_summaries(&scanned_roots, &discovered_paths)?
    };
    emit(BackgroundEvent::ImportFinished {
        imported,
        failed,
        skipped,
        removed_paths,
        cancelled: was_cancelled,
    });
    Ok(())
}

fn store_summary_batch<F>(
    database: &mut Database,
    summaries: &mut Vec<ReplaySummary>,
    emit: &F,
    imported: &mut usize,
    failed: &mut usize,
) where
    F: Fn(BackgroundEvent),
{
    if summaries.is_empty() {
        return;
    }

    // Only committed rows cross IPC, in one bounded event per database batch.
    let mut stored = Vec::with_capacity(summaries.len());
    if database.upsert_summaries(summaries).is_err() {
        for summary in summaries.drain(..) {
            if let Err(error) = database.upsert_summary(&summary) {
                *failed += 1;
                emit(BackgroundEvent::ImportProblem(error));
                continue;
            }
            stored.push(summary);
        }
    } else {
        stored.append(summaries);
    }
    for summary in &stored {
        if summary.parse_error.is_some() {
            *failed += 1;
        } else {
            *imported += 1;
        }
    }
    crate::diagnostics::activity(
        crate::diagnostics::Operation::Import,
        crate::diagnostics::Phase::Committed,
        stored.iter().filter(|s| s.parse_error.is_none()).count() as u64,
    );
    crate::diagnostics::activity(
        crate::diagnostics::Operation::Import,
        crate::diagnostics::Phase::ExpectedProblem,
        stored.iter().filter(|s| s.parse_error.is_some()).count() as u64,
    );
    if !stored.is_empty() {
        emit(BackgroundEvent::SummariesStored(stored));
    }
}

pub fn load_detail(path: &Path, database_path: &Path) -> Result<DetailView, String> {
    validate_replay_path(path)?;
    let fingerprint = FileFingerprint::read(path)?;
    let mut database = Database::open(database_path)?;
    if let Some(json) = database.load_detail(path, fingerprint)? {
        match DetailView::from_json(&json) {
            Ok(detail) => return Ok(detail),
            Err(_) => database.delete_detail(path)?,
        }
    }

    let parser = WicReplayParser::new(path)?;
    let mut document = parser.parse_with_timeline();
    let timeline_schema = document.timeline.schema_version;
    let (hidden, evidence) = crate::roster::prepare_results(&parser, &mut document.replay);
    let (detail, json) = DetailView::cache_parser_document(document, hidden, evidence)?;
    database.save_detail(path, fingerprint, timeline_schema, &json)?;
    Ok(detail)
}

fn parse_summary(path: PathBuf, fingerprint: FileFingerprint) -> ReplaySummary {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(|| path.display().to_string(), str::to_owned);
    let imported_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;

    match WicReplayParser::new(&path) {
        Ok(parser) => {
            let mut replay = parser.parse();
            let (hidden, evidence) = crate::roster::prepare_results(&parser, &mut replay);
            replay.players.retain(|player| !hidden.contains(&player.id));
            for player in &mut replay.players {
                if let Some(evidence) = evidence.iter().find(|e| e.player_id == player.id)
                    && let Some(score) = &evidence.score_before_leave
                {
                    player.score = Some(score.score);
                }
            }
            let overflow = crate::roster::overflow_departure_ids(
                replay
                    .players
                    .iter()
                    .map(|p| (p.id, p.team, p.left_at_seconds.map(f64::from))),
            );
            replay.players.retain(|p| !overflow.contains(&p.id));
            let player_names = replay
                .players
                .iter()
                .map(|player| player.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let factions = searchable_factions(
                replay
                    .players
                    .iter()
                    .filter_map(|player| player.faction.as_deref()),
            );
            let recorder_faction = replay.recorder.as_ref().and_then(|recorder| {
                let mut factions = replay
                    .players
                    .iter()
                    .filter(|player| &player.name == recorder)
                    .filter_map(|player| player.faction.as_deref());
                let faction = factions.next()?;
                factions
                    .all(|candidate| candidate == faction)
                    .then(|| faction.to_owned())
            });
            let classification = &replay.server_classification;
            let server_mode_labels = server_mode::labels(
                classification.few_player_mode,
                classification.match_mode,
                classification.has_bots,
                classification.clan_match,
                classification.tournament_match,
                classification.ranked,
            );
            let format = server_mode::matchup(replay.players.iter().map(|player| player.team))
                .unwrap_or_default();
            let server_modes = server_mode_labels.join(" · ");
            ReplaySummary {
                path,
                file_name,
                replay_name: replay.game_info.replay_name,
                server_name: human_text(&replay.game_info.server_name),
                fingerprint,
                cache_key: PARSER_CACHE_KEY.to_owned(),
                map_name: replay.game_info.map_name,
                map_display_name: replay.game_info.map_display_name,
                game_mode: replay.game_info.game_mode,
                server_modes,
                format,
                date_time: replay.game_info.date_time,
                duration_seconds: replay.duration_seconds,
                recording_seconds: Some(replay.timing.recording_seconds),
                winner: replay.winner,
                player_count: replay.players.len() as u32,
                player_names,
                search_players: replay
                    .players
                    .iter()
                    .map(|player| SearchPlayer {
                        name: player.name.clone(),
                        faction: player.faction.clone(),
                    })
                    .collect(),
                factions,
                recorder: replay.recorder,
                recorder_faction,
                incomplete: replay.incomplete,
                parse_error: None,
                imported_at,
            }
        }
        Err(error) => ReplaySummary {
            path,
            file_name,
            replay_name: None,
            server_name: String::new(),
            fingerprint,
            cache_key: PARSER_CACHE_KEY.to_owned(),
            map_name: String::new(),
            map_display_name: String::new(),
            game_mode: String::new(),
            server_modes: String::new(),
            format: String::new(),
            date_time: String::new(),
            duration_seconds: None,
            recording_seconds: None,
            winner: None,
            player_count: 0,
            player_names: String::new(),
            search_players: Vec::new(),
            factions: String::new(),
            recorder: None,
            recorder_faction: None,
            incomplete: true,
            parse_error: Some(error),
            imported_at,
        },
    }
}

fn searchable_factions<'a>(factions: impl IntoIterator<Item = &'a str>) -> String {
    factions
        .into_iter()
        .filter(|faction| !faction.is_empty() && *faction != "Spectator" && *faction != "Unknown")
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse one managed replay after a filesystem operation and require it to be
/// valid before the library cache is changed.
pub fn refresh_summary(path: &Path) -> Result<ReplaySummary, String> {
    let fingerprint = FileFingerprint::read(path)?;
    let summary = parse_summary(path.to_path_buf(), fingerprint);
    match summary.parse_error.as_deref() {
        Some(error) => Err(error.to_owned()),
        None => Ok(summary),
    }
}

#[cfg(test)]
fn collect_replay_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    collect_replay_paths_until(root, &AtomicBool::new(false))?
        .ok_or_else(|| "Replay discovery was cancelled unexpectedly".to_owned())
}

fn collect_replay_paths_until(
    root: &Path,
    cancelled: &AtomicBool,
) -> Result<Option<Vec<PathBuf>>, String> {
    let mut replay_paths = Vec::new();
    let mut budget = DiscoveryBudget::default();
    budget.account_directory(root)?;
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        if cancelled.load(Ordering::Acquire) {
            return Ok(None);
        }
        let entries = fs::read_dir(&directory)
            .map_err(|error| format!("Cannot read {}: {error}", directory.display()))?;
        for entry in entries {
            budget.account_entry()?;
            let entry = entry.map_err(|error| {
                format!("Cannot read entry in {}: {error}", directory.display())
            })?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Cannot inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                budget.account_directory(&path)?;
                directories.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("wicdemo"))
            {
                budget.account_replay(&path)?;
                replay_paths.push(path);
            }
        }
    }
    replay_paths.sort();
    Ok(Some(replay_paths))
}

fn cancelled_event(imported: usize, failed: usize, skipped: usize) -> BackgroundEvent {
    BackgroundEvent::ImportFinished {
        imported,
        failed,
        skipped,
        removed_paths: Vec::new(),
        cancelled: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(coverage))]
    fn private_path(name: &str) -> PathBuf {
        PathBuf::from(
            std::env::var_os(name).unwrap_or_else(|| panic!("private test requires {name}")),
        )
    }
    #[cfg(not(coverage))]
    use serde_json::Value;
    use std::sync::Mutex;
    use tempfile::tempdir;

    #[test]
    fn background_event_fields_use_the_frontend_camel_case_contract() {
        let event = BackgroundEvent::ImportFinished {
            imported: 1,
            failed: 2,
            skipped: 3,
            removed_paths: vec![PathBuf::from("missing.wicdemo")],
            cancelled: false,
        };

        let json = serde_json::to_value(event).expect("background event JSON");
        assert_eq!(json["type"], "importFinished");
        assert_eq!(json["payload"]["removedPaths"][0], "missing.wicdemo");
        assert!(json["payload"].get("removed_paths").is_none());
    }

    #[test]
    fn bulk_import_emits_bounded_committed_batches_and_flushes_the_tail() {
        let directory = tempdir().expect("temp dir");
        let root = directory.path().join("replays");
        fs::create_dir(&root).expect("replay root");
        for index in 0..130 {
            fs::write(root.join(format!("{index}.wicdemo")), b"invalid replay")
                .expect("synthetic replay");
        }
        let database_path = directory.path().join("library.sqlite3");
        let batches = Mutex::new(Vec::new());
        run_import(&root, 2, &database_path, |event| {
            if let BackgroundEvent::SummariesStored(ref rows) = event {
                let json = serde_json::to_value(&event).expect("event JSON");
                assert_eq!(json["type"], "summariesStored");
                assert_eq!(json["payload"].as_array().unwrap().len(), rows.len());
                // The notification must follow the transaction, including failed parses.
                let database = Database::open(&database_path).expect("database");
                let stored = database.load_summaries().expect("stored rows");
                assert!(
                    rows.iter()
                        .all(|row| stored.iter().any(|saved| saved.path == row.path))
                );
                batches.lock().unwrap().push(rows.len());
            }
        })
        .expect("synthetic import");
        assert_eq!(*batches.lock().unwrap(), vec![64, 64, 2]);
        assert_eq!(
            Database::open(&database_path)
                .unwrap()
                .load_summaries()
                .unwrap()
                .len(),
            130
        );
    }

    #[test]
    fn import_worker_count_is_always_bounded() {
        let available = available_workers();
        assert!(available >= 1);
        assert_eq!(bounded_worker_limit(0), 1);
        assert_eq!(bounded_worker_limit(1), 1);
        assert_eq!(bounded_worker_limit(available), available);
        assert_eq!(bounded_worker_limit(usize::MAX), available);
    }

    #[test]
    fn searchable_factions_are_unique_playable_sides() {
        assert_eq!(
            searchable_factions(["USSR", "Spectator", "USA", "USSR", "Unknown", ""]),
            "USA, USSR"
        );
    }

    #[test]
    fn detail_loading_rejects_a_non_replay_path_before_file_or_cache_access() {
        let error = load_detail(Path::new("not-a-replay.txt"), Path::new("unused.sqlite3"))
            .expect_err("wrong extension");
        assert!(error.contains("must end in .wicdemo"));
    }

    #[test]
    fn replay_discovery_rejects_each_budget_overrun() {
        let path = Path::new("x");

        let mut budget = DiscoveryBudget {
            entries: MAX_DISCOVERY_ENTRIES,
            ..DiscoveryBudget::default()
        };
        assert!(budget.account_entry().is_err());

        let mut budget = DiscoveryBudget {
            directories: MAX_DISCOVERED_DIRECTORIES,
            ..DiscoveryBudget::default()
        };
        assert!(budget.account_directory(path).is_err());

        let mut budget = DiscoveryBudget {
            replays: MAX_DISCOVERED_REPLAYS,
            ..DiscoveryBudget::default()
        };
        assert!(budget.account_replay(path).is_err());

        let mut budget = DiscoveryBudget {
            path_bytes: MAX_DISCOVERY_PATH_BYTES,
            ..DiscoveryBudget::default()
        };
        assert!(budget.account_path(path).is_err());
    }

    #[cfg(not(coverage))]
    fn first_json_difference(left: &Value, right: &Value, path: &str) -> Option<String> {
        match (left, right) {
            (Value::Object(left), Value::Object(right)) => {
                for key in left.keys().chain(right.keys()) {
                    let child_path = format!("{path}.{key}");
                    match (left.get(key), right.get(key)) {
                        (Some(left), Some(right)) => {
                            if let Some(difference) =
                                first_json_difference(left, right, &child_path)
                            {
                                return Some(difference);
                            }
                        }
                        (left, right) => {
                            return Some(format!("{child_path}: {left:?} != {right:?}"));
                        }
                    }
                }
                None
            }
            (Value::Array(left), Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!("{path}.length: {} != {}", left.len(), right.len()));
                }
                left.iter()
                    .zip(right)
                    .enumerate()
                    .find_map(|(index, (left, right))| {
                        first_json_difference(left, right, &format!("{path}[{index}]"))
                    })
            }
            _ if left == right => None,
            _ => Some(format!("{path}: {left:?} != {right:?}")),
        }
    }

    #[test]
    fn recursively_discovers_only_replays_in_deterministic_order() {
        let directory = tempdir().expect("temp dir");
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).expect("nested");
        fs::write(directory.path().join("b.WICDEMO"), b"b").expect("b replay");
        fs::write(nested.join("a.wicdemo"), b"a").expect("a replay");
        fs::write(nested.join("ignore.txt"), b"x").expect("text");

        let paths = collect_replay_paths(directory.path()).expect("paths");
        assert_eq!(paths.len(), 2);
        assert!(paths[0] < paths[1]);
    }

    #[test]
    fn multiple_overlapping_roots_do_not_duplicate_replays() {
        let directory = tempdir().expect("temp dir");
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).expect("nested");
        fs::write(nested.join("sample.wicdemo"), b"not a replay").expect("replay");
        let database_path = directory.path().join("library.sqlite3");
        let events = Mutex::new(Vec::new());

        run_import_roots(
            &[directory.path().to_path_buf(), nested],
            1,
            &database_path,
            |event| events.lock().expect("event lock").push(event),
        )
        .expect("multi-root import");

        let discovered = events
            .into_inner()
            .expect("events")
            .into_iter()
            .find_map(|event| match event {
                BackgroundEvent::ImportPrepared { discovered, .. } => Some(discovered),
                _ => None,
            });
        assert_eq!(discovered, Some(1));
    }

    #[test]
    fn completed_rescan_prunes_only_missing_rows_under_the_scanned_roots() {
        let directory = tempdir().expect("temp dir");
        let root_a = directory.path().join("a");
        let root_b = directory.path().join("b");
        fs::create_dir(&root_a).expect("root a");
        fs::create_dir(&root_b).expect("root b");
        let retained = root_a.join("retained.wicdemo");
        let removed = root_a.join("removed.wicdemo");
        let outside_scan = root_b.join("outside.wicdemo");
        fs::write(&retained, b"not a replay").expect("retained replay");
        fs::write(&removed, b"not a replay").expect("removed replay");
        fs::write(&outside_scan, b"not a replay").expect("outside replay");
        let database_path = directory.path().join("library.sqlite3");

        run_import_roots(&[root_a.clone(), root_b], 1, &database_path, |_| {})
            .expect("initial import");
        fs::remove_file(&removed).expect("remove replay after caching");

        let events = Mutex::new(Vec::new());
        run_import_roots(&[root_a], 1, &database_path, |event| {
            events.lock().expect("event lock").push(event);
        })
        .expect("completed rescan");

        let rows = Database::open(&database_path)
            .expect("database")
            .load_summaries()
            .expect("summaries");
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.path == retained));
        assert!(rows.iter().any(|row| row.path == outside_scan));
        assert!(!rows.iter().any(|row| row.path == removed));

        let removed_paths = events
            .into_inner()
            .expect("events")
            .into_iter()
            .find_map(|event| match event {
                BackgroundEvent::ImportFinished { removed_paths, .. } => Some(removed_paths),
                _ => None,
            })
            .expect("finished event");
        assert_eq!(removed_paths, vec![removed]);
    }

    #[test]
    fn cancelled_or_failed_scan_preserves_missing_cached_rows() {
        let directory = tempdir().expect("temp dir");
        let replay_root = directory.path().join("replays");
        fs::create_dir(&replay_root).expect("replay root");
        let replay = replay_root.join("sample.wicdemo");
        fs::write(&replay, b"not a replay").expect("replay");
        let database_path = directory.path().join("library.sqlite3");
        run_import(&replay_root, 1, &database_path, |_| {}).expect("initial import");
        fs::remove_file(&replay).expect("remove replay after caching");

        run_import_roots_cancellable(
            std::slice::from_ref(&replay_root),
            1,
            &database_path,
            Arc::new(AtomicBool::new(true)),
            |_| {},
        )
        .expect("cancelled rescan");
        assert_eq!(
            Database::open(&database_path)
                .expect("database after cancellation")
                .load_summaries()
                .expect("summaries after cancellation")
                .len(),
            1
        );

        fs::remove_dir(&replay_root).expect("remove scanned root");
        assert!(run_import(&replay_root, 1, &database_path, |_| {}).is_err());
        assert_eq!(
            Database::open(&database_path)
                .expect("database after failure")
                .load_summaries()
                .expect("summaries after failure")
                .len(),
            1
        );
    }

    #[test]
    fn cancelled_import_finishes_without_storing_replays() {
        let directory = tempdir().expect("temp dir");
        fs::write(directory.path().join("sample.wicdemo"), b"not a replay").expect("replay");
        let database_path = directory.path().join("library.sqlite3");
        let events = Mutex::new(Vec::new());
        let cancelled = Arc::new(AtomicBool::new(true));

        run_import_roots_cancellable(
            &[directory.path().to_path_buf()],
            1,
            &database_path,
            cancelled,
            |event| events.lock().expect("event lock").push(event),
        )
        .expect("cancelled import");

        assert!(matches!(
            events.lock().expect("events").last(),
            Some(BackgroundEvent::ImportFinished {
                imported: 0,
                cancelled: true,
                ..
            })
        ));
        assert!(!database_path.exists());
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private replay fixture"]
    fn configured_private_replay_round_trips_through_summary_and_detail_cache() {
        let replay_path = private_path("WIC_REPLAY_VIEWER_SMOKE");
        let fingerprint = FileFingerprint::read(&replay_path).expect("replay fingerprint");
        let summary = parse_summary(replay_path.clone(), fingerprint);
        assert!(summary.parse_error.is_none(), "{:?}", summary.parse_error);

        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let mut database = Database::open(&database_path).expect("database");
        database.upsert_summary(&summary).expect("summary cache");

        let detail = load_detail(&replay_path, &database_path).expect("lazy detail");
        assert_eq!(
            summary.search_players,
            detail
                .overview
                .players
                .iter()
                .map(|player| SearchPlayer {
                    name: player.name.clone(),
                    faction: player.faction.clone(),
                })
                .collect::<Vec<_>>()
        );
        if let Some(expected) = WicReplayParser::new(&replay_path)
            .unwrap()
            .final_screen_players()
        {
            assert_eq!(detail.overview.players.len(), expected.len());
            for player in expected {
                let actual = detail
                    .overview
                    .players
                    .iter()
                    .find(|p| p.id == player.id)
                    .unwrap();
                assert_eq!(actual.name, player.name);
                assert_eq!(actual.score, player.score);
                assert_eq!(actual.team, player.team);
                assert!(actual.score_before_leave.is_none());
            }
            assert_eq!(
                summary.player_names,
                detail
                    .overview
                    .players
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        let cached = load_detail(&replay_path, &database_path).expect("cached final screen");
        assert_eq!(
            serde_json::to_value(&detail).unwrap(),
            serde_json::to_value(cached).unwrap()
        );
        assert_eq!(detail.timeline_schema, 18);
        assert!(!detail.overview.map_display_name.is_empty());
        assert_ne!(detail.overview.server_name, "Unknown");
        assert!(
            database
                .load_detail(&replay_path, fingerprint)
                .expect("cached detail")
                .is_some()
        );
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires the private replay corpus"]
    fn configured_private_corpus_typed_details_match_json_projection() {
        let paths =
            collect_replay_paths(&private_path("WIC_REPLAY_VIEWER_CORPUS")).expect("corpus paths");
        assert!(!paths.is_empty(), "configured corpus must contain replays");
        let sample_count = paths.len().min(16);

        for sample_index in 0..sample_count {
            let path_index = sample_index * paths.len() / sample_count;
            let path = &paths[path_index];
            let parser = WicReplayParser::new(path).expect("sample replay");
            let document = parser.parse_with_timeline();
            let json = serde_json::to_string(&document).expect("parser JSON");
            let legacy = DetailView::from_json(&json).expect("JSON projection");
            let typed = DetailView::from_parser_document(document);

            let typed = serde_json::to_value(typed).expect("typed view JSON");
            let legacy = serde_json::to_value(legacy).expect("legacy view JSON");
            assert_eq!(
                first_json_difference(&typed, &legacy, "$"),
                None,
                "projection differs for {}",
                path.display()
            );
        }
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "optional bounded private-corpus sample; not part of the mandatory private gate"]
    fn optional_private_corpus_sample_exercises_the_bounded_import_pipeline() {
        let corpus_root = private_path("WIC_REPLAY_VIEWER_CORPUS");
        let directory = tempdir().expect("temp dir");
        let sample_root = directory.path().join("sample");
        fs::create_dir(&sample_root).expect("sample directory");
        let paths = collect_replay_paths(&corpus_root).expect("corpus paths");
        let sample_count = paths.len().min(32);
        assert!(sample_count > 0, "configured corpus must contain replays");
        for sample_index in 0..sample_count {
            let path_index = sample_index * paths.len() / sample_count;
            fs::copy(
                &paths[path_index],
                sample_root.join(format!("{sample_index:02}.wicdemo")),
            )
            .expect("copy sampled replay");
        }
        let database_path = directory.path().join("library.sqlite3");
        let received_events = Mutex::new(Vec::new());
        let workers = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1);

        run_import(&sample_root, workers, &database_path, |event| {
            received_events.lock().expect("event lock").push(event)
        })
        .expect("corpus import");

        let all_events = received_events.into_inner().expect("events");
        let discovered = all_events
            .iter()
            .find_map(|event| match event {
                BackgroundEvent::ImportPrepared { discovered, .. } => Some(*discovered),
                _ => None,
            })
            .expect("prepared event");
        assert!(
            all_events
                .iter()
                .any(|event| matches!(event, BackgroundEvent::ImportFinished { .. }))
        );
        let rows = Database::open(&database_path)
            .expect("database")
            .load_summaries()
            .expect("summaries");
        assert_eq!(rows.len(), sample_count);
        assert_eq!(rows.len(), discovered, "every discovered replay is stored");
        assert!(rows.iter().any(|row| row.parse_error.is_none()));
        eprintln!(
            "stored {discovered} replay summaries ({} parsed, {} parser rejects)",
            rows.iter().filter(|row| row.parse_error.is_none()).count(),
            rows.iter().filter(|row| row.parse_error.is_some()).count()
        );
    }
}
