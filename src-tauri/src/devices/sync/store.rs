//! `device_sync_settings`: what this computer keeps about each device's sync,
//! the selection and the summary of the last sync.

use super::selection::SyncSettings;
use super::worker::SyncSummary;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

/// The saved selection for `device_id`, defaults when there is none.
pub fn load_settings(conn: &rusqlite::Connection, device_id: &str) -> Result<SyncSettings> {
    let json: Option<String> = conn
        .query_row(
            "SELECT settings FROM device_sync_settings WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

pub fn save_settings(
    conn: &rusqlite::Connection,
    device_id: &str,
    settings: &SyncSettings,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO device_sync_settings (device_id, settings, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(device_id) DO UPDATE SET settings = excluded.settings,
             updated_at = excluded.updated_at",
        params![device_id, serde_json::to_string(settings)?, now],
    )?;
    Ok(())
}

/// What the last sync to `device_id` from this computer did; `None` before
/// the first, or when it was saved by a version that can't be read.
pub fn last_summary(conn: &rusqlite::Connection, device_id: &str) -> Result<Option<SyncSummary>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT last_summary FROM device_sync_settings WHERE device_id = ?1",
            [device_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

pub fn save_summary(conn: &rusqlite::Connection, summary: &SyncSummary) -> Result<()> {
    conn.execute(
        "INSERT INTO device_sync_settings (device_id, last_summary, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(device_id) DO UPDATE SET last_summary = excluded.last_summary",
        params![
            summary.device_id,
            serde_json::to_string(summary)?,
            summary.finished_at
        ],
    )?;
    Ok(())
}
