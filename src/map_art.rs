//! Map overview art read at runtime from the user's own game installation.
//!
//! World in Conflict ships a 256x256 overview image per map at
//! `maps/<internal_name>/overviewmap.dds` inside its `.sdf` archives. The viewer
//! never bundles or redistributes that art: it reads the user's installed copy,
//! decodes it locally, and caches the result in the viewer's own database. A
//! user with no installation is a normal case, and every replay row falls back
//! to its procedural tile.
//!
//! The user's installation is treated as read-only evidence, exactly as the
//! research workspace treats its binaries. Nothing here writes to it.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::sdf::SdfArchive;

/// Bump when the decode or encode contract changes so cached art is rebuilt.
/// This is deliberately separate from `PARSER_CACHE_KEY`: map art has nothing to
/// do with parser output and must never invalidate cached replay detail.
pub const MAP_ART_CACHE_KEY: &str = "overviewmap/dxt1/png-rgb8/terrain-bounds/command-point-loc/v5";

const DDS_HEADER_SIZE: usize = 128;
const OVERVIEW_BASENAME: &str = "overviewmap.dds";
const HEIGHTMAP_BASENAME: &str = "heightmap.raw";
const HEIGHTMAP_BYTES_PER_SAMPLE: u32 = 2;
const TERRAIN_UNITS_PER_INTERVAL: u32 = 3;
const MAX_DISCOVERED_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_DISCOVERED_ARCHIVES: usize = 1_024;
const MAX_SCANNED_ARCHIVE_ENTRIES: usize = 1_000_000;
const MAX_RETAINED_ARCHIVE_PATH_BYTES: usize = 128 * 1024 * 1024;
const MAX_SELECTED_MAPS: usize = 2_048;
const MAX_MAP_ART_ITEMS: usize = 512;
const MAX_MAP_ART_SOURCE_BYTES: usize = 256 * 1024 * 1024;
const MAX_MAP_ART_PNG_BYTES: usize = 128 * 1024 * 1024;

/// World-space rectangle represented by the cached overview image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapBounds {
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
}

#[derive(Debug, Clone)]
pub struct MapArt {
    pub map_name: String,
    pub source_archive: String,
    pub png: Vec<u8>,
    pub bounds: Option<MapBounds>,
    pub command_point_names: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapAssetKind {
    Overview,
    Heightmap,
    Localization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedEntry {
    archive_index: usize,
    entry_index: usize,
}

#[derive(Debug, Default)]
struct MapAssets {
    overview: Option<SelectedEntry>,
    heightmap: Option<SelectedEntry>,
    localization: Option<SelectedEntry>,
}

/// Where map art can be read from. Both are optional and independent: the game
/// folder moves with the installation, while custom maps always live under the
/// user's Documents folder regardless of where the game is installed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtSources {
    pub install: Option<PathBuf>,
    pub custom_maps: Option<PathBuf>,
}

impl ArtSources {
    pub fn is_empty(&self) -> bool {
        self.install.is_none() && self.custom_maps.is_none()
    }

