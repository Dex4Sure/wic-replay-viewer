use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use wic_replay_parser::editor::rewrite_replay_name;
use wic_replay_parser::parser::{WicReplayParser, read_replay_file, validate_replay_path};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn validate_leaf_file_name(file_name: &str) -> Result<(), String> {
    let candidate = Path::new(file_name);
    if file_name.is_empty()
        || candidate.file_name().and_then(|value| value.to_str()) != Some(file_name)
        || candidate
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Err("File name must not contain a folder path".to_owned());
    }
    if file_name.ends_with(['.', ' '])
        || file_name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
    {
        return Err("File name contains characters unsupported by Windows or WiC".to_owned());
    }
    validate_replay_path(candidate)
}

fn validate_leaf_folder_name(folder_name: &str) -> Result<(), String> {
    let candidate = Path::new(folder_name);
    if folder_name.is_empty()
        || candidate.file_name().and_then(|value| value.to_str()) != Some(folder_name)
        || candidate
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Err("New folder name must not contain a path".to_owned());
    }
    if folder_name.ends_with(['.', ' '])
        || folder_name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
    {
        return Err("New folder name contains characters unsupported by Windows".to_owned());
    }
    Ok(())
}

fn validate_destination(path: &Path) -> Result<(), String> {
    validate_replay_path(path)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Replay destination must include a folder".to_owned())?;
    if !parent.is_dir() {
        return Err(format!(
            "Replay destination folder does not exist: {}",
            parent.display()
        ));
    }
    Ok(())
}

fn unique_sibling(path: &Path, purpose: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Replay path has no parent folder: {}", path.display()))?;
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("Replay file name is not valid UTF-8: {}", path.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{base}.wic-replay-viewer-{purpose}-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Cannot reserve a temporary file beside {}",
        path.display()
    ))
}

fn write_new_file(
    path: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Cannot create {}: {error}", path.display()))?;
    let result = (|| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions).map_err(|error| {
                format!("Cannot preserve permissions on {}: {error}", path.display())
            })?;
        }
        file.write_all(bytes)
            .map_err(|error| format!("Cannot write {}: {error}", path.display()))?;
        file.sync_all()
            .map_err(|error| format!("Cannot finish writing {}: {error}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        drop(file);
        let _ = fs::remove_file(path);
    }
    result
}

fn validate_written_file(path: &Path, expected: &[u8]) -> Result<(), String> {
    let mut file =
        File::open(path).map_err(|error| format!("Cannot reopen {}: {error}", path.display()))?;
    let mut actual = Vec::with_capacity(expected.len());
    file.read_to_end(&mut actual)
        .map_err(|error| format!("Cannot verify {}: {error}", path.display()))?;
    if actual != expected {
        return Err(format!(
            "Written replay differs from the verified output: {}",
            path.display()
        ));
    }
    WicReplayParser::from_bytes(&actual)
        .map_err(|error| format!("Written replay failed parser validation: {error}"))?;
    Ok(())
}

/// Rename one replay within its existing folder without overwriting another
/// file. The caller updates cached paths after this succeeds.
pub fn rename_replay(source: &Path, file_name: &str) -> Result<PathBuf, String> {
    validate_replay_path(source)?;
    validate_leaf_file_name(file_name)?;
    if !source.is_file() {
        return Err(format!("Replay file does not exist: {}", source.display()));
    }
    let destination = source
        .parent()
        .ok_or_else(|| format!("Replay path has no parent folder: {}", source.display()))?
        .join(file_name);
    if destination == source {
        return Err("The replay already has that file name".to_owned());
    }
    if destination.exists() {
        return Err(format!(
            "A file already exists at {}",
            destination.display()
        ));
    }
    let source_bytes = read_replay_file(source)?;
    WicReplayParser::from_bytes(&source_bytes)
        .map_err(|error| format!("Source replay failed validation: {error}"))?;
    // `std::fs::rename` may replace a destination created between an existence
    // check and the syscall on Unix. A hard link has no-overwrite semantics; on
    // filesystems without hard links, an exclusive create provides the same
    // collision guarantee before the source is removed.
    let linked = fs::hard_link(source, &destination).is_ok();
    if !linked {
        let permissions = fs::metadata(source)
            .ok()
            .map(|metadata| metadata.permissions());
        write_new_file(&destination, &source_bytes, permissions)?;
    }
    if let Err(error) = validate_written_file(&destination, &source_bytes) {
        let _ = fs::remove_file(&destination);
        return Err(format!("Renamed replay failed validation: {error}"));
    }
    if let Err(error) = fs::remove_file(source) {
        let _ = fs::remove_file(&destination);
        return Err(format!(
            "Cannot remove the old replay name {}; no change was kept: {error}",
            source.display()
        ));
    }
    Ok(destination)
}

