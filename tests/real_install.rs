//! Ground-truth checks against a real game installation.
//!
//! These are explicitly ignored because the game is not redistributable and
//! cannot live in this repository. Invoking ignored tests without a valid
//! `WIC_GAME_INSTALL` fails closed. Run with:
//!
//! ```text
//! WIC_GAME_INSTALL=/path/to/World\ in\ Conflict cargo test --test real_install
//! ```

use std::path::{Path, PathBuf};

use wic_replay_viewer::database::Database;
use wic_replay_viewer::map_art::{
    ArtSources, collect_map_art, ensure_map_art, internal_map_name, is_custom_maps_dir,
    is_game_install, png_data_url,
};

fn install() -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os("WIC_GAME_INSTALL").expect("private test requires WIC_GAME_INSTALL"),
    );
    assert!(
        is_game_install(&path),
        "{} is not a game installation",
        path.display()
    );
    path
}

fn sources(install: &Path) -> ArtSources {
    ArtSources {
        install: Some(install.to_path_buf()),
        // Optional second source, so this suite also covers community maps when
        // the folder is available.
        custom_maps: std::env::var_os("WIC_CUSTOM_MAPS").map(PathBuf::from),
    }
}

#[test]
#[ignore = "requires a private World in Conflict installation"]
fn decodes_every_overview_map_in_a_real_installation() {
    let install = install();

    let art = collect_map_art(&sources(&install)).expect("scan succeeds");
    assert!(!art.is_empty(), "an installation should carry overview art");

    for entry in &art {
        assert_eq!(
            &entry.png[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            "{} is not a PNG",
            entry.map_name
        );
        assert_eq!(&entry.png[12..16], b"IHDR", "{}", entry.map_name);
        let width = u32::from_be_bytes(entry.png[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(entry.png[20..24].try_into().unwrap());
        assert_eq!(
            (width, height),
            (256, 256),
            "{} decoded to {width}x{height}",
            entry.map_name
        );
        let bounds = entry
            .bounds
            .unwrap_or_else(|| panic!("{} has no verified terrain bounds", entry.map_name));
        assert_eq!(
            (bounds.min_x, bounds.min_z, bounds.max_x, bounds.max_z),
            (0.0, 0.0, 1536.0, 1536.0),
            "{} has unexpected overview terrain bounds",
            entry.map_name
        );
    }

    let mut names: Vec<&str> = art.iter().map(|entry| entry.map_name.as_str()).collect();
    names.sort_unstable();
    let unique = names.len();
    names.dedup();
    assert_eq!(unique, names.len(), "precedence must leave one art per map");

    for map_name in ["europe4", "ustown4", "ustown1"] {
        assert!(
            art.iter().any(|entry| entry.map_name == map_name),
            "named command-point regression map {map_name} was not audited"
        );
    }

    let seattle = art
        .iter()
        .find(|entry| entry.map_name == "seattle1")
        .expect("Seattle ships with the game");
    assert_eq!(
        seattle
            .command_point_names
            .get(&0x2eae_05b8)
            .map(String::as_str),
        Some("Space Needle"),
        "replay command-point IDs must resolve through the installed map locale"
    );
}

/// The three maps shipped in more than one archive must resolve to the later
/// one, which is where the revised art lives.
#[test]
#[ignore = "requires a private World in Conflict installation"]
fn later_archives_win_for_duplicated_maps() {
    let install = install();

    let art = collect_map_art(&sources(&install)).expect("scan succeeds");
    for (map_name, expected_archive) in [
        ("russia3", "wic60.sdf"),
        ("usfarmland1", "wic60.sdf"),
        ("usfarmland3", "wic45.sdf"),
    ] {
        let Some(entry) = art.iter().find(|entry| entry.map_name == map_name) else {
            continue;
        };
        assert!(
            entry.source_archive.ends_with(expected_archive),
            "{map_name} resolved to {} instead of {expected_archive}",
            entry.source_archive
        );
    }
}

/// The path the Tauri commands actually take: decode into the database once,
/// then serve tiles from it, and skip the rescan when nothing changed.
#[test]
#[ignore = "requires a private World in Conflict installation"]
fn caches_decoded_art_and_serves_it_by_internal_map_name() {
    let install = install();

    let directory = tempfile::tempdir().expect("temp dir");
    let database_path = directory.path().join("library.sqlite3");
    let mut database = Database::open(&database_path).expect("database");

    let decoded = ensure_map_art(&mut database, &sources(&install)).expect("first pass decodes");
    assert!(decoded > 0);
    assert!(
        database
            .map_art_is_current(&sources(&install).signature())
            .unwrap()
    );

    // A second pass must be a cache hit, not another scan.
    assert_eq!(
        ensure_map_art(&mut database, &sources(&install)).expect("second pass"),
        decoded
    );

    let png = database
        .map_art_png("russia3")
        .expect("query")
        .expect("russia3 ships with the game");
    let url = png_data_url(&png);
    assert!(url.starts_with("data:image/png;base64,iVBORw0KGgo"));

    // A map that does not exist simply has no art, leaving the procedural tile.
    assert_eq!(database.map_art_png("not_a_real_map").expect("query"), None);
}

/// Regression: replay summaries store the map as `maps/<name>/<name>.ice`, not
/// as the bare `<name>` the art is keyed by. Looking up the raw field finds
/// nothing, which silently left every row on its procedural tile.
#[test]
#[ignore = "requires a private World in Conflict installation"]
fn resolves_art_for_the_map_paths_summaries_actually_store() {
    let install = install();

    let directory = tempfile::tempdir().expect("temp dir");
    let mut database = Database::open(&directory.path().join("library.sqlite3")).expect("database");
    ensure_map_art(&mut database, &sources(&install)).expect("decodes");

    for summary_path in [
        "maps/russia3/russia3.ice",
        "maps/ustown4/ustown4.ice",
        "maps/do_tequila/do_tequila.ice",
    ] {
        let key = internal_map_name(summary_path);
        assert!(
            database.map_art_png(key).expect("query").is_some(),
            "{summary_path} reduced to {key}, which found no art"
        );
    }

    // Community maps resolve to a clean key too. Whether they have art depends
    // on whether a downloaded-maps folder was supplied, so only the key is
    // asserted here; coverage is asserted in the custom-maps test below.
    assert_eq!(
        internal_map_name("maps/do_alaska/do_alaska.ice"),
        "do_alaska"
    );

    // A map that exists in no source simply has none, which is the
    // procedural-tile path.
    assert_eq!(
        database
            .map_art_png(internal_map_name("maps/not_a_real_map/not_a_real_map.ice"))
            .expect("query"),
        None
    );
}

/// Community maps ship as ordinary `.sdf` archives named after the map, in a
/// folder that is independent of where the game is installed. Skipped unless
/// `WIC_CUSTOM_MAPS` points at one.
#[test]
#[ignore = "requires a private downloaded-maps folder"]
fn decodes_community_maps_from_the_downloaded_folder() {
    let custom = PathBuf::from(
        std::env::var_os("WIC_CUSTOM_MAPS").expect("private test requires WIC_CUSTOM_MAPS"),
    );
    assert!(
        is_custom_maps_dir(&custom),
        "{} holds no .sdf archives",
        custom.display()
    );

    let only_custom = ArtSources {
        install: None,
        custom_maps: Some(custom),
    };
    let art = collect_map_art(&only_custom).expect("scan succeeds");
    assert!(!art.is_empty());
    for entry in &art {
        assert_eq!(
            &entry.png[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        assert_eq!(
            entry.map_name,
            entry.map_name.to_ascii_lowercase(),
            "keys are normalised so lookups match summary paths"
        );
        let bounds = entry
            .bounds
            .unwrap_or_else(|| panic!("{} has no verified terrain bounds", entry.map_name));
        assert_eq!(
            (bounds.min_x, bounds.min_z, bounds.max_x, bounds.max_z),
            (0.0, 0.0, 1536.0, 1536.0),
            "{} has unexpected overview terrain bounds",
            entry.map_name
        );
    }
}
