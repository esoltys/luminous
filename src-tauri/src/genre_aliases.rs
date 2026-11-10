//! Non-destructive genre aliases (#1942): the user maps variant spellings
//! (*Prog Rock*, *prog-rock*) onto one canonical genre without touching any
//! file's tags. The `songs.genre` column and the audio files stay as they
//! are; every genre reader looks through [`GenreAliases`] instead, so
//! removing an alias brings the raw value straight back.
//!
//! Aliases are stored flat: every row points at a canonical name, and a
//! canonical name is never itself an alias. [`set_alias`] keeps that true,
//! which makes a lookup one hop and rules out cycles by construction.

use crate::models::{parse_multi_value, LIBRARY_SOURCES_SQL};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One alias → canonical mapping, with how many library songs carry the
/// alias spelling in their raw genre tag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenreAlias {
    pub alias: String,
    pub canonical: String,
    pub song_count: i64,
}

/// The alias table, loaded once per operation. The table is small (one row
/// per variant spelling the user merged), so reading it whole is cheaper than
/// joining it into every genre query.
#[derive(Debug, Default)]
pub struct GenreAliases {
    /// Lowercased alias → canonical name as the user wrote it.
    canonical_of: HashMap<String, String>,
    /// Lowercased canonical → lowercased aliases pointing at it.
    aliases_of: HashMap<String, Vec<String>>,
}

impl GenreAliases {
    pub fn load(conn: &Connection) -> Result<Self> {
        let mut stmt = conn.prepare("SELECT alias_name, canonical_name FROM genre_aliases")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = Self::default();
        for (alias, canonical) in rows {
            let alias = alias.to_lowercase();
            out.aliases_of
                .entry(canonical.to_lowercase())
                .or_default()
                .push(alias.clone());
            out.canonical_of.insert(alias, canonical);
        }
        Ok(out)
    }

    /// Splits a raw `; `-delimited genre string like [`parse_multi_value`],
    /// then replaces each aliased value with its canonical name and drops
    /// repeats (case-insensitively), so `"Prog Rock; Progressive Rock"`
    /// becomes one value. Order of first appearance is kept, because the
    /// first value is a song's main category.
    pub fn canonical_genres(&self, raw: &str) -> Vec<String> {
        let mut seen = HashSet::new();
        parse_multi_value(raw)
            .into_iter()
            .map(|v| {
                self.canonical_of
                    .get(&v.to_lowercase())
                    .cloned()
                    .unwrap_or(v)
            })
            .filter(|v| seen.insert(v.to_lowercase()))
            .collect()
    }

    /// Every lowercased spelling that counts as `name`: its canonical form
    /// plus all aliases of it. A name with no aliases yields just itself, so
    /// callers never special-case the unaliased path.
    pub fn members_of(&self, name: &str) -> HashSet<String> {
        let lower = name.trim().to_lowercase();
        let root = self
            .canonical_of
            .get(&lower)
            .map_or_else(|| lower.clone(), |c| c.to_lowercase());
        let mut members = HashSet::from([root.clone(), lower]);
        if let Some(aliases) = self.aliases_of.get(&root) {
            members.extend(aliases.iter().cloned());
        }
        members
    }

    /// [`Self::members_of`] for every name in `names`.
    pub fn expand(&self, names: &HashSet<String>) -> HashSet<String> {
        names.iter().flat_map(|n| self.members_of(n)).collect()
    }
}

/// Makes `alias` another spelling of `canonical`. Never fails on odd input:
/// a blank alias or an alias equal to its canonical just clears any mapping
/// for `alias`. If `canonical` is itself an alias its target is used, rows
/// that pointed at `alias` follow it to the new canonical name, and mapping
/// a canonical name back onto one of its own aliases re-roots the group
/// (the latest statement wins) rather than forming a cycle.
pub fn set_alias(conn: &mut Connection, alias: &str, canonical: &str) -> Result<()> {
    let (alias, canonical) = (alias.trim(), canonical.trim());
    let tx = conn.transaction()?;
    if alias.is_empty() {
        return Ok(());
    }
    if canonical.is_empty() || alias.eq_ignore_ascii_case(canonical) {
        tx.execute(
            "DELETE FROM genre_aliases WHERE alias_name = ?1",
            params![alias],
        )?;
        return Ok(tx.commit()?);
    }
    let mut target = lookup(&tx, canonical)?;
    if target
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case(alias))
    {
        // `canonical` is currently an alias of `alias`: promote it to root.
        tx.execute(
            "DELETE FROM genre_aliases WHERE alias_name = ?1",
            params![canonical],
        )?;
        target = None;
    }
    let target = target.unwrap_or_else(|| canonical.to_string());
    tx.execute(
        "UPDATE genre_aliases SET canonical_name = ?2 WHERE canonical_name = ?1",
        params![alias, target],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO genre_aliases (alias_name, canonical_name) VALUES (?1, ?2)",
        params![alias, target],
    )?;
    Ok(tx.commit()?)
}

/// Removes the mapping for `alias`. Removing one that doesn't exist is a
/// successful no-op.
pub fn remove_alias(conn: &Connection, alias: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM genre_aliases WHERE alias_name = ?1",
        params![alias.trim()],
    )?;
    Ok(())
}

