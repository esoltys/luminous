//! Portable curation hierarchy (#1312): the Genres and Artist Tags
//! hierarchies mirrored into a `luminous-hierarchy.json` sidecar in the
//! user's designated *default library* (one of the `directories` rows).
//!
//! The DB stays the working copy every query reads; the file is the shared,
//! portable copy. It's loaded on startup and whenever it changes on disk
//! (replacing the four hierarchy tables in one transaction), and rewritten
//! atomically after every hierarchy mutation. Last writer wins at file
//! granularity. A file that doesn't parse is never overwritten — the DB keeps
//! its state and the frontend is told via `hierarchy-sidecar-error` until the
//! file parses again.
//!
//! While a sidecar is attached, reconcile never evicts entries for unused
//! tags (see `tags::TagManager::reconcile_hierarchy`) — an instance that
//! sees only part of the library, or a half-finished scan after a move,
//! must not erase curation from a file other instances share. The UI hides
//! zero-song nodes instead.

use crate::{db::Database, tags::TagManager};
use anyhow::{anyhow, bail, Context, Result};
use notify::Watcher;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

pub const FILE_NAME: &str = "luminous-hierarchy.json";
/// `app_state` key holding the default library's directory path.
pub const SETTING_KEY: &str = "default_library_path";
/// Bumped only for changes an older reader would misinterpret; a file with a
/// newer version is treated like a malformed one (never overwritten).
pub const FORMAT_VERSION: u32 = 1;
/// Mirrors `tags::PALETTE_SIZE` — colors are palette indices.
const PALETTE_SIZE: i32 = 10;
const WATCH_DEBOUNCE: Duration = Duration::from_millis(1000);
/// Emitted with a [`SidecarError`] when the file on disk can't be loaded.
pub const ERROR_EVENT: &str = "hierarchy-sidecar-error";
/// Emitted whenever the default library is linked or unlinked, including
/// automatically — the frontend re-reads [`status`].
pub const CHANGED_EVENT: &str = "default-library-changed";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HierarchyFile {
    pub version: u32,
    #[serde(default)]
    pub genres: Vec<GroupEntry>,
    #[serde(default, rename = "artistTags")]
    pub artist_tags: Vec<GroupEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupEntry {
    pub name: String,
    #[serde(default)]
    pub color: i32,
    #[serde(default)]
    pub children: Vec<String>,
    /// Artist tag groups only: a user-created group kept even with no
    /// artist tagged with it directly.
    #[serde(default, skip_serializing_if = "is_false")]
    pub custom: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// What Settings shows for the default library.
#[derive(Debug, Clone, Serialize)]
pub struct DefaultLibraryStatus {
    pub path: Option<String>,
    pub error: Option<String>,
}

pub fn status(app: &AppHandle) -> Result<DefaultLibraryStatus> {
    let state = app.state::<crate::AppState>();
    let path = default_library(&*state.db.pool.get()?)?;
    Ok(DefaultLibraryStatus {
        error: path.as_ref().and(state.hierarchy_sidecar.last_error()),
        path,
    })
}

/// Why [`set_default_library`] refused a folder. Attached as context to the
/// detailed error, so the command can send the UI just the kind and log the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRefusal {
    Unavailable,
    Broken,
}

impl LinkRefusal {
    /// The code the UI maps to a message.
    pub fn code(self) -> &'static str {
        match self {
            LinkRefusal::Unavailable => "unavailable",
            LinkRefusal::Broken => "broken",
        }
    }
}

impl std::fmt::Display for LinkRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LinkRefusal::Unavailable => "folder unavailable",
            LinkRefusal::Broken => "hierarchy file broken",
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SidecarError {
    pub path: String,
    pub message: String,
}

/// Whether a default library is designated — reconcile skips eviction then.
pub fn is_attached(conn: &Connection) -> bool {
    default_library(conn).ok().flatten().is_some()
}

pub fn default_library(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM app_state WHERE key = ?1",
            params![SETTING_KEY],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten()
        .filter(|v| !v.trim().is_empty()))
}

