//! Database module — SQLite connection pool, schema creation, and migrations.

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Current schema version. Increment when adding migrations.
pub const CURRENT_SCHEMA_VERSION: i32 = 47;

struct Migration {
    version: i32,
    description: &'static str,
    apply: fn(&rusqlite::Connection) -> Result<()>,
}

/// Every migration this build knows how to run, in ascending version order.
/// `run_migrations` applies each entry whose `version` is newer than what's
/// on disk, in this array's order, then records it in `schema_version` —
/// exactly as if each were still its own `if version < N` block. Add new
/// migrations at the end and bump `CURRENT_SCHEMA_VERSION` to match.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_1)?),
    },
    Migration {
        version: 2,
        description: "equalizer settings",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_2)?),
    },
    Migration {
        version: 3,
        description: "unavailable flag for soft-deleted songs",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_3)?),
    },
    Migration {
        version: 4,
        description: "parametric equalizer mode",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_4)?),
    },
    Migration {
        version: 5,
        description: "loudness normalization (#77)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_5)?),
    },
    Migration {
        version: 6,
        description: "playlist last-updated tracking",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_6)?),
    },
    Migration {
        version: 7,
        description: "VBR/CBR bitrate flag",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_7)?),
    },
    Migration {
        version: 8,
        description: "instrumental track flag (#12)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_8)?),
    },
    Migration {
        version: 9,
        description: "auto_play flag for dynamic playlists (#26)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_9)?),
    },
    Migration {
        version: 10,
        description: "play_history for context-aware Recently Played",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_10)?),
    },
    Migration {
        version: 11,
        description: "drop unused excluded_formats setting",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_11)?),
    },
    Migration {
        version: 12,
        description: "queue population mode for auto/dynamic playlists (#120)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_12)?),
    },
    Migration {
        version: 13,
        description: "drop unused songs.mood column",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_13)?),
    },
    Migration {
        version: 14,
        description: "album_ratings table for independent album ratings (#242)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_14)?),
    },
    Migration {
        version: 15,
        description: "waveforms style column for cache invalidation",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_15)?),
    },
    Migration {
        version: 16,
        description: "rename moodbars table to band_waveforms (#217, pre-1.0 rename)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_16)?),
    },
    Migration {
        version: 17,
        description: "artist_profiles table for customizable artist website, tags, social links, bio (#473)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_17)?),
    },
    Migration {
        version: 18,
        description: "tag_groups/tag_assignments for persisted Genres curation hierarchy (#545)",
        apply: |conn| {
            conn.execute_batch(TAG_HIERARCHY_TABLES_SQL)?;
            seed_tag_hierarchy(conn)
        },
    },
    Migration {
        version: 19,
        description: "discard old bare-genre-name auto-playlist rows, superseded by the curated tag: convention (#548)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_19)?),
    },
    Migration {
        version: 20,
        description: "add genresort column to songs table (#151)",
        apply: |conn| {
            let has_genresort: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'genresort'")?
                .exists([])?;
            if !has_genresort {
                conn.execute_batch(MIGRATION_20)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 21,
        description: "pinned_items table for user-curated Home shelf (#222)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_21)?),
    },
    Migration {
        version: 22,
        description: "album_chart_history table for the weekly Top Albums chart (#662)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_22)?),
    },
    Migration {
        version: 23,
        description: "not_included flag to exclude songs from auto/smart-playlist generation (#104)",
        apply: |conn| {
            let has_not_included: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'not_included'")?
                .exists([])?;
            if !has_not_included {
                conn.execute_batch(MIGRATION_23)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 24,
        description: "release type/country/barcode/catalog number columns on songs table (#752)",
        apply: |conn| {
            let has_release_type: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'musicbrainz_release_type'")?
                .exists([])?;
            if !has_release_type {
                conn.execute_batch(MIGRATION_24)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 25,
        description: "nickname, icon, and color metadata columns on directories table (#124)",
        apply: |conn| {
            let has_nickname: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('directories') WHERE name = 'nickname'")?
                .exists([])?;
            if !has_nickname {
                conn.execute_batch(MIGRATION_25)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 26,
        description: "scrobble_cache table for offline scrobbles (#83)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_26)?),
    },
    Migration {
        version: 27,
        description: "play_history song_id index and stats_exclusions table for Personal Stats (#130)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_27)?),
    },
    Migration {
        version: 28,
        description: "context_enrichment/artist_context_enrichment cache tables for the Details pane (#23)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_28)?),
    },
    Migration {
        version: 29,
        description: "webdav_servers and webdav_cache tables for remote WebDAV library support (#682)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_29)?),
    },
    Migration {
        version: 30,
        description: "drop acoustid_id/acoustid_fingerprint/fingerprint columns — AcoustID support removed (#847)",
        apply: |conn| {
            let has_acoustid_id: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'acoustid_id'")?
                .exists([])?;
            if has_acoustid_id {
                conn.execute_batch(MIGRATION_30)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 31,
        description: "nickname, icon, and color metadata columns on webdav_servers table",
        apply: |conn| {
            let has_nickname: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('webdav_servers') WHERE name = 'nickname'")?
                .exists([])?;
            if !has_nickname {
                conn.execute_batch(MIGRATION_31)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 32,
        description: "duration_secs column on play_history for the daily listening heatmap (#890)",
        apply: |conn| {
            let has_duration_secs: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('play_history') WHERE name = 'duration_secs'")?
                .exists([])?;
            if !has_duration_secs {
                conn.execute_batch(MIGRATION_32)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 33,
        description: "dynamic range columns from foo_dr.txt DR Meter logs (#57)",
        apply: |conn| {
            let has_dynamic_range: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'dynamic_range'")?
                .exists([])?;
            if !has_dynamic_range {
                conn.execute_batch(MIGRATION_33)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 34,
        description: "relax songs.path from UNIQUE to UNIQUE(path, beginning_nanosec) for CUE sheet tracks (#78)",
        apply: rebuild_songs_table_without_path_unique,
    },
    Migration {
        version: 35,
        description: "update default target_lufs to -16.0 and drop unused crossfade keys (#946)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_35)?),
    },
    Migration {
        version: 36,
        description: "album_profiles table for album curation, notes, and external links (#950)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_36)?),
    },
    Migration {
        version: 37,
        description: "drop album_profiles.tags -- consolidated into the single embedded songs.genre tag list (#962)",
        apply: |conn| {
            let has_tags: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('album_profiles') WHERE name = 'tags'")?
                .exists([])?;
            if has_tags {
                conn.execute_batch(MIGRATION_37)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 38,
        description: "artist_tag_groups/artist_tag_assignments for single-layer artist tag hierarchy (#1105)",
        apply: |conn| {
            conn.execute_batch(ARTIST_TAG_HIERARCHY_TABLES_SQL)?;
            seed_artist_tag_hierarchy(conn)
        },
    },
    Migration {
        version: 39,
        description: "musicbrainz_artist_id column on artist_profiles for Retrieve Artist Details (#1123)",
        apply: |conn| {
            let has_musicbrainz_artist_id: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('artist_profiles') WHERE name = 'musicbrainz_artist_id'",
                )?
                .exists([])?;
            if !has_musicbrainz_artist_id {
                conn.execute_batch(MIGRATION_39)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 40,
        description: "auto_sync_enabled and sync_interval_minutes columns on webdav_servers for periodic auto-sync (#1082)",
        apply: |conn| {
            let has_auto_sync_enabled: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('webdav_servers') WHERE name = 'auto_sync_enabled'",
                )?
                .exists([])?;
            if !has_auto_sync_enabled {
                conn.execute_batch(MIGRATION_40)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 41,
        description: "fetched_image_filename/fetched_image_source columns on artist_profiles for Retrieve Artist Image (#1127)",
        apply: |conn| {
            let has_fetched_image_filename: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('artist_profiles') WHERE name = 'fetched_image_filename'",
                )?
                .exists([])?;
            if !has_fetched_image_filename {
                conn.execute_batch(MIGRATION_41)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 42,
        description: "sort_name, artist_type, gender, begin_date, end_date, ended, begin_area_name/mbid, area_name/mbid columns on artist_context_enrichment (#1128)",
        apply: |conn| {
            let has_sort_name: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('artist_context_enrichment') WHERE name = 'sort_name'",
                )?
                .exists([])?;
            if !has_sort_name {
                conn.execute_batch(MIGRATION_42)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 43,
        description: "details_fetched and image_fetched on artist_profiles, details_fetched on album_profiles (#1143)",
        apply: |conn| {
            let has_details_fetched: bool = conn
                .prepare(
                    "SELECT 1 FROM pragma_table_info('artist_profiles') WHERE name = 'details_fetched'",
                )?
                .exists([])?;
            if !has_details_fetched {
                conn.execute_batch(MIGRATION_43)?;
            }
            Ok(())
        },
    },    Migration {
        version: 44,
        description: "subsonic_servers, subsonic_cache, and subsonic_album_cache tables for OpenSubsonic servers (#916, #1161)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_44)?),
    },
    Migration {
        version: 45,
        description: "fingerprint column on subsonic_cache so a sync can skip unchanged tracks (#1162)",
        apply: |conn| {
            let has_fingerprint: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('subsonic_cache') WHERE name = 'fingerprint'")?
                .exists([])?;
            if !has_fingerprint {
                conn.execute_batch(MIGRATION_45)?;
            }
            Ok(())
        },
    },
    Migration {
        version: 46,
        description: "subsonic_scrobble_queue retry queue for plays reported to OpenSubsonic servers (#1165)",
        apply: |conn| Ok(conn.execute_batch(MIGRATION_46)?),
    },
    Migration {
        version: 47,
        description: "auth_mode column on subsonic_servers for legacy password and API key sign-in (#1167)",
        apply: |conn| {
            let has_auth_mode: bool = conn
                .prepare("SELECT 1 FROM pragma_table_info('subsonic_servers') WHERE name = 'auth_mode'")?
                .exists([])?;
            if !has_auth_mode {
                conn.execute_batch(MIGRATION_47)?;
            }
            Ok(())
        },
    },
];

#[derive(Debug)]
pub struct Database {
    pub pool: DbPool,
    /// Schema version found in `schema_version` on open, after any migrations this
    /// build knows how to run. Normally equal to `CURRENT_SCHEMA_VERSION`, but can be
    /// higher if this database was last opened by a newer build of the app (older
    /// builds only ever migrate upward, never down) — see `is_newer_than_app`.
    pub schema_version: i32,
}

impl Database {
    /// True when this database's schema is ahead of what this build knows how to
    /// read/write — e.g. a newer build ran migrations this older binary has never
    /// seen. Queries that name columns added/removed by those migrations will fail
    /// even though the app otherwise starts fine.
    pub fn is_newer_than_app(&self) -> bool {
        self.schema_version > CURRENT_SCHEMA_VERSION
    }

    /// Create (or open) the Luminous database in `app_data_dir/luminous.db`.
    pub fn new(app_data_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&app_data_dir).context("failed to create app data directory")?;

        let db_path = app_data_dir.join("luminous.db");
        log::info!("Opening database: {}", db_path.display());

        let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
            // Performance pragmas applied to every new connection
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                     PRAGMA synchronous=NORMAL;
                     PRAGMA foreign_keys=ON;
                     PRAGMA busy_timeout=5000;
                     PRAGMA cache_size=-32000;  -- 32 MB page cache
                     PRAGMA temp_store=MEMORY;",
            )
        });

        let pool = r2d2::Pool::builder()
            .max_size(8)
            .build(manager)
            .context("failed to create connection pool")?;

        let db = Self {
            pool,
            schema_version: 0,
        };
        let schema_version = db.run_migrations()?;

        Ok(Self {
            schema_version,
            ..db
        })
    }

    /// Runs any migrations this build knows about and returns the resulting schema
    /// version. If the database is already ahead of `CURRENT_SCHEMA_VERSION` (opened
    /// by a newer build previously), every `version < N` check below is false, so no
    /// migration runs and the on-disk version is returned unchanged — see
    /// `is_newer_than_app`.
    fn run_migrations(&self) -> Result<i32> {
        let conn = self.pool.get().context("failed to get db connection")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);",
        )?;

        let version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        log::info!("Database schema version: {version} (current: {CURRENT_SCHEMA_VERSION})");

        // Check each migration's own recorded row rather than relying on MAX(version)
        // being contiguous — an interrupted run in the past can leave a gap (e.g. a
        // later migration's row present but an earlier one's missing), and MAX alone
        // would then skip that earlier migration forever since it never re-evaluates
        // versions below the max.
        let mut applied_versions: std::collections::HashSet<i32> = {
            let mut stmt = conn.prepare("SELECT version FROM schema_version")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        for migration in MIGRATIONS {
            if !applied_versions.contains(&migration.version) {
                log::info!(
                    "Running migration {}: {}",
                    migration.version,
                    migration.description
                );
                (migration.apply)(&conn)?;
                conn.execute(
                    "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                    params![migration.version],
                )?;
                applied_versions.insert(migration.version);
            }
        }

        if version > CURRENT_SCHEMA_VERSION {
            log::warn!(
                "Database schema version {version} is newer than this app supports (max {CURRENT_SCHEMA_VERSION}) — was this database last opened by a newer version of Luminous? Skipping migrations; some data may not load until you update the app."
            );
            // No migration above ran (every `version < N` check was false), so the
            // on-disk version is unchanged.
            return Ok(version);
        }

        // Every migration up to CURRENT_SCHEMA_VERSION ran above.
        Ok(CURRENT_SCHEMA_VERSION)
    }
}

/// Runs a synchronous rusqlite operation on a blocking thread rather than the
/// calling `async fn`'s tokio worker — command handlers that don't hold an
/// `AppState` mutex still ran DB work inline on the worker, which contributes
/// to the same scheduler-stall pattern #1097 fixed for the locked cases
/// (#1102). Mirrors `Player::load_loudness_settings`, the existing instance
/// of this shape; callers not already inside a `PlaylistManager`/`Player`/
/// `AudioEngine` method should use this instead of hand-rolling
/// `tokio::task::spawn_blocking` + `pool.get()`.
pub async fn run_blocking<F, R>(db: &Arc<Database>, f: F) -> Result<R>
where
    F: FnOnce(&rusqlite::Connection) -> Result<R> + Send + 'static,
    R: Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.pool.get().context("failed to get db connection")?;
        f(&conn)
    })
    .await
    .context("db task panicked")?
}

// ---------------------------------------------------------------------------
// Migration 1: Full initial schema
// ---------------------------------------------------------------------------

const MIGRATION_1: &str = "
CREATE TABLE IF NOT EXISTS songs (
    id                                INTEGER PRIMARY KEY AUTOINCREMENT,
    source                            INTEGER NOT NULL DEFAULT 0,
    filetype                          INTEGER NOT NULL DEFAULT 0,
    path                              TEXT UNIQUE,
    url                               TEXT,
    stream_url                        TEXT,
    title                             TEXT,
    titlesort                         TEXT,
    artist                            TEXT,
    artistsort                        TEXT,
    album                             TEXT,
    albumsort                         TEXT,
    album_artist                      TEXT,
    album_artist_sort                 TEXT,
    composer                          TEXT,
    composersort                      TEXT,
    performer                         TEXT,
    performersort                     TEXT,
    grouping                          TEXT,
    comment                           TEXT,
    lyrics                            TEXT,
    track                             INTEGER,
    disc                              INTEGER,
    year                              INTEGER,
    originalyear                      INTEGER,
    genre                             TEXT,
    compilation                       BOOLEAN NOT NULL DEFAULT 0,
    bpm                               REAL,
    mood                              TEXT,
    initial_key                       TEXT,
    length_nanosec                    INTEGER,
    beginning_nanosec                 INTEGER NOT NULL DEFAULT 0,
    end_nanosec                       INTEGER NOT NULL DEFAULT 0,
    bitrate                           INTEGER,
    samplerate                        INTEGER,
    bitdepth                          INTEGER,
    channels                          INTEGER,
    filesize                          INTEGER,
    mtime                             INTEGER,
    rating                            REAL NOT NULL DEFAULT -1,
    playcount                         INTEGER NOT NULL DEFAULT 0,
    skipcount                         INTEGER NOT NULL DEFAULT 0,
    lastplayed                        INTEGER,
    lastseen                          INTEGER,
    art_embedded                      BOOLEAN NOT NULL DEFAULT 0,
    art_automatic                     TEXT,
    art_manual                        TEXT,
    art_unset                         BOOLEAN NOT NULL DEFAULT 0,
    cue_path                          TEXT,
    acoustid_id                       TEXT,
    acoustid_fingerprint              TEXT,
    fingerprint                       TEXT,
    musicbrainz_album_artist_id       TEXT,
    musicbrainz_artist_id             TEXT,
    musicbrainz_original_artist_id    TEXT,
    musicbrainz_album_id              TEXT,
    musicbrainz_original_album_id     TEXT,
    musicbrainz_recording_id          TEXT,
    musicbrainz_track_id              TEXT,
    musicbrainz_disc_id               TEXT,
    musicbrainz_release_group_id      TEXT,
    musicbrainz_work_id               TEXT,
    ebur128_integrated_loudness_lufs  REAL,
    ebur128_loudness_range_lu         REAL,
    artist_id                         TEXT,
    album_id                          TEXT,
    song_id                           TEXT,
    added                             INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_songs_artist   ON songs(artist);
CREATE INDEX IF NOT EXISTS idx_songs_album    ON songs(album);
CREATE INDEX IF NOT EXISTS idx_songs_genre    ON songs(genre);
CREATE INDEX IF NOT EXISTS idx_songs_mtime    ON songs(mtime);
CREATE INDEX IF NOT EXISTS idx_songs_source   ON songs(source);

CREATE VIRTUAL TABLE IF NOT EXISTS songs_fts USING fts5(
    title, artist, album, album_artist, composer, performer, genre,
    content='songs',
    content_rowid='id'
);

-- FTS triggers to keep songs_fts in sync
CREATE TRIGGER IF NOT EXISTS songs_ai AFTER INSERT ON songs BEGIN
    INSERT INTO songs_fts(rowid, title, artist, album, album_artist, composer, performer, genre)
    VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
            new.composer, new.performer, new.genre);
END;

CREATE TRIGGER IF NOT EXISTS songs_ad AFTER DELETE ON songs BEGIN
    INSERT INTO songs_fts(songs_fts, rowid, title, artist, album, album_artist, composer, performer, genre)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist,
            old.composer, old.performer, old.genre);
END;

CREATE TRIGGER IF NOT EXISTS songs_au AFTER UPDATE ON songs BEGIN
    INSERT INTO songs_fts(songs_fts, rowid, title, artist, album, album_artist, composer, performer, genre)
    VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist,
            old.composer, old.performer, old.genre);
    INSERT INTO songs_fts(rowid, title, artist, album, album_artist, composer, performer, genre)
    VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
            new.composer, new.performer, new.genre);
END;

CREATE TABLE IF NOT EXISTS directories (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    path    TEXT UNIQUE NOT NULL,
    subdirs BOOLEAN NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS subdirectories (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    directory_id INTEGER NOT NULL REFERENCES directories(id) ON DELETE CASCADE,
    path         TEXT NOT NULL,
    mtime        INTEGER,
    UNIQUE(directory_id, path)
);

CREATE TABLE IF NOT EXISTS playlists (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL,
    dynamic_enabled BOOLEAN NOT NULL DEFAULT 0,
    dynamic_spec    TEXT,
    last_played_row INTEGER,
    created         INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS playlist_items (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    playlist_id         INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    song_id             INTEGER REFERENCES songs(id) ON DELETE SET NULL,
    position            INTEGER NOT NULL,
    uuid                TEXT NOT NULL,
    type                INTEGER NOT NULL DEFAULT 0,
    url                 TEXT,
    stream_url          TEXT,
    additional_metadata TEXT
);

CREATE INDEX IF NOT EXISTS idx_playlist_items_playlist ON playlist_items(playlist_id, position);

CREATE TABLE IF NOT EXISTS waveforms (
    song_id  INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    data     BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS moodbars (
    song_id  INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    data     BLOB NOT NULL,
    style    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS radio_channels (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source        INTEGER NOT NULL,
    name          TEXT,
    url           TEXT,
    thumbnail_url TEXT,
    country       TEXT,
    tags          TEXT,
    codec         TEXT
);
";

const MIGRATION_2: &str = "
CREATE TABLE IF NOT EXISTS equalizer_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    preamp REAL NOT NULL DEFAULT 0.0,
    gains TEXT NOT NULL DEFAULT '0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0'
);
INSERT OR IGNORE INTO equalizer_settings (id, enabled, preamp, gains) VALUES (1, 0, 0.0, '0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0');

CREATE TABLE IF NOT EXISTS app_state (
    key TEXT PRIMARY KEY,
    value TEXT
);
";

// ---------------------------------------------------------------------------
// Migration 3: soft-delete support for missing songs
// ---------------------------------------------------------------------------

const MIGRATION_3: &str = "
ALTER TABLE songs ADD COLUMN unavailable BOOLEAN NOT NULL DEFAULT 0;
";

// ---------------------------------------------------------------------------
// Migration 4: parametric equalizer mode
//   mode:       'graphic10' | 'parametric20'
//   parametric: JSON array of 20 {freq, gain_db, q} bands ('' = defaults)
// ---------------------------------------------------------------------------

const MIGRATION_4: &str = "
ALTER TABLE equalizer_settings ADD COLUMN mode TEXT NOT NULL DEFAULT 'graphic10';
ALTER TABLE equalizer_settings ADD COLUMN parametric TEXT NOT NULL DEFAULT '';
";

// ---------------------------------------------------------------------------
// Migration 5: loudness normalization (#77) — EBU R128 analysis with
// ReplayGain 2.0 tag fallback. `replaygain_*_gain` are stored normalized to
// the classic -18 LUFS ReplayGain reference level (R128_* Opus tags are
// converted from their -23 LUFS reference at ingestion time).
// ---------------------------------------------------------------------------

const MIGRATION_5: &str = "
ALTER TABLE songs ADD COLUMN replaygain_track_gain REAL;
ALTER TABLE songs ADD COLUMN replaygain_album_gain REAL;

CREATE TABLE IF NOT EXISTS loudness_settings (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    enabled          INTEGER NOT NULL DEFAULT 0,
    target_lufs      REAL NOT NULL DEFAULT -18.0,
    mode             TEXT NOT NULL DEFAULT 'track',
    fallback_gain_db REAL NOT NULL DEFAULT -6.0
);
INSERT OR IGNORE INTO loudness_settings (id, enabled, target_lufs, mode, fallback_gain_db)
    VALUES (1, 0, -18.0, 'track', -6.0);
";

// ---------------------------------------------------------------------------
// Migration 6: playlist last-updated tracking. `updated` is bumped whenever a
// playlist's contents or name change (or, for genre auto-playlists, whenever
// they're regenerated) — `created` remains the original creation timestamp.
// ---------------------------------------------------------------------------

const MIGRATION_6: &str = "
ALTER TABLE playlists ADD COLUMN updated INTEGER;
UPDATE playlists SET updated = created WHERE updated IS NULL;
";

// ---------------------------------------------------------------------------
// Migration 7: VBR/CBR bitrate flag
// ---------------------------------------------------------------------------

const MIGRATION_7: &str = "
ALTER TABLE songs ADD COLUMN is_vbr BOOLEAN;
";

// ---------------------------------------------------------------------------
// Migration 8: instrumental track flag (#12)
// ---------------------------------------------------------------------------

const MIGRATION_8: &str = "
ALTER TABLE songs ADD COLUMN is_instrumental BOOLEAN NOT NULL DEFAULT 0;
";

// ---------------------------------------------------------------------------
// Migration 9: auto_play flag for dynamic/auto playlists (#26)
// ---------------------------------------------------------------------------

const MIGRATION_9: &str = "
ALTER TABLE playlists ADD COLUMN auto_play BOOLEAN NOT NULL DEFAULT 1;
UPDATE playlists SET auto_play = 1 WHERE dynamic_enabled = 1;
";

// ---------------------------------------------------------------------------
// Migration 10: play_history for context-aware Recently Played
// ---------------------------------------------------------------------------

const MIGRATION_10: &str = "
CREATE TABLE IF NOT EXISTS play_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    context_type TEXT NOT NULL,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    playlist_id INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
    played_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_play_history_played_at ON play_history(played_at DESC);
";

// ---------------------------------------------------------------------------
// Migration 11: drop the excluded_formats app_state row — the File Formats
// settings tab that wrote it was removed; all supported formats are always
// included now (#109).
// ---------------------------------------------------------------------------

const MIGRATION_11: &str = "
DELETE FROM app_state WHERE key = 'excluded_formats';
";

// ---------------------------------------------------------------------------
// Migration 12: queue population mode (All/Favourites/Familiar/Discover/Deep
// Cuts) for auto- and smart-playlists (#120)
// ---------------------------------------------------------------------------

const MIGRATION_12: &str = "
ALTER TABLE playlists ADD COLUMN population_mode TEXT NOT NULL DEFAULT 'all';
";

// ---------------------------------------------------------------------------
// Migration 13: drop songs.mood — never populated from file tags (no read or
// write path existed) and unreachable from search/Smart Playlists; orphaned
// column.
// ---------------------------------------------------------------------------

const MIGRATION_13: &str = "
ALTER TABLE songs DROP COLUMN mood;
";

// ---------------------------------------------------------------------------
// Migration 14: album_ratings — independent album-level ratings, separate
// from song ratings. Keyed by album title, matching how CollectionScanner::
// get_albums already groups albums (by title alone, not artist) (#242).
// ---------------------------------------------------------------------------

const MIGRATION_14: &str = "
CREATE TABLE IF NOT EXISTS album_ratings (
    album_key TEXT PRIMARY KEY,
    rating REAL NOT NULL DEFAULT -1
);
";

const MIGRATION_15: &str = "
ALTER TABLE waveforms ADD COLUMN style INTEGER NOT NULL DEFAULT 0;
";

// Pre-1.0 rename: the table originally stored a single blended mood color per
// point (#217 replaced that with independent low/mid/high band layers), so
// "moodbars" no longer describes what it holds.
const MIGRATION_16: &str = "
ALTER TABLE moodbars RENAME TO band_waveforms;
";

// ---------------------------------------------------------------------------
// Migration 17: artist_profiles — customizable artist website, tags,
// social links, and bio (#473). Keyed by artist name matching effective_artist.
// ---------------------------------------------------------------------------
const MIGRATION_17: &str = "
CREATE TABLE IF NOT EXISTS artist_profiles (
    artist_key TEXT PRIMARY KEY,
    website TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    social_links TEXT NOT NULL DEFAULT '[]',
    bio TEXT
);
";

// ---------------------------------------------------------------------------
// Migration 19: discard old-convention genre auto-playlist rows (#548).
// Genre auto-playlists used to key `dynamic_spec` on the bare, full raw
// `songs.genre` string (e.g. "Rock", "Metal; Symphonic Metal") — one row per
// distinct string. #548 replaces that with one row per curated tag
// (`tag_groups`/`tag_assignments`, #545), keyed as `tag:<name>`. There's no
// way to map an old bare-string row onto the new convention (it may combine
// several curated tags, or not correspond to any curated tag at all), so
// these rows are simply discarded rather than migrated in place —
// `sync_all_auto_playlists`, run once at startup right after migrations
// (see lib.rs's `setup()`), immediately rebuilds fresh `tag:` rows from the
// curated hierarchy, so this is a convention change on regenerable system
// rows, not a loss of user data. `decade:`/`bpmrange:` auto-playlists and
// user-authored Smart Playlist rule specs (which always contain a
// `field:value` rule, e.g. "artist:Miles Davis") are untouched.
// ---------------------------------------------------------------------------
const MIGRATION_19: &str = "
DELETE FROM playlist_items WHERE playlist_id IN (
    SELECT id FROM playlists
    WHERE dynamic_enabled = 1
      AND dynamic_spec NOT LIKE 'decade:%'
      AND dynamic_spec NOT LIKE 'bpmrange:%'
      AND dynamic_spec NOT LIKE '%:%'
);
DELETE FROM playlists
WHERE dynamic_enabled = 1
  AND dynamic_spec NOT LIKE 'decade:%'
  AND dynamic_spec NOT LIKE 'bpmrange:%'
  AND dynamic_spec NOT LIKE '%:%';
";

// ---------------------------------------------------------------------------
// Migration 20: genresort column on songs table (#151)
// ---------------------------------------------------------------------------
const MIGRATION_20: &str = "
ALTER TABLE songs ADD COLUMN genresort TEXT;
";

const MIGRATION_21: &str = "
CREATE TABLE IF NOT EXISTS pinned_items (
    item_type TEXT NOT NULL,
    ref_key   TEXT NOT NULL,
    position  INTEGER NOT NULL DEFAULT 0,
    pinned_at INTEGER NOT NULL,
    PRIMARY KEY (item_type, ref_key)
);
";

// ---------------------------------------------------------------------------
// Migration 22: album_chart_history — weekly snapshot of the Home "Top
// Albums" chart ranking (#662). Written lazily (no scheduler): each call to
// `CollectionScanner::get_top_albums` upserts the current UTC calendar
// week's rows, then reads prior weeks from this table to derive movement,
// peak rank, and weeks-on-chart. `album_key` matches the raw album title
// convention already used by `album_ratings` (see `stats::set_album_rating`).
// ---------------------------------------------------------------------------
const MIGRATION_22: &str = "
CREATE TABLE IF NOT EXISTS album_chart_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    period_start INTEGER NOT NULL,
    album_key TEXT NOT NULL,
    rank INTEGER NOT NULL,
    play_count INTEGER NOT NULL,
    UNIQUE(period_start, album_key)
);
CREATE INDEX IF NOT EXISTS idx_album_chart_history_album_key ON album_chart_history(album_key);
CREATE INDEX IF NOT EXISTS idx_album_chart_history_period ON album_chart_history(period_start DESC);
";

// ---------------------------------------------------------------------------
// Migration 23: not_included flag (#104)
// ---------------------------------------------------------------------------

const MIGRATION_23: &str = "
ALTER TABLE songs ADD COLUMN not_included BOOLEAN NOT NULL DEFAULT 0;
";

// ---------------------------------------------------------------------------
// Migration 24: release type/country/barcode/catalog number columns (#752).
// Adjacent to the MusicBrainz IDs but not IDs themselves — descriptive
// release metadata Picard writes alongside them.
// ---------------------------------------------------------------------------
const MIGRATION_24: &str = "
ALTER TABLE songs ADD COLUMN musicbrainz_release_type TEXT;
ALTER TABLE songs ADD COLUMN musicbrainz_release_country TEXT;
ALTER TABLE songs ADD COLUMN barcode TEXT;
ALTER TABLE songs ADD COLUMN catalog_number TEXT;
";

// ---------------------------------------------------------------------------
// Migration 25: nickname, icon, and color columns on directories table (#124).
// ---------------------------------------------------------------------------
const MIGRATION_25: &str = "
ALTER TABLE directories ADD COLUMN nickname TEXT;
ALTER TABLE directories ADD COLUMN icon TEXT;
ALTER TABLE directories ADD COLUMN color TEXT;
";

// ---------------------------------------------------------------------------
// Migration 26: scrobble_cache table for offline scrobbles (#83).
// ---------------------------------------------------------------------------
const MIGRATION_26: &str = "
CREATE TABLE IF NOT EXISTS scrobble_cache (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    service            TEXT NOT NULL DEFAULT 'listenbrainz',
    artist             TEXT NOT NULL,
    track              TEXT NOT NULL,
    album              TEXT,
    duration_ms        INTEGER,
    track_number       INTEGER,
    recording_mbid     TEXT,
    release_mbid       TEXT,
    artist_mbids       TEXT,
    release_group_mbid TEXT,
    track_mbid         TEXT,
    listened_at        INTEGER NOT NULL,
    attempts           INTEGER NOT NULL DEFAULT 0,
    last_attempt       INTEGER,
    last_error         TEXT,
    created_at         INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_scrobble_cache_created ON scrobble_cache(created_at);
";

// ---------------------------------------------------------------------------
// Migration 27: play_history.song_id index (aggregation joins for Personal
// Stats previously had no index to use) and stats_exclusions — a generic
// per-entity-kind exclusion table for the "Don't include in stats" flag
// (#130). One table instead of per-entity boolean columns because most of
// these entities (album/artist/genre) are denormalized text on `songs`, not
// rows with their own primary key — `entity_key` follows the same raw-string
// keying convention as `album_ratings.album_key`/`artist_profiles.artist_key`.
// ---------------------------------------------------------------------------
const MIGRATION_27: &str = "
CREATE INDEX IF NOT EXISTS idx_play_history_song_id ON play_history(song_id);
CREATE TABLE IF NOT EXISTS stats_exclusions (
    entity_type TEXT NOT NULL,
    entity_key TEXT NOT NULL,
    PRIMARY KEY (entity_type, entity_key)
);
";

// ---------------------------------------------------------------------------
// Migration 28: context_enrichment/artist_context_enrichment — cached results
// from the Details pane's external lookups (MusicBrainz ratings/genres/tags,
// CritiqueBrainz reviews, Wikipedia bio), keyed on the MusicBrainz release
// group / artist IDs already stored on `songs` (see MIGRATION_1/24). `fetched_at`
// (unix seconds) is the first TTL column in this schema — read-time code treats
// a row older than 30 days as stale and refetches, rather than an explicit
// expiry mechanism here.
// ---------------------------------------------------------------------------
const MIGRATION_28: &str = "
CREATE TABLE IF NOT EXISTS context_enrichment (
    release_group_id TEXT PRIMARY KEY,
    mb_rating REAL,
    mb_rating_votes INTEGER,
    mb_tags TEXT NOT NULL DEFAULT '[]',
    mb_release_country TEXT,
    critiquebrainz_rating REAL,
    critiquebrainz_review_count INTEGER,
    critiquebrainz_review_links TEXT NOT NULL DEFAULT '[]',
    fetched_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS artist_context_enrichment (
    artist_id TEXT PRIMARY KEY,
    wikidata_id TEXT,
    wikipedia_extract TEXT,
    wikipedia_page_url TEXT,
    wikipedia_thumbnail_url TEXT,
    fetched_at INTEGER NOT NULL
);
";

// ---------------------------------------------------------------------------
// Migration 29: webdav_servers and webdav_cache — remote WebDAV library
// storage, sync states, and file metadata cache (#682).
// ---------------------------------------------------------------------------
const MIGRATION_29: &str = "
CREATE TABLE IF NOT EXISTS webdav_servers (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    url            TEXT NOT NULL,
    username       TEXT,
    password       TEXT,
    remote_path    TEXT NOT NULL DEFAULT '/',
    enabled        BOOLEAN NOT NULL DEFAULT 1,
    sync_status    TEXT NOT NULL DEFAULT 'idle',
    last_synced_at INTEGER,
    created_at     INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE TABLE IF NOT EXISTS webdav_cache (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id      INTEGER NOT NULL REFERENCES webdav_servers(id) ON DELETE CASCADE,
    remote_path    TEXT NOT NULL,
    etag           TEXT,
    size           INTEGER NOT NULL DEFAULT 0,
    last_modified  TEXT,
    song_id        INTEGER REFERENCES songs(id) ON DELETE SET NULL,
    cached_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(server_id, remote_path)
);
CREATE INDEX IF NOT EXISTS idx_webdav_cache_server ON webdav_cache(server_id);
";

// ---------------------------------------------------------------------------
// Migration 30: AcoustID support removed (#847) — drop the fingerprint-
// lookup columns. `fingerprint` (distinct from `acoustid_fingerprint`) was
// dead weight even before this: declared in the schema and the `Song`
// model but never populated or read anywhere.
// ---------------------------------------------------------------------------

const MIGRATION_30: &str = "
ALTER TABLE songs DROP COLUMN acoustid_id;
ALTER TABLE songs DROP COLUMN acoustid_fingerprint;
ALTER TABLE songs DROP COLUMN fingerprint;
";

// ---------------------------------------------------------------------------
// Migration 31: nickname, icon, and color metadata columns on webdav_servers
// — aligns WebDAV server badges with the watched-folder badges added in
// MIGRATION_25.
// ---------------------------------------------------------------------------
const MIGRATION_31: &str = "
ALTER TABLE webdav_servers ADD COLUMN nickname TEXT;
ALTER TABLE webdav_servers ADD COLUMN icon TEXT;
ALTER TABLE webdav_servers ADD COLUMN color TEXT;
";

// ---------------------------------------------------------------------------
// Migration 32: duration_secs column on play_history — the daily listening
// heatmap (#890) needs each play's length to sum minutes-listened per day,
// which play_history didn't previously record (only that a play happened).
// Existing rows default to 0 and are simply undercounted; there's no way to
// recover their original duration retroactively.
// ---------------------------------------------------------------------------
const MIGRATION_32: &str = "
ALTER TABLE play_history ADD COLUMN duration_secs INTEGER NOT NULL DEFAULT 0;
";

// ---------------------------------------------------------------------------
// Migration 33: dynamic range columns from foo_dr.txt DR Meter logs (#57).
// `dynamic_range`/`_peak`/`_rms` are per-track figures matched from the log's
// table; `dynamic_range_album` is the log's album-wide "Official DR value"
// (or "Weighted" variant), duplicated onto every song in the folder since
// there's no dedicated albums table (album-level fields like `album_artist`
// already follow this convention). `dr_log_mtime` is scanner bookkeeping
// only (not exposed to the frontend) — the modification time of the
// `foo_dr.txt` this song's row was last parsed from, so `collection.rs` can
// skip re-parsing a folder whose log hasn't changed since the last scan.
// ---------------------------------------------------------------------------
const MIGRATION_33: &str = "
ALTER TABLE songs ADD COLUMN dynamic_range INTEGER;
ALTER TABLE songs ADD COLUMN dynamic_range_peak REAL;
ALTER TABLE songs ADD COLUMN dynamic_range_rms REAL;
ALTER TABLE songs ADD COLUMN dynamic_range_album INTEGER;
ALTER TABLE songs ADD COLUMN dr_log_mtime INTEGER;
";

// ---------------------------------------------------------------------------
// Migration 34: relax `songs.path` from a plain UNIQUE column to a composite
// UNIQUE(path, beginning_nanosec) index, so multiple CUE sheet tracks (#78)
// can share one physical media file's path — differentiated by their INDEX 01
// start offset. Plain (non-CUE) songs keep beginning_nanosec = 0, so their
// uniqueness is unaffected.
//
// SQLite has no `ALTER TABLE ... DROP CONSTRAINT`, so this does the standard
// SQLite table rebuild: create `songs_new` without the column-level UNIQUE,
// copy every row across (`SELECT *`), drop the old table, and rename. Unlike
// every other migration in this file, the new table's column list isn't a
// fixed SQL string — it's built at runtime from `PRAGMA table_info(songs)`
// (see `rebuild_songs_table_without_path_unique` below), so this migration
// stays correct regardless of which other, unrelated `songs`-column
// migrations happen to have already run on this database (a real scenario in
// this codebase: parallel feature branches occasionally pick the same next
// migration version number against a shared dev app-data folder, and
// whichever runs first "claims" that version — a fixed column list here
// would silently drop any column added by a same-numbered sibling migration
// instead of preserving it). `PRAGMA table_info` never reports a column-level
// UNIQUE (that's a separate index), so simply not re-declaring one is exactly
// how this drops it.
//
// The whole rebuild is one transaction, so a crash mid-migration leaves the
// original `songs` table untouched rather than half-renamed. `PRAGMA
// foreign_keys` is toggled off/on around (not inside) that transaction, per
// SQLite's rule that it can't change mid-transaction — other tables'
// `REFERENCES songs(id)` stay valid across the rebuild since every row keeps
// its original `id`, and SQLite re-syncs the AUTOINCREMENT sequence and the
// `sqlite_sequence` name entry automatically on RENAME TO.
// ---------------------------------------------------------------------------
fn rebuild_songs_table_without_path_unique(conn: &rusqlite::Connection) -> Result<()> {
    // Idempotency / interrupted-run safety: if a previous attempt already got
    // as far as creating the new unique index, the rebuild already happened.
    let already_done: bool = conn
        .prepare(
            "SELECT 1 FROM sqlite_master WHERE type='index' AND tbl_name='songs' AND name='idx_songs_path_begin'",
        )?
        .exists([])?;
    if already_done {
        return Ok(());
    }

    struct ColumnDef {
        name: String,
        ty: String,
        notnull: bool,
        dflt_value: Option<String>,
        pk: bool,
    }

    let mut stmt = conn.prepare("SELECT cid, name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('songs') ORDER BY cid")?;
    let columns: Vec<ColumnDef> = stmt
        .query_map([], |row| {
            Ok(ColumnDef {
                name: row.get(1)?,
                ty: row.get(2)?,
                notnull: row.get::<_, i64>(3)? != 0,
                dflt_value: row.get(4)?,
                pk: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    let mut col_defs = Vec::with_capacity(columns.len());
    for col in &columns {
        let mut def = format!("{} {}", col.name, col.ty);
        if col.pk {
            // `id` is this table's only primary key column, and always has
            // been AUTOINCREMENT — `pragma_table_info` reports `pk` but not
            // AUTOINCREMENT, so that part is a known invariant, not derived.
            def.push_str(" PRIMARY KEY AUTOINCREMENT");
        } else if col.notnull {
            def.push_str(" NOT NULL");
        }
        if let Some(d) = &col.dflt_value {
            // `dflt_value` comes back with any wrapping parens already
            // stripped for expression defaults (e.g. `strftime(...)` for
            // `added`) but bare for literal defaults (e.g. `0`) — wrapping
            // every default in parens is valid SQLite for both cases, so
            // there's no need to tell them apart.
            def.push_str(&format!(" DEFAULT ({d})"));
        }
        col_defs.push(def);
    }
    let create_songs_new = format!("CREATE TABLE songs_new ({})", col_defs.join(", "));

    conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
    let rebuild = (|| -> Result<()> {
        conn.execute_batch("BEGIN TRANSACTION;")?;
        conn.execute_batch("DROP TABLE IF EXISTS songs_new;")?;
        conn.execute_batch(&create_songs_new)?;
        conn.execute_batch(
            "INSERT INTO songs_new SELECT * FROM songs;
             DROP TABLE songs;
             ALTER TABLE songs_new RENAME TO songs;

             CREATE UNIQUE INDEX IF NOT EXISTS idx_songs_path_begin ON songs(path, beginning_nanosec);
             CREATE INDEX IF NOT EXISTS idx_songs_artist   ON songs(artist);
             CREATE INDEX IF NOT EXISTS idx_songs_album    ON songs(album);
             CREATE INDEX IF NOT EXISTS idx_songs_genre    ON songs(genre);
             CREATE INDEX IF NOT EXISTS idx_songs_mtime    ON songs(mtime);
             CREATE INDEX IF NOT EXISTS idx_songs_source   ON songs(source);

             CREATE TRIGGER IF NOT EXISTS songs_ai AFTER INSERT ON songs BEGIN
                 INSERT INTO songs_fts(rowid, title, artist, album, album_artist, composer, performer, genre)
                 VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
                         new.composer, new.performer, new.genre);
             END;

             CREATE TRIGGER IF NOT EXISTS songs_ad AFTER DELETE ON songs BEGIN
                 INSERT INTO songs_fts(songs_fts, rowid, title, artist, album, album_artist, composer, performer, genre)
                 VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist,
                         old.composer, old.performer, old.genre);
             END;

             CREATE TRIGGER IF NOT EXISTS songs_au AFTER UPDATE ON songs BEGIN
                 INSERT INTO songs_fts(songs_fts, rowid, title, artist, album, album_artist, composer, performer, genre)
                 VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist,
                         old.composer, old.performer, old.genre);
                 INSERT INTO songs_fts(rowid, title, artist, album, album_artist, composer, performer, genre)
                 VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
                         new.composer, new.performer, new.genre);
             END;",
        )?;
        conn.execute_batch("COMMIT;")?;
        Ok(())
    })();
    if rebuild.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    // Always try to restore enforcement, even if the rebuild failed.
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    rebuild
}

// ---------------------------------------------------------------------------
// Migration 35: update default target_lufs from -18.0 to -16.0 (#946) and clean
// up dead crossfade keys from app_state.
// ---------------------------------------------------------------------------
const MIGRATION_35: &str = "
UPDATE loudness_settings SET target_lufs = -16.0 WHERE id = 1 AND target_lufs = -18.0;
DELETE FROM app_state WHERE key IN ('crossfade_manual_enabled', 'crossfade_manual_duration_ms');
";

// ---------------------------------------------------------------------------
// Migration 36: album_profiles — customizable album description/liner notes,
// website, curated tags, and release-specific external links (#950).
// Keyed by album name matching songs.album.
// ---------------------------------------------------------------------------
const MIGRATION_36: &str = "
CREATE TABLE IF NOT EXISTS album_profiles (
    album_key TEXT PRIMARY KEY,
    artist_key TEXT,
    description TEXT,
    website TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    links TEXT NOT NULL DEFAULT '[]'
);
";

// ---------------------------------------------------------------------------
// Migration 37: drop album_profiles.tags (#962) — album detail view had two
// independent, disagreeing tag lists: this curated DB-only column (editable
// via the album profile editor) and the embedded `songs.genre` ID3 tag
// (editable via the album tag editor), silently merged for display in the
// header's genre chip row so neither editor's own field matched what was
// shown. The two editors are consolidated into one, backed solely by the
// embedded genre tag — the only list that's ever actually written to disk —
// so there's exactly one source of truth. Any tags a user had already saved
// here are not migrated forward into `songs.genre`: this table shipped only
// on the still-unreleased 2.0 line, so no released version ever wrote real
// user data into it.
// ---------------------------------------------------------------------------
const MIGRATION_37: &str = "
ALTER TABLE album_profiles DROP COLUMN tags;
";

// ---------------------------------------------------------------------------
// Migration 18: tag_groups/tag_assignments — a persisted, curatable Genres
// hierarchy (#545) layered on top of the existing `songs.genre` string
// column. `songs.genre` remains the source of truth for which songs carry
// which tag; these tables only remember, per tag name, which primary genre
// "card" it's been curated under, its display color, and manual ordering —
// independent of any single song's own genre-list order (see tags.rs's
// `get_genre_graph` doc comment on why that per-song order can't serve as a
// stable hierarchy on its own).
// ---------------------------------------------------------------------------
/// Also used directly by `TagManager` (see `tags::ensure_hierarchy_tables`) to
/// self-heal regardless of what the on-disk `schema_version` claims — cheap
/// and idempotent (`CREATE TABLE IF NOT EXISTS`), so it's safe to re-run on
/// every `TagManager::new()` rather than trusting the migration bookkeeping
/// alone to have actually created these tables.
pub const TAG_HIERARCHY_TABLES_SQL: &str = "
CREATE TABLE IF NOT EXISTS tag_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color_index INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tag_assignments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tag_name    TEXT NOT NULL UNIQUE COLLATE NOCASE,
    group_id    INTEGER NOT NULL REFERENCES tag_groups(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
";

/// One-time seed for migration 18, run once right after the tables above are
/// created: derives an initial hierarchy from the same root/child convention
/// `TagManager::compute_graph` already uses (each song's first genre value is
/// its main category, the rest are subgenres), so upgrading users land on a
/// sensible starting point instead of an empty Genres page. Every root
/// becomes a `tag_groups` row (ordered by song count, colored round-robin
/// from the 10-hue palette the Genres UI uses); every child is assigned to
/// whichever root it appeared under most often. Purely a starting point —
/// from here on, `tag_groups`/`tag_assignments` are the persisted source of
/// truth and are only reconciled (not recomputed) against library changes,
/// see `tags::reconcile_hierarchy`. `INSERT OR IGNORE` throughout makes this
/// safe to re-run if migration 18 is ever interrupted before its version
/// marker is recorded.
fn seed_tag_hierarchy(conn: &rusqlite::Connection) -> Result<()> {
    use std::collections::HashMap;

    let sql = format!(
        "SELECT genre FROM songs
         WHERE source IN ({lib}) AND unavailable = 0 AND genre IS NOT NULL AND genre != ''",
        lib = *crate::models::LIBRARY_SOURCES_SQL
    );
    let mut stmt = conn.prepare(&sql)?;
    let lists: Vec<Vec<String>> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .map(|raw| {
            raw.split("; ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .collect();

    // root_key (lowercase) -> (display name, song count)
    let mut root_counts: HashMap<String, (String, i64)> = HashMap::new();
    // child_key (lowercase) -> (display name, root_key -> count)
    let mut child_roots: HashMap<String, (String, HashMap<String, i64>)> = HashMap::new();

    for values in &lists {
        let root = &values[0];
        let root_key = root.to_lowercase();
        root_counts
            .entry(root_key.clone())
            .or_insert_with(|| (root.clone(), 0))
            .1 += 1;
        for child in &values[1..] {
            let child_key = child.to_lowercase();
            let entry = child_roots
                .entry(child_key)
                .or_insert_with(|| (child.clone(), HashMap::new()));
            *entry.1.entry(root_key.clone()).or_insert(0) += 1;
        }
    }

    let mut roots: Vec<(String, String, i64)> = root_counts
        .into_iter()
        .map(|(key, (name, count))| (key, name, count))
        .collect();
    roots.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });

    let mut group_ids: HashMap<String, i64> = HashMap::new();
    for (i, (root_key, name, _count)) in roots.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO tag_groups (name, color_index, sort_order) VALUES (?1, ?2, ?3)",
            params![name, (i % 10) as i32, i as i32],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM tag_groups WHERE name = ?1 COLLATE NOCASE",
            params![name],
            |row| row.get(0),
        )?;
        group_ids.insert(root_key.clone(), id);
    }

    let mut sort_order = 0i32;
    for (name, per_root) in child_roots.into_values() {
        let best_root_key = per_root
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(root_key, _)| root_key);
        let Some(group_id) = best_root_key.and_then(|k| group_ids.get(&k)) else {
            continue;
        };
        conn.execute(
            "INSERT OR IGNORE INTO tag_assignments (tag_name, group_id, sort_order) VALUES (?1, ?2, ?3)",
            params![name, group_id, sort_order],
        )?;
        sort_order += 1;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Migration 38: artist_tag_groups/artist_tag_assignments — a persisted,
// curatable Artist Tags hierarchy (#1105).
// ---------------------------------------------------------------------------
pub const ARTIST_TAG_HIERARCHY_TABLES_SQL: &str = "
CREATE TABLE IF NOT EXISTS artist_tag_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color_index INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    is_custom   INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS artist_tag_assignments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    tag_name    TEXT NOT NULL UNIQUE COLLATE NOCASE,
    group_id    INTEGER NOT NULL REFERENCES artist_tag_groups(id) ON DELETE CASCADE,
    sort_order  INTEGER NOT NULL DEFAULT 0
);
";

// ---------------------------------------------------------------------------
// Migration 39: artist_profiles.musicbrainz_artist_id — the artist's
// MusicBrainz ID, distinct from any per-song tagged MBID, captured from a
// release-group's `artist-credit` during "Retrieve Album Details" or from a
// song's tagged MBID as a fallback. Drives "Retrieve Artist Details" (#1123).
// ---------------------------------------------------------------------------
const MIGRATION_39: &str = "
ALTER TABLE artist_profiles ADD COLUMN musicbrainz_artist_id TEXT;
";

// ---------------------------------------------------------------------------
// Migration 40: auto_sync_enabled and sync_interval_minutes on webdav_servers
// (#1082). WebDAV has no filesystem-watch equivalent to notice remote
// changes, so each server that opts in gets its own periodic-poll schedule
// instead — see `remote_scheduler::AutoSyncScheduler`.
// ---------------------------------------------------------------------------
const MIGRATION_40: &str = "
ALTER TABLE webdav_servers ADD COLUMN auto_sync_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE webdav_servers ADD COLUMN sync_interval_minutes INTEGER NOT NULL DEFAULT 60;
";

// ---------------------------------------------------------------------------
// Migration 41: artist_profiles.fetched_image_filename/fetched_image_source —
// an artist portrait fetched via "Retrieve Artist Image" (#1127) from fanart.tv
// or, lacking an API key/match, Wikidata's P18 property. Cached under
// `CoverManager`'s covers_dir, same convention as `songs.art_automatic`.
// ---------------------------------------------------------------------------
const MIGRATION_41: &str = "
ALTER TABLE artist_profiles ADD COLUMN fetched_image_filename TEXT;
ALTER TABLE artist_profiles ADD COLUMN fetched_image_source TEXT;
";

// ---------------------------------------------------------------------------
// Migration 42: artist_context_enrichment structured artist fields —
// sort name, gender, life span (begin, end, ended), birth/formation place
// (begin_area), and containing country/area from MusicBrainz (#1128).
// ---------------------------------------------------------------------------
const MIGRATION_42: &str = "
ALTER TABLE artist_context_enrichment ADD COLUMN sort_name TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN artist_type TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN gender TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN begin_date TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN end_date TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN ended INTEGER;
ALTER TABLE artist_context_enrichment ADD COLUMN begin_area_name TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN begin_area_mbid TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN area_name TEXT;
ALTER TABLE artist_context_enrichment ADD COLUMN area_mbid TEXT;
";

// ---------------------------------------------------------------------------
// Migration 43: details_fetched and image_fetched on artist_profiles,
// details_fetched on album_profiles (#1143).
// ---------------------------------------------------------------------------
const MIGRATION_43: &str = "
ALTER TABLE artist_profiles ADD COLUMN details_fetched INTEGER NOT NULL DEFAULT 0;
ALTER TABLE artist_profiles ADD COLUMN image_fetched INTEGER NOT NULL DEFAULT 0;
ALTER TABLE album_profiles ADD COLUMN details_fetched INTEGER NOT NULL DEFAULT 0;
";

// ---------------------------------------------------------------------------
// Migration 44: OpenSubsonic servers (#916, #1161). Songs synced from a
// server live in `songs` with `source = 5` and a credential-free
// `subsonic://{server_id}/{track_id}` path; `subsonic_cache` maps each remote
// track to its song row and remembers the last rating/star seen on the
// server, so a sync only adopts server-side changes made since then (and
// never clobbers a local edit). `subsonic_album_cache` does the same for
// album stars/ratings, keyed to `album_ratings.album_key`.
// ---------------------------------------------------------------------------
const MIGRATION_44: &str = "
CREATE TABLE IF NOT EXISTS subsonic_servers (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    name                  TEXT NOT NULL,
    url                   TEXT NOT NULL,
    username              TEXT NOT NULL,
    password              TEXT,
    enabled               BOOLEAN NOT NULL DEFAULT 1,
    sync_status           TEXT NOT NULL DEFAULT 'idle',
    last_synced_at        INTEGER,
    created_at            INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    nickname              TEXT,
    icon                  TEXT,
    color                 TEXT,
    auto_sync_enabled     INTEGER NOT NULL DEFAULT 0,
    sync_interval_minutes INTEGER NOT NULL DEFAULT 60,
    report_plays          INTEGER NOT NULL DEFAULT 1,
    server_type           TEXT,
    server_version        TEXT,
    extensions_json       TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS subsonic_cache (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id      INTEGER NOT NULL REFERENCES subsonic_servers(id) ON DELETE CASCADE,
    remote_id      TEXT NOT NULL,
    song_id        INTEGER REFERENCES songs(id) ON DELETE SET NULL,
    album_id       TEXT,
    cover_art_id   TEXT,
    server_rating  INTEGER,
    server_starred INTEGER NOT NULL DEFAULT 0,
    cached_at      INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(server_id, remote_id)
);
CREATE INDEX IF NOT EXISTS idx_subsonic_cache_song ON subsonic_cache(song_id);

CREATE TABLE IF NOT EXISTS subsonic_album_cache (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id       INTEGER NOT NULL REFERENCES subsonic_servers(id) ON DELETE CASCADE,
    remote_album_id TEXT NOT NULL,
    album_key       TEXT NOT NULL,
    server_rating   INTEGER,
    server_starred  INTEGER NOT NULL DEFAULT 0,
    cached_at       INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(server_id, remote_album_id)
);
CREATE INDEX IF NOT EXISTS idx_subsonic_album_cache_key ON subsonic_album_cache(album_key);
";

// ---------------------------------------------------------------------------
// Migration 45: a hash of each synced track's mapped metadata, so a re-sync
// only rewrites the `songs` rows whose server-side metadata changed (#1162).
// ---------------------------------------------------------------------------
const MIGRATION_45: &str = "
ALTER TABLE subsonic_cache ADD COLUMN fingerprint TEXT;
";

// ---------------------------------------------------------------------------
// Migration 46: plays that couldn't be reported to an OpenSubsonic server
// (offline, timeout) wait here and are retried on the next report (#1165).
// Kept apart from `scrobble_cache`, which drains to ListenBrainz only.
// ---------------------------------------------------------------------------
const MIGRATION_46: &str = "
CREATE TABLE IF NOT EXISTS subsonic_scrobble_queue (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   INTEGER NOT NULL REFERENCES subsonic_servers(id) ON DELETE CASCADE,
    remote_id   TEXT NOT NULL,
    listened_at INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_subsonic_scrobble_queue_server ON subsonic_scrobble_queue(server_id);
";

// ---------------------------------------------------------------------------
// Migration 47: per-server sign-in method (#1167) — 'token' (salted token,
// the default), 'password' (legacy p=enc:), or 'apiKey' (OpenSubsonic API
// key, held in the password column).
// ---------------------------------------------------------------------------
const MIGRATION_47: &str = "
ALTER TABLE subsonic_servers ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'token';
";

fn seed_artist_tag_hierarchy(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT json_each.value
         FROM artist_profiles, json_each(artist_profiles.tags)
         ORDER BY json_each.value COLLATE NOCASE",
    )?;
    let tags: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for (i, tag) in tags.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO artist_tag_groups (name, color_index, sort_order) VALUES (?1, ?2, ?3)",
            params![tag, (i % 10) as i32, i as i32],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_initialization() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();

        let conn = db.pool.get().unwrap();
        let tables_count: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('songs', 'directories', 'playlists')",
            [],
            |r| r.get(0)
        ).unwrap();
        assert_eq!(tables_count, 3);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_schema_newer_than_app_detected_without_running_migrations_backward() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!db.is_newer_than_app());
        drop(db);

        // Simulate a newer build having stamped a future schema version onto
        // this database, as if it were opened by that build previously.
        {
            let manager = SqliteConnectionManager::file(temp_dir.join("luminous.db"));
            let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![CURRENT_SCHEMA_VERSION + 1],
            )
            .unwrap();
        }

        // Reopening with this (older) build must not error and must not try
        // to run migrations backward — it should just surface the mismatch.
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION + 1);
        assert!(db.is_newer_than_app());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_reopen_heals_a_gap_left_by_an_interrupted_migration() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration_gap_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        // Simulate an interrupted migration 23: its schema_version row never
        // got written even though every migration around it did, and its
        // column was never added — as would happen if the app crashed
        // between `apply()` completing for a later migration and 23's own
        // `INSERT INTO schema_version`. A MAX(version)-based runner would
        // see 26 as the max and conclude 23 (< 26) already ran.
        {
            let conn = db.pool.get().unwrap();
            conn.execute("DELETE FROM schema_version WHERE version = 23", [])
                .unwrap();
            conn.execute("ALTER TABLE songs DROP COLUMN not_included", [])
                .unwrap();
        }
        drop(db);

        let reopened = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(reopened.schema_version, CURRENT_SCHEMA_VERSION);
        let conn = reopened.pool.get().unwrap();
        let has_not_included: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('songs') WHERE name = 'not_included'")
            .unwrap()
            .exists([])
            .unwrap();
        assert!(
            has_not_included,
            "reopening should have re-run migration 23 and healed the gap"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_34_relaxes_path_uniqueness_for_cue_tracks() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration34_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("luminous.db");

        // Build the pre-migration-34 schema by replaying every earlier
        // migration directly, exactly as `run_migrations` would have left a
        // real database that was last opened before this migration existed.
        {
            let manager = SqliteConnectionManager::file(&db_path);
            let pool = r2d2::Pool::builder().max_size(1).build(manager).unwrap();
            let conn = pool.get().unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY);",
            )
            .unwrap();
            for migration in MIGRATIONS.iter().filter(|m| m.version < 34) {
                (migration.apply)(&conn).unwrap();
                conn.execute(
                    "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                    params![migration.version],
                )
                .unwrap();
            }

            // Simulate schema drift from an unrelated sibling migration that
            // happened to run against this same database first (a real
            // scenario: two feature branches independently bumped
            // CURRENT_SCHEMA_VERSION to the same number against a shared dev
            // app-data folder). The rebuild must preserve this column even
            // though it's never mentioned in this migration's own code.
            conn.execute_batch("ALTER TABLE songs ADD COLUMN unrelated_sibling_column TEXT;")
                .unwrap();

            conn.execute(
                "INSERT INTO songs (path, title, unrelated_sibling_column) VALUES ('shared.flac', 'Whole File', 'kept')",
                [],
            )
            .unwrap();
            let webdav_song_id: i64 = conn
                .query_row("SELECT id FROM songs WHERE path = 'shared.flac'", [], |r| {
                    r.get(0)
                })
                .unwrap();

            // The pre-migration-33 schema must still enforce UNIQUE(path).
            let dup = conn.execute(
                "INSERT INTO songs (path, title, beginning_nanosec) VALUES ('shared.flac', 'Track 2', 5)",
                [],
            );
            assert!(
                dup.is_err(),
                "old schema should still reject a bare duplicate path"
            );

            // A row with a foreign key into songs(id), to confirm the rebuild
            // doesn't orphan or corrupt it.
            conn.execute(
                "INSERT INTO webdav_servers (id, name, url) VALUES (1, 'test', 'https://example.com')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO webdav_cache (server_id, remote_path, song_id) VALUES (1, '/shared.flac', ?1)",
                params![webdav_song_id],
            )
            .unwrap();
        }

        // Reopening through the normal path runs (only) migration 33.
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);
        let conn = db.pool.get().unwrap();

        let (title, id, sibling_col): (String, i64, String) = conn
            .query_row(
                "SELECT title, id, unrelated_sibling_column FROM songs WHERE path = 'shared.flac' AND beginning_nanosec = 0",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(title, "Whole File");
        assert_eq!(
            sibling_col, "kept",
            "a column added by an unrelated sibling migration must survive the rebuild"
        );

        let cached_song_id: i64 = conn
            .query_row(
                "SELECT song_id FROM webdav_cache WHERE remote_path = '/shared.flac'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            cached_song_id, id,
            "foreign key into songs(id) must survive the table rebuild"
        );

        // The new schema must allow a CUE sibling: same path, different
        // beginning_nanosec.
        conn.execute(
            "INSERT INTO songs (path, title, beginning_nanosec) VALUES ('shared.flac', 'Track 2', 5)",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM songs WHERE path = 'shared.flac'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        // But an exact duplicate (same path AND beginning_nanosec) is still rejected.
        let dup2 = conn.execute(
            "INSERT INTO songs (path, title, beginning_nanosec) VALUES ('shared.flac', 'Track 2 Again', 5)",
            [],
        );
        assert!(
            dup2.is_err(),
            "new schema should still reject an exact (path, beginning_nanosec) duplicate"
        );

        drop(conn);
        drop(db);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_19_discards_old_bare_genre_rows_but_keeps_others() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration19_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        // Simulate a pre-migration database: stamp an earlier version and
        // seed rows in every convention migration 19 must tell apart.
        // `schema_version` accumulates one row per version ever applied
        // (`MAX(version)` is what's read back), so downgrading requires
        // clearing every row at/above 19, not just upserting a row for 18 —
        // that alone would leave the already-inserted 19 row as the max and
        // migration 19 would look already-applied on reopen.
        {
            let conn = db.pool.get().unwrap();
            conn.execute("DELETE FROM schema_version WHERE version >= 19", [])
                .unwrap();
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (18)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec) VALUES ('Rock', 1, 'Rock')",
                [],
            )
            .unwrap();
            let old_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO playlist_items (playlist_id, position, uuid, type) VALUES (?1, 0, 'u1', 0)",
                params![old_id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec) VALUES ('1980s', 1, 'decade:1980s')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec) VALUES ('Down-Tempo BPM', 1, 'bpmrange:60-90')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO playlists (name, dynamic_enabled, dynamic_spec) VALUES ('Miles Mix', 1, 'artist:Miles Davis')",
                [],
            )
            .unwrap();
        }

        // Reopening runs migrations forward, including the new migration 19.
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        let remaining_specs: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT dynamic_spec FROM playlists WHERE dynamic_enabled = 1 ORDER BY dynamic_spec")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect()
        };
        assert!(
            !remaining_specs.contains(&"Rock".to_string()),
            "old bare-genre-name row must be discarded"
        );
        assert!(remaining_specs.contains(&"decade:1980s".to_string()));
        assert!(remaining_specs.contains(&"bpmrange:60-90".to_string()));
        assert!(remaining_specs.contains(&"artist:Miles Davis".to_string()));

        let orphaned_items: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_items WHERE uuid = 'u1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            orphaned_items, 0,
            "the discarded playlist's items must go with it"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_25_adds_directory_metadata_columns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration25_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO directories (path, subdirs, nickname, icon, color) VALUES (?1, 1, ?2, ?3, ?4)",
            params!["/test/music", "My Library", "hard-drive", "#3b82f6"],
        )
        .unwrap();

        let (nickname, icon, color): (Option<String>, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT nickname, icon, color FROM directories WHERE path = '/test/music'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert_eq!(nickname.as_deref(), Some("My Library"));
        assert_eq!(icon.as_deref(), Some("hard-drive"));
        assert_eq!(color.as_deref(), Some("#3b82f6"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_28_context_enrichment_tables_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration28_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO context_enrichment (release_group_id, mb_rating, mb_rating_votes, mb_tags, critiquebrainz_rating, critiquebrainz_review_count, critiquebrainz_review_links, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["rg-123", 4.5_f64, 10_i64, r#"["black metal","norwegian"]"#, 3.8_f64, 2_i64, r#"[{"url":"https://critiquebrainz.org/review/x"}]"#, 1_700_000_000_i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO artist_context_enrichment (artist_id, wikidata_id, wikipedia_extract, wikipedia_page_url, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["artist-456", "Q12345", "An example artist bio.", "https://en.wikipedia.org/wiki/Example", 1_700_000_000_i64],
        )
        .unwrap();

        let (mb_rating, mb_tags): (Option<f64>, String) = conn
            .query_row(
                "SELECT mb_rating, mb_tags FROM context_enrichment WHERE release_group_id = 'rg-123'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(mb_rating, Some(4.5));
        assert_eq!(mb_tags, r#"["black metal","norwegian"]"#);

        let wikidata_id: Option<String> = conn
            .query_row(
                "SELECT wikidata_id FROM artist_context_enrichment WHERE artist_id = 'artist-456'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(wikidata_id.as_deref(), Some("Q12345"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_29_webdav_tables_round_trip() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration29_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO webdav_servers (name, url, username, remote_path) VALUES (?1, ?2, ?3, ?4)",
            params![
                "My NAS",
                "http://nas.local:8080/remote.php/webdav",
                "musicuser",
                "/Music"
            ],
        )
        .unwrap();

        let server_id = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO webdav_cache (server_id, remote_path, etag, size, last_modified) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![server_id, "/Music/track.flac", "etag-12345", 10_485_760_i64, "Wed, 21 Oct 2025 07:28:00 GMT"],
        )
        .unwrap();

        let (server_name, remote_path, size): (String, String, i64) = conn
            .query_row(
                "SELECT s.name, c.remote_path, c.size FROM webdav_servers s JOIN webdav_cache c ON s.id = c.server_id WHERE s.id = ?1",
                params![server_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert_eq!(server_name, "My NAS");
        assert_eq!(remote_path, "/Music/track.flac");
        assert_eq!(size, 10_485_760);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_40_webdav_auto_sync_columns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration40_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();

        // New rows default to auto-sync disabled with a 60-minute interval.
        conn.execute(
            "INSERT INTO webdav_servers (name, url, remote_path) VALUES (?1, ?2, ?3)",
            params![
                "My NAS",
                "http://nas.local:8080/remote.php/webdav",
                "/Music"
            ],
        )
        .unwrap();
        let server_id = conn.last_insert_rowid();

        let (auto_sync_enabled, sync_interval_minutes): (bool, i64) = conn
            .query_row(
                "SELECT auto_sync_enabled, sync_interval_minutes FROM webdav_servers WHERE id = ?1",
                params![server_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(!auto_sync_enabled);
        assert_eq!(sync_interval_minutes, 60);

        conn.execute(
            "UPDATE webdav_servers SET auto_sync_enabled = 1, sync_interval_minutes = 15 WHERE id = ?1",
            params![server_id],
        )
        .unwrap();
        let (auto_sync_enabled, sync_interval_minutes): (bool, i64) = conn
            .query_row(
                "SELECT auto_sync_enabled, sync_interval_minutes FROM webdav_servers WHERE id = ?1",
                params![server_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(auto_sync_enabled);
        assert_eq!(sync_interval_minutes, 15);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_44_subsonic_tables() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration44_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO subsonic_servers (name, url, username, password) VALUES (?1, ?2, ?3, ?4)",
            params!["Home", "https://music.example.com", "alice", "pw"],
        )
        .unwrap();
        let server_id = conn.last_insert_rowid();

        let (enabled, auto_sync, interval, report_plays, sync_status, extensions): (
            bool,
            bool,
            i64,
            bool,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT enabled, auto_sync_enabled, sync_interval_minutes, report_plays, sync_status, extensions_json
                 FROM subsonic_servers WHERE id = ?1",
                params![server_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert!(enabled);
        assert!(!auto_sync);
        assert_eq!(interval, 60);
        assert!(report_plays);
        assert_eq!(sync_status, "idle");
        assert_eq!(extensions, "[]");

        conn.execute(
            "INSERT INTO songs (title, path, source) VALUES ('Knowing', 'subsonic://1/abc', 5)",
            [],
        )
        .unwrap();
        let song_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO subsonic_cache (server_id, remote_id, song_id, server_rating, server_starred)
             VALUES (?1, 'abc', ?2, 5, 1)",
            params![server_id, song_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO subsonic_album_cache (server_id, remote_album_id, album_key, server_starred)
             VALUES (?1, 'alb1', 'Bloom', 1)",
            params![server_id],
        )
        .unwrap();

        // One cache row per (server, remote id).
        assert!(conn
            .execute(
                "INSERT INTO subsonic_cache (server_id, remote_id) VALUES (?1, 'abc')",
                params![server_id],
            )
            .is_err());

        // Deleting the song keeps the cache row but detaches it.
        conn.execute("DELETE FROM songs WHERE id = ?1", params![song_id])
            .unwrap();
        let cached_song: Option<i64> = conn
            .query_row(
                "SELECT song_id FROM subsonic_cache WHERE remote_id = 'abc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cached_song, None);

        // Deleting the server cascades to both caches.
        conn.execute(
            "DELETE FROM subsonic_servers WHERE id = ?1",
            params![server_id],
        )
        .unwrap();
        let remaining: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM subsonic_cache) + (SELECT COUNT(*) FROM subsonic_album_cache)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_35_target_lufs_and_crossfade_cleanup() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration35_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        let target_lufs: f64 = conn
            .query_row(
                "SELECT target_lufs FROM loudness_settings WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(target_lufs, -16.0);

        // Simulate an upgrade scenario: insert an app_state row with old crossfade keys,
        // re-run migration 35, and ensure they are removed.
        conn.execute(
            "INSERT INTO app_state (key, value) VALUES ('crossfade_manual_enabled', 'true'), ('crossfade_manual_duration_ms', '1000')",
            [],
        )
        .unwrap();

        conn.execute_batch(MIGRATION_35).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM app_state WHERE key IN ('crossfade_manual_enabled', 'crossfade_manual_duration_ms')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_38_artist_tag_hierarchy() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration38_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        // Verify tables exist
        let tables_exist: bool = conn
            .query_row(
                "SELECT COUNT(*) = 2 FROM sqlite_master WHERE type = 'table' AND name IN ('artist_tag_groups', 'artist_tag_assignments')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(tables_exist);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_39_adds_artist_profiles_musicbrainz_artist_id_column() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration39_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO artist_profiles (artist_key, musicbrainz_artist_id) VALUES (?1, ?2)",
            params!["Shania Twain", "042c0697-3948-4720-bf43-690240aeac43"],
        )
        .unwrap();

        let mbid: Option<String> = conn
            .query_row(
                "SELECT musicbrainz_artist_id FROM artist_profiles WHERE artist_key = 'Shania Twain'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            mbid.as_deref(),
            Some("042c0697-3948-4720-bf43-690240aeac43")
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_41_adds_artist_profiles_fetched_image_columns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration40_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO artist_profiles (artist_key, fetched_image_filename, fetched_image_source) VALUES (?1, ?2, ?3)",
            params!["Shania Twain", "artist-abc123.jpg", "fanart"],
        )
        .unwrap();

        let (filename, source): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT fetched_image_filename, fetched_image_source FROM artist_profiles WHERE artist_key = 'Shania Twain'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(filename.as_deref(), Some("artist-abc123.jpg"));
        assert_eq!(source.as_deref(), Some("fanart"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_42_adds_artist_context_enrichment_columns() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration42_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO artist_context_enrichment (
                artist_id,
                sort_name,
                artist_type,
                gender,
                begin_date,
                end_date,
                ended,
                begin_area_name,
                begin_area_mbid,
                area_name,
                area_mbid,
                fetched_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                "artist-bowie",
                "Bowie, David",
                "Person",
                "male",
                "1947-01-08",
                "2016-01-10",
                1,
                "Brixton",
                "d9e80e14-d07f-4ca6-b8db-60cb1c07cb81",
                "United Kingdom",
                "8a754a16-0027-4a29-b6d7-2b40ea0481ed",
                1000
            ],
        )
        .unwrap();

        struct Row {
            sort_name: Option<String>,
            artist_type: Option<String>,
            gender: Option<String>,
            begin_date: Option<String>,
            end_date: Option<String>,
            ended: Option<i64>,
            begin_area: Option<String>,
            area: Option<String>,
        }

        let row: Row = conn
            .query_row(
                "SELECT sort_name, artist_type, gender, begin_date, end_date, ended, begin_area_name, area_name
                 FROM artist_context_enrichment WHERE artist_id = 'artist-bowie'",
                [],
                |r| {
                    Ok(Row {
                        sort_name: r.get(0)?,
                        artist_type: r.get(1)?,
                        gender: r.get(2)?,
                        begin_date: r.get(3)?,
                        end_date: r.get(4)?,
                        ended: r.get(5)?,
                        begin_area: r.get(6)?,
                        area: r.get(7)?,
                    })
                },
            )
            .unwrap();

        assert_eq!(row.sort_name.as_deref(), Some("Bowie, David"));
        assert_eq!(row.artist_type.as_deref(), Some("Person"));
        assert_eq!(row.gender.as_deref(), Some("male"));
        assert_eq!(row.begin_date.as_deref(), Some("1947-01-08"));
        assert_eq!(row.end_date.as_deref(), Some("2016-01-10"));
        assert_eq!(row.ended, Some(1));
        assert_eq!(row.begin_area.as_deref(), Some("Brixton"));
        assert_eq!(row.area.as_deref(), Some("United Kingdom"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_migration_43_adds_auto_fetch_flags() {
        let temp_dir = std::env::temp_dir().join(format!(
            "luminous_migration43_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::new(temp_dir.clone()).unwrap();
        assert_eq!(db.schema_version, CURRENT_SCHEMA_VERSION);

        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO artist_profiles (artist_key, details_fetched, image_fetched) VALUES (?1, 1, 1)",
            params!["Radiohead"],
        )
        .unwrap();

        let (details_fetched, image_fetched): (bool, bool) = conn
            .query_row(
                "SELECT details_fetched, image_fetched FROM artist_profiles WHERE artist_key = 'Radiohead'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(details_fetched);
        assert!(image_fetched);

        conn.execute(
            "INSERT INTO album_profiles (album_key, details_fetched) VALUES (?1, 1)",
            params!["OK Computer"],
        )
        .unwrap();

        let album_details_fetched: bool = conn
            .query_row(
                "SELECT details_fetched FROM album_profiles WHERE album_key = 'OK Computer'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(album_details_fetched);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