fn lookup(conn: &Connection, name: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT canonical_name FROM genre_aliases WHERE alias_name = ?1",
            params![name],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?)
}

/// All mappings, ordered by canonical then alias, each with the number of
/// library songs whose raw genre tag contains the alias spelling.
pub fn list_aliases(conn: &Connection) -> Result<Vec<GenreAlias>> {
    let mut stmt = conn
        .prepare("SELECT alias_name, canonical_name FROM genre_aliases ORDER BY canonical_name COLLATE NOCASE, alias_name COLLATE NOCASE")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let sql = format!(
        "SELECT genre FROM songs
         WHERE source IN ({lib}) AND unavailable = 0 AND genre IS NOT NULL AND genre != ''",
        lib = *LIBRARY_SOURCES_SQL
    );
    let mut counts: HashMap<String, i64> = HashMap::new();
    let mut stmt = conn.prepare(&sql)?;
    for raw in stmt.query_map([], |r| r.get::<_, String>(0))?.flatten() {
        for value in parse_multi_value(&raw) {
            *counts.entry(value.to_lowercase()).or_insert(0) += 1;
        }
    }
    Ok(rows
        .into_iter()
        .map(|(alias, canonical)| GenreAlias {
            song_count: counts.get(&alias.to_lowercase()).copied().unwrap_or(0),
            alias,
            canonical,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn conn() -> (
        tempfile::TempDir,
        r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>,
    ) {
        let dir = tempfile::Builder::new()
            .prefix("luminous_genre_aliases_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(dir.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();
        (dir, conn)
    }

    fn pairs(conn: &Connection) -> Vec<(String, String)> {
        list_aliases(conn)
            .unwrap()
            .into_iter()
            .map(|a| (a.alias, a.canonical))
            .collect()
    }

    #[test]
    fn canonicalizes_each_token_and_dedupes() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "Prog Rock", "Progressive Rock").unwrap();
        let aliases = GenreAliases::load(&c).unwrap();
        assert_eq!(
            aliases.canonical_genres("prog rock; Art Rock; Progressive Rock"),
            vec!["Progressive Rock", "Art Rock"]
        );
        assert_eq!(aliases.canonical_genres("Jazz"), vec!["Jazz"]);
    }

    #[test]
    fn members_of_covers_canonical_and_aliases() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "Prog Rock", "Progressive Rock").unwrap();
        set_alias(&mut c, "prog-rock", "Progressive Rock").unwrap();
        let aliases = GenreAliases::load(&c).unwrap();
        let expected: HashSet<String> = ["progressive rock", "prog rock", "prog-rock"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(aliases.members_of("Progressive Rock"), expected);
        assert_eq!(aliases.members_of("PROG ROCK"), expected);
        assert_eq!(
            aliases.members_of("Jazz"),
            HashSet::from(["jazz".to_string()])
        );
    }

    #[test]
    fn chains_flatten() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "A", "B").unwrap();
        set_alias(&mut c, "B", "C").unwrap();
        assert_eq!(
            pairs(&c),
            vec![("A".into(), "C".into()), ("B".into(), "C".into())]
        );
        // Aliasing onto an alias lands on its root.
        set_alias(&mut c, "D", "A").unwrap();
        assert!(pairs(&c).contains(&("D".into(), "C".into())));
    }

    #[test]
    fn reverse_mapping_reroots_instead_of_cycling() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "A", "B").unwrap();
        set_alias(&mut c, "D", "B").unwrap();
        set_alias(&mut c, "B", "A").unwrap();
        assert_eq!(
            pairs(&c),
            vec![("B".into(), "A".into()), ("D".into(), "A".into())]
        );
    }

    #[test]
    fn self_alias_blank_and_missing_removal_are_noops() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "A", "B").unwrap();
        set_alias(&mut c, "  ", "B").unwrap();
        remove_alias(&c, "nope").unwrap();
        assert_eq!(pairs(&c).len(), 1);
        set_alias(&mut c, "a", "A").unwrap();
        assert!(pairs(&c).is_empty());
    }

    #[test]
    fn remove_restores_raw_value() {
        let (_d, mut c) = conn();
        set_alias(&mut c, "Prog Rock", "Progressive Rock").unwrap();
        remove_alias(&c, "prog rock").unwrap();
        let aliases = GenreAliases::load(&c).unwrap();
        assert_eq!(aliases.canonical_genres("Prog Rock"), vec!["Prog Rock"]);
    }

    #[test]
    fn list_counts_songs_with_the_alias_spelling() {
        let (_d, mut c) = conn();
        for (path, genre) in [
            ("/a", "Prog Rock; Art Rock"),
            ("/b", "prog rock"),
            ("/c", "Jazz"),
        ] {
            c.execute(
                "INSERT INTO songs (source, filetype, path, genre) VALUES (1, 0, ?1, ?2)",
                params![path, genre],
            )
            .unwrap();
        }
        set_alias(&mut c, "Prog Rock", "Progressive Rock").unwrap();
        assert_eq!(list_aliases(&c).unwrap()[0].song_count, 2);
    }
}