// ---------------------------------------------------------------------------
// DB <-> model
// ---------------------------------------------------------------------------

fn export_tables(
    conn: &Connection,
    groups_table: &str,
    assignments_table: &str,
    has_custom: bool,
) -> Result<Vec<GroupEntry>> {
    let custom_col = if has_custom { "is_custom" } else { "0" };
    let mut groups_stmt = conn.prepare(&format!(
        "SELECT id, name, color_index, {custom_col} FROM {groups_table} ORDER BY sort_order, id"
    ))?;
    let groups: Vec<(i64, String, i32, i64)> = groups_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut children_stmt = conn.prepare(&format!(
        "SELECT tag_name FROM {assignments_table} WHERE group_id = ?1 ORDER BY sort_order, id"
    ))?;
    groups
        .into_iter()
        .map(|(id, name, color, custom)| {
            let children = children_stmt
                .query_map(params![id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(GroupEntry {
                name,
                color,
                children,
                custom: custom != 0,
            })
        })
        .collect()
}

/// Snapshot of both hierarchies as they stand in the DB.
pub fn export(conn: &Connection) -> Result<HierarchyFile> {
    Ok(HierarchyFile {
        version: FORMAT_VERSION,
        genres: export_tables(conn, "tag_groups", "tag_assignments", false)?,
        artist_tags: export_tables(conn, "artist_tag_groups", "artist_tag_assignments", true)?,
    })
}

fn import_tables(
    conn: &Connection,
    groups_table: &str,
    assignments_table: &str,
    has_custom: bool,
    entries: &[GroupEntry],
) -> Result<()> {
    conn.execute(&format!("DELETE FROM {assignments_table}"), [])?;
    conn.execute(&format!("DELETE FROM {groups_table}"), [])?;

    // Names are unique case-insensitively across groups *and* children (a
    // top-level genre can't also nest as a child, same rule reconcile
    // enforces) — groups claim names first, then the first occurrence wins,
    // so a hand-edited duplicate degrades gracefully instead of failing.
    let mut seen: HashSet<String> = HashSet::new();
    let entries: Vec<&GroupEntry> = entries
        .iter()
        .filter(|g| !g.name.trim().is_empty() && seen.insert(g.name.to_lowercase()))
        .collect();

    for (sort, group) in entries.iter().enumerate() {
        let color = group.color.rem_euclid(PALETTE_SIZE);
        if has_custom {
            conn.execute(
                &format!(
                    "INSERT INTO {groups_table} (name, color_index, sort_order, is_custom)
                     VALUES (?1, ?2, ?3, ?4)"
                ),
                params![group.name, color, sort as i64, group.custom as i64],
            )?;
        } else {
            conn.execute(
                &format!(
                    "INSERT INTO {groups_table} (name, color_index, sort_order) VALUES (?1, ?2, ?3)"
                ),
                params![group.name, color, sort as i64],
            )?;
        }
        let group_id = conn.last_insert_rowid();
        let mut child_sort = 0i64;
        for child in &group.children {
            if child.trim().is_empty() || !seen.insert(child.to_lowercase()) {
                continue;
            }
            conn.execute(
                &format!(
                    "INSERT INTO {assignments_table} (tag_name, group_id, sort_order)
                     VALUES (?1, ?2, ?3)"
                ),
                params![child, group_id, child_sort],
            )?;
            child_sort += 1;
        }
    }
    Ok(())
}

/// Replaces both hierarchies in the DB with `file`'s, in one transaction.
pub fn import(conn: &Connection, file: &HierarchyFile) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    import_tables(&tx, "tag_groups", "tag_assignments", false, &file.genres)?;
    import_tables(
        &tx,
        "artist_tag_groups",
        "artist_tag_assignments",
        true,
        &file.artist_tags,
    )?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn serialize(file: &HierarchyFile) -> Result<String> {
    let mut s = serde_json::to_string_pretty(file)?;
    s.push('\n');
    Ok(s)
}

pub fn parse(content: &str) -> Result<HierarchyFile> {
    let file: HierarchyFile = serde_json::from_str(content).context("not valid hierarchy JSON")?;
    if file.version == 0 || file.version > FORMAT_VERSION {
        bail!(
            "unsupported version {} (this Luminous reads version {FORMAT_VERSION})",
            file.version
        );
    }
    Ok(file)
}

/// Writes via a temp file in the same directory plus a rename, so another
/// instance (or a crash mid-write) never sees a half-written file.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content).with_context(|| format!("writing {}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow!(e).context(format!("replacing {}", path.display())));
    }
    Ok(())
}

