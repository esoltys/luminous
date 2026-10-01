//! User EQ presets (#1335): named parametric filter lists plus a preamp,
//! stored in `eq_user_presets`. Names are unique case-insensitively.
//!
//! Errors are stable codes the frontend maps to translated messages.

use crate::equalizer::ParametricBand;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

pub const ERR_EMPTY_NAME: &str = "empty_name";
pub const ERR_DUPLICATE_NAME: &str = "duplicate_name";
pub const ERR_NOT_FOUND: &str = "not_found";

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
    use crate::equalizer::ParametricKind;

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
}
