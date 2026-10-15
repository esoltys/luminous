//! EQ persistence: user presets (#1335) — named parametric filter lists plus
//! a preamp, stored in `eq_user_presets`, unique case-insensitively — and the
//! running EQ state, stored in the single `equalizer_settings` row.
//!
//! Errors are stable codes the frontend maps to translated messages.

use luminous_equalizer::{
    builtin_preset_name, EqMode, EqualizerState, ModeSettings, ParametricBand,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const ERR_EMPTY_NAME: &str = "empty_name";
pub const ERR_DUPLICATE_NAME: &str = "duplicate_name";
pub const ERR_NOT_FOUND: &str = "not_found";

/// The `active_preset` label naming a saved user preset. The equalizer treats
/// preset labels as opaque; this prefix is how the app tells a user preset
/// from a built-in name.
pub fn user_preset_key(id: i64) -> String {
    format!("user:{id}")
}

/// The user-preset id in an `active_preset` label, if it names one.
pub fn parse_user_preset_key(key: &str) -> Option<i64> {
    key.strip_prefix("user:")?.parse().ok()
}

fn mode_name(mode: EqMode) -> &'static str {
    match mode {
        EqMode::Graphic10 => "graphic10",
        EqMode::Parametric => "parametric",
    }
}

fn other_mode(mode: EqMode) -> EqMode {
    match mode {
        EqMode::Graphic10 => EqMode::Parametric,
        EqMode::Parametric => EqMode::Graphic10,
    }
}

/// Saves the whole EQ state. Fire-and-forget — a failed EQ write is invisible
/// to the user mid-drag and nothing the UI can act on.
pub fn save_state(conn: &Connection, state: &EqualizerState) {
    let gains = state
        .gains
        .iter()
        .map(|g| g.to_string())
        .collect::<Vec<String>>()
        .join(",");
    let parametric = serde_json::to_string(&state.parametric).unwrap_or_default();
    // The row stores the active mode's preamp/preset and the other mode's
    // as `inactive_*` (migration 56).
    let active = state.settings(state.mode);
    let inactive = state.settings(other_mode(state.mode));
    let result = conn.execute(
        "UPDATE equalizer_settings
         SET enabled = ?1, preamp = ?2, gains = ?3, mode = ?4, parametric = ?5,
             active_preset = ?6, inactive_preamp = ?7, inactive_preset = ?8
         WHERE id = 1",
        params![
            state.enabled,
            active.preamp as f64,
            gains,
            mode_name(state.mode),
            parametric,
            active.preset.as_deref().unwrap_or(""),
            inactive.preamp as f64,
            inactive.preset.as_deref().unwrap_or(""),
        ],
    );
    if let Err(e) = result {
        log::warn!("Failed to save equalizer settings: {e}");
    }
}

/// The saved EQ state, or `None` if the row can't be read. Unreadable
/// fields fall back rather than failing the whole restore: a bad gain reads
/// 0 dB, unparseable bands an empty list (the default layout), and a preset
/// that no longer exists (e.g. the removed built-in "Headphones") Custom.
pub fn load_state(conn: &Connection) -> Option<EqualizerState> {
    let (enabled, preamp, gains, mode, parametric, preset, inactive_preamp, inactive_preset) = conn
        .query_row(
            "SELECT enabled, preamp, gains, mode, parametric, active_preset,
                    inactive_preamp, inactive_preset
             FROM equalizer_settings WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i32>(0)? != 0,
                    row.get::<_, f64>(1)? as f32,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, f64>(6)? as f32,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .ok()?;

    let mut parsed_gains = [0.0f32; 10];
    for (slot, val) in parsed_gains.iter_mut().zip(gains.split(',')) {
        *slot = val.parse().unwrap_or(0.0);
    }
    // "parametric20" is the pre-#1332 name (migration 53 rewrites it).
    let mode = if mode == "parametric" || mode == "parametric20" {
        EqMode::Parametric
    } else {
        EqMode::Graphic10
    };
    let known = |p: String| {
        Some(p).filter(|p| parse_user_preset_key(p).is_some() || builtin_preset_name(p).is_some())
    };
    let active = ModeSettings {
        preamp,
        preset: known(preset),
    };
    let inactive = ModeSettings {
        preamp: inactive_preamp,
        preset: known(inactive_preset),
    };
    let (graphic_settings, parametric_settings) = match mode {
        EqMode::Graphic10 => (active, inactive),
        EqMode::Parametric => (inactive, active),
    };
    Some(EqualizerState {
        enabled,
        mode,
        gains: parsed_gains,
        parametric: serde_json::from_str(&parametric).unwrap_or_default(),
        graphic_settings,
        parametric_settings,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UserPreset {
    pub id: i64,
    pub name: String,
}

pub struct StoredPreset {
    pub bands: Vec<ParametricBand>,
    pub preamp: f32,
}

fn clean_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        Err(ERR_EMPTY_NAME.into())
    } else {
        Ok(name)
    }
}

fn map_write_err(e: rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(err, _)
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            ERR_DUPLICATE_NAME.into()
        }
        other => other.to_string(),
    }
}