fn content_hash(content: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    h.finish()
}

pub fn sidecar_path(library_dir: &str) -> PathBuf {
    Path::new(library_dir).join(FILE_NAME)
}

// ---------------------------------------------------------------------------
// Runtime state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Inner {
    /// Hash of the content last written or loaded — lets the watcher ignore
    /// Luminous's own writes and lets write-through skip no-op rewrites.
    last_hash: Option<u64>,
    /// Set while the file on disk doesn't parse: never overwrite it.
    blocked: bool,
    /// Why the file couldn't be loaded, while `blocked` — kept so a
    /// frontend that wasn't listening yet (startup) can still show it.
    last_error: Option<String>,
    watcher: Option<notify::RecommendedWatcher>,
}

/// Held in `AppState`.
#[derive(Default)]
pub struct HierarchySidecar {
    inner: parking_lot::Mutex<Inner>,
}

enum LoadOutcome {
    Loaded,
    Unchanged,
    Missing,
}

impl HierarchySidecar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reads and applies the file. On a parse failure, blocks writes and
    /// returns the error; the DB is left untouched.
    fn load(&self, db: &Database, path: &Path) -> Result<LoadOutcome> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LoadOutcome::Missing),
            Err(e) => return Err(anyhow!(e).context(format!("reading {}", path.display()))),
        };
        let hash = content_hash(&content);
        let mut inner = self.inner.lock();
        if inner.last_hash == Some(hash) {
            // Our own write, or the same broken content already reported.
            return Ok(LoadOutcome::Unchanged);
        }
        let file = match parse(&content) {
            Ok(f) => f,
            Err(e) => {
                inner.blocked = true;
                inner.last_hash = Some(hash);
                inner.last_error = Some(format!("{e:#}"));
                return Err(e);
            }
        };
        let conn = db.pool.get()?;
        import(&conn, &file)?;
        inner.blocked = false;
        inner.last_error = None;
        inner.last_hash = Some(hash);
        Ok(LoadOutcome::Loaded)
    }

    /// Rewrites the file from the DB if attached, writable, and changed.
    /// Failures are logged — the DB remains the working copy either way.
    pub fn write_through(&self, db: &Database) {
        if let Err(e) = self.try_write_through(db) {
            log::warn!("Hierarchy sidecar write failed: {e:#}");
        }
    }

    fn try_write_through(&self, db: &Database) -> Result<()> {
        let conn = db.pool.get()?;
        let Some(dir) = default_library(&conn)? else {
            return Ok(());
        };
        if !Path::new(&dir).is_dir() {
            // Library offline (unmounted drive, unreachable share): keep the
            // DB copy and catch up on the next write once it's back.
            return Ok(());
        }
        let content = serialize(&export(&conn)?)?;
        drop(conn);
        let hash = content_hash(&content);
        let mut inner = self.inner.lock();
        if inner.blocked || inner.last_hash == Some(hash) {
            return Ok(());
        }
        write_atomic(&sidecar_path(&dir), &content)?;
        inner.last_hash = Some(hash);
        Ok(())
    }

    fn forget(&self) {
        *self.inner.lock() = Inner::default();
    }

    /// The current load error, if the attached file doesn't parse.
    pub fn last_error(&self) -> Option<String> {
        self.inner.lock().last_error.clone()
    }

    fn start_watching(&self, app: &AppHandle, dir: &str) {
        let (tx, rx) = mpsc::channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let touches_file = event
                    .paths
                    .iter()
                    .any(|p| p.file_name().is_some_and(|n| n == FILE_NAME));
                if touches_file && !event.kind.is_access() {
                    let _ = tx.send(());
                }
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Hierarchy sidecar watcher unavailable: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(Path::new(dir), notify::RecursiveMode::NonRecursive) {
            log::warn!("Can't watch {dir} for hierarchy changes: {e}");
            return;
        }
        self.inner.lock().watcher = Some(watcher);

        let app = app.clone();
        let _ = std::thread::Builder::new()
            .name("luminous-hierarchy-watcher".into())
            .spawn(move || {
                // Ends when the watcher (and so `tx`) is dropped on detach.
                while rx.recv().is_ok() {
                    loop {
                        match rx.recv_timeout(WATCH_DEBOUNCE) {
                            Ok(()) => continue,
                            Err(mpsc::RecvTimeoutError::Timeout) => break,
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    sync_from_disk(&app, false);
                }
            });
    }
}

fn emit_error(app: &AppHandle, path: &Path, e: &anyhow::Error) {
    log::warn!("Hierarchy sidecar {} not loaded: {e:#}", path.display());
    let _ = app.emit(
        ERROR_EVENT,
        SidecarError {
            path: path.display().to_string(),
            message: format!("{e:#}"),
        },
    );
}

/// Loads the sidecar into the DB (writing the DB's hierarchy out when there's
/// no file yet), reconciles, writes back any auto-placements, and tells the
/// frontend to refresh. Runs on startup and after an external change.
fn sync_from_disk(app: &AppHandle, create_if_missing: bool) {
    let state = app.state::<crate::AppState>();
    let db = &state.db;
    let sidecar = &state.hierarchy_sidecar;
    let Ok(conn) = db.pool.get() else { return };
    let Ok(Some(dir)) = default_library(&conn) else {
        return;
    };
    drop(conn);
    if !Path::new(&dir).is_dir() {
        log::warn!("Default library {dir} is unavailable; using the local hierarchy");
        return;
    }
    let path = sidecar_path(&dir);
    match sidecar.load(db, &path) {
        Ok(LoadOutcome::Unchanged) => return,
        Ok(LoadOutcome::Missing) if !create_if_missing => return,
        Ok(_) => {}
        Err(e) => {
            emit_error(app, &path, &e);
            return;
        }
    }
    let manager = TagManager::new(Arc::clone(db));
    if let Err(e) = manager.reconcile_hierarchy() {
        log::error!("Tag hierarchy reconcile failed: {e}");
    }
    if let Err(e) = manager.reconcile_artist_hierarchy() {
        log::error!("Artist tag hierarchy reconcile failed: {e}");
    }
    sidecar.write_through(db);
    let _ = app.emit("tags-changed", ());
    let _ = app.emit("artist-tags-changed", ());
}

/// Startup: link a lone watched folder if no default was ever chosen, then
/// load the attached sidecar (if any) and start watching it.
pub fn init(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let unchosen = state
        .db
        .pool
        .get()
        .ok()
        .and_then(|conn| auto_default_candidate(&conn).ok().flatten());
    if unchosen.is_some() {
        // Linking loads and starts watching.
        ensure_default(app);
        return;
    }
    let dir = state
        .db
        .pool
        .get()
        .ok()
        .and_then(|conn| default_library(&conn).ok().flatten());
    let Some(dir) = dir else { return };
    sync_from_disk(app, true);
    state.hierarchy_sidecar.start_watching(app, &dir);
}

/// Designates `dir` (which must be a watched folder) as the default library,
/// or detaches with `None`. Linking adopts an existing sidecar — replacing the
/// local hierarchy — or writes the local hierarchy out if there is none. A
/// sidecar that doesn't parse fails the link and leaves everything as it was.
pub fn set_default_library(app: &AppHandle, dir: Option<String>) -> Result<()> {
    let state = app.state::<crate::AppState>();
    let db = &state.db;
    let sidecar = &state.hierarchy_sidecar;
    sidecar.forget();

    let conn = db.pool.get()?;
    let Some(dir) = dir.filter(|d| !d.trim().is_empty()) else {
        // An empty value records "None" as a choice, so a lone watched folder
        // isn't linked again behind the user's back (see `ensure_default`).
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, '')",
            params![SETTING_KEY],
        )?;
        drop(conn);
        let _ = app.emit(CHANGED_EVENT, ());
        return Ok(());
    };
    check_linkable(&conn, &dir)?;
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
        params![SETTING_KEY, dir],
    )?;
    drop(conn);

    sync_from_disk(app, true);
    sidecar.start_watching(app, &dir);
    let _ = app.emit(CHANGED_EVENT, ());
    Ok(())
}

