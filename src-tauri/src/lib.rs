use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

mod command_core;
use command_core::{
    DetailLoadCoordinator, FileOperationGuard, begin_file_operation, canonical_directories,
    paths_as_strings,
};

use directories::ProjectDirs;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use wic_replay_viewer::database::{
    CUSTOM_MAPS_SETTING, Database, GAME_INSTALL_SETTING, REPLAY_DIR_SEEDED_SETTING,
};
use wic_replay_viewer::detail::DetailView;
use wic_replay_viewer::importer::{
    BackgroundEvent, available_workers, load_detail, refresh_summary, run_import_roots_cancellable,
};
use wic_replay_viewer::map_art::{
    ArtSources, detect_custom_maps, detect_install, detect_replay_directory, ensure_map_art,
    internal_map_name, png_data_url, resolve_custom_maps, resolve_install,
};
use wic_replay_viewer::model::ReplaySummary;
use wic_replay_viewer::playback::{self, PlaybackObjective, PlaybackView};
use wic_replay_viewer::replay_management::{
    change_replay_name, export_replay, export_replays, rename_replay,
};

const IMPORT_EVENT: &str = "replay-import";
const MAX_MAP_ART_BATCH_SIZE: usize = 128;

struct AppState {
    database_path: PathBuf,
    importing: Arc<AtomicBool>,
    import_cancelled: Arc<AtomicBool>,
    file_operation_busy: Arc<AtomicBool>,
    detail_loads: Arc<DetailLoadCoordinator<DetailView>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitialState {
    summaries: Vec<ReplaySummary>,
    locations: Vec<String>,
    importing: bool,
    cached_map_art_count: usize,
}

/// What the UI needs to describe map art: where the game is, where one was
/// detected, how many tiles are cached, and why a chosen folder was rejected.
/// No installation is a normal state, so `problem` stays empty unless the user
/// picked something that is not a game folder.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct MapArtState {
    install_path: Option<String>,
    custom_maps_path: Option<String>,
    map_count: usize,
    problem: Option<String>,
    cache_changed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemovedLocation {
    locations: Vec<String>,
    removed_replay_paths: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayOperationResult {
    path: String,
    summary: Option<ReplaySummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayBatchExportResult {
    folder: String,
    exports: Vec<ReplayOperationResult>,
}

#[tauri::command]
async fn initial_state(state: State<'_, AppState>) -> Result<InitialState, String> {
    use wic_replay_viewer::diagnostics::Code;
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::Startup, Phase::Started, 1);
    let result = initial_state_impl(state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::incident(Code::OperationFailed, Operation::Startup);
    } else {
        diagnostics::activity(Operation::Startup, Phase::Finished, 1);
    }
    result
}

async fn initial_state_impl(state: State<'_, AppState>) -> Result<InitialState, String> {
    let database_path = state.database_path.clone();
    let importing = Arc::clone(&state.importing);
    tauri::async_runtime::spawn_blocking(move || {
        let mut database = Database::open(&database_path)?;
        database.reconcile_library_locations()?;
        let summaries = database.load_summaries()?;
        let locations = paths_as_strings(database.library_locations()?);
        let cached_map_art_count = database.map_art_count()?;
        Ok(InitialState {
            summaries,
            locations,
            importing: importing.load(Ordering::Acquire),
            cached_map_art_count,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Cheap heartbeat: polling must not reload SQLite while parsers are active.
#[tauri::command]
fn import_running(state: State<'_, AppState>) -> bool {
    state.importing.load(Ordering::Acquire)
}

#[tauri::command]
fn add_library_locations(
    roots: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = add_library_locations_impl(roots, state);
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

fn add_library_locations_impl(
    roots: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let roots = canonical_directories(roots)?;
    if roots.is_empty() {
        return Err("Choose at least one existing replay folder".to_owned());
    }
    let mut database = Database::open(&state.database_path)?;
    database.add_library_locations(&roots)?;
    database.library_locations().map(paths_as_strings)
}

#[tauri::command]
fn start_import(
    roots: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::Import, Phase::Started, 1);
    let result = start_import_impl(roots, app, state);
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::Import, Phase::ExpectedProblem, 1);
    }

    result
}

fn start_import_impl(
    roots: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let roots = canonical_directories(roots)?;
    if roots.is_empty() {
        return Err("Add at least one existing replay folder first".to_owned());
    }

    let file_operation = begin_file_operation(
        &state.file_operation_busy,
        "Wait for the current replay-management operation before scanning",
    )?;

    state
        .importing
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| "A replay import is already running".to_owned())?;
    state.import_cancelled.store(false, Ordering::Release);

    let setup = (|| {
        let mut database = Database::open(&state.database_path)?;
        database.add_library_locations(&roots)?;
        database.library_locations().map(paths_as_strings)
    })();
    let locations = match setup {
        Ok(locations) => locations,
        Err(error) => {
            state.importing.store(false, Ordering::Release);
            return Err(error);
        }
    };

    let database_path = state.database_path.clone();
    let importing = Arc::clone(&state.importing);
    let import_cancelled = Arc::clone(&state.import_cancelled);
    let worker_limit = available_workers();
    thread::Builder::new()
        .name("wic-replay-import-coordinator".to_owned())
        .spawn(move || {
            let _file_operation = file_operation;
            let result = run_import_roots_cancellable(
                &roots,
                worker_limit,
                &database_path,
                Arc::clone(&import_cancelled),
                |event| {
                    if let BackgroundEvent::ImportFinished {
                        imported,
                        cancelled,
                        ..
                    } = &event
                    {
                        use wic_replay_viewer::diagnostics::{self, Operation, Phase};
                        diagnostics::activity(
                            Operation::Import,
                            if *cancelled {
                                Phase::Cancelled
                            } else {
                                Phase::Finished
                            },
                            *imported as u64,
                        );
                    }
                    if let Err(error) = app.emit(IMPORT_EVENT, event) {
                        wic_replay_viewer::diagnostics::incident(
                            wic_replay_viewer::diagnostics::Code::CallbackFailed,
                            wic_replay_viewer::diagnostics::Operation::Import,
                        );
                        eprintln!("Replay import event delivery failed: {error}");
                    }
                },
            );
            if let Err(error) = result {
                let _ = app.emit(IMPORT_EVENT, BackgroundEvent::ImportProblem(error));
                let _ = app.emit(
                    IMPORT_EVENT,
                    BackgroundEvent::ImportFinished {
                        imported: 0,
                        failed: 1,
                        skipped: 0,
                        removed_paths: Vec::new(),
                        cancelled: false,
                    },
                );
            }
            importing.store(false, Ordering::Release);
            import_cancelled.store(false, Ordering::Release);
        })
        .map_err(|error| {
            state.importing.store(false, Ordering::Release);
            format!("Cannot start replay import: {error}")
        })?;

    Ok(locations)
}

#[tauri::command]
fn cancel_import(state: State<'_, AppState>) -> bool {
    if !state.importing.load(Ordering::Acquire) {
        return false;
    }
    state.import_cancelled.store(true, Ordering::Release);
    true
}

#[tauri::command]
fn remove_library_location(
    path: String,
    state: State<'_, AppState>,
) -> Result<RemovedLocation, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = remove_library_location_impl(path, state);
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

fn remove_library_location_impl(
    path: String,
    state: State<'_, AppState>,
) -> Result<RemovedLocation, String> {
    if state.importing.load(Ordering::Acquire) {
        return Err("Wait for the current import before removing a location".to_owned());
    }
    let removed_root = PathBuf::from(path);
    let database = Database::open(&state.database_path)?;
    database.remove_library_location(&removed_root)?;
    let remaining = database.library_locations()?;
    let mut removed_replay_paths = Vec::new();
    for summary in database.load_summaries()? {
        if summary.path.starts_with(&removed_root)
            && !remaining.iter().any(|root| summary.path.starts_with(root))
        {
            database.delete_summary(&summary.path)?;
            removed_replay_paths.push(summary.path.to_string_lossy().into_owned());
        }
    }
    Ok(RemovedLocation {
        locations: paths_as_strings(remaining),
        removed_replay_paths,
    })
}

#[tauri::command]
async fn replay_detail(path: String, state: State<'_, AppState>) -> Result<DetailView, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::Detail, Phase::Started, 1);
    let result = replay_detail_impl(path, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::Detail, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::Detail, Phase::Finished, 1);
    }
    result
}

async fn replay_detail_impl(
    path: String,
    state: State<'_, AppState>,
) -> Result<DetailView, String> {
    let database_path = state.database_path.clone();
    let path = PathBuf::from(path);
    let detail_loads = Arc::clone(&state.detail_loads);
    tauri::async_runtime::spawn_blocking(move || {
        detail_loads.load(path.clone(), || load_detail(&path, &database_path))
    })
    .await
    .map_err(|error| format!("Replay detail task failed: {error}"))?
}

#[tauri::command]
async fn replay_playback(path: String) -> Result<PlaybackView, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::Playback, Phase::Started, 1);
    let result = replay_playback_impl(path).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::Playback, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::Playback, Phase::Finished, 1);
    }
    result
}

