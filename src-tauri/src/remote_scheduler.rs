//! Periodic auto-sync scheduling for remote library servers — WebDAV (#1082)
//! and OpenSubsonic (#916, #1162).
//!
//! Local watched folders get live change notifications from a filesystem
//! watcher (`collection::start_watcher`); a remote server can't push change
//! notifications, so a server that opts into auto-sync instead gets its own
//! tokio interval task that re-runs its sync on a schedule. Tasks are keyed
//! by `(RemoteKind, server id)` so a single server's schedule can be replaced
//! or cancelled (on save/delete) without touching any other server's timer or
//! restarting the app.

use crate::covermanager::CoverManager;
use crate::db::Database;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;

/// Which kind of remote server a schedule belongs to. Server ids are only
/// unique within their own table, so the kind is part of every key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteKind {
    WebDav,
    Subsonic,
}

impl RemoteKind {
    const ALL: [RemoteKind; 2] = [RemoteKind::WebDav, RemoteKind::Subsonic];

    fn table(self) -> &'static str {
        match self {
            RemoteKind::WebDav => "webdav_servers",
            RemoteKind::Subsonic => "subsonic_servers",
        }
    }

    fn label(self) -> &'static str {
        match self {
            RemoteKind::WebDav => "WebDAV",
            RemoteKind::Subsonic => "OpenSubsonic",
        }
    }

    async fn sync(
        self,
        server_id: i64,
        app: AppHandle,
        db: Arc<Database>,
        cover_manager: Arc<CoverManager>,
    ) -> Result<(), String> {
        match self {
            RemoteKind::WebDav => {
                crate::commands::webdav::sync_webdav_server_inner(server_id, app, db, cover_manager)
                    .await
                    .map(|_| ())
            }
            RemoteKind::Subsonic => crate::commands::subsonic::sync_subsonic_server_inner(
                server_id,
                app,
                db,
                cover_manager,
            )
            .await
            .map(|_| ()),
        }
    }
}

type ScheduleKey = (RemoteKind, i64);

#[derive(Default)]
pub struct AutoSyncScheduler {
    tasks: parking_lot::Mutex<HashMap<ScheduleKey, JoinHandle<()>>>,
    /// Unix timestamp (seconds) each scheduled server's next tick is due —
    /// purely in-memory, recomputed on every (re)schedule and after every
    /// tick, so the frontend can show "next sync in N minutes" (#1082).
    next_run: parking_lot::Mutex<HashMap<ScheduleKey, i64>>,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl AutoSyncScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// (Re)schedules periodic auto-sync for `server_id` at `interval_minutes`,
    /// replacing any timer already running for that server. The first sync
    /// fires after the interval elapses (not immediately), since scheduling
    /// happens both at startup and on every settings save, and a server that
    /// was just synced shouldn't be re-synced right away.
    ///
    /// Takes `self: &Arc<Self>` (rather than `&self`) so the spawned task can
    /// hold its own `Arc` clone of the scheduler to update `next_run` on each
    /// tick — every caller already reaches this through `AppState`'s
    /// `Arc<AutoSyncScheduler>`, so this is transparent at call sites.
    pub fn reschedule(
        self: &Arc<Self>,
        app: AppHandle,
        db: Arc<Database>,
        cover_manager: Arc<CoverManager>,
        kind: RemoteKind,
        server_id: i64,
        interval_minutes: i64,
    ) {
        self.cancel(kind, server_id);

        let key = (kind, server_id);
        let interval_secs = interval_minutes.max(1) as u64 * 60;
        let period = std::time::Duration::from_secs(interval_secs);
        self.next_run
            .lock()
            .insert(key, now_unix() + interval_secs as i64);

        let scheduler = Arc::clone(self);
        let handle = tauri::async_runtime::spawn(async move {
            let mut interval =
                tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;
                scheduler
                    .next_run
                    .lock()
                    .insert(key, now_unix() + interval_secs as i64);

                if Self::is_syncing(&db, kind, server_id) {
                    log::debug!(
                        "Skipping scheduled {} auto-sync for server {server_id}: a sync is already in flight",
                        kind.label()
                    );
                    continue;
                }

                if let Err(e) = kind
                    .sync(
                        server_id,
                        app.clone(),
                        Arc::clone(&db),
                        Arc::clone(&cover_manager),
                    )
                    .await
                {
                    log::warn!(
                        "Scheduled {} auto-sync failed for server {server_id}: {e}",
                        kind.label()
                    );
                }
            }
        });

        self.tasks.lock().insert(key, handle);
    }

    /// Cancels the running auto-sync timer for `server_id`, if any.
    pub fn cancel(&self, kind: RemoteKind, server_id: i64) {
        if let Some(handle) = self.tasks.lock().remove(&(kind, server_id)) {
            handle.abort();
        }
        self.next_run.lock().remove(&(kind, server_id));
    }

    /// Unix timestamp (seconds) of `server_id`'s next scheduled auto-sync, or
    /// `None` if it has no timer running.
    pub fn next_run_at(&self, kind: RemoteKind, server_id: i64) -> Option<i64> {
        self.next_run.lock().get(&(kind, server_id)).copied()
    }

    fn is_syncing(db: &Arc<Database>, kind: RemoteKind, server_id: i64) -> bool {
        db.pool
            .get()
            .ok()
            .and_then(|conn| {
                conn.query_row(
                    &format!("SELECT sync_status FROM {} WHERE id = ?1", kind.table()),
                    rusqlite::params![server_id],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            })
            .map(|status| status == "syncing")
            .unwrap_or(false)
    }

    /// Starts a schedule for every server (of every kind) that currently has
    /// auto-sync enabled. Called once at app startup, mirroring how the folder
    /// watcher is started from the servers/directories on disk rather than
    /// assuming no reschedule ever happened.
    pub fn start_all_from_db(
        self: &Arc<Self>,
        app: AppHandle,
        db: Arc<Database>,
        cover_manager: Arc<CoverManager>,
    ) {
        let conn = match db.pool.get() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to load remote servers for auto-sync scheduling: {e}");
                return;
            }
        };

        let mut servers: Vec<(RemoteKind, i64, i64)> = Vec::new();
        for kind in RemoteKind::ALL {
            let sql = format!(
                "SELECT id, sync_interval_minutes FROM {} WHERE enabled = 1 AND auto_sync_enabled = 1",
                kind.table()
            );
            match conn.prepare(&sql) {
                Ok(mut stmt) => match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
                    Ok(rows) => servers.extend(rows.flatten().map(|(id, mins)| (kind, id, mins))),
                    Err(e) => log::warn!("Failed to read {} auto-sync servers: {e}", kind.label()),
                },
                Err(e) => log::warn!("Failed to query {} auto-sync servers: {e}", kind.label()),
            }
        }
        drop(conn);

        for (kind, server_id, interval_minutes) in servers {
            self.reschedule(
                app.clone(),
                Arc::clone(&db),
                Arc::clone(&cover_manager),
                kind,
                server_id,
                interval_minutes,
            );
        }
    }
}