    /// Identifies the exact inputs, so changing either folder rebuilds the cache.
    pub fn signature(&self) -> String {
        let text = |path: &Option<PathBuf>| {
            path.as_ref()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        format!(
            "{MAP_ART_CACHE_KEY}|game={}|custom={}",
            text(&self.install),
            text(&self.custom_maps)
        )
    }
}

/// A directory is a game installation if it holds at least one numbered archive.
pub fn is_game_install(directory: &Path) -> bool {
    !shipped_archives(directory).is_empty()
}

/// A custom map folder is any directory holding `.sdf` archives. Community maps
/// are distributed as ordinary archives named after the map, not as `wic<n>.sdf`.
pub fn is_custom_maps_dir(directory: &Path) -> bool {
    !custom_archives(directory).is_empty()
}

/// Resolve a user-chosen directory to an installation, allowing one level of
/// slack so picking a parent such as `steamapps/common` still works.
pub fn resolve_install(directory: &Path) -> Option<PathBuf> {
    resolve_directory(directory, is_game_install)
}

/// The same slack for custom maps, so picking `World in Conflict` or
/// `Downloaded` resolves to the `maps` folder beneath it.
pub fn resolve_custom_maps(directory: &Path) -> Option<PathBuf> {
    if is_custom_maps_dir(directory) {
        return Some(directory.to_path_buf());
    }
    // The canonical layout, from either of the two levels above it.
    for relative in ["maps", "Downloaded/maps"] {
        let candidate = join_relative(directory, relative);
        if is_custom_maps_dir(&candidate) {
            return Some(candidate);
        }
    }
    resolve_directory(directory, is_custom_maps_dir)
}

fn resolve_directory(directory: &Path, accept: fn(&Path) -> bool) -> Option<PathBuf> {
    if accept(directory) {
        return Some(directory.to_path_buf());
    }
    let mut children: Vec<PathBuf> = std::fs::read_dir(directory)
        .ok()?
        .flatten()
        .take(MAX_DISCOVERED_DIRECTORY_ENTRIES)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    children.into_iter().find(|child| accept(child))
}

/// Join a `/`-separated relative path using the platform's own separator.
fn join_relative(base: &Path, relative: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for part in relative.split('/') {
        path.push(part);
    }
    path
}

/// Best-effort detection of a likely installation.
pub fn detect_install() -> Option<PathBuf> {
    candidate_install_roots()
        .into_iter()
        .find(|candidate| is_game_install(candidate))
}

/// Best-effort detection of the custom map folder.
///
/// Community maps always live at `Documents/World in Conflict/Downloaded/maps`
/// relative to the user profile, wherever the game itself is installed. On Wine
/// and Proton that profile sits inside a prefix, so an installation path is also
/// searched upwards for its own `drive_c`.
pub fn detect_custom_maps(install: Option<&Path>) -> Option<PathBuf> {
    world_in_conflict_documents(install)
        .into_iter()
        .map(|root| join_relative(&root, "Downloaded/maps"))
        .find(|candidate| is_custom_maps_dir(candidate))
}

/// The game's default replay folder, beside the custom map folder.
pub fn detect_replay_directory(install: Option<&Path>) -> Option<PathBuf> {
    world_in_conflict_documents(install)
        .into_iter()
        .map(|root| root.join("Replay"))
        .find(|candidate| candidate.is_dir())
}

/// Candidate `Documents/World in Conflict` folders, most specific first.
fn world_in_conflict_documents(install: Option<&Path>) -> Vec<PathBuf> {
    documents_roots(install, home_directory())
}

/// The home directory is a parameter so the Wine and Proton layouts can be
/// exercised against a synthetic prefix without mutating process environment.
fn documents_roots(install: Option<&Path>, home: Option<PathBuf>) -> Vec<PathBuf> {
    const TITLE: &str = "World in Conflict";
    let mut roots: Vec<PathBuf> = Vec::new();

    // A Wine or Proton prefix that contains the installation itself is the most
    // reliable source, because it is the profile the game actually writes to.
    if let Some(install) = install {
        for prefix in install.ancestors() {
            if prefix.file_name().is_some_and(|name| name == "drive_c") {
                roots.extend(prefix_documents(prefix, TITLE));
                break;
            }
        }
    }

    if let Some(home) = home {
        // Native Windows, including the common OneDrive redirection.
        roots.push(join_relative(&home, "Documents").join(TITLE));
        roots.push(join_relative(&home, "OneDrive/Documents").join(TITLE));

        // Plain Wine prefixes.
        for prefix in [".wine/drive_c", ".wine32/drive_c"] {
            roots.extend(prefix_documents(&join_relative(&home, prefix), TITLE));
        }

        // Proton per-title prefixes under Steam.
        for steam in [
            ".steam/steam/steamapps/compatdata",
            ".local/share/Steam/steamapps/compatdata",
        ] {
            let compatdata = join_relative(&home, steam);
            let Ok(entries) = std::fs::read_dir(&compatdata) else {
                continue;
            };
            let mut prefixes: Vec<PathBuf> = entries
                .flatten()
                .take(MAX_DISCOVERED_DIRECTORY_ENTRIES)
                .map(|entry| join_relative(&entry.path(), "pfx/drive_c"))
                .filter(|path| path.is_dir())
                .collect();
            prefixes.sort();
            for prefix in prefixes {
                roots.extend(prefix_documents(&prefix, TITLE));
            }
        }
    }

    roots.retain(|root| root.is_dir());
    roots.dedup();
    roots
}

/// `drive_c/users/<profile>/Documents/<title>` for each profile in a prefix.
fn prefix_documents(drive_c: &Path, title: &str) -> Vec<PathBuf> {
    let users = drive_c.join("users");
    let Ok(entries) = std::fs::read_dir(&users) else {
        return Vec::new();
    };
    let mut profiles: Vec<PathBuf> = entries
        .flatten()
        .take(MAX_DISCOVERED_DIRECTORY_ENTRIES)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    // `steamuser` is Proton's fixed profile name; prefer it when present.
    profiles.sort_by_key(|path| {
        let is_steam_user = path.file_name().is_some_and(|name| name == "steamuser");
        (!is_steam_user, path.clone())
    });
    let mut roots = Vec::new();
    for profile in profiles {
        roots.push(join_relative(&profile, "Documents").join(title));
        roots.push(join_relative(&profile, "My Documents").join(title));
    }
    roots
}

fn candidate_install_roots() -> Vec<PathBuf> {
    const RELATIVE: [&str; 6] = [
        ".steam/steam/steamapps/common/World in Conflict",
        ".local/share/Steam/steamapps/common/World in Conflict",
        ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/World in Conflict",
        ".wine/drive_c/Program Files (x86)/Ubisoft/World in Conflict",
        ".wine/drive_c/Program Files (x86)/Sierra Entertainment/World in Conflict",
        ".wine/drive_c/GOG Games/World in Conflict Complete Edition",
    ];
    const ABSOLUTE: [&str; 6] = [
        r"C:\Program Files (x86)\Steam\steamapps\common\World in Conflict",
        r"C:\Program Files (x86)\Ubisoft\World in Conflict",
        r"C:\Program Files (x86)\Sierra Entertainment\World in Conflict",
        r"C:\Program Files (x86)\Massive Entertainment\World in Conflict",
        r"C:\GOG Games\World in Conflict Complete Edition",
        r"C:\Program Files\GOG Galaxy\Games\World in Conflict Complete Edition",
    ];

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = home_directory() {
        roots.extend(
            RELATIVE
                .iter()
                .map(|relative| join_relative(&home, relative)),
        );
    }
    roots.extend(ABSOLUTE.iter().map(PathBuf::from));
    roots
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Every `.sdf` in a directory, ordered so a later archive overrides an earlier
/// one for the same map.
///
/// Observed in the shipped corpus: `russia3`, `usfarmland1`, and `usfarmland3`
/// each appear in both a base and a patch archive, and in all three cases the
/// higher-numbered archive carries a deliberate revision rather than a
/// recompression. Archives are therefore applied in ascending numeric order. Any
/// archive without a trailing number sorts last, which is also how community
/// maps are named.
fn sdf_archives(directory: &Path, shipped_only: bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut archives: Vec<(u64, u64, String, PathBuf)> = Vec::new();
    for entry in entries.flatten().take(MAX_DISCOVERED_DIRECTORY_ENTRIES) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let lowered = name.to_ascii_lowercase();
        if !lowered.ends_with(".sdf") {
            continue;
        }
        if shipped_only && !lowered.starts_with("wic") {
            continue;
        }
        if archives.len() >= MAX_DISCOVERED_ARCHIVES {
            break;
        }
        let stem = &lowered[..lowered.len() - 4];
        let digits: String = stem
            .chars()
            .skip_while(|character| !character.is_ascii_digit())
            .collect();
        match digits.parse::<u64>() {
            Ok(number) if !digits.is_empty() => archives.push((0, number, lowered, path)),
            _ => archives.push((1, 0, lowered, path)),
        }
    }
    archives.sort();
    archives.into_iter().map(|(_, _, _, path)| path).collect()
}

fn shipped_archives(directory: &Path) -> Vec<PathBuf> {
    sdf_archives(directory, true)
}

fn custom_archives(directory: &Path) -> Vec<PathBuf> {
    sdf_archives(directory, false)
}

fn account_opened_archive_storage(
    total_entries: &mut usize,
    total_path_bytes: &mut usize,
    entry_count: usize,
    path_bytes: usize,
) -> Result<(), String> {
    let next_entries = total_entries
        .checked_add(entry_count)
        .ok_or_else(|| "Map archive entry count overflows".to_owned())?;
    if next_entries > MAX_SCANNED_ARCHIVE_ENTRIES {
        return Err(format!(
            "Map archives exceed the maximum of {MAX_SCANNED_ARCHIVE_ENTRIES} retained entries"
        ));
    }
    let next_path_bytes = total_path_bytes
        .checked_add(path_bytes)
        .ok_or_else(|| "Map archive path storage overflows".to_owned())?;
    if next_path_bytes > MAX_RETAINED_ARCHIVE_PATH_BYTES {
        return Err(format!(
            "Map archive paths exceed the {} MiB aggregate retained limit",
            MAX_RETAINED_ARCHIVE_PATH_BYTES / 1024 / 1024
        ));
    }
    *total_entries = next_entries;
    *total_path_bytes = next_path_bytes;
    Ok(())
}

fn account_decoded_source_bytes(total: &mut usize, bytes: u32) -> Result<(), String> {
    *total = total
        .checked_add(bytes as usize)
        .ok_or_else(|| "Decoded map-art source size overflows".to_owned())?;
    if *total > MAX_MAP_ART_SOURCE_BYTES {
        return Err(format!(
            "Decoded map-art sources exceed the {} MiB aggregate work limit",
            MAX_MAP_ART_SOURCE_BYTES / 1024 / 1024
        ));
    }
    Ok(())
}

/// Decode every map overview image the given sources provide.
///
/// Shipped archives are read first and custom maps second, so a community map
/// overrides a shipped one of the same name. Missing or unreadable archives are
/// skipped rather than failing the scan: a partial installation should still
/// show the art it does have.
pub fn collect_map_art(sources: &ArtSources) -> Result<Vec<MapArt>, String> {
    let mut archives: Vec<PathBuf> = Vec::new();
    if let Some(install) = &sources.install {
        let shipped = shipped_archives(install);
        if shipped.is_empty() {
            return Err(format!(
                "No World in Conflict archives found in {}",
                install.display()
            ));
        }
        archives.extend(shipped);
    }
    if let Some(custom) = &sources.custom_maps {
        archives.extend(custom_archives(custom));
    }
    if archives.is_empty() {
        return Err("No map archives to read".to_owned());
    }

    let mut opened = Vec::new();
    let mut retained_entries = 0usize;
    let mut retained_path_bytes = 0usize;
    for path in archives {
        let Ok(archive) = SdfArchive::open(&path) else {
            continue;
        };
        let path_bytes = archive.entries().iter().try_fold(0usize, |total, entry| {
            total
                .checked_add(entry.path.len())
                .ok_or_else(|| "Map archive path storage overflows".to_owned())
        })?;
        account_opened_archive_storage(
            &mut retained_entries,
            &mut retained_path_bytes,
            archive.entries().len(),
            path_bytes,
        )?;
        opened.push((path, archive));
    }

    // Resolve each path independently. Later numbered archives and then custom
    // maps replace only the files they actually contain, matching the game's
    // effective archive view instead of coupling metadata to the art archive.
    let mut selected: BTreeMap<String, MapAssets> = BTreeMap::new();
    let mut scanned_entries = 0usize;
    for (archive_index, (_, archive)) in opened.iter().enumerate() {
        for (entry_index, entry) in archive.entries().iter().enumerate() {
            scanned_entries = scanned_entries
                .checked_add(1)
                .ok_or_else(|| "Map archive entry count overflows".to_owned())?;
            if scanned_entries > MAX_SCANNED_ARCHIVE_ENTRIES {
                return Err(format!(
                    "Map archives exceed the maximum of {MAX_SCANNED_ARCHIVE_ENTRIES} scanned entries"
                ));
            }
            let Some((map_name, kind)) = map_asset(&entry.path) else {
                continue;
            };
            let normalized = map_name.to_ascii_lowercase();
            if !selected.contains_key(&normalized) && selected.len() >= MAX_SELECTED_MAPS {
                return Err(format!(
                    "Map archives exceed the maximum of {MAX_SELECTED_MAPS} selected maps"
                ));
            }
            record_map_asset(
                &mut selected,
                map_name,
                kind,
                SelectedEntry {
                    archive_index,
                    entry_index,
                },
            );
        }
    }

    let mut art = Vec::new();
    let mut total_source_bytes = 0usize;
    let mut total_png_bytes = 0usize;
    for (map_name, assets) in selected {
        let Some(overview) = assets.overview else {
            continue;
        };
        if art.len() >= MAX_MAP_ART_ITEMS {
            return Err(format!(
                "Map archives exceed the maximum of {MAX_MAP_ART_ITEMS} decoded overview images"
            ));
        }
        let (archive_path, archive) = &opened[overview.archive_index];
        let overview_entry = &archive.entries()[overview.entry_index];
        account_decoded_source_bytes(&mut total_source_bytes, overview_entry.uncompressed_size)?;
        let Ok(decoded) = archive.read_entry(overview_entry) else {
            continue;
        };
        let Ok(png) = dds_to_png(&decoded) else {
            continue;
        };
        total_png_bytes = total_png_bytes
            .checked_add(png.len())
            .ok_or_else(|| "Decoded map-art size overflows".to_owned())?;
        if total_png_bytes > MAX_MAP_ART_PNG_BYTES {
            return Err(format!(
                "Decoded map art exceeds the {} MiB aggregate limit",
                MAX_MAP_ART_PNG_BYTES / 1024 / 1024
            ));
        }
        let bounds = assets.heightmap.and_then(|heightmap| {
            let entry = &opened[heightmap.archive_index].1.entries()[heightmap.entry_index];
            terrain_bounds(entry.uncompressed_size).ok()
        });
        let command_point_names = if let Some(localization) = assets.localization {
            let archive = &opened[localization.archive_index].1;
            let entry = &archive.entries()[localization.entry_index];
            account_decoded_source_bytes(&mut total_source_bytes, entry.uncompressed_size)?;
            archive
                .read_entry(entry)
                .ok()
                .map_or_else(BTreeMap::new, |data| command_point_names(&data))
        } else {
            BTreeMap::new()
        };
        art.push(MapArt {
            map_name,
            source_archive: archive_path.to_string_lossy().into_owned(),
            png,
            bounds,
            command_point_names,
        });
    }
    Ok(art)
}

fn record_map_asset(
    selected: &mut BTreeMap<String, MapAssets>,
    map_name: &str,
    kind: MapAssetKind,
    entry: SelectedEntry,
) {
    let assets = selected.entry(map_name.to_ascii_lowercase()).or_default();
    match kind {
        MapAssetKind::Overview => assets.overview = Some(entry),
        MapAssetKind::Heightmap => assets.heightmap = Some(entry),
        MapAssetKind::Localization => assets.localization = Some(entry),
    }
}

/// Read the map's own localized `CommandPoint__*` UI labels. The replay stores
/// only the Adler-32 object ID, while `.loc` stores tab-separated property/value
/// rows such as `...CommandPoint__0.myUiName<TAB>Space Needle`.
fn command_point_names(data: &[u8]) -> BTreeMap<u32, String> {
    const MAX_COMMAND_POINTS: usize = 256;
    const MAX_LABEL_BYTES: usize = 128;

    let data = data.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(data);
    let Ok(text) = std::str::from_utf8(data) else {
        return BTreeMap::new();
    };
    let mut names = BTreeMap::new();
    for line in text.lines() {
        if names.len() >= MAX_COMMAND_POINTS {
            break;
        }
        let Some((property_path, raw_label)) = line.split_once('\t') else {
            continue;
        };
        let Some((object_path, property)) = property_path.rsplit_once('.') else {
            continue;
        };
        if property != "myUiName" {
            continue;
        }
        let Some(object_name) = object_path.rsplit('.').next() else {
            continue;
        };
        if !object_name.starts_with("CommandPoint__") {
            continue;
        }
        let label = raw_label.trim();
        if label.is_empty() || label.len() > MAX_LABEL_BYTES || label.chars().any(char::is_control)
        {
            continue;
        }
        names
            .entry(adler32(object_name.as_bytes()))
            .or_insert_with(|| label.to_owned());
    }
    names
}

fn adler32(bytes: &[u8]) -> u32 {
    let mut a = 1_u32;
    let mut b = 0_u32;
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// Derive the exact terrain rectangle represented by `overviewmap.dds` from
/// the associated 16-bit square heightmap. Heightmap samples are spaced three
/// world units apart, so N samples describe N-1 terrain intervals.
fn terrain_bounds(uncompressed_size: u32) -> Result<MapBounds, String> {
    if !uncompressed_size.is_multiple_of(HEIGHTMAP_BYTES_PER_SAMPLE) {
        return Err("Heightmap size is not aligned to 16-bit samples".to_owned());
    }
    let sample_count = uncompressed_size / HEIGHTMAP_BYTES_PER_SAMPLE;
    let side = sample_count.isqrt();
    if side < 2 || side.checked_mul(side) != Some(sample_count) {
        return Err("Heightmap samples do not form a square grid".to_owned());
    }
    let extent = (side - 1)
        .checked_mul(TERRAIN_UNITS_PER_INTERVAL)
        .ok_or_else(|| "Heightmap terrain extent overflows".to_owned())?;
    Ok(MapBounds {
        min_x: 0.0,
        min_z: 0.0,
        max_x: extent as f32,
        max_z: extent as f32,
    })
}

/// Reduce a replay summary's map field to the internal name art is keyed by.
///
/// Summaries carry the map as an archive path such as
/// `maps/ustown4/ustown4.ice`, not as the bare `ustown4` that names the art
/// directory. Anything that is already a bare name is returned unchanged, so
/// this is safe to apply to either form.
pub fn internal_map_name(raw: &str) -> &str {
    let trimmed = raw.trim();
    let mut parts = trimmed.split(['/', '\\']).filter(|part| !part.is_empty());
    match (parts.next(), parts.next()) {
        (Some(root), Some(name)) if root.eq_ignore_ascii_case("maps") => name,
        _ => trimmed,
    }
}

/// The runtime map assets needed for overview projection and objective labels.
fn map_asset(entry_path: &str) -> Option<(&str, MapAssetKind)> {
    let mut parts = entry_path.split('/');
    let root = parts.next()?;
    let map_name = parts.next()?;
    let basename = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if !root.eq_ignore_ascii_case("maps") {
        return None;
    }
    let kind = if basename.eq_ignore_ascii_case(OVERVIEW_BASENAME) {
        MapAssetKind::Overview
    } else if basename.eq_ignore_ascii_case(HEIGHTMAP_BASENAME) {
        MapAssetKind::Heightmap
    } else if basename.eq_ignore_ascii_case(&format!("{map_name}.loc")) {
        MapAssetKind::Localization
    } else {
        return None;
    };
    (!map_name.is_empty()).then_some((map_name, kind))
}

/// Decode the top mip level of a DXT1 DDS and re-encode it as an RGB PNG.
///
/// Every shipped overview map is 256x256 DXT1. Anything else is rejected rather
/// than guessed at, which leaves the row on its procedural tile.
pub fn dds_to_png(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < DDS_HEADER_SIZE || &data[..4] != b"DDS " {
        return Err("Not a DDS file".to_owned());
    }
    let height = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    let width = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    if &data[84..88] != b"DXT1" {
        return Err("Overview art is not DXT1".to_owned());
    }
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return Err(format!("Implausible overview dimensions {width}x{height}"));
    }

    let blocks_wide = width.div_ceil(4) as usize;
    let blocks_high = height.div_ceil(4) as usize;
    let needed = blocks_wide * blocks_high * 8;
    let payload = data
        .get(DDS_HEADER_SIZE..DDS_HEADER_SIZE + needed)
        .ok_or_else(|| "Overview art is shorter than its top mip level".to_owned())?;

    let pixels = decode_dxt1(payload, width as usize, height as usize, blocks_wide);
    encode_png(&pixels, width, height)
}

fn expand565(color: u16) -> [u8; 3] {
    let red = ((color >> 11) & 0x1F) as u8;
    let green = ((color >> 5) & 0x3F) as u8;
    let blue = (color & 0x1F) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
    ]
}

fn decode_dxt1(payload: &[u8], width: usize, height: usize, blocks_wide: usize) -> Vec<u8> {
    let mut pixels = vec![0_u8; width * height * 3];
    for (block_index, block) in payload.as_chunks::<8>().0.iter().enumerate() {
        let block_x = (block_index % blocks_wide) * 4;
        let block_y = (block_index / blocks_wide) * 4;
        let color0 = u16::from_le_bytes([block[0], block[1]]);
        let color1 = u16::from_le_bytes([block[2], block[3]]);
        let bits = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
        let first = expand565(color0);
        let second = expand565(color1);

        let mut palette = [[0_u8; 3]; 4];
        palette[0] = first;
        palette[1] = second;
        if color0 > color1 {
            for channel in 0..3 {
                palette[2][channel] =
                    ((2 * u16::from(first[channel]) + u16::from(second[channel])) / 3) as u8;
                palette[3][channel] =
                    ((u16::from(first[channel]) + 2 * u16::from(second[channel])) / 3) as u8;
            }
        } else {
            for channel in 0..3 {
                palette[2][channel] =
                    ((u16::from(first[channel]) + u16::from(second[channel])) / 2) as u8;
            }
            // The fourth entry is transparent black in DXT1a. Overview maps are
            // opaque, so drawing it as black is the honest reading.
            palette[3] = [0, 0, 0];
        }

        for row in 0..4 {
            for column in 0..4 {
                let x = block_x + column;
                let y = block_y + row;
                if x >= width || y >= height {
                    continue;
                }
                let selector = ((bits >> (2 * (row * 4 + column))) & 0x3) as usize;
                let offset = (y * width + x) * 3;
                pixels[offset..offset + 3].copy_from_slice(&palette[selector]);
            }
        }
    }
    pixels
}

/// Minimal RGB8 PNG encoder.
///
/// Hand-rolled rather than pulling in an image crate: the output is one fixed
/// shape (8-bit truecolour, no interlacing, filter 0) and this keeps the
/// viewer's dependency surface unchanged apart from the zlib codec it already
/// builds for the parser.
fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut raw = Vec::with_capacity(pixels.len() + height as usize);
    for row in pixels.chunks_exact(width as usize * 3) {
        raw.push(0); // filter type 0 (None)
        raw.extend_from_slice(row);
    }

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&raw)
        .map_err(|error| format!("Cannot compress overview art: {error}"))?;
    let compressed = encoder
        .finish()
        .map_err(|error| format!("Cannot finish overview art: {error}"))?;

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut png = Vec::with_capacity(compressed.len() + 64);
    png.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    write_chunk(&mut png, b"IHDR", &header);
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    Ok(png)
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    output.extend_from_slice(&crc.sum().to_be_bytes());
}