async fn replay_playback_impl(path: String) -> Result<PlaybackView, String> {
    tauri::async_runtime::spawn_blocking(move || playback::load(Path::new(&path)))
        .await
        .map_err(|error| format!("Replay playback task failed: {error}"))?
}

#[tauri::command]
async fn replay_objectives(path: String) -> Result<Vec<PlaybackObjective>, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::Playback, Phase::Started, 1);
    let result = replay_objectives_impl(path).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::Playback, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::Playback, Phase::Finished, 1);
    }
    result
}

async fn replay_objectives_impl(path: String) -> Result<Vec<PlaybackObjective>, String> {
    tauri::async_runtime::spawn_blocking(move || playback::load_objectives(Path::new(&path)))
        .await
        .map_err(|error| format!("Replay objective task failed: {error}"))?
}

fn begin_management(state: &AppState) -> Result<FileOperationGuard, String> {
    if state.importing.load(Ordering::Acquire) {
        return Err("Wait for the current library scan before managing replays".to_owned());
    }
    begin_file_operation(
        &state.file_operation_busy,
        "Another replay-management operation is already running",
    )
}

#[tauri::command]
async fn rename_replay_file(
    path: String,
    file_name: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = rename_replay_file_impl(path, file_name, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

async fn rename_replay_file_impl(
    path: String,
    file_name: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    let file_operation = begin_management(&state)?;
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _file_operation = file_operation;
        let source = PathBuf::from(path);
        let destination = rename_replay(&source, &file_name)?;
        let summary = match refresh_summary(&destination) {
            Ok(summary) => summary,
            Err(error) => {
                let _ = std::fs::rename(&destination, &source);
                return Err(format!(
                    "Renamed replay could not be indexed and was restored: {error}"
                ));
            }
        };
        let mut database = Database::open(&database_path)?;
        if let Err(error) = database.upsert_summary(&summary) {
            let _ = std::fs::rename(&destination, &source);
            return Err(format!(
                "Cannot update the replay library; the file was restored: {error}"
            ));
        }
        if let Err(error) = database.delete_summary(&source) {
            let _ = database.delete_summary(&destination);
            let _ = std::fs::rename(&destination, &source);
            return Err(format!(
                "Cannot replace the old replay-library entry; the file was restored: {error}"
            ));
        }
        Ok(ReplayOperationResult {
            path: destination.to_string_lossy().into_owned(),
            summary: Some(summary),
        })
    })
    .await
    .map_err(|error| format!("Replay rename task failed: {error}"))?
}

