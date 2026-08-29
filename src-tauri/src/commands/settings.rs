use crate::AppState;
use std::collections::HashMap;
use tauri::State;

/// Fire-and-forget by design: a failed preference write is nothing the UI
/// can act on, so it's logged here instead of rejecting the invoke() and
/// forcing every caller into a try/catch it can only console.error in.
/// (The `Result` is a Tauri requirement for async commands borrowing State —
/// this command always returns `Ok`.)
#[tauri::command]
pub async fn set_app_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let result = state
        .db
        .pool
        .get()
        .map_err(|e| e.to_string())
        .and_then(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
                rusqlite::params![key, value],
            )
            .map_err(|e| e.to_string())
        });
    if let Err(e) = result {
        log::error!("Failed to persist app setting '{key}': {e}");
    }
    Ok(())
}

/// Typed UI preferences. The schema (keys, value domains, defaults) lives
/// here rather than being implied by whatever strings the frontend happens
/// to write into the app_state KV table. Storage stays one KV row per field
/// for backwards compatibility with existing databases.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct UiPreferences {
    pub rating_style: String,
    pub seekbar_mode: String,
    pub acoustid_api_key: String,
    pub albums_view_mode: String,
    pub artists_view_mode: String,
    pub playlists_auto_view_mode: String,
    pub playlists_custom_view_mode: String,
    pub genre_view_mode: String,
    pub genre_cards_view_mode: String,
    pub genre_sort_field: String,
    pub genre_sort_asc: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            rating_style: "heart".into(),
            seekbar_mode: "waveform".into(),
            acoustid_api_key: String::new(),
            albums_view_mode: "cards".into(),
            artists_view_mode: "cards".into(),
            playlists_auto_view_mode: "cards".into(),
            playlists_custom_view_mode: "cards".into(),
            genre_view_mode: "genre".into(),
            genre_cards_view_mode: "cards".into(),
            genre_sort_field: "name".into(),
            genre_sort_asc: true,
        }
    }
}

impl UiPreferences {
    /// Field ↔ app_state key mapping, shared by load and store so the two
    /// can't drift.
    fn fields(&mut self) -> [(&'static str, &mut String, &'static [&'static str]); 10] {
        const RATING: &[&str] = &["heart", "stars"];
        const SEEKBAR: &[&str] = &["waveform", "bands"];
        const VIEW: &[&str] = &["cards", "rows"];
        const GENRE_VIEW: &[&str] = &["genre", "tags"];
        const GENRE_SORT: &[&str] = &["name", "count"];
        const ANY: &[&str] = &[];
        [
            ("rating_style", &mut self.rating_style, RATING),
            ("seekbar_mode", &mut self.seekbar_mode, SEEKBAR),
            ("acoustid_api_key", &mut self.acoustid_api_key, ANY),
            ("albums_view_mode", &mut self.albums_view_mode, VIEW),
            ("artists_view_mode", &mut self.artists_view_mode, VIEW),
            (
                "playlists_auto_view_mode",
                &mut self.playlists_auto_view_mode,
                VIEW,
            ),
            (
                "playlists_custom_view_mode",
                &mut self.playlists_custom_view_mode,
                VIEW,
            ),
            ("genre_view_mode", &mut self.genre_view_mode, GENRE_VIEW),
            (
                "genre_cards_view_mode",
                &mut self.genre_cards_view_mode,
                VIEW,
            ),
            ("genre_sort_field", &mut self.genre_sort_field, GENRE_SORT),
        ]
    }
}

#[tauri::command]
pub fn get_ui_preferences(state: State<'_, AppState>) -> UiPreferences {
    let mut prefs = UiPreferences::default();
    let Ok(conn) = state.db.pool.get() else {
        return prefs;
    };
    for (key, slot, allowed) in prefs.fields() {
        let stored: Option<String> = conn
            .query_row(
                "SELECT value FROM app_state WHERE key = ?1",
                rusqlite::params![key],
                |row| row.get(0),
            )
            .ok();
        if let Some(v) = stored {
            // An out-of-domain stored value falls back to the default rather
            // than leaking into the UI.
            if allowed.is_empty() || allowed.contains(&v.as_str()) {
                *slot = v;
            }
        }
    }
    // `genre_sort_asc` is a bool, not a domain-checked String, so it's not
    // part of the `fields()` mapping above — persisted the same way the
    // FadeSettings bools are (a literal "true"/"false" string).
    if let Ok(v) = conn.query_row(
        "SELECT value FROM app_state WHERE key = 'genre_sort_asc'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        prefs.genre_sort_asc = v == "true";
    }
    prefs
}

/// Fire-and-forget like [`set_app_setting`] — always `Ok`. Values outside a
/// field's domain are silently replaced with the default on the next load.
#[tauri::command]
pub async fn set_ui_preferences(
    state: State<'_, AppState>,
    mut prefs: UiPreferences,
) -> Result<(), String> {
    let Ok(conn) = state.db.pool.get() else {
        log::error!("Failed to persist UI preferences: no DB connection");
        return Ok(());
    };
    let genre_sort_asc = prefs.genre_sort_asc;
    for (key, slot, _) in prefs.fields() {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, slot.as_str()],
        ) {
            log::error!("Failed to persist UI preference '{key}': {e}");
        }
    }
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES ('genre_sort_asc', ?1)",
        rusqlite::params![genre_sort_asc.to_string()],
    ) {
        log::error!("Failed to persist UI preference 'genre_sort_asc': {e}");
    }
    Ok(())
}