/// Encode PNG bytes as a `data:` URL for direct use in an `<img>` element.
pub fn png_data_url(png: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(png.len().div_ceil(3) * 4 + 22);
    encoded.push_str("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let bytes = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let value = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        encoded.push(ALPHABET[(value >> 18) as usize & 0x3F] as char);
        encoded.push(ALPHABET[(value >> 12) as usize & 0x3F] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(value >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[value as usize & 0x3F] as char
        } else {
            '='
        });
    }
    encoded
}

/// Rebuild the cached art only when a source folder or the decode contract
/// changed. Startup on unchanged folders therefore costs one string comparison
/// rather than a rescan.
pub fn ensure_map_art(
    database: &mut crate::database::Database,
    sources: &ArtSources,
) -> Result<usize, String> {
    let signature = sources.signature();
    if database.map_art_is_current(&signature)? {
        return database.map_art_count();
    }
    if sources.is_empty() {
        database.clear_map_art()?;
        return Ok(0);
    }
    let art = collect_map_art(sources)?;
    database.replace_map_art(&signature, &art)?;
    Ok(art.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_storage_before_retaining_another_open_archive() {
        let mut entries = 0;
        let mut path_bytes = 0;
        account_opened_archive_storage(
            &mut entries,
            &mut path_bytes,
            MAX_SCANNED_ARCHIVE_ENTRIES,
            MAX_RETAINED_ARCHIVE_PATH_BYTES,
        )
        .expect("aggregate storage at limits");
        assert!(
            account_opened_archive_storage(&mut entries, &mut path_bytes, 1, 0)
                .expect_err("entry over limit")
                .contains("retained entries")
        );

        let mut entries = 0;
        let mut path_bytes = MAX_RETAINED_ARCHIVE_PATH_BYTES;
        assert!(
            account_opened_archive_storage(&mut entries, &mut path_bytes, 0, 1)
                .expect_err("path bytes over limit")
                .contains("aggregate retained limit")
        );

        let mut source_bytes = MAX_MAP_ART_SOURCE_BYTES - 1;
        account_decoded_source_bytes(&mut source_bytes, 1).expect("source bytes at limit");
        assert!(
            account_decoded_source_bytes(&mut source_bytes, 1)
                .expect_err("decoded source bytes over limit")
                .contains("aggregate work limit")
        );
    }

    fn dxt1_dds(width: u32, height: u32, color0: u16, color1: u16, indices: u32) -> Vec<u8> {
        let mut data = vec![0_u8; DDS_HEADER_SIZE];
        data[..4].copy_from_slice(b"DDS ");
        data[4..8].copy_from_slice(&124_u32.to_le_bytes());
        data[12..16].copy_from_slice(&height.to_le_bytes());
        data[16..20].copy_from_slice(&width.to_le_bytes());
        data[84..88].copy_from_slice(b"DXT1");
        for _ in 0..width.div_ceil(4) * height.div_ceil(4) {
            data.extend(color0.to_le_bytes());
            data.extend(color1.to_le_bytes());
            data.extend(indices.to_le_bytes());
        }
        data
    }

    #[test]
    fn derives_authoritative_terrain_bounds() {
        assert_eq!(
            terrain_bounds(526_338),
            Ok(MapBounds {
                min_x: 0.0,
                min_z: 0.0,
                max_x: 1536.0,
                max_z: 1536.0
            })
        );
    }

    #[test]
    fn rejects_ambiguous_heightmap_sizes() {
        assert!(terrain_bounds(526_337).is_err(), "odd byte count");
        assert!(terrain_bounds(526_336).is_err(), "non-square sample count");
        assert!(terrain_bounds(2).is_err(), "single sample has no interval");
    }

    #[test]
    fn reduces_summary_map_paths_to_internal_names() {
        // The exact shape stored in replay summaries.
        assert_eq!(internal_map_name("maps/ustown4/ustown4.ice"), "ustown4");
        assert_eq!(
            internal_map_name("maps/do_farmland_night/do_farmland_night.ice"),
            "do_farmland_night"
        );
        // Already-bare names pass through untouched.
        assert_eq!(internal_map_name("russia3"), "russia3");
        assert_eq!(internal_map_name("  russia3  "), "russia3");
        // Windows separators and odd input do not break the reduction.
        assert_eq!(internal_map_name(r"maps\ustown4\ustown4.ice"), "ustown4");
        assert_eq!(internal_map_name("maps"), "maps");
        assert_eq!(internal_map_name(""), "");
    }

    #[test]
    fn accepts_only_projection_asset_paths() {
        assert_eq!(
            map_asset("maps/russia3/overviewmap.dds"),
            Some(("russia3", MapAssetKind::Overview))
        );
        assert_eq!(
            map_asset("maps/russia3/heightmap.raw"),
            Some(("russia3", MapAssetKind::Heightmap))
        );
        assert_eq!(
            map_asset("maps/russia3/russia3.loc"),
            Some(("russia3", MapAssetKind::Localization))
        );
        assert_eq!(map_asset("maps/russia3/other.loc"), None);
        assert_eq!(map_asset("maps/russia3/minimap.dds"), None);
        assert_eq!(map_asset("gui/russia3/overviewmap.dds"), None);
        assert_eq!(map_asset("maps/overviewmap.dds"), None);
        assert_eq!(map_asset("maps/a/b/overviewmap.dds"), None);
    }

    #[test]
    fn resolves_localized_command_point_object_names() {
        let names = command_point_names(
            b"\xef\xbb\xbfmyMap.CommandPoint__0.myUiName\tSpace Needle\r\n\
              myMap.CommandPoint__1.myUiName\tTunnel Entrance\r\n\
              myMap.CommandPoint__1.myWeCapturedText\tWe captured it\r\n\
              myMap.Other__0.myUiName\tNot a command point\r\n",
        );
        assert_eq!(
            names.get(&0x2eae_05b8).map(String::as_str),
            Some("Space Needle")
        );
        assert_eq!(
            names.get(&0x2eaf_05b9).map(String::as_str),
            Some("Tunnel Entrance")
        );
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn resolves_map_asset_precedence_independently() {
        let mut selected = BTreeMap::new();
        let overview = SelectedEntry {
            archive_index: 0,
            entry_index: 4,
        };
        let patched_heightmap = SelectedEntry {
            archive_index: 2,
            entry_index: 7,
        };
        let localization = SelectedEntry {
            archive_index: 3,
            entry_index: 8,
        };
        record_map_asset(&mut selected, "RUSSIA3", MapAssetKind::Overview, overview);
        record_map_asset(
            &mut selected,
            "russia3",
            MapAssetKind::Heightmap,
            SelectedEntry {
                archive_index: 0,
                entry_index: 5,
            },
        );
        record_map_asset(
            &mut selected,
            "russia3",
            MapAssetKind::Heightmap,
            patched_heightmap,
        );
        record_map_asset(
            &mut selected,
            "russia3",
            MapAssetKind::Localization,
            localization,
        );

        let assets = &selected["russia3"];
        assert_eq!(assets.overview, Some(overview));
        assert_eq!(assets.heightmap, Some(patched_heightmap));
        assert_eq!(assets.localization, Some(localization));
    }

    #[test]
    fn orders_archives_so_later_patches_win() {
        let directory = tempfile::tempdir().unwrap();
        for name in [
            "wic2.sdf",
            "wic60.sdf",
            "wic3.sdf",
            "wic45.sdf",
            "wicmod.sdf",
            "notes.txt",
        ] {
            std::fs::write(directory.path().join(name), b"x").unwrap();
        }
        let ordered: Vec<String> = shipped_archives(directory.path())
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            ordered,
            [
                "wic2.sdf",
                "wic3.sdf",
                "wic45.sdf",
                "wic60.sdf",
                "wicmod.sdf"
            ],
            "ascending archive number, unnumbered content last"
        );

        // Community maps are plain `.sdf` files with no `wic` prefix, so the
        // shipped listing must ignore them while the custom listing keeps them.
        std::fs::write(directory.path().join("do_Alaska.sdf"), b"x").unwrap();
        let shipped = shipped_archives(directory.path());
        assert!(!shipped.iter().any(|path| path.ends_with("do_Alaska.sdf")));
        assert!(
            custom_archives(directory.path())
                .iter()
                .any(|path| path.ends_with("do_Alaska.sdf"))
        );
    }

    #[test]
    fn recognises_an_install_by_its_archives() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!is_game_install(directory.path()));
        std::fs::write(directory.path().join("wic1.sdf"), b"x").unwrap();
        assert!(is_game_install(directory.path()));
    }

    #[test]
    fn resolves_an_install_one_level_below_the_chosen_folder() {
        let parent = tempfile::tempdir().unwrap();
        let install = parent.path().join("World in Conflict");
        std::fs::create_dir(&install).unwrap();
        std::fs::write(install.join("wic1.sdf"), b"x").unwrap();
        assert_eq!(resolve_install(parent.path()), Some(install));
        assert_eq!(resolve_install(&parent.path().join("missing")), None);
    }

    #[test]
    fn recognises_a_custom_maps_folder() {
        let directory = tempfile::tempdir().unwrap();
        let maps = join_relative(directory.path(), "Downloaded/maps");
        std::fs::create_dir_all(&maps).unwrap();
        assert!(!is_custom_maps_dir(directory.path()));
        // A community map is named after the map, not `wic<n>.sdf`.
        std::fs::write(maps.join("do_Alaska.sdf"), b"x").unwrap();
        assert!(is_custom_maps_dir(&maps));
        assert!(!is_game_install(&maps), "custom maps are not an install");

        // Picking `World in Conflict` or `Downloaded` resolves down to `maps`.
        assert_eq!(
            resolve_custom_maps(directory.path()).as_deref(),
            Some(maps.as_path())
        );
        assert_eq!(
            resolve_custom_maps(&directory.path().join("Downloaded")).as_deref(),
            Some(maps.as_path())
        );
        assert_eq!(resolve_custom_maps(&maps).as_deref(), Some(maps.as_path()));
    }

    #[test]
    fn signature_changes_with_either_folder() {
        let game_only = ArtSources {
            install: Some(PathBuf::from("/games/wic")),
            custom_maps: None,
        };
        let both = ArtSources {
            install: Some(PathBuf::from("/games/wic")),
            custom_maps: Some(PathBuf::from("/docs/maps")),
        };
        let moved_game = ArtSources {
            install: Some(PathBuf::from("/elsewhere/wic")),
            custom_maps: Some(PathBuf::from("/docs/maps")),
        };
        assert_ne!(game_only.signature(), both.signature());
        assert_ne!(both.signature(), moved_game.signature());
        assert_eq!(both.signature(), both.clone().signature());
        assert!(ArtSources::default().is_empty());
        assert!(!game_only.is_empty());
    }

    #[test]
    fn decodes_a_dxt1_block_to_its_first_colour() {
        // color0 > color1 selects the opaque four-colour mode; every index is 0,
        // so every pixel takes color0, pure red in RGB565.
        let dds = dxt1_dds(4, 4, 0xF800, 0x001F, 0);
        let png = dds_to_png(&dds).expect("decodes");
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 4);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 4);
        assert_eq!(png[24], 8, "8 bits per channel");
        assert_eq!(png[25], 2, "truecolour RGB");

        let pixels = decode_dxt1(&dds[DDS_HEADER_SIZE..], 4, 4, 1);
        assert_eq!(pixels.len(), 4 * 4 * 3);
        assert!(
            pixels
                .as_chunks::<3>()
                .0
                .iter()
                .all(|pixel| pixel == &[255, 0, 0])
        );
    }

    #[test]
    fn rejects_art_it_cannot_read() {
        assert!(dds_to_png(b"not a dds file at all").is_err());
        let mut wrong_format = dxt1_dds(4, 4, 0xF800, 0x001F, 0);
        wrong_format[84..88].copy_from_slice(b"DXT5");
        assert!(dds_to_png(&wrong_format).is_err());
        let truncated = dxt1_dds(64, 64, 0xF800, 0x001F, 0)[..DDS_HEADER_SIZE + 16].to_vec();
        assert!(dds_to_png(&truncated).is_err());
    }

    #[test]
    fn encodes_data_urls() {
        assert_eq!(png_data_url(b""), "data:image/png;base64,");
        assert_eq!(png_data_url(b"a"), "data:image/png;base64,YQ==");
        assert_eq!(png_data_url(b"ab"), "data:image/png;base64,YWI=");
        assert_eq!(png_data_url(b"abc"), "data:image/png;base64,YWJj");
        assert_eq!(png_data_url(b"abcd"), "data:image/png;base64,YWJjZA==");
    }

    #[test]
    fn reports_a_folder_with_no_archives() {
        let directory = tempfile::tempdir().unwrap();
        let sources = ArtSources {
            install: Some(directory.path().to_path_buf()),
            custom_maps: None,
        };
        assert!(collect_map_art(&sources).is_err());
        assert!(collect_map_art(&ArtSources::default()).is_err());
    }
}

