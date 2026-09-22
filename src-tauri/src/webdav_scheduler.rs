//! Periodic auto-sync scheduling for WebDAV servers (#1082).
//!
//! Local watched folders get live change notifications from a filesystem
//! watcher (`collection::start_watcher`); a plain WebDAV server can't push
//! change notifications, so a server that opts into auto-sync instead gets
//! its own tokio interval task that re-runs `sync_webdav_server_inner` on a
//! schedule. Tasks are keyed by server id so a single server's schedule can
//! be replaced or cancelled (on save/delete) without touching any other
//! server's timer or restarting the app.

use crate::covermanager::CoverManager;
use crate::db::Database;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;

#[derive(Default)]
pub struct AutoSyncScheduler {
    tasks: parking_lot::Mutex<HashMap<i64, JoinHandle<()>>>,
    /// Unix timestamp (seconds) each scheduled server's next tick is due —
    /// purely in-memory, recomputed on every (re)schedule and after every
    /// tick, so the frontend can show "next sync in N minutes" (#1082).
    next_run: parking_lot::Mutex<HashMap<i64, i64>>,
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
        server_id: i64,
        interval_minutes: i64,
    ) {
        self.cancel(server_id);

        let interval_secs = interval_minutes.max(1) as u64 * 60;
        let period = std::time::Duration::from_secs(interval_secs);
        self.next_run
            .lock()
            .insert(server_id, now_unix() + interval_secs as i64);

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
                    .insert(server_id, now_unix() + interval_secs as i64);

                if Self::is_syncing(&db, server_id) {
                    log::debug!(
                        "Skipping scheduled WebDAV auto-sync for server {server_id}: a sync is already in flight"
                    );
                    continue;
                }

                if let Err(e) = crate::commands::webdav::sync_webdav_server_inner(
                    server_id,
                    app.clone(),
                    Arc::clone(&db),
                    Arc::clone(&cover_manager),
                )
                .await
                {
                    log::warn!("Scheduled WebDAV auto-sync failed for server {server_id}: {e}");
                }
            }
        });

        self.tasks.lock().insert(server_id, handle);
    }

    /// Cancels the running auto-sync timer for `server_id`, if any.
    pub fn cancel(&self, server_id: i64) {
        if let Some(handle) = self.tasks.lock().remove(&server_id) {
            handle.abort();
        }
        self.next_run.lock().remove(&server_id);
    }

    /// Unix timestamp (seconds) of `server_id`'s next scheduled auto-sync, or
    /// `None` if it has no timer running.
    pub fn next_run_at(&self, server_id: i64) -> Option<i64> {
        self.next_run.lock().get(&server_id).copied()
    }

    fn is_syncing(db: &Arc<Database>, server_id: i64) -> bool {
        db.pool
            .get()
            .ok()
            .and_then(|conn| {
                conn.query_row(
                    "SELECT sync_status FROM webdav_servers WHERE id = ?1",
                    rusqlite::params![server_id],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            })
            .map(|status| status == "syncing")
            .unwrap_or(false)
    }

    /// Starts a schedule for every server that currently has auto-sync
    /// enabled. Called once at app startup, mirroring how the folder watcher
    /// is started from the servers/directories on disk rather than assuming
    /// no reschedule ever happened.
    pub fn start_all_from_db(
        self: &Arc<Self>,
        app: AppHandle,
        db: Arc<Database>,
        cover_manager: Arc<CoverManager>,
    ) {
        let conn = match db.pool.get() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to load WebDAV servers for auto-sync scheduling: {e}");
                return;
            }
        };

        let servers: Vec<(i64, i64)> = match conn.prepare(
            "SELECT id, sync_interval_minutes FROM webdav_servers WHERE enabled = 1 AND auto_sync_enabled = 1",
        ) {
            Ok(mut stmt) => {
                let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)));
                match rows {
                    Ok(rows) => rows.flatten().collect(),
                    Err(e) => {
                        log::warn!("Failed to read WebDAV auto-sync servers: {e}");
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to query WebDAV auto-sync servers: {e}");
                Vec::new()
            }
        };
        drop(conn);

        for (server_id, interval_minutes) in servers {
            self.reschedule(
                app.clone(),
                Arc::clone(&db),
                Arc::clone(&cover_manager),
                server_id,
                interval_minutes,
            );
        }
    }
}