/// Refuses, with a [`LinkRefusal`] context, a folder that isn't watched or
/// available, or whose sidecar doesn't parse.
fn check_linkable(conn: &Connection, dir: &str) -> Result<()> {
    let known: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM directories WHERE path = ?1)",
        params![dir],
        |r| r.get(0),
    )?;
    if !known {
        return Err(anyhow!("{dir} is not a watched folder").context(LinkRefusal::Unavailable));
    }
    if !Path::new(dir).is_dir() {
        return Err(anyhow!("{dir} is not available").context(LinkRefusal::Unavailable));
    }
    let path = sidecar_path(dir);
    if path.exists() {
        std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))
            .and_then(|content| {
                parse(&content).with_context(|| format!("{} can't be used", path.display()))
            })
            .context(LinkRefusal::Broken)?;
    }
    Ok(())
}

/// The folder to link automatically: the only watched folder, while the user
/// has never chosen a default library (no `app_state` row — an explicit
/// "None" is stored as an empty value and is respected).
pub fn auto_default_candidate(conn: &Connection) -> Result<Option<String>> {
    let chosen: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_state WHERE key = ?1)",
        params![SETTING_KEY],
        |r| r.get(0),
    )?;
    if chosen {
        return Ok(None);
    }
    let mut stmt = conn.prepare("SELECT path FROM directories LIMIT 2")?;
    let dirs: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(match dirs.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    })
}