/// Every user preset, alphabetically.
pub fn list(conn: &Connection) -> Result<Vec<UserPreset>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name FROM eq_user_presets ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UserPreset {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredUserPreset {
    pub id: i64,
    pub name: String,
    pub bands: Vec<ParametricBand>,
    pub preamp: f32,
}

/// Every user preset with its stored bands, ordered by name.
pub fn list_with_bands(conn: &Connection) -> Result<Vec<StoredUserPreset>, String> {
    let mut stmt = conn
        .prepare("SELECT id, name, bands, preamp FROM eq_user_presets ORDER BY name COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let id: i64 = r.get(0)?;
            let name: String = r.get(1)?;
            let bands_json: String = r.get(2)?;
            let preamp: f64 = r.get(3)?;
            let bands: Vec<ParametricBand> = serde_json::from_str(&bands_json).unwrap_or_default();
            Ok(StoredUserPreset {
                id,
                name,
                bands,
                preamp: preamp as f32,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

pub fn get(conn: &Connection, id: i64) -> Result<StoredPreset, String> {
    let row: Option<(String, f64)> = conn
        .query_row(
            "SELECT bands, preamp FROM eq_user_presets WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (bands, preamp) = row.ok_or(ERR_NOT_FOUND)?;
    Ok(StoredPreset {
        bands: serde_json::from_str(&bands).map_err(|e| e.to_string())?,
        preamp: preamp as f32,
    })
}

/// Save a new preset; returns its id.
pub fn create(
    conn: &Connection,
    name: &str,
    bands: &[ParametricBand],
    preamp: f32,
) -> Result<i64, String> {
    let name = clean_name(name)?;
    let bands = serde_json::to_string(bands).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO eq_user_presets (name, bands, preamp) VALUES (?1, ?2, ?3)",
        params![name, bands, preamp as f64],
    )
    .map_err(map_write_err)?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<(), String> {
    let name = clean_name(name)?;
    let changed = conn
        .execute(
            "UPDATE eq_user_presets SET name = ?1 WHERE id = ?2",
            params![name, id],
        )
        .map_err(map_write_err)?;
    if changed == 0 {
        return Err(ERR_NOT_FOUND.into());
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    conn.execute("DELETE FROM eq_user_presets WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use luminous_equalizer::ParametricKind;

    fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::Builder::new()
            .prefix("luminous_eq_presets_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        (dir, db)
    }

    fn bands() -> Vec<ParametricBand> {
        vec![ParametricBand {
            kind: ParametricKind::LowShelf,
            freq: 80.0,
            gain_db: 3.5,
            q: 0.7,
            enabled: false,
        }]
    }

    #[test]
    fn create_get_list_rename_delete() {
        let (_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let b = create(&conn, "  Zeta  ", &bands(), -1.5).unwrap();
        let a = create(&conn, "alpha", &bands(), 0.0).unwrap();

        let stored = get(&conn, b).unwrap();
        assert_eq!(stored.bands, bands());
        assert_eq!(stored.preamp, -1.5);
        assert_eq!(
            list(&conn).unwrap(),
            vec![
                UserPreset {
                    id: a,
                    name: "alpha".into()
                },
                UserPreset {
                    id: b,
                    name: "Zeta".into()
                },
            ]
        );

        rename(&conn, b, "Beta").unwrap();
        assert_eq!(list(&conn).unwrap()[1].name, "Beta");
        delete(&conn, a).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);
        assert_eq!(get(&conn, a).err().as_deref(), Some(ERR_NOT_FOUND));
    }

    #[test]
    fn names_are_unique_ignoring_case_and_never_empty() {
        let (_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let id = create(&conn, "Studio", &bands(), 0.0).unwrap();
        let other = create(&conn, "Other", &bands(), 0.0).unwrap();
        assert_eq!(
            create(&conn, "STUDIO", &bands(), 0.0).err().as_deref(),
            Some(ERR_DUPLICATE_NAME)
        );
        assert_eq!(
            rename(&conn, other, "studio").err().as_deref(),
            Some(ERR_DUPLICATE_NAME)
        );
        assert_eq!(
            create(&conn, "   ", &bands(), 0.0).err().as_deref(),
            Some(ERR_EMPTY_NAME)
        );
        // Re-casing a preset's own name is not a clash with itself.
        rename(&conn, id, "STUDIO").unwrap();
        assert_eq!(
            rename(&conn, 999, "x").err().as_deref(),
            Some(ERR_NOT_FOUND)
        );
    }

    #[test]
    fn eq_state_round_trips_through_the_settings_row() {
        let (_dir, db) = test_db();
        let conn = db.pool.get().unwrap();
        let mut gains = [0.0; 10];
        gains[0] = 1.5;
        let state = EqualizerState {
            enabled: true,
            mode: EqMode::Parametric,
            gains,
            parametric: bands(),
            graphic_settings: ModeSettings {
                preamp: -2.0,
                preset: Some("Rock".into()),
            },
            parametric_settings: ModeSettings {
                preamp: -4.5,
                preset: Some(user_preset_key(3)),
            },
        };
        save_state(&conn, &state);
        assert_eq!(load_state(&conn), Some(state));

        // A built-in that has since been removed loads as Custom.
        conn.execute(
            "UPDATE equalizer_settings SET active_preset = 'Headphones'",
            [],
        )
        .unwrap();
        assert_eq!(load_state(&conn).unwrap().parametric_settings.preset, None);
    }
}
