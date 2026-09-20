//! Fade/crossfade settings persistence (#79) — read by both the
//! `commands::settings` IPC layer and `Player` directly. Lives here rather
//! than in `commands::settings` so `Player` doesn't have to reach up into
//! the IPC command layer for what's really just a settings read, mirroring
//! `loudness::get_settings`'s equivalent split for loudness normalization.

use crate::db::Database;
use crate::models::FadeSettings;
use std::collections::HashMap;

pub fn get_fade_settings_from_db(db: &Database) -> Result<FadeSettings, String> {
    let conn = db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM app_state WHERE key LIKE 'fade_%' OR key LIKE 'crossfade_%'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut map = HashMap::new();
    for (k, v) in rows.flatten() {
        map.insert(k, v);
    }

    let defaults = FadeSettings::default();
    Ok(FadeSettings {
        fade_pause_enabled: map
            .get("fade_pause_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.fade_pause_enabled),
        fade_pause_duration_ms: map
            .get("fade_pause_duration_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.fade_pause_duration_ms),
        crossfade_auto_enabled: map
            .get("crossfade_auto_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_auto_enabled),
        crossfade_auto_duration_secs: map
            .get("crossfade_auto_duration_secs")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.crossfade_auto_duration_secs),
        crossfade_suppress_same_album: map
            .get("crossfade_suppress_same_album")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_suppress_same_album),
    })
}