#[tauri::command]
async fn change_replay_in_game_name(
    path: String,
    replay_name: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = change_replay_in_game_name_impl(path, replay_name, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

async fn change_replay_in_game_name_impl(
    path: String,
    replay_name: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    let file_operation = begin_management(&state)?;
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _file_operation = file_operation;
        let path = PathBuf::from(path);
        change_replay_name(&path, &replay_name)?;
        let summary = refresh_summary(&path)?;
        let mut database = Database::open(&database_path)?;
        database.upsert_summary(&summary)?;
        Ok(ReplayOperationResult {
            path: path.to_string_lossy().into_owned(),
            summary: Some(summary),
        })
    })
    .await
    .map_err(|error| format!("Replay name-change task failed: {error}"))?
}

#[tauri::command]
async fn export_replay_copy(
    path: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = export_replay_copy_impl(path, destination, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

async fn export_replay_copy_impl(
    path: String,
    destination: String,
    state: State<'_, AppState>,
) -> Result<ReplayOperationResult, String> {
    let file_operation = begin_management(&state)?;
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _file_operation = file_operation;
        let source = PathBuf::from(path);
        let destination = PathBuf::from(destination);
        export_replay(&source, &destination)?;

        let mut database = Database::open(&database_path)?;
        let in_library = database
            .library_locations()?
            .iter()
            .any(|root| destination.starts_with(root));
        let summary = if in_library {
            let summary = refresh_summary(&destination)?;
            database.upsert_summary(&summary)?;
            Some(summary)
        } else {
            None
        };
        Ok(ReplayOperationResult {
            path: destination.to_string_lossy().into_owned(),
            summary,
        })
    })
    .await
    .map_err(|error| format!("Replay export task failed: {error}"))?
}

#[tauri::command]
async fn export_replay_copies(
    paths: Vec<String>,
    destination_folder: String,
    new_folder_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<ReplayBatchExportResult, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::FileOperation, Phase::Started, 1);
    let result = export_replay_copies_impl(paths, destination_folder, new_folder_name, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(
            Operation::FileOperation,
            Phase::ExpectedProblem,
            1,
        );
    } else {
        diagnostics::activity(Operation::FileOperation, Phase::Finished, 1);
    }
    result
}

async fn export_replay_copies_impl(
    paths: Vec<String>,
    destination_folder: String,
    new_folder_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<ReplayBatchExportResult, String> {
    let file_operation = begin_management(&state)?;
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _file_operation = file_operation;
        let sources = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
        let selected_folder = PathBuf::from(destination_folder);
        let (destination_folder, destinations) =
            export_replays(&sources, &selected_folder, new_folder_name.as_deref())?;

        let mut database = Database::open(&database_path)?;
        let library_locations = database.library_locations()?;
        let exports = destinations
            .into_iter()
            .map(|destination| {
                let in_library = library_locations
                    .iter()
                    .any(|root| destination.starts_with(root));
                let summary = if in_library {
                    let summary = refresh_summary(&destination)?;
                    database.upsert_summary(&summary)?;
                    Some(summary)
                } else {
                    None
                };
                Ok(ReplayOperationResult {
                    path: destination.to_string_lossy().into_owned(),
                    summary,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ReplayBatchExportResult {
            folder: destination_folder.to_string_lossy().into_owned(),
            exports,
        })
    })
    .await
    .map_err(|error| format!("Replay batch-export task failed: {error}"))?
}

/// Resolve one art source setting.
///
/// Three states are distinguished deliberately. An absent setting means the
/// folder has never been configured, so detection runs and a hit is remembered.
/// An empty setting means the user cleared it on purpose, so detection stays out
/// of the way. Anything else is their chosen path.
fn resolve_source(
    database: &Database,
    key: &str,
    detect: impl FnOnce() -> Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    match database.setting(key)? {
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => Ok(Some(PathBuf::from(value))),
        None => {
            let found = detect();
            // Only remember a hit. A miss stays unset so a game installed later
            // is still picked up on a future launch.
            if let Some(path) = &found {
                database.set_setting(key, &path.to_string_lossy())?;
            }
            Ok(found)
        }
    }
}

/// Read both art sources, refresh the cache if stale, and describe the result.
/// A configured folder that has since disappeared reports a problem but keeps
/// the setting, so a temporarily unmounted drive is not forgotten.
fn read_map_art_state(database_path: &Path) -> Result<MapArtState, String> {
    let mut database = Database::open(database_path)?;
    let cached_count = database.map_art_count()?;
    let cached_signature = database.map_art_signature()?;
    let install = resolve_source(&database, GAME_INSTALL_SETTING, detect_install)?;
    // Community maps live under the user's Documents folder, wherever the game
    // itself is installed; the installation only helps locate a Wine prefix.
    let custom_maps = resolve_source(&database, CUSTOM_MAPS_SETTING, || {
        detect_custom_maps(install.as_deref())
    })?;

    let mut problem = None;
    let mut usable = ArtSources::default();
    let mut source_unreachable = false;
    for (path, label, slot) in [
        (&install, "Game folder", 0_u8),
        (&custom_maps, "Custom maps folder", 1_u8),
    ] {
        let Some(path) = path else { continue };
        if !path.is_dir() {
            problem.get_or_insert(format!("{label} {} is not reachable", path.display()));
            source_unreachable = true;
            continue;
        }
        if slot == 0 {
            usable.install = Some(path.clone());
        } else {
            usable.custom_maps = Some(path.clone());
        }
    }

    let text = |path: &Option<PathBuf>| {
        path.as_ref()
            .map(|value| value.to_string_lossy().into_owned())
    };
    // A removable, network, Wine, or Proton path can be temporarily absent at
    // login. Keep serving the last valid cache until every configured source is
    // reachable again; rebuilding from only the surviving subset would discard
    // valid images.
    let (map_count, cache_changed) = if source_unreachable {
        (cached_count, false)
    } else {
        let expected_signature = usable.signature();
        match ensure_map_art(&mut database, &usable) {
            Ok(count) => (
                count,
                cached_signature.as_deref() != Some(expected_signature.as_str())
                    && (cached_count > 0 || count > 0),
            ),
            Err(error) => {
                problem.get_or_insert(error);
                (database.map_art_count().unwrap_or(cached_count), false)
            }
        }
    };

    Ok(MapArtState {
        install_path: text(&install),
        custom_maps_path: text(&custom_maps),
        map_count,
        problem,
        cache_changed,
    })
}

#[tauri::command]
async fn map_art_state(state: State<'_, AppState>) -> Result<MapArtState, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::MapArt, Phase::Started, 1);
    let result = map_art_state_impl(state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::MapArt, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::MapArt, Phase::Finished, 1);
    }
    result
}

async fn map_art_state_impl(state: State<'_, AppState>) -> Result<MapArtState, String> {
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || read_map_art_state(&database_path))
        .await
        .map_err(|error| format!("Map art task failed: {error}"))?
}

