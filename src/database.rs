// Preserve detailed UI errors; the report receives only a fixed database code.
macro_rules! database_error {
    ($($argument:tt)*) => {{
        crate::diagnostics::incident(crate::diagnostics::Code::DatabaseFailed, crate::diagnostics::Operation::Database);
        format!($($argument)*)
    }};
}

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::map_art::{MapArt, MapBounds};
use crate::model::{
    DATABASE_SCHEMA_VERSION, FileFingerprint, PARSER_CACHE_KEY, ReplaySummary, sort_summaries,
};

/// Decoded map overview art, keyed by the internal map name. Held in its own
/// table so it can be rebuilt or dropped without touching cached replay detail.
const MAP_ART_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS map_art (
     map_name TEXT PRIMARY KEY NOT NULL,
     source_archive TEXT NOT NULL,
     png BLOB NOT NULL,
     min_x REAL,
     min_z REAL,
     max_x REAL,
     max_z REAL,
     command_point_names TEXT NOT NULL DEFAULT '{}'
 );";

#[derive(Debug, Clone, PartialEq)]
pub struct CachedMapArt {
    pub png: Vec<u8>,
    pub bounds: Option<MapBounds>,
    pub command_point_names: BTreeMap<u32, String>,
}

/// Identifies which installation the cached art came from and under which
/// decode contract, so a changed game path or codec rebuilds it.
pub const MAP_ART_SIGNATURE_KEY: &str = "map_art_signature";
pub const GAME_INSTALL_SETTING: &str = "game_install_path";
pub const CUSTOM_MAPS_SETTING: &str = "custom_maps_path";
pub const REPLAY_DIR_SEEDED_SETTING: &str = "default_replay_dir_seeded";

pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| database_error!("Cannot create {}: {error}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| database_error!("Cannot open {}: {error}", path.display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|error| database_error!("Cannot configure SQLite busy timeout: {error}"))?;
        let mut database = Self { connection };
        database.initialize()?;
        Ok(database)
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = NORMAL;",
            )
            .map_err(|error| database_error!("Cannot configure replay database: {error}"))?;

        let version: i32 = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| database_error!("Cannot read replay database version: {error}"))?;
        if version == 0 {
            self.connection
                .execute_batch(
                    "CREATE TABLE app_meta (
                         key TEXT PRIMARY KEY NOT NULL,
                         value TEXT NOT NULL
                     );
                     CREATE TABLE settings (
                         key TEXT PRIMARY KEY NOT NULL,
                         value TEXT NOT NULL
                     );
                     CREATE TABLE library_locations (
                         path TEXT PRIMARY KEY NOT NULL,
                         added_at INTEGER NOT NULL
                     );
                     CREATE TABLE replay_summaries (
                         path TEXT PRIMARY KEY NOT NULL,
                         file_name TEXT NOT NULL,
                         replay_name TEXT,
                         server_name TEXT NOT NULL,
                         file_size INTEGER NOT NULL,
                         modified_ns INTEGER NOT NULL,
                         cache_key TEXT NOT NULL,
                         map_name TEXT NOT NULL,
                         map_display_name TEXT NOT NULL,
                         game_mode TEXT NOT NULL,
                         server_modes TEXT NOT NULL,
                         format TEXT NOT NULL,
                         date_time TEXT NOT NULL,
                         duration_seconds REAL,
                         recording_seconds REAL,
                         winner TEXT,
                         player_count INTEGER NOT NULL,
                         player_names TEXT NOT NULL,
                         factions TEXT NOT NULL,
                         search_players TEXT NOT NULL DEFAULT '[]',
                         recorder TEXT,
                         recorder_faction TEXT,
                         incomplete INTEGER NOT NULL,
                         parse_error TEXT,
                         imported_at INTEGER NOT NULL
                     );
                     CREATE INDEX replay_summaries_date
                         ON replay_summaries(date_time DESC);
                     CREATE INDEX replay_summaries_map
                         ON replay_summaries(map_display_name COLLATE NOCASE);
                     CREATE TABLE replay_details (
                         path TEXT PRIMARY KEY NOT NULL
                             REFERENCES replay_summaries(path) ON DELETE CASCADE,
                         file_size INTEGER NOT NULL,
                         modified_ns INTEGER NOT NULL,
                         cache_key TEXT NOT NULL,
                         timeline_schema INTEGER NOT NULL,
                         json TEXT NOT NULL
                     );
                     CREATE TABLE map_art (
                         map_name TEXT PRIMARY KEY NOT NULL,
                         source_archive TEXT NOT NULL,
                         png BLOB NOT NULL,
                         min_x REAL,
                         min_z REAL,
                         max_x REAL,
                         max_z REAL,
                         command_point_names TEXT NOT NULL DEFAULT '{}'
                     );",
                )
                .map_err(|error| database_error!("Cannot create replay database: {error}"))?;
            self.connection
                .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION)
                .map_err(|error| database_error!("Cannot version replay database: {error}"))?;
        } else if version == 1 {
            let transaction = self.connection.transaction().map_err(|error| {
                database_error!("Cannot start replay database migration: {error}")
            })?;
            transaction
                .execute_batch(
                    "CREATE TABLE library_locations (
                         path TEXT PRIMARY KEY NOT NULL,
                         added_at INTEGER NOT NULL
                     );
                     INSERT OR IGNORE INTO library_locations(path, added_at)
                         SELECT value, CAST(strftime('%s', 'now') AS INTEGER)
                         FROM settings
                         WHERE key = 'last_import_dir' AND value != '';
                     ALTER TABLE replay_summaries ADD COLUMN recorder_faction TEXT;
                     ALTER TABLE replay_summaries ADD COLUMN server_modes TEXT NOT NULL DEFAULT '';
                     ALTER TABLE replay_summaries ADD COLUMN recording_seconds REAL;
                     PRAGMA user_version = 5;",
                )
                .map_err(|error| database_error!("Cannot migrate replay database: {error}"))?;
            transaction.commit().map_err(|error| {
                database_error!("Cannot commit replay database migration: {error}")
            })?;
        } else if version == 2 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN recorder_faction TEXT;
                     ALTER TABLE replay_summaries ADD COLUMN server_modes TEXT NOT NULL DEFAULT '';
                     ALTER TABLE replay_summaries ADD COLUMN recording_seconds REAL;
                     PRAGMA user_version = 5;",
                )
                .map_err(|error| database_error!("Cannot migrate replay database: {error}"))?;
        } else if version == 3 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN server_modes TEXT NOT NULL DEFAULT '';
                     ALTER TABLE replay_summaries ADD COLUMN recording_seconds REAL;
                     PRAGMA user_version = 5;",
                )
                .map_err(|error| database_error!("Cannot migrate replay database: {error}"))?;
        } else if version == 4 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN recording_seconds REAL;
                     PRAGMA user_version = 5;",
                )
                .map_err(|error| database_error!("Cannot migrate replay database: {error}"))?;
        } else if version != 5
            && version != 6
            && version != 7
            && version != 8
            && version != 9
            && version != 10
            && version != 11
            && version != 12
            && version != DATABASE_SCHEMA_VERSION
        {
            return Err(format!(
                "Unsupported replay database schema {version}; expected {DATABASE_SCHEMA_VERSION}"
            ));
        }

        // Every pre-6 path above lands on schema 5. Adding map art is additive.
        if version != 0 && version < 6 {
            self.connection
                .execute_batch(&format!("{MAP_ART_TABLE_SQL} PRAGMA user_version = 6;"))
                .map_err(|error| database_error!("Cannot add map art storage: {error}"))?;
        }
        if version != 0 && version < 7 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN format TEXT NOT NULL DEFAULT '';
                     PRAGMA user_version = 7;",
                )
                .map_err(|error| database_error!("Cannot add replay format storage: {error}"))?;
        }
        // Pre-6 databases create the current map table above, so only schemas
        // that already owned the old three-column table need ALTERs.
        if (6..8).contains(&version) {
            self.connection
                .execute_batch(
                    "ALTER TABLE map_art ADD COLUMN min_x REAL;
                     ALTER TABLE map_art ADD COLUMN min_z REAL;
                     ALTER TABLE map_art ADD COLUMN max_x REAL;
                     ALTER TABLE map_art ADD COLUMN max_z REAL;
                     PRAGMA user_version = 8;",
                )
                .map_err(|error| database_error!("Cannot add map coordinate bounds: {error}"))?;
        }
        if (6..9).contains(&version) {
            self.connection
                .execute_batch(
                    "ALTER TABLE map_art ADD COLUMN command_point_names TEXT NOT NULL DEFAULT '{}';
                     PRAGMA user_version = 9;",
                )
                .map_err(|error| database_error!("Cannot add command-point names: {error}"))?;
        }
        if version != 0 && version < 11 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN replay_name TEXT;
                     PRAGMA user_version = 11;",
                )
                .map_err(|error| {
                    database_error!("Cannot add in-game replay-name storage: {error}")
                })?;
        }
        if version != 0 && version < 12 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN server_name TEXT NOT NULL DEFAULT '';
                     ALTER TABLE replay_summaries ADD COLUMN factions TEXT NOT NULL DEFAULT '';
                     UPDATE replay_summaries SET cache_key = '';
                     PRAGMA user_version = 12;",
                )
                .map_err(|error| {
                    database_error!("Cannot add searchable replay metadata: {error}")
                })?;
        }
        if version != 0 && version < 13 {
            self.connection
                .execute_batch(
                    "ALTER TABLE replay_summaries ADD COLUMN search_players TEXT NOT NULL DEFAULT '[]';
                     UPDATE replay_summaries SET cache_key = '';
                     PRAGMA user_version = 13;",
                )
                .map_err(|error| database_error!("Cannot add player search storage: {error}"))?;
        }
        if version != 0 && version < DATABASE_SCHEMA_VERSION {
            self.connection
                .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION)
                .map_err(|error| {
                    database_error!("Cannot finish replay database migration: {error}")
                })?;
        }

        let stored_key: Option<String> = self
            .connection
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'parser_cache_key'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| database_error!("Cannot read parser cache key: {error}"))?;
        if stored_key.as_deref() != Some(PARSER_CACHE_KEY) {
            let transaction = self
                .connection
                .transaction()
                .map_err(|error| database_error!("Cannot start cache invalidation: {error}"))?;
            transaction
                .execute("DELETE FROM replay_details", [])
                .map_err(|error| database_error!("Cannot invalidate replay details: {error}"))?;
            transaction
                .execute("UPDATE replay_summaries SET cache_key = ''", [])
                .map_err(|error| database_error!("Cannot mark replay summaries stale: {error}"))?;
            transaction
                .execute(
                    "INSERT INTO app_meta(key, value) VALUES('parser_cache_key', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [PARSER_CACHE_KEY],
                )
                .map_err(|error| database_error!("Cannot store parser cache key: {error}"))?;
            transaction
                .commit()
                .map_err(|error| database_error!("Cannot commit cache invalidation: {error}"))?;
        }
        Ok(())
    }

    pub fn load_summaries(&self) -> Result<Vec<ReplaySummary>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT path, file_name, file_size, modified_ns, cache_key,
                        map_name, map_display_name, game_mode, server_modes, format, date_time,
                        duration_seconds, winner, player_count, player_names,
                        recorder, recorder_faction, incomplete, parse_error, imported_at,
                        recording_seconds, replay_name, server_name, factions, search_players
                 FROM replay_summaries",
            )
            .map_err(|error| database_error!("Cannot prepare replay query: {error}"))?;
        let rows = statement
            .query_map([], row_to_summary)
            .map_err(|error| database_error!("Cannot query replay summaries: {error}"))?;
        let mut summaries = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| database_error!("Cannot decode replay summaries: {error}"))?;
        sort_summaries(&mut summaries);
        Ok(summaries)
    }

    pub fn summary_is_current(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
    ) -> Result<bool, String> {
        let found: i64 = self
            .connection
            .query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM replay_summaries
                     WHERE path = ?1 AND file_size = ?2 AND modified_ns = ?3
                       AND cache_key = ?4
                 )",
                params![
                    path_text(path),
                    size_to_sql(fingerprint.size)?,
                    fingerprint.modified_ns,
                    PARSER_CACHE_KEY,
                ],
                |row| row.get(0),
            )
            .map_err(|error| database_error!("Cannot check replay cache: {error}"))?;
        Ok(found != 0)
    }

    /// Load the fingerprints that are valid under the current parser contract.
    /// Import discovery can compare every file against this one snapshot instead
    /// of issuing a SQLite query for every replay in the library.
    pub fn current_summary_fingerprints(
        &self,
    ) -> Result<HashMap<PathBuf, FileFingerprint>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT path, file_size, modified_ns FROM replay_summaries
                 WHERE cache_key = ?1",
            )
            .map_err(|error| database_error!("Cannot prepare replay cache query: {error}"))?;
        let rows = statement
            .query_map([PARSER_CACHE_KEY], |row| {
                let file_size: i64 = row.get(1)?;
                Ok((
                    PathBuf::from(row.get::<_, String>(0)?),
                    FileFingerprint {
                        size: file_size.max(0) as u64,
                        modified_ns: row.get(2)?,
                    },
                ))
            })
            .map_err(|error| database_error!("Cannot query replay cache: {error}"))?;
        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(|error| database_error!("Cannot decode replay cache: {error}"))
    }

    pub fn upsert_summary(&mut self, summary: &ReplaySummary) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start replay update: {error}"))?;
        upsert_summary_in_transaction(&transaction, summary)?;
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit replay update: {error}"))
    }

    /// Store a bounded group atomically. Callers can fall back to
    /// `upsert_summary` after an error to retain per-replay failure isolation.
    pub fn upsert_summaries(&mut self, summaries: &[ReplaySummary]) -> Result<(), String> {
        if summaries.is_empty() {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start replay batch update: {error}"))?;
        for summary in summaries {
            upsert_summary_in_transaction(&transaction, summary)?;
        }
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit replay batch update: {error}"))
    }

    pub fn load_detail(
        &self,
        path: &Path,
        fingerprint: FileFingerprint,
    ) -> Result<Option<String>, String> {
        self.connection
            .query_row(
                "SELECT json FROM replay_details
                 WHERE path = ?1 AND file_size = ?2 AND modified_ns = ?3
                   AND cache_key = ?4",
                params![
                    path_text(path),
                    size_to_sql(fingerprint.size)?,
                    fingerprint.modified_ns,
                    PARSER_CACHE_KEY,
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| database_error!("Cannot load replay detail: {error}"))
    }

    pub fn save_detail(
        &mut self,
        path: &Path,
        fingerprint: FileFingerprint,
        timeline_schema: u32,
        json: &str,
    ) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO replay_details(
                     path, file_size, modified_ns, cache_key, timeline_schema, json
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(path) DO UPDATE SET
                     file_size = excluded.file_size,
                     modified_ns = excluded.modified_ns,
                     cache_key = excluded.cache_key,
                     timeline_schema = excluded.timeline_schema,
                     json = excluded.json",
                params![
                    path_text(path),
                    size_to_sql(fingerprint.size)?,
                    fingerprint.modified_ns,
                    PARSER_CACHE_KEY,
                    timeline_schema,
                    json,
                ],
            )
            .map_err(|error| database_error!("Cannot cache replay detail: {error}"))?;
        Ok(())
    }

    pub fn delete_detail(&self, path: &Path) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM replay_details WHERE path = ?1",
                [path_text(path)],
            )
            .map_err(|error| database_error!("Cannot discard replay detail: {error}"))?;
        Ok(())
    }

    pub fn library_locations(&self) -> Result<Vec<PathBuf>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT path FROM library_locations ORDER BY added_at, path COLLATE NOCASE")
            .map_err(|error| database_error!("Cannot prepare library location query: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
            .map_err(|error| database_error!("Cannot query library locations: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| database_error!("Cannot decode library locations: {error}"))
    }

    /// Repair legacy spellings without dropping locations on disconnected drives.
    /// Replay caches are deliberately untouched: only saved folder rows are merged.
    pub fn reconcile_library_locations(&mut self) -> Result<(), String> {
        let paths = self.library_locations()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start location repair: {error}"))?;
        for path in paths {
            let Ok(canonical) = fs::canonicalize(&path) else {
                continue;
            };
            if !canonical.is_dir() || canonical.as_os_str() == path.as_os_str() {
                continue;
            }
            transaction
                .execute(
                    "INSERT INTO library_locations(path, added_at)
                 SELECT ?1, added_at FROM library_locations WHERE path = ?2
                 ON CONFLICT(path) DO UPDATE SET added_at = MIN(added_at, excluded.added_at)",
                    params![path_text(&canonical), path_text(&path)],
                )
                .map_err(|error| database_error!("Cannot merge library location: {error}"))?;
            transaction
                .execute(
                    "DELETE FROM library_locations WHERE path = ?1",
                    [path_text(&path)],
                )
                .map_err(|error| database_error!("Cannot remove duplicate location: {error}"))?;
        }
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit location repair: {error}"))
    }

    pub fn add_library_locations(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        self.reconcile_library_locations()?;
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start library location update: {error}"))?;
        let added_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        for path in paths {
            let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            transaction
                .execute(
                    "INSERT OR IGNORE INTO library_locations(path, added_at) VALUES(?1, ?2)",
                    params![path_text(&canonical), added_at],
                )
                .map_err(|error| {
                    database_error!("Cannot add library location {}: {error}", path.display())
                })?;
        }
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit library location update: {error}"))
    }

    pub fn remove_library_location(&self, path: &Path) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM library_locations WHERE path = ?1",
                [path_text(path)],
            )
            .map_err(|error| {
                database_error!("Cannot remove library location {}: {error}", path.display())
            })?;
        Ok(())
    }

    pub fn delete_summary(&self, path: &Path) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM replay_summaries WHERE path = ?1",
                [path_text(path)],
            )
            .map_err(|error| database_error!("Cannot remove replay {}: {error}", path.display()))?;
        Ok(())
    }

    /// Remove cached rows that disappeared from a successfully scanned root.
    ///
    /// Filtering paths in Rust preserves path-component semantics: a root such
    /// as `/replays/main` must never match a sibling such as `/replays/main-old`.
    /// The deletion is transactional so the frontend can drop exactly the same
    /// rows only after the cache update has committed.
    pub fn prune_missing_summaries(
        &mut self,
        scanned_roots: &[PathBuf],
        discovered_paths: &BTreeSet<PathBuf>,
    ) -> Result<Vec<PathBuf>, String> {
        let cached_paths = {
            let mut statement = self
                .connection
                .prepare("SELECT path FROM replay_summaries")
                .map_err(|error| database_error!("Cannot prepare replay-pruning query: {error}"))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0).map(PathBuf::from))
                .map_err(|error| {
                    database_error!("Cannot query replay paths for pruning: {error}")
                })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
                database_error!("Cannot decode replay paths for pruning: {error}")
            })?
        };

        let mut missing = cached_paths
            .into_iter()
            .filter(|path| {
                scanned_roots.iter().any(|root| path.starts_with(root))
                    && !discovered_paths.contains(path)
            })
            .collect::<Vec<_>>();
        missing.sort();
        if missing.is_empty() {
            return Ok(missing);
        }

        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start replay-pruning transaction: {error}"))?;
        for path in &missing {
            transaction
                .execute(
                    "DELETE FROM replay_summaries WHERE path = ?1",
                    [path_text(path)],
                )
                .map_err(|error| {
                    database_error!("Cannot remove missing replay {}: {error}", path.display())
                })?;
        }
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit replay pruning: {error}"))?;
        Ok(missing)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>, String> {
        self.connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| database_error!("Cannot read setting {key}: {error}"))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO settings(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|error| database_error!("Cannot store setting {key}: {error}"))?;
        Ok(())
    }

    pub fn clear_setting(&self, key: &str) -> Result<(), String> {
        self.connection
            .execute("DELETE FROM settings WHERE key = ?1", [key])
            .map_err(|error| database_error!("Cannot clear setting {key}: {error}"))?;
        Ok(())
    }

    /// The installation and decode contract the cached art was built from.
    pub fn map_art_signature(&self) -> Result<Option<String>, String> {
        self.connection
            .query_row(
                "SELECT value FROM app_meta WHERE key = ?1",
                [MAP_ART_SIGNATURE_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| database_error!("Cannot read map art signature: {error}"))
    }

    pub fn map_art_is_current(&self, signature: &str) -> Result<bool, String> {
        Ok(self.map_art_signature()?.as_deref() == Some(signature))
    }

    /// Replace the whole cache in one transaction, so a failed rebuild never
    /// leaves a half-populated set of tiles behind.
    pub fn replace_map_art(&mut self, signature: &str, art: &[MapArt]) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start map art update: {error}"))?;
        transaction
            .execute("DELETE FROM map_art", [])
            .map_err(|error| database_error!("Cannot clear map art: {error}"))?;
        for entry in art {
            transaction
                .execute(
                    "INSERT OR REPLACE INTO map_art(
                         map_name, source_archive, png, min_x, min_z, max_x, max_z,
                         command_point_names
                     ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        entry.map_name,
                        entry.source_archive,
                        entry.png,
                        entry.bounds.map(|value| value.min_x),
                        entry.bounds.map(|value| value.min_z),
                        entry.bounds.map(|value| value.max_x),
                        entry.bounds.map(|value| value.max_z),
                        serde_json::to_string(&entry.command_point_names).map_err(|error| {
                            database_error!("Cannot encode command-point names: {error}")
                        })?,
                    ],
                )
                .map_err(|error| {
                    database_error!("Cannot store map art for {}: {error}", entry.map_name)
                })?;
        }
        transaction
            .execute(
                "INSERT INTO app_meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![MAP_ART_SIGNATURE_KEY, signature],
            )
            .map_err(|error| database_error!("Cannot store map art signature: {error}"))?;
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit map art update: {error}"))
    }

    pub fn clear_map_art(&mut self) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| database_error!("Cannot start map art reset: {error}"))?;
        transaction
            .execute("DELETE FROM map_art", [])
            .map_err(|error| database_error!("Cannot clear map art: {error}"))?;
        transaction
            .execute(
                "DELETE FROM app_meta WHERE key = ?1",
                [MAP_ART_SIGNATURE_KEY],
            )
            .map_err(|error| database_error!("Cannot clear map art signature: {error}"))?;
        transaction
            .commit()
            .map_err(|error| database_error!("Cannot commit map art reset: {error}"))
    }

    /// Force the next source refresh without discarding the last usable image
    /// set first. The replacement itself is transactional, so an unreadable or
    /// temporarily unavailable installation can continue using cached art.
    pub fn invalidate_map_art_signature(&self) -> Result<(), String> {
        self.connection
            .execute(
                "DELETE FROM app_meta WHERE key = ?1",
                [MAP_ART_SIGNATURE_KEY],
            )
            .map_err(|error| database_error!("Cannot invalidate map art signature: {error}"))?;
        Ok(())
    }

    pub fn map_art_png(&self, map_name: &str) -> Result<Option<Vec<u8>>, String> {
        self.connection
            .query_row(
                "SELECT png FROM map_art WHERE map_name = ?1 COLLATE NOCASE",
                [map_name],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| database_error!("Cannot read map art for {map_name}: {error}"))
    }

    /// Read several cached images through one prepared statement and one
    /// connection. Results stay aligned with `map_names`, including misses.
    pub fn map_art_pngs(&self, map_names: &[String]) -> Result<Vec<Option<Vec<u8>>>, String> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT png FROM map_art WHERE map_name = ?1 COLLATE NOCASE")
            .map_err(|error| database_error!("Cannot prepare map art batch: {error}"))?;
        map_names
            .iter()
            .map(|map_name| {
                statement
                    .query_row([map_name], |row| row.get(0))
                    .optional()
                    .map_err(|error| database_error!("Cannot read map art for {map_name}: {error}"))
            })
            .collect()
    }

    pub fn map_art_assets(
        &self,
        map_names: &[String],
    ) -> Result<Vec<Option<CachedMapArt>>, String> {
        let mut statement = self
            .connection
            .prepare_cached(
                "SELECT png, min_x, min_z, max_x, max_z, command_point_names
                 FROM map_art WHERE map_name = ?1 COLLATE NOCASE",
            )
            .map_err(|error| database_error!("Cannot prepare map asset batch: {error}"))?;
        map_names
            .iter()
            .map(|map_name| {
                statement
                    .query_row([map_name], |row| {
                        let values: (Option<f32>, Option<f32>, Option<f32>, Option<f32>) =
                            (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?);
                        let bounds = match values {
                            (Some(min_x), Some(min_z), Some(max_x), Some(max_z)) => {
                                Some(MapBounds {
                                    min_x,
                                    min_z,
                                    max_x,
                                    max_z,
                                })
                            }
                            _ => None,
                        };
                        Ok(CachedMapArt {
                            png: row.get(0)?,
                            bounds,
                            command_point_names: serde_json::from_str(&row.get::<_, String>(5)?)
                                .unwrap_or_default(),
                        })
                    })
                    .optional()
                    .map_err(|error| {
                        database_error!("Cannot read map asset for {map_name}: {error}")
                    })
            })
            .collect()
    }

    pub fn map_art_count(&self) -> Result<usize, String> {
        let count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM map_art", [], |row| row.get(0))
            .map_err(|error| database_error!("Cannot count map art: {error}"))?;
        Ok(count.max(0) as usize)
    }
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReplaySummary> {
    let file_size: i64 = row.get(2)?;
    Ok(ReplaySummary {
        path: PathBuf::from(row.get::<_, String>(0)?),
        file_name: row.get(1)?,
        fingerprint: FileFingerprint {
            size: file_size.max(0) as u64,
            modified_ns: row.get(3)?,
        },
        cache_key: row.get(4)?,
        map_name: row.get(5)?,
        map_display_name: row.get(6)?,
        game_mode: row.get(7)?,
        server_modes: row.get(8)?,
        format: row.get(9)?,
        date_time: row.get(10)?,
        duration_seconds: row.get(11)?,
        recording_seconds: row.get(20)?,
        replay_name: row.get(21)?,
        server_name: row.get(22)?,
        winner: row.get(12)?,
        player_count: row.get::<_, i64>(13)?.max(0) as u32,
        player_names: row.get(14)?,
        factions: row.get(23)?,
        search_players: serde_json::from_str(&row.get::<_, String>(24)?).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                24,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        recorder: row.get(15)?,
        recorder_faction: row.get(16)?,
        incomplete: row.get(17)?,
        parse_error: row.get(18)?,
        imported_at: row.get(19)?,
    })
}