#[cfg(test)]
mod prefix_tests {
    use super::*;

    fn touch_maps(root: &Path) -> PathBuf {
        let maps = join_relative(root, "Downloaded/maps");
        std::fs::create_dir_all(&maps).unwrap();
        std::fs::write(maps.join("do_Alaska.sdf"), b"x").unwrap();
        maps
    }

    #[test]
    fn finds_documents_in_a_wine_prefix() {
        let home = tempfile::tempdir().unwrap();
        let documents = join_relative(
            home.path(),
            ".wine/drive_c/users/steamuser/Documents/World in Conflict",
        );
        std::fs::create_dir_all(&documents).unwrap();
        let maps = touch_maps(&documents);

        let roots = documents_roots(None, Some(home.path().to_path_buf()));
        assert!(
            roots.contains(&documents),
            "wine prefix not among {roots:?}"
        );
        // The full detection path resolves to the maps folder itself.
        assert_eq!(
            documents_roots(None, Some(home.path().to_path_buf()))
                .into_iter()
                .map(|root| join_relative(&root, "Downloaded/maps"))
                .find(|candidate| is_custom_maps_dir(candidate)),
            Some(maps)
        );
    }

    #[test]
    fn finds_documents_in_a_proton_compatdata_prefix() {
        let home = tempfile::tempdir().unwrap();
        let documents = join_relative(
            home.path(),
            ".steam/steam/steamapps/compatdata/1silly/pfx/drive_c/users/steamuser/Documents/World in Conflict",
        );
        std::fs::create_dir_all(&documents).unwrap();
        touch_maps(&documents);

        let roots = documents_roots(None, Some(home.path().to_path_buf()));
        assert!(
            roots.contains(&documents),
            "proton prefix not among {roots:?}"
        );
    }