/// Export an exact replay copy without touching the source.
pub fn export_replay(source: &Path, destination: &Path) -> Result<(), String> {
    validate_replay_path(source)?;
    validate_destination(destination)?;
    if source == destination {
        return Err("Export destination must differ from the source replay".to_owned());
    }
    if destination.exists() {
        return Err(format!(
            "Export will not overwrite the existing file {}",
            destination.display()
        ));
    }
    let source_bytes = read_replay_file(source)?;
    WicReplayParser::from_bytes(&source_bytes)
        .map_err(|error| format!("Source replay failed validation: {error}"))?;
    let permissions = fs::metadata(source)
        .ok()
        .map(|metadata| metadata.permissions());
    write_new_file(destination, &source_bytes, permissions)?;
    if let Err(error) = validate_written_file(destination, &source_bytes) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

/// Export several exact copies into one folder. Every source and destination is
/// validated before the first write. If a later write fails, copies created by
/// this call are removed while every source remains untouched.
pub fn export_replays(
    sources: &[PathBuf],
    selected_folder: &Path,
    new_folder_name: Option<&str>,
) -> Result<(PathBuf, Vec<PathBuf>), String> {
    if sources.is_empty() {
        return Err("Select at least one replay to export".to_owned());
    }
    if !selected_folder.is_dir() {
        return Err(format!(
            "Replay export folder does not exist: {}",
            selected_folder.display()
        ));
    }
    let destination_folder = match new_folder_name {
        Some(folder_name) => {
            validate_leaf_folder_name(folder_name)?;
            let path = selected_folder.join(folder_name);
            if path.exists() {
                return Err(format!(
                    "Cannot create export folder because it already exists: {}",
                    path.display()
                ));
            }
            path
        }
        None => selected_folder.to_path_buf(),
    };

    let mut destinations = Vec::with_capacity(sources.len());
    let mut destination_names = HashSet::with_capacity(sources.len());
    for source in sources {
        validate_replay_path(source)?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("Replay file name is not valid UTF-8: {}", source.display()))?;
        let destination = destination_folder.join(file_name);
        if source == &destination {
            return Err(format!(
                "Export destination must differ from the source replay: {}",
                source.display()
            ));
        }
        if !destination_names.insert(file_name.to_lowercase()) {
            return Err(format!(
                "Selected replays contain the same file name: {file_name}"
            ));
        }
        if destination.exists() {
            return Err(format!(
                "Export will not overwrite the existing file {}",
                destination.display()
            ));
        }
        let source_bytes = read_replay_file(source)?;
        WicReplayParser::from_bytes(&source_bytes)
            .map_err(|error| format!("Source replay failed validation: {error}"))?;
        destinations.push(destination);
    }

    let created_folder = new_folder_name.is_some();
    if created_folder {
        fs::create_dir(&destination_folder).map_err(|error| {
            format!(
                "Cannot create replay export folder {}: {error}",
                destination_folder.display()
            )
        })?;
    }

    let mut created = Vec::with_capacity(sources.len());
    for (source, destination) in sources.iter().zip(&destinations) {
        if let Err(error) = export_replay(source, destination) {
            for path in &created {
                let _ = fs::remove_file(path);
            }
            if created_folder {
                let _ = fs::remove_dir(&destination_folder);
            }
            return Err(error);
        }
        created.push(destination.clone());
    }
    Ok((destination_folder, created))
}