/// Links the only watched folder as the default library if the user hasn't
/// chosen one. Runs at startup and whenever the watched folders change. A
/// folder that can't be linked (offline, malformed sidecar) is left unlinked
/// and retried next time.
pub fn ensure_default(app: &AppHandle) {
    let state = app.state::<crate::AppState>();
    let candidate = state
        .db
        .pool
        .get()
        .map_err(anyhow::Error::from)
        .and_then(|conn| auto_default_candidate(&conn));
    match candidate {
        Ok(Some(dir)) => {
            if let Err(e) = set_default_library(app, Some(dir.clone())) {
                log::warn!("Not linking {dir} as the default library: {e:#}");
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("Default library check failed: {e:#}"),
    }
}

/// Detaches when the default library's folder stops being watched — back to
/// "never chosen", so a single remaining folder becomes the default.
pub fn on_directory_removed(app: &AppHandle, removed: &str) {
    let state = app.state::<crate::AppState>();
    let Ok(conn) = state.db.pool.get() else {
        return;
    };
    let is_default = default_library(&conn)
        .ok()
        .flatten()
        .is_some_and(|d| d == removed);
    if !is_default {
        return;
    }
    state.hierarchy_sidecar.forget();
    if let Err(e) = conn.execute("DELETE FROM app_state WHERE key = ?1", params![SETTING_KEY]) {
        log::warn!("Failed to detach hierarchy sidecar: {e}");
    }
    drop(conn);
    let _ = app.emit(CHANGED_EVENT, ());
}

/// Write-through for async callers holding `AppState`.
pub async fn write_through(state: &crate::AppState) {
    let db = Arc::clone(&state.db);
    let sidecar = Arc::clone(&state.hierarchy_sidecar);
    let _ = tokio::task::spawn_blocking(move || sidecar.write_through(&db)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (tempfile::TempDir, Arc<Database>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().to_path_buf()).unwrap());
        // Ensure the hierarchy tables exist.
        let _ = TagManager::new(Arc::clone(&db));
        (dir, db)
    }

    fn sample() -> HierarchyFile {
        HierarchyFile {
            version: 1,
            genres: vec![
                GroupEntry {
                    name: "Metal".into(),
                    color: 3,
                    children: vec!["Folk Metal".into(), "Progressive Metal".into()],
                    custom: false,
                },
                GroupEntry {
                    name: "Jazz".into(),
                    color: 0,
                    children: vec![],
                    custom: false,
                },
            ],
            artist_tags: vec![GroupEntry {
                name: "Award-Winning".into(),
                color: 5,
                children: vec!["Juno Award".into()],
                custom: true,
            }],
        }
    }

    #[test]
    fn import_then_export_round_trips_order_colors_and_custom_flag() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        import(&conn, &sample()).unwrap();
        assert_eq!(export(&conn).unwrap(), sample());
    }

    #[test]
    fn import_replaces_existing_hierarchy() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO tag_groups (name, color_index, sort_order) VALUES ('Pop', 1, 0)",
            [],
        )
        .unwrap();
        import(&conn, &sample()).unwrap();
        let names: Vec<String> = export(&conn)
            .unwrap()
            .genres
            .into_iter()
            .map(|g| g.name)
            .collect();
        assert_eq!(names, ["Metal", "Jazz"]);
    }

    #[test]
    fn import_drops_duplicate_names_and_wraps_colors() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        let file = HierarchyFile {
            version: 1,
            genres: vec![
                GroupEntry {
                    name: "Rock".into(),
                    color: 13,
                    children: vec!["rock".into(), "Punk".into()],
                    custom: false,
                },
                GroupEntry {
                    name: "ROCK".into(),
                    color: 0,
                    children: vec![],
                    custom: false,
                },
                GroupEntry {
                    name: "Punk".into(),
                    color: 0,
                    children: vec![],
                    custom: false,
                },
            ],
            artist_tags: vec![],
        };
        import(&conn, &file).unwrap();
        let out = export(&conn).unwrap();
        // Groups claim their names first — as in reconcile, a top-level
        // genre never also nests as a child, so "Punk" stays a group.
        let names: Vec<&str> = out.genres.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Rock", "Punk"]);
        assert_eq!(out.genres[0].color, 3);
        assert!(out.genres[0].children.is_empty());
    }

    #[test]
    fn serialized_format_is_stable() {
        let json = serialize(&sample()).unwrap();
        assert!(json.contains("\"artistTags\""));
        assert!(json.contains("\"custom\": true"));
        // `custom` is omitted when false, keeping genre entries minimal.
        assert_eq!(json.matches("\"custom\"").count(), 1);
        assert_eq!(parse(&json).unwrap(), sample());
    }

    #[test]
    fn parse_rejects_malformed_and_future_versions() {
        assert!(parse("{ not json").is_err());
        assert!(parse(r#"{"version": 2, "genres": []}"#).is_err());
        assert!(parse(r#"{"version": 0}"#).is_err());
        assert!(parse(r#"{"version": 1}"#).is_ok());
    }

    #[test]
    fn write_atomic_replaces_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    fn attach(db: &Database, lib: &Path) {
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
                params![SETTING_KEY, lib.to_string_lossy()],
            )
            .unwrap();
    }

    fn watch(conn: &Connection, path: &str) {
        conn.execute("INSERT INTO directories (path) VALUES (?1)", params![path])
            .unwrap();
    }

    fn refusal(conn: &Connection, dir: &str) -> Option<LinkRefusal> {
        check_linkable(conn, dir)
            .err()
            .map(|e| *e.downcast_ref::<LinkRefusal>().expect("refusal kind"))
    }

    #[test]
    fn link_refusals_carry_their_kind_for_the_ui() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        let lib = tempfile::tempdir().unwrap();
        let lib_path = lib.path().to_string_lossy().to_string();
        assert_eq!(refusal(&conn, &lib_path), Some(LinkRefusal::Unavailable));

        watch(&conn, &lib_path);
        assert_eq!(refusal(&conn, &lib_path), None);

        std::fs::write(sidecar_path(&lib_path), "{ not json").unwrap();
        assert_eq!(refusal(&conn, &lib_path), Some(LinkRefusal::Broken));

        let offline = "Z:\\Nowhere\\Music";
        watch(&conn, offline);
        assert_eq!(refusal(&conn, offline), Some(LinkRefusal::Unavailable));
    }

    #[test]
    fn only_watched_folder_is_the_auto_default_until_a_choice_is_made() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        assert_eq!(auto_default_candidate(&conn).unwrap(), None);

        watch(&conn, "Z:\\Music Library");
        assert_eq!(
            auto_default_candidate(&conn).unwrap().as_deref(),
            Some("Z:\\Music Library")
        );

        watch(&conn, "Z:\\BandCamp");
        assert_eq!(auto_default_candidate(&conn).unwrap(), None);

        conn.execute("DELETE FROM directories WHERE path = 'Z:\\BandCamp'", [])
            .unwrap();
        assert!(auto_default_candidate(&conn).unwrap().is_some());
    }

    #[test]
    fn explicit_none_is_not_overridden_by_the_auto_default() {
        let (_d, db) = test_db();
        let conn = db.pool.get().unwrap();
        watch(&conn, "Z:\\Music Library");
        // What `set_default_library(None)` stores.
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, '')",
            params![SETTING_KEY],
        )
        .unwrap();
        assert_eq!(default_library(&conn).unwrap(), None);
        assert_eq!(auto_default_candidate(&conn).unwrap(), None);
    }

    #[test]
    fn malformed_file_is_never_overwritten_and_db_is_kept() {
        let (_d, db) = test_db();
        let lib = tempfile::tempdir().unwrap();
        attach(&db, lib.path());
        let conn = db.pool.get().unwrap();
        import(&conn, &sample()).unwrap();
        drop(conn);
        let path = sidecar_path(&lib.path().to_string_lossy());
        std::fs::write(&path, "{ broken").unwrap();

        let sidecar = HierarchySidecar::new();
        assert!(sidecar.load(&db, &path).is_err());
        sidecar.write_through(&db);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
        let conn = db.pool.get().unwrap();
        assert_eq!(export(&conn).unwrap(), sample());
        drop(conn);

        // Once fixed on disk, it loads and writes resume.
        std::fs::write(&path, serialize(&sample()).unwrap()).unwrap();
        assert!(matches!(
            sidecar.load(&db, &path).unwrap(),
            LoadOutcome::Loaded
        ));
        db.pool
            .get()
            .unwrap()
            .execute(
                "UPDATE tag_groups SET color_index = 7 WHERE name = 'Jazz'",
                [],
            )
            .unwrap();
        sidecar.write_through(&db);
        let written = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written.genres[1].color, 7);
    }

    #[test]
    fn own_write_is_recognized_as_unchanged() {
        let (_d, db) = test_db();
        let lib = tempfile::tempdir().unwrap();
        attach(&db, lib.path());
        import(&db.pool.get().unwrap(), &sample()).unwrap();
        let sidecar = HierarchySidecar::new();
        sidecar.write_through(&db);
        let path = sidecar_path(&lib.path().to_string_lossy());
        assert!(matches!(
            sidecar.load(&db, &path).unwrap(),
            LoadOutcome::Unchanged
        ));
    }

    #[test]
    fn write_through_is_a_no_op_when_detached() {
        let (_d, db) = test_db();
        let lib = tempfile::tempdir().unwrap();
        HierarchySidecar::new().write_through(&db);
        assert!(!sidecar_path(&lib.path().to_string_lossy()).exists());
        assert!(!is_attached(&db.pool.get().unwrap()));
    }
}