    #[test]
    fn prefers_the_prefix_that_holds_the_installation() {
        // A game inside one prefix must not pick up a different prefix's profile.
        let home = tempfile::tempdir().unwrap();
        let drive_c = join_relative(home.path(), "elsewhere/pfx/drive_c");
        let install = join_relative(&drive_c, "GOG Games/World in Conflict");
        std::fs::create_dir_all(&install).unwrap();
        let documents = join_relative(&drive_c, "users/steamuser/Documents/World in Conflict");
        std::fs::create_dir_all(&documents).unwrap();
        touch_maps(&documents);

        // The installation's own prefix is searched even though it sits under no
        // standard home-relative path, and it comes first.
        let roots = documents_roots(Some(&install), Some(home.path().to_path_buf()));
        assert_eq!(roots.first(), Some(&documents));
        assert_eq!(
            detect_custom_maps(Some(&install)),
            Some(join_relative(&documents, "Downloaded/maps"))
        );
    }

    #[test]
    fn prefers_the_steamuser_profile_over_others() {
        let home = tempfile::tempdir().unwrap();
        let drive_c = join_relative(home.path(), ".wine/drive_c");
        for profile in ["someone", "steamuser"] {
            let documents = join_relative(&drive_c, &format!("users/{profile}/Documents"))
                .join("World in Conflict");
            std::fs::create_dir_all(&documents).unwrap();
            touch_maps(&documents);
        }
        let roots = documents_roots(None, Some(home.path().to_path_buf()));
        let first = roots.first().expect("a root");
        assert!(
            first.to_string_lossy().contains("steamuser"),
            "expected steamuser first, got {first:?}"
        );
    }

    #[test]
    fn finds_the_default_replay_folder_beside_the_maps() {
        let home = tempfile::tempdir().unwrap();
        let documents = join_relative(
            home.path(),
            ".wine/drive_c/users/steamuser/Documents/World in Conflict",
        );
        std::fs::create_dir_all(documents.join("Replay")).unwrap();
        let roots = documents_roots(None, Some(home.path().to_path_buf()));
        let replay = roots
            .into_iter()
            .map(|root| root.join("Replay"))
            .find(|candidate| candidate.is_dir());
        assert_eq!(replay, Some(documents.join("Replay")));
    }
}