/// Replace the in-game name through a validated sibling file. The original is
/// retained as a temporary backup until the replacement has been moved into
/// place successfully.
pub fn change_replay_name(source: &Path, replay_name: &str) -> Result<(), String> {
    validate_replay_path(source)?;
    let source_bytes = read_replay_file(source)?;
    let output = rewrite_replay_name(&source_bytes, replay_name)?;
    if output == source_bytes {
        return Ok(());
    }
    let permissions = fs::metadata(source)
        .ok()
        .map(|metadata| metadata.permissions());
    let temporary = unique_sibling(source, "write")?;
    write_new_file(&temporary, &output, permissions)?;
    if let Err(error) = validate_written_file(&temporary, &output) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    let backup = unique_sibling(source, "backup")?;
    if let Err(error) = fs::rename(source, &backup) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "Cannot prepare {} for replacement: {error}",
            source.display()
        ));
    }
    if let Err(error) = fs::rename(&temporary, source) {
        let rollback = fs::rename(&backup, source);
        let _ = fs::remove_file(&temporary);
        return Err(match rollback {
            Ok(()) => format!("Cannot replace replay; the original was restored: {error}"),
            Err(rollback_error) => format!(
                "Cannot replace replay ({error}) and cannot restore {}: {rollback_error}; backup remains at {}",
                source.display(),
                backup.display()
            ),
        });
    }
    if let Err(error) = fs::remove_file(&backup) {
        return Err(format!(
            "Replay was updated, but its recovery copy could not be removed: {} ({error})",
            backup.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const REPLAY_NAME_HASH: [u8; 4] = [0xef, 0x03, 0x7c, 0x15];
    const EVENT_HASH: [u8; 4] = [0x03, 0x02, 0xb5, 0x05];
    const TEAM_WINS_HASH: [u8; 4] = [0x29, 0x03, 0xb8, 0x0d];

    fn fixture(name: Option<&str>) -> Vec<u8> {
        let mut stream = vec![0; 47];
        stream.extend_from_slice(b"maps/ustown1/ustown1.ice\0");
        if let Some(name) = name {
            let mut payload = name
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            payload.extend_from_slice(&[0, 0]);
            let total = u32::try_from(13 + payload.len()).expect("fixture field");
            stream.extend_from_slice(&REPLAY_NAME_HASH);
            stream.extend_from_slice(&total.to_le_bytes());
            stream.push(5);
            stream.extend_from_slice(&total.to_le_bytes());
            stream.extend_from_slice(&payload);
        }
        stream.resize(16 * 1024 + 11, 0x33);
        stream.extend_from_slice(&EVENT_HASH);
        stream.extend_from_slice(&[0x15, 0, 0, 0]);
        stream.push(6);
        stream.extend_from_slice(&21u32.to_le_bytes());
        stream.extend_from_slice(&0f32.to_bits().to_le_bytes());
        stream.extend_from_slice(&TEAM_WINS_HASH);

        let mut raw = Vec::new();
        raw.extend_from_slice(b"\r\0BinTagFormat2");
        for chunk in stream.chunks(16 * 1024) {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
            encoder.write_all(chunk).expect("compress fixture");
            let compressed = encoder.finish().expect("finish fixture");
            raw.extend_from_slice(
                &u32::try_from(compressed.len())
                    .expect("compressed fixture")
                    .to_le_bytes(),
            );
            raw.extend_from_slice(&compressed);
        }
        raw
    }

    fn parsed_name(path: &Path) -> Option<String> {
        WicReplayParser::new(path)
            .expect("valid replay")
            .parse()
            .game_info
            .replay_name
    }

    #[test]
    fn export_preserves_exact_bytes_and_refuses_overwrite() {
        let directory = tempdir().expect("temp dir");
        let source = directory.path().join("source.wicdemo");
        let destination = directory.path().join("exported.wicdemo");
        let original = fixture(Some("demo01"));
        fs::write(&source, &original).expect("source fixture");

        export_replay(&source, &destination).expect("export replay");
        assert_eq!(fs::read(&source).expect("unchanged source"), original);
        assert_eq!(fs::read(&destination).expect("exact copy"), original);
        assert!(export_replay(&source, &destination).is_err());
        assert_eq!(fs::read(&source).expect("source after collision"), original);
    }

    #[test]
    fn batch_export_is_exact_and_preflights_every_collision() {
        let directory = tempdir().expect("temp dir");
        let sources = directory.path().join("sources");
        let destination = directory.path().join("exports");
        fs::create_dir_all(&sources).expect("source folder");
        fs::create_dir_all(&destination).expect("export folder");
        let first = sources.join("first.wicdemo");
        let second = sources.join("second.wicdemo");
        let first_bytes = fixture(Some("first"));
        let second_bytes = fixture(Some("second"));
        fs::write(&first, &first_bytes).expect("first source");
        fs::write(&second, &second_bytes).expect("second source");

        let (export_folder, exported) =
            export_replays(&[first.clone(), second.clone()], &destination, None)
                .expect("batch export");
        assert_eq!(export_folder, destination);
        assert_eq!(
            exported,
            [
                destination.join("first.wicdemo"),
                destination.join("second.wicdemo")
            ]
        );
        assert_eq!(fs::read(&exported[0]).expect("first export"), first_bytes);
        assert_eq!(fs::read(&exported[1]).expect("second export"), second_bytes);
        assert_eq!(fs::read(&first).expect("first source remains"), first_bytes);
        assert_eq!(
            fs::read(&second).expect("second source remains"),
            second_bytes
        );

        let another_destination = directory.path().join("collision-exports");
        fs::create_dir_all(&another_destination).expect("collision folder");
        fs::write(another_destination.join("second.wicdemo"), b"occupied")
            .expect("occupied destination");
        assert!(
            export_replays(&[first.clone(), second.clone()], &another_destination, None).is_err()
        );
        assert!(!another_destination.join("first.wicdemo").exists());
        assert_eq!(
            fs::read(another_destination.join("second.wicdemo")).expect("collision preserved"),
            b"occupied"
        );

        let (new_folder, nested_exports) = export_replays(
            &[first.clone(), second.clone()],
            directory.path(),
            Some("Tournament finals"),
        )
        .expect("export into a new folder");
        assert_eq!(new_folder, directory.path().join("Tournament finals"));
        assert_eq!(
            fs::read(&nested_exports[0]).expect("nested first"),
            first_bytes
        );
        assert_eq!(
            fs::read(&nested_exports[1]).expect("nested second"),
            second_bytes
        );
        assert!(export_replays(&[first], directory.path(), Some("Tournament finals")).is_err());
        assert!(export_replays(&[second], directory.path(), Some("../escape")).is_err());
    }

    #[test]
    fn direct_name_change_is_validated_and_failed_edits_preserve_the_source() {
        let directory = tempdir().expect("temp dir");
        let source = directory.path().join("source.wicdemo");
        fs::write(&source, fixture(Some("demo01"))).expect("source fixture");

        change_replay_name(&source, "Named in the viewer").expect("change name");
        assert_eq!(parsed_name(&source).as_deref(), Some("Named in the viewer"));
        let valid = fs::read(&source).expect("valid changed replay");
        assert!(change_replay_name(&source, "bad\nname").is_err());
        assert_eq!(fs::read(&source).expect("unchanged after error"), valid);

        let missing = directory.path().join("missing-field.wicdemo");
        let missing_bytes = fixture(None);
        fs::write(&missing, &missing_bytes).expect("missing fixture");
        assert!(change_replay_name(&missing, "Cannot inject").is_err());
        assert_eq!(
            fs::read(&missing).expect("unchanged missing fixture"),
            missing_bytes
        );
    }

    #[test]
    fn rename_changes_only_the_file_name_and_refuses_an_occupied_name() {
        let directory = tempdir().expect("temp dir");
        let source = directory.path().join("source.wicdemo");
        let source_bytes = fixture(Some("demo01"));
        fs::write(&source, &source_bytes).expect("source fixture");
        let renamed = rename_replay(&source, "descriptive name.wicdemo").expect("rename");
        assert!(!source.exists());
        assert_eq!(
            fs::read(&renamed).expect("renamed replay"),
            source_bytes,
            "renaming the file must not change its contents"
        );
        assert_eq!(parsed_name(&renamed).as_deref(), Some("demo01"));

        let occupied = directory.path().join("occupied.wicdemo");
        fs::write(&occupied, b"occupied").expect("occupied destination");
        assert!(rename_replay(&renamed, "occupied.wicdemo").is_err());
        assert!(renamed.exists());
        assert_eq!(fs::read(occupied).expect("not overwritten"), b"occupied");
    }

    #[cfg(not(coverage))]
    #[test]
    #[ignore = "requires a private replay fixture"]
    fn configured_private_replay_changes_only_replay_name_semantics() {
        let source_path = PathBuf::from(
            std::env::var_os("WIC_REPLAY_VIEWER_SMOKE")
                .expect("private test requires WIC_REPLAY_VIEWER_SMOKE"),
        );
        let source_bytes = fs::read(&source_path).expect("private replay source");
        let original = WicReplayParser::from_bytes(&source_bytes).expect("private replay parser");
        let original_document = original.parse();
        assert!(
            original_document.game_info.replay_name.is_some(),
            "WIC_REPLAY_VIEWER_SMOKE must contain an editable ReplayName field"
        );
        let mut original_json =
            serde_json::to_value(original_document).expect("original semantic JSON");

        let directory = tempdir().expect("temp dir");
        let exported = directory.path().join("rewritten.wicdemo");
        export_replay(&source_path, &exported).expect("validated private replay export");
        change_replay_name(&exported, "WiC Replay Viewer regression")
            .expect("validated private replay name change");
        assert_eq!(
            fs::read(&source_path).expect("source after export"),
            source_bytes,
            "export must not alter private source evidence"
        );

        let rewritten = WicReplayParser::new(&exported).expect("rewritten private replay parser");
        let rewritten_document = rewritten.parse();
        assert_eq!(
            rewritten_document.game_info.replay_name.as_deref(),
            Some("WiC Replay Viewer regression")
        );
        let mut rewritten_json =
            serde_json::to_value(rewritten_document).expect("rewritten semantic JSON");
        original_json["gameInfo"]["replayName"] = serde_json::Value::Null;
        rewritten_json["gameInfo"]["replayName"] = serde_json::Value::Null;
        assert_eq!(
            rewritten_json, original_json,
            "changing ReplayName must preserve every other parsed replay value"
        );
    }
}
