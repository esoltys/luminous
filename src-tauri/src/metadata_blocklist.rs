//! Persistent metadata rejection and blocklist engine (#1581).
//!
//! Stores user rejections of automated online matches (lyrics, artwork,
//! MusicBrainz IDs) so the background enrichment pipelines do not re-fetch
//! or re-associate the rejected remote entity.

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

/// Represents an explicit rejection of an online metadata association.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataRejection {
    pub id: i64,
    /// Type of local entity: "song", "album", or "artist".
    pub entity_type: String,
    /// Local identifier: song ID string, album key, or artist key.
    pub entity_id: String,
    /// Category of metadata: "lyrics", "cover_art", or "musicbrainz".
    pub category: String,
    /// Provider source: "lrclib", "netease", "lyrics_ovh", "itunes", "musicbrainz", or "*".
    pub provider: String,
    /// Remote identifier, URL, or wildcard "*".
    pub rejected_identifier: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

pub struct MetadataBlocklist;

impl MetadataBlocklist {
    /// Record a rejected remote metadata match. Idempotent (`INSERT OR IGNORE`).
    pub fn reject(
        conn: &Connection,
        entity_type: &str,
        entity_id: &str,
        category: &str,
        provider: &str,
        rejected_identifier: &str,
    ) -> Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO metadata_rejections (entity_type, entity_id, category, provider, rejected_identifier)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![entity_type, entity_id, category, provider, rejected_identifier],
        )?;
        Ok(())
    }

    /// Check if a specific remote match has been rejected.
    /// Also returns `true` if a wildcard rejection (`"*"`) exists for this (entity, category, provider).
    pub fn is_rejected(
        conn: &Connection,
        entity_type: &str,
        entity_id: &str,
        category: &str,
        provider: &str,
        rejected_identifier: &str,
    ) -> Result<bool> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM metadata_rejections
             WHERE entity_type = ?1 AND entity_id = ?2 AND category = ?3
               AND (provider = ?4 OR provider = '*')
               AND (rejected_identifier = ?5 OR rejected_identifier = '*')",
            params![
                entity_type,
                entity_id,
                category,
                provider,
                rejected_identifier
            ],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Check if ANY rejection exists for this entity, category, and provider.
    pub fn is_provider_rejected(
        conn: &Connection,
        entity_type: &str,
        entity_id: &str,
        category: &str,
        provider: &str,
    ) -> Result<bool> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM metadata_rejections
             WHERE entity_type = ?1 AND entity_id = ?2 AND category = ?3
               AND (provider = ?4 OR provider = '*')",
            params![entity_type, entity_id, category, provider],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// List all rejected providers for an entity and category.
    pub fn get_rejected_providers(
        conn: &Connection,
        entity_type: &str,
        entity_id: &str,
        category: &str,
    ) -> Result<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT provider FROM metadata_rejections
             WHERE entity_type = ?1 AND entity_id = ?2 AND category = ?3",
        )?;
        let rows = stmt.query_map(params![entity_type, entity_id, category], |row| row.get(0))?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// List rejections filtered optionally by entity_type, entity_id, and category.
    pub fn list_rejections(
        conn: &Connection,
        entity_type: Option<&str>,
        entity_id: Option<&str>,
        category: Option<&str>,
    ) -> Result<Vec<MetadataRejection>> {
        let mut sql = "SELECT id, entity_type, entity_id, category, provider, rejected_identifier, created_at FROM metadata_rejections WHERE 1=1".to_string();
        let mut param_vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(et) = entity_type {
            sql.push_str(" AND entity_type = ?");
            param_vals.push(Box::new(et.to_string()));
        }
        if let Some(eid) = entity_id {
            sql.push_str(" AND entity_id = ?");
            param_vals.push(Box::new(eid.to_string()));
        }
        if let Some(cat) = category {
            sql.push_str(" AND category = ?");
            param_vals.push(Box::new(cat.to_string()));
        }
        sql.push_str(" ORDER BY id DESC");

        let mut stmt = conn.prepare(&sql)?;
        let params_slice: Vec<&dyn rusqlite::ToSql> =
            param_vals.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_slice.as_slice(), |row| {
            Ok(MetadataRejection {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                category: row.get(3)?,
                provider: row.get(4)?,
                rejected_identifier: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Unblock a specific rejection by primary key id. Idempotent.
    pub fn unblock(conn: &Connection, id: i64) -> Result<()> {
        conn.execute("DELETE FROM metadata_rejections WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Clear all rejections for an entity and category.
    pub fn clear(
        conn: &Connection,
        entity_type: &str,
        entity_id: &str,
        category: &str,
    ) -> Result<()> {
        conn.execute(
            "DELETE FROM metadata_rejections
             WHERE entity_type = ?1 AND entity_id = ?2 AND category = ?3",
            params![entity_type, entity_id, category],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE metadata_rejections (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type         TEXT NOT NULL,
                entity_id           TEXT NOT NULL,
                category            TEXT NOT NULL,
                provider            TEXT NOT NULL,
                rejected_identifier TEXT NOT NULL,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(entity_type, entity_id, category, provider, rejected_identifier)
            );
            CREATE INDEX idx_metadata_rejections_lookup
                ON metadata_rejections (entity_type, entity_id, category);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_reject_and_is_rejected() {
        let conn = setup_test_db();
        assert!(
            !MetadataBlocklist::is_rejected(&conn, "song", "42", "lyrics", "lrclib", "123")
                .unwrap()
        );

        MetadataBlocklist::reject(&conn, "song", "42", "lyrics", "lrclib", "123").unwrap();
        assert!(
            MetadataBlocklist::is_rejected(&conn, "song", "42", "lyrics", "lrclib", "123").unwrap()
        );
        // Different entity
        assert!(
            !MetadataBlocklist::is_rejected(&conn, "song", "43", "lyrics", "lrclib", "123")
                .unwrap()
        );
        // Different provider
        assert!(
            !MetadataBlocklist::is_rejected(&conn, "song", "42", "lyrics", "netease", "123")
                .unwrap()
        );
        // Different identifier
        assert!(
            !MetadataBlocklist::is_rejected(&conn, "song", "42", "lyrics", "lrclib", "456")
                .unwrap()
        );
    }

    #[test]
    fn test_wildcard_rejection() {
        let conn = setup_test_db();
        // Reject all matches from lrclib for song 42
        MetadataBlocklist::reject(&conn, "song", "42", "lyrics", "lrclib", "*").unwrap();
        assert!(
            MetadataBlocklist::is_rejected(&conn, "song", "42", "lyrics", "lrclib", "any_id")
                .unwrap()
        );
        assert!(
            MetadataBlocklist::is_provider_rejected(&conn, "song", "42", "lyrics", "lrclib")
                .unwrap()
        );
        assert!(
            !MetadataBlocklist::is_provider_rejected(&conn, "song", "42", "lyrics", "netease")
                .unwrap()
        );
    }

    #[test]
    fn test_list_and_unblock() {
        let conn = setup_test_db();
        MetadataBlocklist::reject(&conn, "song", "10", "lyrics", "lrclib", "id-1").unwrap();
        MetadataBlocklist::reject(&conn, "song", "10", "lyrics", "netease", "id-2").unwrap();
        MetadataBlocklist::reject(
            &conn,
            "album",
            "album_key_1",
            "cover_art",
            "itunes",
            "url-1",
        )
        .unwrap();

        let song_rejections =
            MetadataBlocklist::list_rejections(&conn, Some("song"), Some("10"), Some("lyrics"))
                .unwrap();
        assert_eq!(song_rejections.len(), 2);

        let id_to_remove = song_rejections[0].id;
        MetadataBlocklist::unblock(&conn, id_to_remove).unwrap();

        let after_unblock =
            MetadataBlocklist::list_rejections(&conn, Some("song"), Some("10"), Some("lyrics"))
                .unwrap();
        assert_eq!(after_unblock.len(), 1);

        MetadataBlocklist::clear(&conn, "song", "10", "lyrics").unwrap();
        let after_clear =
            MetadataBlocklist::list_rejections(&conn, Some("song"), Some("10"), Some("lyrics"))
                .unwrap();
        assert_eq!(after_clear.len(), 0);
    }
}
