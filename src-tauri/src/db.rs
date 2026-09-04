//! Database module — SQLite connection pool, schema creation, and migrations.

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::path::PathBuf;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Current schema version. Increment when adding migrations.
pub const CURRENT_SCHEMA_VERSION: i32 = 24;

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
        // Several other in-flight worktrees on this machine already claim
        // schema versions 21-23 for unrelated changes, and all worktrees'
        // dev builds share one app-data DB (same tauri.conf.json
        // `identifier`) — picking 21 here got silently skipped against that
        // shared DB once one of those other builds ran first and recorded a
        // higher version. 24 is clear of every sibling worktree's version as
        // of this writing; whichever of these PRs merges to main first keeps
        // its number, and the others get renumbered in a rebase.
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

        for migration in MIGRATIONS {
            if version < migration.version {
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

    let mut stmt = conn.prepare(
        "SELECT genre FROM songs
         WHERE source IN (1, 2) AND unavailable = 0 AND genre IS NOT NULL AND genre != ''",
    )?;
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
}
