//! Database module — SQLite connection pool, schema creation, and migrations.

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::PathBuf;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Current schema version. Increment when adding migrations.
const CURRENT_SCHEMA_VERSION: i32 = 10;

#[derive(Debug)]
pub struct Database {
    pub pool: DbPool,
}

impl Database {
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
                     PRAGMA cache_size=-32000;  -- 32 MB page cache
                     PRAGMA temp_store=MEMORY;",
            )
        });

        let pool = r2d2::Pool::builder()
            .max_size(8)
            .build(manager)
            .context("failed to create connection pool")?;

        let db = Self { pool };
        db.run_migrations()?;

        Ok(db)
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.pool.get().context("failed to get db connection")?;

        // Create schema_version table if it doesn't exist
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

        if version < 1 {
            log::info!("Running migration 1: initial schema");
            conn.execute_batch(MIGRATION_1)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![1],
            )?;
        }

        if version < 2 {
            log::info!("Running migration 2: equalizer settings");
            conn.execute_batch(MIGRATION_2)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![2],
            )?;
        }

        if version < 3 {
            log::info!("Running migration 3: unavailable flag for soft-deleted songs");
            conn.execute_batch(MIGRATION_3)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![3],
            )?;
        }

        if version < 4 {
            log::info!("Running migration 4: parametric equalizer mode");
            conn.execute_batch(MIGRATION_4)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![4],
            )?;
        }

        if version < 5 {
            log::info!("Running migration 5: loudness normalization (#77)");
            conn.execute_batch(MIGRATION_5)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![5],
            )?;
        }

        if version < 6 {
            log::info!("Running migration 6: playlist last-updated tracking");
            conn.execute_batch(MIGRATION_6)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![6],
            )?;
        }

        if version < 7 {
            log::info!("Running migration 7: VBR/CBR bitrate flag");
            conn.execute_batch(MIGRATION_7)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![7],
            )?;
        }

        if version < 8 {
            log::info!("Running migration 8: instrumental track flag (#12)");
            conn.execute_batch(MIGRATION_8)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![8],
            )?;
        }

        if version < 9 {
            log::info!("Running migration 9: auto_play flag for dynamic playlists (#26)");
            conn.execute_batch(MIGRATION_9)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![9],
            )?;
        }

        if version < 10 {
            log::info!("Running migration 10: play_history for context-aware Recently Played");
            conn.execute_batch(MIGRATION_10)?;
            conn.execute(
                "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
                params![10],
            )?;
        }

        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_initialization() {
        // Use a unique temp path for testing
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

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