/// Point the viewer at a folder. One level of slack is allowed on each, so
/// picking `steamapps/common` or `World in Conflict` still resolves.
#[tauri::command]
async fn set_art_source(
    kind: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<MapArtState, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::MapArt, Phase::Started, 1);
    let result = set_art_source_impl(kind, path, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::MapArt, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::MapArt, Phase::Finished, 1);
    }
    result
}

async fn set_art_source_impl(
    kind: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<MapArtState, String> {
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let chosen = PathBuf::from(&path);
        let (key, resolved, expected) = match kind.as_str() {
            "install" => (
                GAME_INSTALL_SETTING,
                resolve_install(&chosen),
                "World in Conflict archives",
            ),
            "customMaps" => (
                CUSTOM_MAPS_SETTING,
                resolve_custom_maps(&chosen),
                "downloaded map archives",
            ),
            other => return Err(format!("Unknown art source {other}")),
        };
        let Some(resolved) = resolved else {
            let mut current = read_map_art_state(&database_path)?;
            current.problem = Some(format!("No {expected} in {}", chosen.display()));
            return Ok(current);
        };
        let database = Database::open(&database_path)?;
        database.set_setting(key, &resolved.to_string_lossy())?;
        drop(database);
        read_map_art_state(&database_path)
    })
    .await
    .map_err(|error| format!("Map art task failed: {error}"))?
}

/// Forget one folder. The setting is kept as an empty value so detection does
/// not immediately put it back.
#[tauri::command]
async fn clear_art_source(kind: String, state: State<'_, AppState>) -> Result<MapArtState, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::MapArt, Phase::Started, 1);
    let result = clear_art_source_impl(kind, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::MapArt, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::MapArt, Phase::Finished, 1);
    }
    result
}

async fn clear_art_source_impl(
    kind: String,
    state: State<'_, AppState>,
) -> Result<MapArtState, String> {
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let key = match kind.as_str() {
            "install" => GAME_INSTALL_SETTING,
            "customMaps" => CUSTOM_MAPS_SETTING,
            other => return Err(format!("Unknown art source {other}")),
        };
        let database = Database::open(&database_path)?;
        database.set_setting(key, "")?;
        drop(database);
        read_map_art_state(&database_path)
    })
    .await
    .map_err(|error| format!("Map art task failed: {error}"))?
}

/// Force a decode for a game patched in place under an unchanged path. The old
/// rows remain usable until a replacement commits successfully.
#[tauri::command]
async fn rescan_map_art(state: State<'_, AppState>) -> Result<MapArtState, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::MapArt, Phase::Started, 1);
    let result = rescan_map_art_impl(state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::MapArt, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::MapArt, Phase::Finished, 1);
    }
    result
}

async fn rescan_map_art_impl(state: State<'_, AppState>) -> Result<MapArtState, String> {
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let database = Database::open(&database_path)?;
        database.invalidate_map_art_signature()?;
        drop(database);
        read_map_art_state(&database_path)
    })
    .await
    .map_err(|error| format!("Map art task failed: {error}"))?
}