#[tauri::command]
pub async fn get_all_app_settings(
    state: State<'_, AppState>,
) -> Result<HashMap<String, String>, String> {
    let conn = state.db.pool.get().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_state")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;

    let mut settings = HashMap::new();
    for (k, v) in rows.flatten() {
        settings.insert(k, v);
    }
    Ok(settings)
}

/// Reads the in-memory flag `tray.rs` already keeps in sync with the
/// `app_state` row of the same name — no DB round-trip needed for the read.
#[tauri::command]
pub fn get_minimize_to_tray_enabled(state: State<'_, AppState>) -> bool {
    state
        .minimize_to_tray
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// Persists the setting and updates the in-memory flag `tray.rs`'s
/// `CloseRequested` handler reads, so the new value takes effect on the
/// very next window close — not just after a restart.
#[tauri::command]
pub async fn set_minimize_to_tray_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state
        .minimize_to_tray
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    let Ok(conn) = state.db.pool.get() else {
        log::error!("Failed to persist minimize_to_tray setting: no DB connection");
        return Ok(());
    };
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES ('minimize_to_tray', ?1)",
        rusqlite::params![enabled.to_string()],
    ) {
        log::error!("Failed to persist minimize_to_tray setting: {e}");
    }
    Ok(())
}

#[tauri::command]
pub fn get_commit_hash() -> String {
    option_env!("BUILD_COMMIT_HASH").unwrap_or("").to_string()
}

#[derive(serde::Serialize)]
pub struct DbSchemaStatus {
    pub db_version: i32,
    pub app_version: i32,
    pub db_newer_than_app: bool,
}

/// Lets the frontend distinguish "library is genuinely empty" from "this
/// database was last opened by a newer build and this one can't read its
/// current schema" — the latter looks identical to an empty library (queries
/// naming a since-added/removed column just fail) without this check.
#[tauri::command]
pub fn get_db_schema_status(state: State<'_, crate::AppState>) -> DbSchemaStatus {
    DbSchemaStatus {
        db_version: state.db.schema_version,
        app_version: crate::db::CURRENT_SCHEMA_VERSION,
        db_newer_than_app: state.db.is_newer_than_app(),
    }
}

pub fn get_fade_settings_from_db(
    db: &crate::db::Database,
) -> Result<crate::models::FadeSettings, String> {
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

    let defaults = crate::models::FadeSettings::default();
    Ok(crate::models::FadeSettings {
        fade_pause_enabled: map
            .get("fade_pause_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.fade_pause_enabled),
        fade_pause_duration_ms: map
            .get("fade_pause_duration_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.fade_pause_duration_ms),
        crossfade_manual_enabled: map
            .get("crossfade_manual_enabled")
            .map(|v| v == "true")
            .unwrap_or(defaults.crossfade_manual_enabled),
        crossfade_manual_duration_ms: map
            .get("crossfade_manual_duration_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(defaults.crossfade_manual_duration_ms),
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

#[tauri::command]
pub async fn get_fade_settings(
    state: State<'_, AppState>,
) -> Result<crate::models::FadeSettings, String> {
    get_fade_settings_from_db(&state.db)
}

/// Fire-and-forget for the same reason as [`set_app_setting`] — always `Ok`.
#[tauri::command]
pub async fn set_fade_settings(
    state: State<'_, AppState>,
    settings: crate::models::FadeSettings,
) -> Result<(), String> {
    let conn = match state.db.pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            log::error!("Failed to persist fade settings: {e}");
            return Ok(());
        }
    };
    let pairs = [
        (
            "fade_pause_enabled",
            settings.fade_pause_enabled.to_string(),
        ),
        (
            "fade_pause_duration_ms",
            settings.fade_pause_duration_ms.to_string(),
        ),
        (
            "crossfade_manual_enabled",
            settings.crossfade_manual_enabled.to_string(),
        ),
        (
            "crossfade_manual_duration_ms",
            settings.crossfade_manual_duration_ms.to_string(),
        ),
        (
            "crossfade_auto_enabled",
            settings.crossfade_auto_enabled.to_string(),
        ),
        (
            "crossfade_auto_duration_secs",
            settings.crossfade_auto_duration_secs.to_string(),
        ),
        (
            "crossfade_suppress_same_album",
            settings.crossfade_suppress_same_album.to_string(),
        ),
    ];

    for (k, v) in pairs {
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES (?1, ?2)",
            rusqlite::params![k, v],
        ) {
            log::error!("Failed to persist fade setting '{k}': {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genre_sort_defaults_match_genre_view_defaults() {
        let prefs = UiPreferences::default();
        assert_eq!(prefs.genre_sort_field, "name");
        assert!(prefs.genre_sort_asc);
    }

    #[test]
    fn genre_sort_field_is_mapped_alongside_sibling_genre_view_fields() {
        let mut prefs = UiPreferences::default();
        let mapped = prefs
            .fields()
            .into_iter()
            .find(|(key, _, _)| *key == "genre_sort_field")
            .expect("genre_sort_field must be part of the persisted field mapping");
        assert_eq!(mapped.2, &["name", "count"]);
    }
}