fn upsert_summary_in_transaction(
    transaction: &Transaction<'_>,
    summary: &ReplaySummary,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO replay_summaries(
                 path, file_name, file_size, modified_ns, cache_key,
                 map_name, map_display_name, game_mode, server_modes, format, date_time,
                 duration_seconds, winner, player_count, player_names,
                 recorder, recorder_faction, incomplete, parse_error, imported_at,
                 recording_seconds, replay_name, server_name, factions, search_players
             ) VALUES(
                 ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22,
                 ?23, ?24, ?25
             )
             ON CONFLICT(path) DO UPDATE SET
                 file_name = excluded.file_name,
                 file_size = excluded.file_size,
                 modified_ns = excluded.modified_ns,
                 cache_key = excluded.cache_key,
                 map_name = excluded.map_name,
                 map_display_name = excluded.map_display_name,
                 game_mode = excluded.game_mode,
                 server_modes = excluded.server_modes,
                 format = excluded.format,
                 date_time = excluded.date_time,
                 duration_seconds = excluded.duration_seconds,
                 winner = excluded.winner,
                 player_count = excluded.player_count,
                 player_names = excluded.player_names,
                 recorder = excluded.recorder,
                 recorder_faction = excluded.recorder_faction,
                 incomplete = excluded.incomplete,
                 parse_error = excluded.parse_error,
                 imported_at = excluded.imported_at,
                 recording_seconds = excluded.recording_seconds,
                 replay_name = excluded.replay_name,
                 server_name = excluded.server_name,
                 factions = excluded.factions,
                 search_players = excluded.search_players",
            params![
                path_text(&summary.path),
                summary.file_name,
                size_to_sql(summary.fingerprint.size)?,
                summary.fingerprint.modified_ns,
                summary.cache_key,
                summary.map_name,
                summary.map_display_name,
                summary.game_mode,
                summary.server_modes,
                summary.format,
                summary.date_time,
                summary.duration_seconds,
                summary.winner,
                summary.player_count,
                summary.player_names,
                summary.recorder,
                summary.recorder_faction,
                summary.incomplete,
                summary.parse_error,
                summary.imported_at,
                summary.recording_seconds,
                summary.replay_name,
                summary.server_name,
                summary.factions,
                serde_json::to_string(&summary.search_players)
                    .map_err(|error| database_error!("Cannot encode search players: {error}"))?,
            ],
        )
        .map_err(|error| database_error!("Cannot store {}: {error}", summary.path.display()))?;
    transaction
        .execute(
            "DELETE FROM replay_details WHERE path = ?1",
            [path_text(&summary.path)],
        )
        .map_err(|error| {
            database_error!(
                "Cannot invalidate replay detail for {}: {error}",
                summary.path.display()
            )
        })?;
    Ok(())
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn size_to_sql(size: u64) -> Result<i64, String> {
    i64::try_from(size).map_err(|_| format!("Replay file is too large for SQLite: {size} bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn summary(path: PathBuf) -> ReplaySummary {
        ReplaySummary {
            file_name: "sample.wicdemo".to_owned(),
            replay_name: Some("In-game sample".to_owned()),
            server_name: "WiCGate Ranked Server".to_owned(),
            path,
            fingerprint: FileFingerprint {
                size: 123,
                modified_ns: 456,
            },
            cache_key: PARSER_CACHE_KEY.to_owned(),
            map_name: "do_Hometown".to_owned(),
            map_display_name: "Hometown".to_owned(),
            game_mode: "Domination".to_owned(),
            server_modes: "FPM · Bots".to_owned(),
            format: "1vs1".to_owned(),
            date_time: "2009-01-01".to_owned(),
            duration_seconds: Some(600.0),
            recording_seconds: Some(660.0),
            winner: Some("NATO".to_owned()),
            player_count: 2,
            search_players: vec![crate::model::SearchPlayer {
                name: "Dexter".into(),
                faction: Some("NATO".into()),
            }],
            player_names: "Alpha, Bravo".to_owned(),
            factions: "NATO, USSR".to_owned(),
            recorder: Some("Alpha".to_owned()),
            recorder_faction: Some("USSR".to_owned()),
            incomplete: false,
            parse_error: None,
            imported_at: 789,
        }
    }

    #[test]
    fn repairs_folder_aliases_and_preserves_missing_locations() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Replay");
        fs::create_dir(&root).unwrap();
        let canonical = root.canonicalize().unwrap();
        let alias = root.join(".");
        let missing = directory.path().join("disconnected");
        let mut database = Database::open(&directory.path().join("library.sqlite3")).unwrap();
        for (path, time) in [(&alias, 1), (&canonical, 2), (&missing, 3)] {
            database
                .connection
                .execute(
                    "INSERT INTO library_locations VALUES (?1, ?2)",
                    params![path_text(path), time],
                )
                .unwrap();
        }
        database.reconcile_library_locations().unwrap();
        database
            .add_library_locations(&[root, alias, canonical.clone()])
            .unwrap();
        assert_eq!(
            database.library_locations().unwrap(),
            vec![canonical.clone(), missing]
        );
        let added_at: i64 = database
            .connection
            .query_row(
                "SELECT added_at FROM library_locations WHERE path = ?1",
                [path_text(&canonical)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(added_at, 1);
        database.remove_library_location(&canonical).unwrap();
        assert_eq!(database.library_locations().unwrap().len(), 1);
    }

    #[test]
    fn stores_summaries_settings_and_lazy_details() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let replay_path = directory.path().join("sample.wicdemo");
        let mut database = Database::open(&database_path).expect("database");
        let row = summary(replay_path.clone());

        database.upsert_summary(&row).expect("store summary");
        assert!(
            database
                .summary_is_current(&replay_path, row.fingerprint)
                .expect("current check")
        );
        let summaries = database.load_summaries().expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].recorder_faction.as_deref(), Some("USSR"));
        assert_eq!(summaries[0].replay_name.as_deref(), Some("In-game sample"));
        assert_eq!(summaries[0].server_name, "WiCGate Ranked Server");
        assert_eq!(summaries[0].factions, "NATO, USSR");
        assert_eq!(
            summaries[0].search_players,
            summary(replay_path.clone()).search_players
        );

        let locations = [directory.path().join("one"), directory.path().join("two")];
        database
            .add_library_locations(&locations)
            .expect("add locations");
        assert_eq!(database.library_locations().expect("locations"), locations);
        database
            .remove_library_location(&locations[0])
            .expect("remove location");
        assert_eq!(
            database.library_locations().expect("remaining locations"),
            vec![locations[1].clone()]
        );

        database
            .save_detail(&replay_path, row.fingerprint, 6, "{\"timeline\":{}}")
            .expect("save detail");
        assert!(
            database
                .load_detail(&replay_path, row.fingerprint)
                .expect("load detail")
                .is_some()
        );

        database.upsert_summary(&row).expect("refresh summary");
        assert!(
            database
                .load_detail(&replay_path, row.fingerprint)
                .expect("load invalidated detail")
                .is_none()
        );
    }

    #[test]
    fn batched_summaries_match_single_row_updates() {
        let directory = tempdir().expect("temp dir");
        let single_path = directory.path().join("single.sqlite3");
        let batch_path = directory.path().join("batch.sqlite3");
        let mut first = summary(directory.path().join("first.wicdemo"));
        first.file_name = "first.wicdemo".to_owned();
        let mut second = summary(directory.path().join("second.wicdemo"));
        second.file_name = "second.wicdemo".to_owned();
        second.fingerprint = FileFingerprint {
            size: 987,
            modified_ns: 654,
        };
        second.map_display_name = "Seaside".to_owned();
        let rows = vec![first, second];

        let mut single = Database::open(&single_path).expect("single database");
        for row in &rows {
            single.upsert_summary(row).expect("single-row update");
        }
        let mut batched = Database::open(&batch_path).expect("batch database");
        batched.upsert_summaries(&rows).expect("batch update");

        assert_eq!(
            serde_json::to_value(single.load_summaries().expect("single summaries"))
                .expect("single JSON"),
            serde_json::to_value(batched.load_summaries().expect("batch summaries"))
                .expect("batch JSON")
        );
        assert_eq!(
            batched
                .current_summary_fingerprints()
                .expect("current fingerprints"),
            rows.iter()
                .map(|row| (row.path.clone(), row.fingerprint))
                .collect()
        );
    }

    #[test]
    fn failed_summary_batch_rolls_back_every_row() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let mut database = Database::open(&database_path).expect("database");
        let valid = summary(directory.path().join("valid.wicdemo"));
        let mut invalid = summary(directory.path().join("oversized.wicdemo"));
        invalid.fingerprint.size = u64::MAX;

        let error = database
            .upsert_summaries(&[valid, invalid])
            .expect_err("oversized fingerprint must reject the batch");

        assert!(error.contains("too large for SQLite"), "{error}");
        assert!(
            database.load_summaries().expect("summaries").is_empty(),
            "the valid first row must be rolled back with the failed batch"
        );
    }

    #[test]
    fn parser_key_change_marks_summaries_stale_and_drops_details() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let replay_path = directory.path().join("sample.wicdemo");
        let fingerprint = FileFingerprint {
            size: 123,
            modified_ns: 456,
        };
        {
            let mut database = Database::open(&database_path).expect("database");
            let row = summary(replay_path.clone());
            database.upsert_summary(&row).expect("summary");
            database
                .save_detail(&replay_path, fingerprint, 6, "{}")
                .expect("detail");
            database
                .connection
                .execute(
                    "UPDATE app_meta SET value = 'old' WHERE key = 'parser_cache_key'",
                    [],
                )
                .expect("old key");
        }

        let database = Database::open(&database_path).expect("reopen database");
        assert!(database.load_summaries().expect("rows")[0].stale());
        assert!(
            database
                .load_detail(&replay_path, fingerprint)
                .expect("detail lookup")
                .is_none()
        );
    }

    #[test]
    fn migrates_the_single_v1_import_folder_into_library_locations() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let legacy_location = directory.path().join("legacy replays");
        {
            let database = Database::open(&database_path).expect("database");
            database
                .connection
                .execute(
                    "INSERT INTO settings(key, value) VALUES('last_import_dir', ?1)",
                    [path_text(&legacy_location)],
                )
                .expect("legacy setting");
            database
                .connection
                .execute_batch(
                    "DROP TABLE library_locations;
                     ALTER TABLE replay_summaries DROP COLUMN recorder_faction;
                     ALTER TABLE replay_summaries DROP COLUMN server_modes;
                     ALTER TABLE replay_summaries DROP COLUMN recording_seconds;
                     ALTER TABLE replay_summaries DROP COLUMN format;
                     ALTER TABLE replay_summaries DROP COLUMN replay_name;
                     ALTER TABLE replay_summaries DROP COLUMN server_name;
                     ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                     PRAGMA user_version = 1;",
                )
                .expect("simulate v1 database");
        }

        let migrated = Database::open(&database_path).expect("migrated database");
        assert_eq!(
            migrated.library_locations().expect("locations"),
            vec![legacy_location]
        );
    }

    #[test]
    fn migrates_v2_summaries_for_recorder_factions() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        {
            let database = Database::open(&database_path).expect("database");
            database
                .connection
                .execute_batch(
                    "ALTER TABLE replay_summaries DROP COLUMN recorder_faction;
                     ALTER TABLE replay_summaries DROP COLUMN server_modes;
                     ALTER TABLE replay_summaries DROP COLUMN recording_seconds;
                     ALTER TABLE replay_summaries DROP COLUMN format;
                     ALTER TABLE replay_summaries DROP COLUMN replay_name;
                     ALTER TABLE replay_summaries DROP COLUMN server_name;
                     ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                     PRAGMA user_version = 2;",
                )
                .expect("simulate v2 database");
        }

        let migrated = Database::open(&database_path).expect("migrated database");
        let columns = migrated
            .connection
            .prepare("SELECT recorder_faction FROM replay_summaries")
            .expect("recorder faction column");
        assert_eq!(
            migrated
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version"),
            DATABASE_SCHEMA_VERSION
        );
        drop(columns);
    }

    #[test]
    fn migrates_v3_summaries_for_server_modes() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        {
            let database = Database::open(&database_path).expect("database");
            database
                .connection
                .execute_batch(
                    "ALTER TABLE replay_summaries DROP COLUMN server_modes;
                     ALTER TABLE replay_summaries DROP COLUMN recording_seconds;
                     ALTER TABLE replay_summaries DROP COLUMN format;
                     ALTER TABLE replay_summaries DROP COLUMN replay_name;
                     ALTER TABLE replay_summaries DROP COLUMN server_name;
                     ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                     PRAGMA user_version = 3;",
                )
                .expect("simulate v3 database");
        }

        let migrated = Database::open(&database_path).expect("migrated database");
        migrated
            .connection
            .prepare("SELECT server_modes FROM replay_summaries")
            .expect("server modes column");
        assert_eq!(
            migrated
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version"),
            DATABASE_SCHEMA_VERSION
        );
    }

    #[test]
    fn caches_map_art_until_the_installation_changes() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let mut database = Database::open(&database_path).expect("database");

        assert_eq!(database.map_art_count().expect("count"), 0);
        assert!(!database.map_art_is_current("sig-a").expect("signature"));

        let art = vec![
            MapArt {
                map_name: "russia3".to_owned(),
                source_archive: "/games/wic/wic60.sdf".to_owned(),
                png: vec![1, 2, 3],
                bounds: Some(MapBounds {
                    min_x: 1.0,
                    min_z: 2.0,
                    max_x: 3.0,
                    max_z: 4.0,
                }),
                command_point_names: BTreeMap::from([(0x2eae_05b8, "Space Needle".to_owned())]),
            },
            MapArt {
                map_name: "berlin1".to_owned(),
                source_archive: "/games/wic/wic60.sdf".to_owned(),
                png: vec![4, 5],
                bounds: None,
                command_point_names: BTreeMap::new(),
            },
        ];
        database.replace_map_art("sig-a", &art).expect("store");

        assert_eq!(database.map_art_count().expect("count"), 2);
        assert!(database.map_art_is_current("sig-a").expect("current"));
        assert_eq!(
            database.map_art_png("russia3").expect("read"),
            Some(vec![1, 2, 3])
        );
        // Replay summaries carry the internal name with original casing.
        assert_eq!(
            database.map_art_png("Russia3").expect("read"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(database.map_art_png("nosuchmap").expect("read"), None);
        assert_eq!(
            database
                .map_art_assets(&["Russia3".to_owned()])
                .expect("asset")[0]
                .as_ref()
                .and_then(|asset| asset.bounds),
            Some(MapBounds {
                min_x: 1.0,
                min_z: 2.0,
                max_x: 3.0,
                max_z: 4.0
            })
        );
        assert_eq!(
            database
                .map_art_assets(&["Russia3".to_owned()])
                .expect("asset")[0]
                .as_ref()
                .and_then(|asset| asset.command_point_names.get(&0x2eae_05b8))
                .map(String::as_str),
            Some("Space Needle")
        );
        assert_eq!(
            database
                .map_art_pngs(&[
                    "Russia3".to_owned(),
                    "nosuchmap".to_owned(),
                    "berlin1".to_owned(),
                ])
                .expect("batch read"),
            vec![Some(vec![1, 2, 3]), None, Some(vec![4, 5])]
        );

        // Pointing at a different folder changes the signature and invalidates.
        assert!(!database.map_art_is_current("sig-b").expect("stale"));

        database
            .invalidate_map_art_signature()
            .expect("invalidate signature");
        assert_eq!(database.map_art_count().expect("preserved count"), 2);
        assert_eq!(
            database.map_art_png("russia3").expect("preserved art"),
            Some(vec![1, 2, 3])
        );
        assert!(!database.map_art_is_current("sig-a").expect("invalidated"));

        database.clear_map_art().expect("clear");
        assert_eq!(database.map_art_count().expect("count"), 0);
        assert!(!database.map_art_is_current("sig-a").expect("cleared"));
    }

    #[test]
    fn map_art_is_independent_of_the_parser_cache_key() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let mut database = Database::open(&database_path).expect("database");
        database
            .replace_map_art(
                "sig-a",
                &[MapArt {
                    map_name: "europe1".to_owned(),
                    source_archive: "/games/wic/wic2.sdf".to_owned(),
                    png: vec![9],
                    bounds: None,
                    command_point_names: BTreeMap::new(),
                }],
            )
            .expect("store");
        drop(database);

        // Simulate a parser bump: cached detail is discarded, art is not.
        let connection = Connection::open(&database_path).expect("reopen");
        connection
            .execute(
                "UPDATE app_meta SET value = 'stale' WHERE key = 'parser_cache_key'",
                [],
            )
            .expect("invalidate parser cache");
        drop(connection);

        let database = Database::open(&database_path).expect("reopen database");
        assert_eq!(database.map_art_count().expect("count"), 1);
        assert!(database.map_art_is_current("sig-a").expect("current"));
    }

    #[test]
    fn migrates_a_v5_database_by_adding_map_art() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        {
            let database = Database::open(&database_path).expect("database");
            database
                .connection
                .execute_batch(
                    "DROP TABLE map_art;
                     ALTER TABLE replay_summaries DROP COLUMN format;
                     ALTER TABLE replay_summaries DROP COLUMN replay_name;
                     ALTER TABLE replay_summaries DROP COLUMN server_name;
                     ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                     PRAGMA user_version = 5;",
                )
                .expect("simulate v5 database");
        }

        let migrated = Database::open(&database_path).expect("migrated database");
        assert_eq!(migrated.map_art_count().expect("count"), 0);
        assert_eq!(
            migrated
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version"),
            DATABASE_SCHEMA_VERSION
        );
    }

    #[test]
    fn migrates_v7_map_art_without_discarding_pngs() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        {
            let database = Database::open(&database_path).expect("database");
            database.connection.execute_batch(
                "INSERT INTO map_art(map_name, source_archive, png) VALUES('russia3', 'wic60.sdf', X'010203');
                 CREATE TABLE old_map_art AS SELECT map_name, source_archive, png FROM map_art;
                 DROP TABLE map_art;
                 ALTER TABLE old_map_art RENAME TO map_art;
                 ALTER TABLE replay_summaries DROP COLUMN replay_name;
                 ALTER TABLE replay_summaries DROP COLUMN server_name;
                 ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                 PRAGMA user_version = 7;",
            ).expect("simulate v7 database");
        }
        let migrated = Database::open(&database_path).expect("migrated database");
        assert_eq!(
            migrated.map_art_png("russia3").expect("png"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(
            migrated
                .map_art_assets(&["russia3".to_owned()])
                .expect("asset")[0]
                .as_ref()
                .and_then(|asset| asset.bounds),
            None
        );
        assert!(
            migrated
                .map_art_assets(&["russia3".to_owned()])
                .expect("asset")[0]
                .as_ref()
                .is_some_and(|asset| asset.command_point_names.is_empty())
        );
    }

    #[test]
    fn migrates_v8_map_art_for_command_point_names_without_discarding_pngs() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        {
            let database = Database::open(&database_path).expect("database");
            database
                .connection
                .execute_batch(
                    "INSERT INTO map_art(map_name, source_archive, png)
                         VALUES('seattle1', 'wic60.sdf', X'010203');
                     ALTER TABLE map_art DROP COLUMN command_point_names;
                     ALTER TABLE replay_summaries DROP COLUMN replay_name;
                     ALTER TABLE replay_summaries DROP COLUMN server_name;
                     ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                     PRAGMA user_version = 8;",
                )
                .expect("simulate v8 database");
        }
        let migrated = Database::open(&database_path).expect("migrated database");
        let asset = migrated
            .map_art_assets(&["seattle1".to_owned()])
            .expect("asset")
            .remove(0)
            .expect("Seattle art");
        assert_eq!(asset.png, vec![1, 2, 3]);
        assert!(asset.command_point_names.is_empty());
    }

    #[test]
    fn advances_schema_nine_without_creating_local_map_name_storage() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        drop(Database::open(&database_path).expect("current database"));

        let connection = Connection::open(&database_path).expect("schema fixture");
        connection
            .execute_batch(
                "ALTER TABLE replay_summaries DROP COLUMN replay_name;
                 ALTER TABLE replay_summaries DROP COLUMN server_name;
                 ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;",
            )
            .expect("remove post-schema-9 summary columns");
        connection
            .pragma_update(None, "user_version", 9)
            .expect("schema 9 fixture");
        drop(connection);

        let database = Database::open(&database_path).expect("migrated database");
        assert_eq!(
            database
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version"),
            DATABASE_SCHEMA_VERSION
        );
        assert_eq!(
            database
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'map_names'",
                    [],
                    |row| row.get::<_, i32>(0),
                )
                .expect("map-name table count"),
            0
        );
    }

    #[test]
    fn migrates_v11_search_metadata_and_marks_summaries_stale() {
        let directory = tempdir().expect("temp dir");
        let database_path = directory.path().join("library.sqlite3");
        let replay_path = directory.path().join("sample.wicdemo");
        {
            let mut database = Database::open(&database_path).expect("current database");
            database
                .upsert_summary(&summary(replay_path.clone()))
                .expect("store summary");
        }

        let connection = Connection::open(&database_path).expect("schema fixture");
        connection
            .execute_batch(
                "ALTER TABLE replay_summaries DROP COLUMN server_name;
                 ALTER TABLE replay_summaries DROP COLUMN factions;
                     ALTER TABLE replay_summaries DROP COLUMN search_players;
                 PRAGMA user_version = 11;",
            )
            .expect("simulate v11 database");
        drop(connection);

        let database = Database::open(&database_path).expect("migrated database");
        let summaries = database.load_summaries().expect("summaries");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].server_name, "");
        assert_eq!(summaries[0].factions, "");
        assert!(summaries[0].stale());
        assert_eq!(
            database
                .connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
                .expect("schema version"),
            DATABASE_SCHEMA_VERSION
        );
    }
    #[test]
    fn migrates_v12_player_search_without_dropping_lazy_details() {
        let directory = tempdir().unwrap();
        let database_path = directory.path().join("library.sqlite3");
        let replay_path = directory.path().join("sample.wicdemo");
        let row = summary(replay_path.clone());
        {
            let mut database = Database::open(&database_path).unwrap();
            database.upsert_summary(&row).unwrap();
            database
                .save_detail(&replay_path, row.fingerprint, 18, "cached detail")
                .unwrap();
            database
                .connection
                .execute_batch(
                    "ALTER TABLE replay_summaries DROP COLUMN search_players;
                 PRAGMA user_version = 12;",
                )
                .unwrap();
        }
        let mut database = Database::open(&database_path).unwrap();
        let migrated = database.load_summaries().unwrap();
        assert!(migrated[0].stale());
        assert!(migrated[0].search_players.is_empty());
        assert_eq!(migrated[0].player_names, row.player_names);
        assert!(
            database
                .load_detail(&replay_path, row.fingerprint)
                .unwrap()
                .is_some()
        );
        database.upsert_summary(&row).unwrap();
        let refreshed = database.load_summaries().unwrap();
        assert!(!refreshed[0].stale());
        assert_eq!(refreshed[0].search_players, row.search_players);
    }
}