/// Cached overview art for the visible maps. One command, connection, and
/// prepared statement replace the previous per-tile database/IPC round trips.
fn read_map_art_batch(
    database_path: &Path,
    map_names: Vec<String>,
) -> Result<HashMap<String, Option<MapArtAsset>>, String> {
    let mut requests = HashMap::new();
    for raw in map_names {
        let request = raw.trim();
        if request.is_empty() {
            continue;
        }
        requests
            .entry(request.to_owned())
            .or_insert_with(|| internal_map_name(request).to_owned());
    }
    if requests.len() > MAX_MAP_ART_BATCH_SIZE {
        return Err(format!(
            "Map art batch has {} names; maximum is {MAX_MAP_ART_BATCH_SIZE}",
            requests.len()
        ));
    }

    let (request_names, lookup_names): (Vec<_>, Vec<_>) = requests.into_iter().unzip();
    let database = Database::open(database_path)?;
    let assets = database.map_art_assets(&lookup_names)?;
    Ok(request_names
        .into_iter()
        .zip(assets)
        .map(|(name, asset)| {
            (
                name,
                asset.map(|asset| MapArtAsset {
                    image_url: png_data_url(&asset.png),
                    bounds: asset.bounds.map(|bounds| MapBoundsDto {
                        min_x: bounds.min_x,
                        min_z: bounds.min_z,
                        max_x: bounds.max_x,
                        max_z: bounds.max_z,
                    }),
                    command_point_names: asset.command_point_names,
                }),
            )
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MapArtAsset {
    image_url: String,
    bounds: Option<MapBoundsDto>,
    command_point_names: std::collections::BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct MapBoundsDto {
    min_x: f32,
    min_z: f32,
    max_x: f32,
    max_z: f32,
}

#[tauri::command]
async fn map_art_batch(
    map_names: Vec<String>,
    state: State<'_, AppState>,
) -> Result<HashMap<String, Option<MapArtAsset>>, String> {
    use wic_replay_viewer::diagnostics::{self, Operation, Phase};
    diagnostics::activity(Operation::MapArt, Phase::Started, 1);
    let result = map_art_batch_impl(map_names, state).await;
    if result.is_err() {
        wic_replay_viewer::diagnostics::activity(Operation::MapArt, Phase::ExpectedProblem, 1);
    } else {
        diagnostics::activity(Operation::MapArt, Phase::Finished, 1);
    }
    result
}

async fn map_art_batch_impl(
    map_names: Vec<String>,
    state: State<'_, AppState>,
) -> Result<HashMap<String, Option<MapArtAsset>>, String> {
    let database_path = state.database_path.clone();
    tauri::async_runtime::spawn_blocking(move || read_map_art_batch(&database_path, map_names))
        .await
        .map_err(|error| format!("Map art task failed: {error}"))?
}

/// Offer the game's default replay folder once, on a library that has none.
/// Adding a location does not import anything; the user still chooses to scan,
/// and can remove it like any other. Seeding is recorded so removing it sticks.
fn seed_default_replay_directory(database: &mut Database) -> Result<(), String> {
    if database.setting(REPLAY_DIR_SEEDED_SETTING)?.is_some() {
        return Ok(());
    }
    if !database.library_locations()?.is_empty() {
        database.set_setting(REPLAY_DIR_SEEDED_SETTING, "skipped")?;
        return Ok(());
    }
    let install = database
        .setting(GAME_INSTALL_SETTING)?
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let Some(replays) = detect_replay_directory(install.as_deref()) else {
        return Ok(());
    };
    database.add_library_locations(&[replays])?;
    database.set_setting(REPLAY_DIR_SEEDED_SETTING, "seeded")?;
    Ok(())
}

/// Explicit developer harness entry point, compiled out of production builds.
#[cfg(debug_assertions)]
pub fn run_error_reporting_probe(scenario: String, root: Option<String>) {
    assert!(matches!(
        scenario.as_str(),
        "healthy" | "javascript" | "native" | "panic" | "exit" | "import"
    ));
    assert!(
        std::env::var_os("XDG_DATA_HOME").is_some(),
        "Use an isolated XDG_DATA_HOME"
    );
    PROBE.set((scenario, root)).expect("one probe per process");
    run();
}
#[cfg(debug_assertions)]
static PROBE: std::sync::OnceLock<(String, Option<String>)> = std::sync::OnceLock::new();

#[cfg(debug_assertions)]
fn attach_probe(app: &AppHandle) {
    let Some((scenario, root)) = PROBE.get().cloned() else {
        return;
    };
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_secs(35));
        eprintln!("Starting diagnostic probe: {scenario}");
        match scenario.as_str() {
            "javascript" => {
                app.get_webview_window("main").unwrap().eval("setTimeout(() => { const end = performance.now() + 20000; while (performance.now() < end) {} }, 0)").unwrap();
            }
            "native" => {
                app.run_on_main_thread(|| thread::sleep(std::time::Duration::from_secs(20)))
                    .unwrap();
            }
            "panic" => {
                let _ = thread::spawn(|| panic!("PRIVATE_PROBE_MESSAGE /private/replay.wicdemo"))
                    .join();
            }
            "exit" => std::process::exit(94),
            "import" => {
                let state = app.state::<AppState>();
                start_import(
                    vec![root.expect("import requires a replay directory")],
                    app.clone(),
                    state,
                )
                .expect("probe import starts");
                while app.state::<AppState>().importing.load(Ordering::Acquire) {
                    thread::sleep(std::time::Duration::from_secs(1));
                }
                eprintln!("Diagnostic probe import finished");
            }
            _ => {}
        }
        thread::sleep(std::time::Duration::from_secs(30));
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if error_helper::supervise() {
        return;
    }
    reporting::initialize();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            reporting::attach(app.handle());
            let project_dirs = ProjectDirs::from("org", "Wicgate", "WiC Replay Viewer")
                .ok_or_else(|| {
                    std::io::Error::other("Cannot determine application data directory")
                })?;
            let database_path = project_dirs.data_dir().join("library.sqlite3");
            let mut database = Database::open(&database_path).map_err(std::io::Error::other)?;
            // Best effort: a missing default folder is normal and must not block startup.
            let _ = seed_default_replay_directory(&mut database);
            drop(database);
            app.manage(AppState {
                database_path,
                importing: Arc::new(AtomicBool::new(false)),
                import_cancelled: Arc::new(AtomicBool::new(false)),
                file_operation_busy: Arc::new(AtomicBool::new(false)),
                detail_loads: Arc::new(DetailLoadCoordinator::default()),
            });
            #[cfg(debug_assertions)]
            attach_probe(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            reporting::report_frontend_error,
            reporting::diagnostic_heartbeat,
            initial_state,
            import_running,
            add_library_locations,
            start_import,
            cancel_import,
            remove_library_location,
            replay_detail,
            replay_playback,
            replay_objectives,
            rename_replay_file,
            change_replay_in_game_name,
            export_replay_copy,
            export_replay_copies,
            map_art_state,
            set_art_source,
            clear_art_source,
            rescan_map_art,
            map_art_batch
        ])
        .build(tauri::generate_context!())
        .expect("error while building WiC Replay Viewer")
        .run(|_, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                reporting::stop();
            }
        });
}

mod error_helper;

// Native UI integration stays outside the portable reporting state machine.
mod reporting {
    use super::*;
    use std::sync::{Mutex, OnceLock, atomic::AtomicU64};
    use std::time::{Duration, Instant};
    use wic_replay_viewer::diagnostics::{
        self, BuildInfo, Code, CodeLocation, Observation, Operation, Phase, Reporter, UiContext,
        Watchdog,
    };
    static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();
    pub struct Runtime {
        pub reporter: Arc<Reporter>,
        started: Instant,
        native: AtomicU64,
        frontend: AtomicU64,
        frame: AtomicU64,
        events: AtomicU64,
        visible: AtomicBool,
        native_visible: AtomicBool,
        ui: Mutex<UiContext>,
        pending: AtomicBool,
        stopped: AtomicBool,
    }
    impl Runtime {
        fn now(&self) -> u64 {
            self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
        }
        fn observation(&self) -> Observation {
            let now = self.now();
            Observation {
                native_age_ms: now.saturating_sub(self.native.load(Ordering::Relaxed)),
                frontend_age_ms: now.saturating_sub(self.frontend.load(Ordering::Relaxed)),
                frame_age_ms: now.saturating_sub(self.frame.load(Ordering::Relaxed)),
                visible: self.visible.load(Ordering::Relaxed)
                    && self.native_visible.load(Ordering::Relaxed),
                events_received: self.events.load(Ordering::Relaxed),
                ui: self.ui.lock().unwrap_or_else(|e| e.into_inner()).clone(),
                ..Observation::default()
            }
        }
    }
    fn webview_version() -> Vec<u32> {
        #[cfg(target_os = "linux")]
        // SAFETY: these library-version accessors have no pointers or GTK state.
        unsafe {
            vec![
                webkit2gtk::ffi::webkit_get_major_version(),
                webkit2gtk::ffi::webkit_get_minor_version(),
                webkit2gtk::ffi::webkit_get_micro_version(),
            ]
        }
        #[cfg(not(target_os = "linux"))]
        vec![]
    }
    pub(super) fn build_info() -> BuildInfo {
        BuildInfo {
            os_version: std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .ok()
                .map(|s| {
                    s.trim()
                        .split('.')
                        .take(3)
                        .map_while(|v| v.parse().ok())
                        .collect()
                })
                .unwrap_or_default(),
            webview_version: webview_version(),
            version: env!("CARGO_PKG_VERSION").into(),
            fingerprint: env!("WIC_BUILD_FINGERPRINT").into(),
            revision: env!("WIC_BUILD_REVISION").into(),
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            assets: serde_json::from_str(env!("WIC_REPORT_ASSETS")).unwrap_or_default(),
        }
    }
    pub fn initialize() {
        let reporter = Reporter::start_session(build_info(), error_helper::child_session());
        reporter.install();
        let panic_reporter = reporter.clone();
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let location = info.location().and_then(|p| {
                let file = p.file().replace('\\', "/");
                let candidates = [
                    file.clone(),
                    format!("src-tauri/{file}"),
                    format!("parser/rust_parser/{file}"),
                ];
                let mut matches = candidates
                    .into_iter()
                    .filter(|s| panic_reporter.allows_location(s));
                let asset = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                Some(CodeLocation {
                    asset,
                    line: p.line(),
                    column: p.column(),
                })
            });
            panic_reporter.panic_location(location);
            previous(info);
        }));
        let _ = RUNTIME.set(Arc::new(Runtime {
            reporter,
            started: Instant::now(),
            native: AtomicU64::new(0),
            frontend: AtomicU64::new(0),
            frame: AtomicU64::new(0),
            events: AtomicU64::new(0),
            visible: AtomicBool::new(true),
            native_visible: AtomicBool::new(true),
            ui: Mutex::new(UiContext::default()),
            pending: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        }));
        diagnostics::activity(Operation::Startup, Phase::Started, 1);
    }
    pub fn attach(app: &AppHandle) {
        let Some(runtime) = RUNTIME.get().cloned() else {
            return;
        };
        let app = app.clone();
        platform_signals(&app);
        let _ = thread::Builder::new()
            .name("wic-ui-watchdog".into())
            .spawn(move || {
                let mut watchdog = Watchdog::default();
                while !runtime.stopped.load(Ordering::Acquire) {
                    let enabled = runtime.reporter.enabled();
                    if enabled && !runtime.pending.swap(true, Ordering::AcqRel) {
                        let state = runtime.clone();
                        let window = app.get_webview_window("main");
                        if app
                            .run_on_main_thread(move || {
                                state.native.store(state.now(), Ordering::Relaxed);
                                if let Some(window) = window {
                                    state.native_visible.store(
                                        window.is_visible().unwrap_or(true)
                                            && !window.is_minimized().unwrap_or(false),
                                        Ordering::Relaxed,
                                    );
                                }
                                state.pending.store(false, Ordering::Release);
                            })
                            .is_err()
                        {
                            runtime.pending.store(false, Ordering::Release);
                        }
                    }
                    let observation = runtime.observation();
                    runtime.reporter.observe(observation.clone());
                    for (code, recovered) in watchdog.tick(runtime.now(), &observation, enabled) {
                        if recovered {
                            runtime.reporter.recovered(code);
                        } else {
                            runtime.reporter.incident(
                                code,
                                Operation::Frontend,
                                observation.clone(),
                                vec![],
                            );
                        }
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            });
    }
    #[cfg(target_os = "linux")]
    fn platform_signals(app: &AppHandle) {
        use webkit2gtk::WebViewExt;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.with_webview(|webview| {
                let view = webview.inner();
                view.connect_is_web_process_responsive_notify(|view| {
                    if let Some(r) = RUNTIME.get() {
                        if !view.is_web_process_responsive() {
                            r.reporter.incident(
                                Code::WebviewUnresponsive,
                                Operation::Frontend,
                                r.observation(),
                                vec![],
                            );
                        } else {
                            r.reporter.recovered(Code::WebviewUnresponsive);
                        }
                    }
                });
                view.connect_web_process_terminated(|_, reason| {
                    if let Some(r) = RUNTIME.get() {
                        use webkit2gtk::glib::translate::IntoGlib;
                        let mut observation = r.observation();
                        observation.webview_reason = Some(reason.into_glib());
                        r.reporter.incident(
                            Code::WebviewTerminated,
                            Operation::Frontend,
                            observation,
                            vec![],
                        );
                    }
                });
            });
        }
    }
    #[cfg(windows)]
    fn platform_signals(app: &AppHandle) {
        use webview2_com::ProcessFailedEventHandler;
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.with_webview(|webview| unsafe {
                if let Ok(view) = webview.controller().CoreWebView2() {
                    let handler = ProcessFailedEventHandler::create(Box::new(|_, args| {
                        if let Some(r) = RUNTIME.get() {
                            let mut observation = r.observation();
                            if let Some(args) = args {
                                let mut kind = Default::default();
                                if args.ProcessFailedKind(&mut kind).is_ok() {
                                    observation.webview_reason = Some(kind.0);
                                }
                            }
                            let code = if observation.webview_reason == Some(
                                webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE.0
                            ) { Code::WebviewUnresponsive } else { Code::WebviewTerminated };
                            r.reporter.incident(code, Operation::Frontend, observation, vec![]);
                        }
                        Ok(())
                    }));
                    let mut token = 0;
                    let _ = view.add_ProcessFailed(&handler, &mut token);
                }
            });
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    fn platform_signals(_: &AppHandle) {}
    pub fn stop() {
        if let Some(r) = RUNTIME.get() {
            r.stopped.store(true, Ordering::Release);
            r.reporter.stop();
        }
    }
    fn reporter() -> Result<Arc<Reporter>, String> {
        RUNTIME
            .get()
            .map(|r| r.reporter.clone())
            .ok_or_else(|| "Local error reporting is unavailable.".into())
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    pub struct FrontendFailure {
        code: Code,
        locations: Vec<CodeLocation>,
    }
    #[tauri::command]
    pub async fn report_frontend_error(failure: FrontendFailure) -> Result<(), String> {
        if !matches!(
            failure.code,
            Code::FrontendError | Code::UnhandledRejection | Code::VueError | Code::CallbackFailed
        ) || failure.locations.len() > 32
        {
            return Err("Invalid frontend incident.".into());
        }
        let r = reporter()?;
        if failure
            .locations
            .iter()
            .any(|l| l.asset.len() > 160 || !r.allows_location(&l.asset))
        {
            return Err("Invalid source location.".into());
        }
        r.incident(
            failure.code,
            Operation::Frontend,
            Observation::default(),
            failure.locations,
        );
        Ok(())
    }
    #[tauri::command]
    pub async fn diagnostic_heartbeat(
        visible: bool,
        frame: bool,
        events_received: u64,
        ui: UiContext,
    ) -> Result<(), String> {
        let r = RUNTIME
            .get()
            .ok_or_else(|| "Local error reporting is unavailable.".to_owned())?;
        r.frontend.store(r.now(), Ordering::Relaxed);
        *r.ui.lock().unwrap_or_else(|e| e.into_inner()) = ui;
        r.visible.store(visible, Ordering::Relaxed);
        r.events.store(events_received, Ordering::Relaxed);
        if frame {
            r.frame.store(r.now(), Ordering::Relaxed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    use tempfile::tempdir;
    use wic_replay_viewer::map_art::MapArt;

    #[test]
    fn unreachable_sources_keep_the_last_valid_map_art_cache() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let missing_install = directory.path().join("temporarily-unmounted-game");
        let sources = ArtSources {
            install: Some(missing_install.clone()),
            custom_maps: None,
        };
        {
            let mut database = Database::open(&database_path).expect("database");
            database
                .set_setting(
                    GAME_INSTALL_SETTING,
                    missing_install.to_string_lossy().as_ref(),
                )
                .expect("install setting");
            database
                .set_setting(CUSTOM_MAPS_SETTING, "")
                .expect("disabled custom maps");
            database
                .replace_map_art(
                    &sources.signature(),
                    &[MapArt {
                        map_name: "russia3".to_owned(),
                        source_archive: "/game/wic60.sdf".to_owned(),
                        png: vec![1, 2, 3],
                        bounds: None,
                        command_point_names: Default::default(),
                    }],
                )
                .expect("cached art");
        }

        let state = read_map_art_state(&database_path).expect("state");
        assert_eq!(state.map_count, 1);
        assert!(!state.cache_changed);
        assert!(
            state
                .problem
                .as_deref()
                .is_some_and(|problem| problem.contains("is not reachable"))
        );

        let database = Database::open(&database_path).expect("reopen database");
        assert_eq!(
            database.map_art_png("russia3").expect("preserved art"),
            Some(vec![1, 2, 3])
        );
        assert!(
            database
                .map_art_is_current(&sources.signature())
                .expect("preserved signature")
        );
        database
            .invalidate_map_art_signature()
            .expect("force rescan");
        drop(database);

        let rescanned = read_map_art_state(&database_path).expect("rescan state");
        assert_eq!(rescanned.map_count, 1);
        assert!(!rescanned.cache_changed);
        let database = Database::open(&database_path).expect("reopen after rescan");
        assert_eq!(
            database.map_art_png("russia3").expect("art after rescan"),
            Some(vec![1, 2, 3])
        );
        assert!(
            database
                .map_art_signature()
                .expect("invalidated signature")
                .is_none()
        );
    }

    #[test]
    fn map_art_batch_normalizes_names_and_returns_misses() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let mut database = Database::open(&database_path).expect("database");
        database
            .replace_map_art(
                "signature",
                &[MapArt {
                    map_name: "russia3".to_owned(),
                    source_archive: "/game/wic60.sdf".to_owned(),
                    png: vec![1, 2, 3],
                    bounds: None,
                    command_point_names: std::collections::BTreeMap::from([(
                        0x2eae_05b8,
                        "Space Needle".to_owned(),
                    )]),
                }],
            )
            .expect("cached art");
        drop(database);

        let batch = read_map_art_batch(
            &database_path,
            vec![
                "maps/Russia3/Russia3.ice".to_owned(),
                "missing".to_owned(),
                "  russia3  ".to_owned(),
            ],
        )
        .expect("batch");
        let expected = MapArtAsset {
            image_url: png_data_url(&[1, 2, 3]),
            bounds: None,
            command_point_names: std::collections::BTreeMap::from([(
                0x2eae_05b8,
                "Space Needle".to_owned(),
            )]),
        };
        assert_eq!(
            batch.get("maps/Russia3/Russia3.ice"),
            Some(&Some(expected.clone()))
        );
        assert_eq!(batch.get("russia3"), Some(&Some(expected)));
        assert_eq!(batch.get("missing"), Some(&None));
    }

    #[test]
    fn simultaneous_detail_requests_share_one_loader_and_completed_keys_can_retry() {
        const REQUESTS: usize = 8;
        let coordinator = Arc::new(DetailLoadCoordinator::default());
        let barrier = Arc::new(Barrier::new(REQUESTS));
        let calls = Arc::new(AtomicUsize::new(0));
        let path = PathBuf::from("shared.wicdemo");
        let detail = DetailView::from_json(
            r#"{
                "replay": {
                    "gameInfo": {
                        "mapName": "maps/test/test.ice",
                        "mapDisplayName": "Test",
                        "serverName": "Test",
                        "dateTime": "2009-01-01",
                        "gameMode": "Domination"
                    },
                    "players": [],
                    "incomplete": false
                },
                "timeline": {
                    "schemaVersion": 18,
                    "durationSeconds": 0.0,
                    "phases": [],
                    "participants": [],
                    "events": [],
                    "preMatchChat": [],
                    "postMatchChat": [],
                    "coverage": {
                        "chat": "visibleToRecorder",
                        "tacticalAid": "recorderOnly",
                        "tacticalAidMarkers": "visibleFactionWithPlayer",
                        "tacticalAidDeployments": "bothFactionsTeamOnly"
                    },
                    "recorderTacticalAidUsage": { "playerId": null }
                }
            }"#,
        )
        .expect("empty detail");
        let mut threads = Vec::new();

        for _ in 0..REQUESTS {
            let coordinator = Arc::clone(&coordinator);
            let barrier = Arc::clone(&barrier);
            let calls = Arc::clone(&calls);
            let path = path.clone();
            let detail = detail.clone();
            threads.push(thread::spawn(move || {
                barrier.wait();
                coordinator.load(path, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(50));
                    Ok(detail)
                })
            }));
        }

        for thread in threads {
            assert_eq!(
                thread
                    .join()
                    .expect("request thread")
                    .expect("coalesced detail")
                    .timeline_schema,
                18
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let retry = coordinator.load(path, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err("retry failure".to_owned())
        });
        assert_eq!(retry.expect_err("retry failure"), "retry failure");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
