//! Artist and album profile persistence (#473, #950, #2097).
//!
//! All reads and writes to `artist_profiles` and `album_profiles` go through
//! this module's public functions, which enforce case-insensitive canonical
//! artist keying (`canonical_artist_key`), country code normalization, tag-casing
//! canonicalization, and composite album keying.

use crate::models::{AlbumLink, AlbumProfile, ArtistProfile, ArtistSocialLink};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

/// Column list shared by every `artist_profiles` read, in the order
/// `artist_profile_from_row` expects.
const ARTIST_PROFILE_COLUMNS: &str = "artist_key, website, tags, social_links, bio, musicbrainz_artist_id, fetched_image_filename, fetched_image_source, details_fetched, image_fetched, fetched_logo_filename, fetched_background_filename, logo_fetched, background_fetched, country";

fn artist_profile_from_row(row: &rusqlite::Row) -> rusqlite::Result<ArtistProfile> {
    let tags_json: String = row.get(2)?;
    let social_links_json: String = row.get(3)?;
    Ok(ArtistProfile {
        artist_key: row.get(0)?,
        website: row.get(1)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        social_links: serde_json::from_str::<Vec<ArtistSocialLink>>(&social_links_json)
            .unwrap_or_default(),
        bio: row.get(4)?,
        musicbrainz_artist_id: row.get(5)?,
        fetched_image_filename: row.get(6)?,
        fetched_image_source: row.get(7)?,
        details_fetched: row.get(8)?,
        image_fetched: row.get(9)?,
        fetched_logo_filename: row.get(10)?,
        fetched_background_filename: row.get(11)?,
        logo_fetched: row.get(12)?,
        background_fetched: row.get(13)?,
        country: row.get(14)?,
    })
}

/// Retrieve customizable profile for an artist from SQLite (#473).
pub fn get_artist_profile(conn: &Connection, artist: &str) -> Result<ArtistProfile> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ARTIST_PROFILE_COLUMNS} FROM artist_profiles WHERE artist_key = ?1 COLLATE NOCASE"
    ))?;
    match stmt.query_row(params![artist], artist_profile_from_row) {
        Ok(profile) => Ok(profile),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ArtistProfile {
            artist_key: artist.to_string(),
            ..Default::default()
        }),
        Err(e) => Err(e.into()),
    }
}

/// Renames every occurrence of a tag in *other* artists' profiles that
/// matches `tag` case-insensitively but not exactly, to `tag`'s casing.
/// Tags are otherwise freeform per-artist strings with no shared canonical
/// row, so without this, saving "Canadian" on one artist while another
/// already has "canadian" would leave the same tag split across two cases
/// in the library-wide tag list (see `get_library_artist_tags`).
fn canonicalize_artist_tag_casing(
    conn: &Connection,
    current_artist_key: &str,
    tag: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT artist_key, tags FROM artist_profiles WHERE artist_key <> ?1 COLLATE NOCASE",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map(params![current_artist_key], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;

    for (artist_key, tags_json) in rows {
        let mut tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let mut changed = false;
        for t in tags.iter_mut() {
            if t != tag && t.to_lowercase() == tag.to_lowercase() {
                *t = tag.to_string();
                changed = true;
            }
        }
        if changed {
            let new_tags_json = serde_json::to_string(&tags)?;
            conn.execute(
                "UPDATE artist_profiles SET tags = ?1 WHERE artist_key = ?2",
                params![new_tags_json, artist_key],
            )?;
        }
    }

    Ok(())
}

/// The spelling a profile for `artist` is stored under (#2097).
///
/// `artist_profiles.artist_key` is a case-insensitive key (`COLLATE NOCASE`):
/// "The War on Drugs" and "The War On Drugs" are the same row, for every
/// reader and writer, so an external tool needs no casing of its own. The
/// stored *spelling* is the library's — the artist as the songs table spells
/// it — when the library has one; otherwise the name as given. Every write
/// below resolves through here, and an upsert rewrites a stale spelling.
fn canonical_artist_key(conn: &Connection, artist: &str) -> Result<String> {
    // Fast path: the name is already a library spelling (what the app itself
    // passes). `idx_songs_artist` serves it; only a differently-cased name
    // pays the scan below, which no index can serve.
    let exact: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM songs WHERE artist = ?1)",
        params![artist],
        |row| row.get(0),
    )?;
    if exact {
        return Ok(artist.to_string());
    }
    let spelling: Option<String> = conn
        .query_row(
            "SELECT COALESCE(NULLIF(album_artist, ''), artist) FROM songs
             WHERE COALESCE(NULLIF(album_artist, ''), artist) = ?1 COLLATE NOCASE
             ORDER BY COALESCE(NULLIF(album_artist, ''), artist) = ?1 DESC
             LIMIT 1",
            params![artist],
            |row| row.get(0),
        )
        .optional()?;
    Ok(spelling.unwrap_or_else(|| artist.to_string()))
}

/// Upsert an artist profile into SQLite (#473). The key is stored in the
/// library's spelling (see `canonical_artist_key`); the returned profile
/// carries it.
pub fn set_artist_profile(conn: &Connection, profile: &ArtistProfile) -> Result<ArtistProfile> {
    let profile = ArtistProfile {
        artist_key: canonical_artist_key(conn, &profile.artist_key)?,
        country: normalize_country_code(profile.country.as_deref()),
        ..profile.clone()
    };
    let profile = &profile;
    let tags_json = serde_json::to_string(&profile.tags)?;
    let social_links_json = serde_json::to_string(&profile.social_links)?;

    conn.execute(
        "INSERT INTO artist_profiles (artist_key, website, tags, social_links, bio, musicbrainz_artist_id, fetched_image_filename, fetched_image_source, details_fetched, image_fetched, fetched_logo_filename, fetched_background_filename, logo_fetched, background_fetched, country)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(artist_key) DO UPDATE SET
            artist_key = excluded.artist_key,
            website = excluded.website,
            tags = excluded.tags,
            social_links = excluded.social_links,
            bio = excluded.bio,
            musicbrainz_artist_id = excluded.musicbrainz_artist_id,
            fetched_image_filename = excluded.fetched_image_filename,
            fetched_image_source = excluded.fetched_image_source,
            details_fetched = excluded.details_fetched,
            image_fetched = excluded.image_fetched,
            fetched_logo_filename = excluded.fetched_logo_filename,
            fetched_background_filename = excluded.fetched_background_filename,
            logo_fetched = excluded.logo_fetched,
            background_fetched = excluded.background_fetched,
            country = excluded.country",
        params![
            profile.artist_key,
            profile.website,
            tags_json,
            social_links_json,
            profile.bio,
            profile.musicbrainz_artist_id,
            profile.fetched_image_filename,
            profile.fetched_image_source,
            profile.details_fetched as i32,
            profile.image_fetched as i32,
            profile.fetched_logo_filename,
            profile.fetched_background_filename,
            profile.logo_fetched as i32,
            profile.background_fetched as i32,
            profile.country
        ],
    )?;

    for tag in &profile.tags {
        canonicalize_artist_tag_casing(conn, &profile.artist_key, tag)?;
    }

    Ok(profile.clone())
}

/// Update only `artist_profiles.bio` (inserting a minimal row if absent),
/// leaving `tags`, links, and enrichment columns untouched.
pub fn set_artist_bio(conn: &Connection, artist: &str, bio: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO artist_profiles (artist_key, bio)
         VALUES (?1, ?2)
         ON CONFLICT(artist_key) DO UPDATE SET
            artist_key = excluded.artist_key, bio = excluded.bio",
        params![canonical_artist_key(conn, artist)?, bio],
    )?;
    Ok(())
}

/// Update only `artist_profiles.musicbrainz_artist_id` (inserting a minimal
/// row if absent), leaving `tags`, bio, and other columns untouched.
pub fn set_artist_mbid(conn: &Connection, artist: &str, mbid: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO artist_profiles (artist_key, musicbrainz_artist_id)
         VALUES (?1, ?2)
         ON CONFLICT(artist_key) DO UPDATE SET
            artist_key = excluded.artist_key,
            musicbrainz_artist_id = excluded.musicbrainz_artist_id",
        params![canonical_artist_key(conn, artist)?, mbid],
    )?;
    Ok(())
}

/// Fills a blank `artist_profiles.country` from MusicBrainz, for the profile
/// already carrying that MusicBrainz artist id (#2095). Returns the profile keys
/// it changed, so the caller can tell the rest of the app only when something did.
///
/// A column-scoped, conditional write on purpose: it never creates a profile
/// (a row appearing just because an artist was viewed would read as curated to
/// external tools), never overwrites a country the user or an external tool set,
/// and never touches tags or any other column. A non-code value is ignored.
pub fn fill_blank_artist_country_by_mbid(
    conn: &Connection,
    mbid: &str,
    country: &str,
) -> Result<Vec<String>> {
    let Some(code) = normalize_country_code(Some(country)) else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(
        "SELECT artist_key FROM artist_profiles
         WHERE musicbrainz_artist_id = ?1 AND COALESCE(country, '') = ''",
    )?;
    let keys: Vec<String> = stmt
        .query_map(params![mbid], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for key in &keys {
        conn.execute(
            "UPDATE artist_profiles SET country = ?1 WHERE artist_key = ?2",
            params![code, key],
        )?;
    }
    Ok(keys)
}

/// Canonical stored form of an artist's country: an uppercase ISO 3166-1
/// alpha-2 code. Anything else (blank, wrong length, non-letters) becomes
/// `None`, so the column only ever holds a code or NULL (#2095).
pub fn normalize_country_code(raw: Option<&str>) -> Option<String> {
    let code = raw?.trim();
    (code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic()))
        .then(|| code.to_ascii_uppercase())
}

/// Retrieve all saved artist profiles in SQLite (#473).
pub fn get_all_artist_profiles(conn: &Connection) -> Result<Vec<ArtistProfile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ARTIST_PROFILE_COLUMNS} FROM artist_profiles ORDER BY artist_key COLLATE NOCASE"
    ))?;
    let profiles = stmt
        .query_map([], artist_profile_from_row)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(profiles)
}

const ALBUM_PROFILE_COLUMNS: &str = "album_key, artist_key, description, website, links, details_fetched, fetched_cover_filename, fetched_disc_filename, cover_fetched, disc_fetched";

fn album_profile_from_row(row: &rusqlite::Row) -> rusqlite::Result<AlbumProfile> {
    let album_key: String = row.get(0)?;
    let links_json: String = row.get(4)?;
    let links: Vec<AlbumLink> = serde_json::from_str(&links_json).unwrap_or_else(|e| {
        log::warn!("Failed to parse album_profiles.links for '{album_key}': {e}");
        Vec::new()
    });
    Ok(AlbumProfile {
        album_key,
        artist_key: row.get(1)?,
        description: row.get(2)?,
        website: row.get(3)?,
        links,
        details_fetched: row.get(5)?,
        fetched_cover_filename: row.get(6)?,
        fetched_disc_filename: row.get(7)?,
        cover_fetched: row.get(8)?,
        disc_fetched: row.get(9)?,
    })
}

/// Retrieve customizable profile and liner notes for an album from SQLite (#950).
/// Rows luminous-mcp saved under a raw title are rekeyed first (see
/// `album_key::repair_album_keys`).
pub fn get_album_profile(conn: &Connection, album: &str) -> Result<AlbumProfile> {
    crate::album_key::rekey_album_profiles(conn)?;
    let folded = crate::album_key::fold_key(album);
    let sep = crate::album_key::COMPOSITE_KEY_SEPARATOR.to_string();
    let mut stmt = conn.prepare(&format!(
        "SELECT {ALBUM_PROFILE_COLUMNS} FROM album_profiles
         WHERE album_key = ?1 OR album_key LIKE '%' || ?2 || ?1
         ORDER BY CASE WHEN album_key = ?1 THEN 0 ELSE 1 END LIMIT 1"
    ))?;
    match stmt.query_row(params![folded, sep], album_profile_from_row) {
        Ok(profile) => Ok(profile),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(AlbumProfile {
            album_key: album.to_string(),
            ..Default::default()
        }),
        Err(e) => Err(e.into()),
    }
}

/// Upsert an album profile into SQLite (#950), stored under the
/// composite `album_key` of `profile.album_key`.
pub fn set_album_profile(conn: &Connection, profile: &AlbumProfile) -> Result<AlbumProfile> {
    let links_json = serde_json::to_string(&profile.links)?;
    let resolved_key = if profile
        .album_key
        .contains(crate::album_key::COMPOSITE_KEY_SEPARATOR)
    {
        crate::album_key::fold_key(&profile.album_key)
    } else {
        let folded = crate::album_key::fold_key(&profile.album_key);
        if let Some(art) = profile
            .artist_key
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            crate::album_key::album_key_for(&profile.album_key, None, Some(art), false)
        } else {
            let sep = crate::album_key::COMPOSITE_KEY_SEPARATOR.to_string();
            conn.query_row(
                "SELECT album_key FROM songs WHERE album_key = ?1 OR album_key LIKE '%' || ?2 || ?1 LIMIT 1",
                params![folded, sep],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or(folded)
        }
    };

    conn.execute(
        "INSERT INTO album_profiles (album_key, artist_key, description, website, links, details_fetched, fetched_cover_filename, fetched_disc_filename, cover_fetched, disc_fetched)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(album_key) DO UPDATE SET
            artist_key = excluded.artist_key,
            description = excluded.description,
            website = excluded.website,
            links = excluded.links,
            details_fetched = excluded.details_fetched,
            fetched_cover_filename = excluded.fetched_cover_filename,
            fetched_disc_filename = excluded.fetched_disc_filename,
            cover_fetched = excluded.cover_fetched,
            disc_fetched = excluded.disc_fetched",
        params![
            resolved_key,
            profile.artist_key,
            profile.description,
            profile.website,
            links_json,
            profile.details_fetched as i32,
            profile.fetched_cover_filename,
            profile.fetched_disc_filename,
            profile.cover_fetched as i32,
            profile.disc_fetched as i32
        ],
    )?;

    Ok(profile.clone())
}

/// Retrieve all saved album profiles in SQLite (#950).
pub fn get_all_album_profiles(conn: &Connection) -> Result<Vec<AlbumProfile>> {
    crate::album_key::rekey_album_profiles(conn)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {ALBUM_PROFILE_COLUMNS} FROM album_profiles ORDER BY album_key"
    ))?;
    let profiles = stmt
        .query_map([], album_profile_from_row)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{upsert_song, CollectionScanner};
    use crate::db::Database;
    use crate::models::{QueuePopulationMode, Song, SongSource};
    use std::sync::Arc;

    #[test]
    fn test_artist_profile_crud() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        // Initially unconfigured artist returns default profile
        let initial = get_artist_profile(&conn, "Shania Twain").unwrap();
        assert_eq!(initial.artist_key, "Shania Twain");
        assert_eq!(initial.website, None);
        assert!(initial.tags.is_empty());
        assert!(initial.social_links.is_empty());
        assert_eq!(initial.bio, None);
        assert_eq!(initial.musicbrainz_artist_id, None);

        // Save profile
        let profile = ArtistProfile {
            artist_key: "Shania Twain".to_string(),
            website: Some("https://www.shaniatwain.com".to_string()),
            tags: vec![
                "pop".to_string(),
                "country".to_string(),
                "canadian".to_string(),
            ],
            social_links: vec![
                ArtistSocialLink {
                    platform: "instagram".to_string(),
                    handle_or_url: "@shaniatwain".to_string(),
                },
                ArtistSocialLink {
                    platform: "youtube".to_string(),
                    handle_or_url: "https://youtube.com/@ShaniaTwain".to_string(),
                },
            ],
            bio: Some("Canadian singer-songwriter".to_string()),
            musicbrainz_artist_id: Some("042c0697-3948-4720-bf43-690240aeac43".to_string()),
            fetched_image_filename: None,
            fetched_image_source: None,
            details_fetched: false,
            image_fetched: false,
            ..Default::default()
        };

        set_artist_profile(&conn, &profile).unwrap();

        // Retrieve saved profile (case-insensitive key match)
        let loaded = get_artist_profile(&conn, "shania twain").unwrap();
        assert_eq!(loaded.artist_key, "Shania Twain");
        assert_eq!(
            loaded.website,
            Some("https://www.shaniatwain.com".to_string())
        );
        assert_eq!(loaded.tags, vec!["pop", "country", "canadian"]);
        assert_eq!(loaded.social_links.len(), 2);
        assert_eq!(loaded.social_links[0].platform, "instagram");
        assert_eq!(loaded.social_links[0].handle_or_url, "@shaniatwain");
        assert_eq!(loaded.bio, Some("Canadian singer-songwriter".to_string()));
        assert_eq!(
            loaded.musicbrainz_artist_id,
            Some("042c0697-3948-4720-bf43-690240aeac43".to_string())
        );

        // Get all profiles
        let all = get_all_artist_profiles(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].artist_key, "Shania Twain");

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// #2097: the key is one row per artist whatever the casing, stored in the
    /// library's spelling, and the tag filter sees it under either casing.
    #[test]
    fn test_artist_profile_key_is_case_insensitive_and_library_spelled() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_key_case_test_")
            .tempdir()
            .unwrap();
        let db = Arc::new(Database::new(temp_dir_guard.path().to_path_buf()).unwrap());
        let scanner = CollectionScanner::new(db.clone());
        let conn = db.pool.get().unwrap();

        upsert_song(
            &conn,
            &Song {
                source: SongSource::LocalFile,
                path: Some("path/wod.mp3".to_string()),
                title: Some("Under the Pressure".to_string()),
                artist: Some("The War on Drugs".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // An external writer saves under different casing: the stored key
        // and the returned profile use the library's spelling.
        let saved = set_artist_profile(
            &conn,
            &ArtistProfile {
                artist_key: "The War On Drugs".to_string(),
                tags: vec!["indie".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(saved.artist_key, "The War on Drugs");

        // The library's own spelling (the common case) resolves to itself.
        assert_eq!(
            canonical_artist_key(&conn, "The War on Drugs").unwrap(),
            "The War on Drugs"
        );

        // Scoped writes under yet another casing hit the same row.
        set_artist_bio(&conn, "THE WAR ON DRUGS", Some("Philadelphia band")).unwrap();
        set_artist_mbid(&conn, "the war on drugs", "mbid-1").unwrap();

        let all = get_all_artist_profiles(&conn).unwrap();
        assert_eq!(all.len(), 1, "one row per artist, got {all:?}");
        assert_eq!(all[0].artist_key, "The War on Drugs");
        assert_eq!(all[0].tags, vec!["indie"]);
        assert_eq!(all[0].bio.as_deref(), Some("Philadelphia band"));
        assert_eq!(all[0].musicbrainz_artist_id.as_deref(), Some("mbid-1"));

        // The artist-tag filter finds the song regardless of key casing.
        let songs = scanner
            .get_songs_by_artist_tag("indie", 10, QueuePopulationMode::All)
            .unwrap();
        assert_eq!(songs.len(), 1);

        // An artist the library doesn't have keeps the spelling given; a
        // later save in another casing updates that row in place.
        set_artist_bio(&conn, "Unsigned Act", Some("a")).unwrap();
        set_artist_bio(&conn, "UNSIGNED ACT", Some("b")).unwrap();
        let all = get_all_artist_profiles(&conn).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].bio.as_deref(), Some("b"));
    }

    #[test]
    fn test_scoped_artist_column_updates_preserve_tags() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_scoped_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        set_artist_profile(
            &conn,
            &ArtistProfile {
                artist_key: "Nelly Furtado".to_string(),
                tags: vec!["Award-Winning".to_string(), "Canadian".to_string()],
                website: Some("https://nellyfurtado.com".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        set_artist_bio(&conn, "Nelly Furtado", Some("Adopted bio")).unwrap();
        set_artist_mbid(
            &conn,
            "Nelly Furtado",
            "13655113-b181-4718-8a6b-e3a929e7e46a",
        )
        .unwrap();

        let loaded = get_artist_profile(&conn, "Nelly Furtado").unwrap();
        assert_eq!(loaded.tags, vec!["Award-Winning", "Canadian"]);
        assert_eq!(loaded.website.as_deref(), Some("https://nellyfurtado.com"));
        assert_eq!(loaded.bio.as_deref(), Some("Adopted bio"));
        assert_eq!(
            loaded.musicbrainz_artist_id.as_deref(),
            Some("13655113-b181-4718-8a6b-e3a929e7e46a")
        );

        // Also works when no row existed yet
        set_artist_bio(&conn, "New Artist", Some("Fresh bio")).unwrap();
        let new_loaded = get_artist_profile(&conn, "New Artist").unwrap();
        assert_eq!(new_loaded.bio.as_deref(), Some("Fresh bio"));
        assert!(new_loaded.tags.is_empty());
    }

    #[test]
    fn test_country_round_trip_normalises_to_uppercase_code() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_country_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        // The editor's value is stored as an uppercase code; junk becomes NULL.
        let mut profile = ArtistProfile {
            artist_key: "Rush".to_string(),
            country: Some(" ca ".to_string()),
            ..Default::default()
        };
        // The returned profile carries the stored form, not the raw input.
        let saved = set_artist_profile(&conn, &profile).unwrap();
        assert_eq!(saved.country.as_deref(), Some("CA"));
        assert_eq!(
            get_artist_profile(&conn, "RUSH")
                .unwrap()
                .country
                .as_deref(),
            Some("CA")
        );

        profile.country = Some("Canada".to_string());
        assert_eq!(set_artist_profile(&conn, &profile).unwrap().country, None);
        assert_eq!(get_artist_profile(&conn, "Rush").unwrap().country, None);
    }

    #[test]
    fn test_fill_blank_artist_country_by_mbid_only_fills_blank_and_touches_nothing_else() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_country_fill_test_")
            .tempdir()
            .unwrap();
        let db = Database::new(temp_dir_guard.path().to_path_buf()).unwrap();
        let conn = db.pool.get().unwrap();

        let profile = |key: &str, mbid: &str, country: Option<&str>| ArtistProfile {
            artist_key: key.to_string(),
            musicbrainz_artist_id: Some(mbid.to_string()),
            country: country.map(str::to_string),
            tags: vec!["Grammy Award".to_string()],
            bio: Some("A bio".to_string()),
            ..Default::default()
        };
        set_artist_profile(&conn, &profile("Blank", "mbid-1", None)).unwrap();
        set_artist_profile(&conn, &profile("Manual", "mbid-2", Some("GB"))).unwrap();

        let filled = fill_blank_artist_country_by_mbid(&conn, "mbid-1", "us").unwrap();
        assert_eq!(filled, vec!["Blank".to_string()]);
        let blank = get_artist_profile(&conn, "Blank").unwrap();
        assert_eq!(blank.country.as_deref(), Some("US"));
        assert_eq!(
            blank.tags,
            vec!["Grammy Award"],
            "only the country column changes"
        );
        assert_eq!(blank.bio.as_deref(), Some("A bio"));

        // A country already set is never overwritten, and reports nothing changed.
        assert!(fill_blank_artist_country_by_mbid(&conn, "mbid-2", "US")
            .unwrap()
            .is_empty());
        assert_eq!(
            get_artist_profile(&conn, "Manual")
                .unwrap()
                .country
                .as_deref(),
            Some("GB")
        );

        // No profile carries the id: nothing is created.
        assert!(fill_blank_artist_country_by_mbid(&conn, "mbid-3", "US")
            .unwrap()
            .is_empty());
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM artist_profiles", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2);

        // A value that is not an alpha-2 code is ignored.
        assert!(fill_blank_artist_country_by_mbid(&conn, "mbid-1", "Canada")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_artist_tag_casing_propagates_across_artists() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_artist_tag_casing_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        set_artist_profile(
            &conn,
            &ArtistProfile {
                artist_key: "Shania Twain".to_string(),
                website: None,
                tags: vec!["canadian".to_string()],
                social_links: vec![],
                bio: None,
                musicbrainz_artist_id: None,
                fetched_image_filename: None,
                fetched_image_source: None,
                details_fetched: false,
                image_fetched: false,
                ..Default::default()
            },
        )
        .unwrap();
        set_artist_profile(
            &conn,
            &ArtistProfile {
                artist_key: "Alanis Morissette".to_string(),
                website: None,
                tags: vec!["canadian".to_string(), "rock".to_string()],
                social_links: vec![],
                bio: None,
                musicbrainz_artist_id: None,
                fetched_image_filename: None,
                fetched_image_source: None,
                details_fetched: false,
                image_fetched: false,
                ..Default::default()
            },
        )
        .unwrap();

        // Re-saving "Shania Twain" with "Canadian" (different case) should
        // not just update her own row, but re-case every other artist's
        // matching tag too, so the library-wide tag list has one casing.
        set_artist_profile(
            &conn,
            &ArtistProfile {
                artist_key: "Shania Twain".to_string(),
                website: None,
                tags: vec!["Canadian".to_string()],
                social_links: vec![],
                bio: None,
                musicbrainz_artist_id: None,
                fetched_image_filename: None,
                fetched_image_source: None,
                details_fetched: false,
                image_fetched: false,
                ..Default::default()
            },
        )
        .unwrap();

        let alanis = get_artist_profile(&conn, "Alanis Morissette").unwrap();
        assert_eq!(alanis.tags, vec!["Canadian", "rock"]);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_album_profile_crud() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_album_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        // Initially unconfigured album returns default profile
        let initial = get_album_profile(&conn, "Come On Over").unwrap();
        assert_eq!(initial.album_key, "Come On Over");
        assert_eq!(initial.artist_key, None);
        assert_eq!(initial.description, None);
        assert_eq!(initial.website, None);
        assert!(initial.links.is_empty());

        // Save album profile
        let profile = AlbumProfile {
            album_key: "Come On Over".to_string(),
            artist_key: Some("Shania Twain".to_string()),
            description: Some(
                "Iconic 1997 studio album recorded with producer Mutt Lange. [Wikipedia](https://en.wikipedia.org/wiki/Come_On_Over)".to_string(),
            ),
            website: Some("https://shaniatwain.com/music/come-on-over".to_string()),
            links: vec![
                AlbumLink {
                    platform: "bandcamp".to_string(),
                    handle_or_url: "https://shaniatwain.bandcamp.com/album/come-on-over".to_string(),
                },
                AlbumLink {
                    platform: "discogs".to_string(),
                    handle_or_url: "https://www.discogs.com/master/132556-Shania-Twain-Come-On-Over".to_string(),
                },
            ],
            details_fetched: false,
            ..Default::default()
        };

        set_album_profile(&conn, &profile).unwrap();

        // Retrieve saved profile (case-insensitive key match)
        let loaded = get_album_profile(&conn, "come on over").unwrap();
        assert_eq!(
            loaded.album_key,
            crate::album_key::album_key_for("come on over", None, Some("Shania Twain"), false),
            "stored under its composite album_key"
        );
        assert_eq!(loaded.artist_key, Some("Shania Twain".to_string()));
        assert_eq!(
            loaded.description,
            Some("Iconic 1997 studio album recorded with producer Mutt Lange. [Wikipedia](https://en.wikipedia.org/wiki/Come_On_Over)".to_string())
        );
        assert_eq!(
            loaded.website,
            Some("https://shaniatwain.com/music/come-on-over".to_string())
        );
        assert_eq!(loaded.links.len(), 2);
        assert_eq!(loaded.links[0].platform, "bandcamp");
        assert_eq!(
            loaded.links[0].handle_or_url,
            "https://shaniatwain.bandcamp.com/album/come-on-over"
        );
        assert_eq!(loaded.links[1].platform, "discogs");

        // Get all album profiles
        let all = get_all_album_profiles(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].album_key,
            crate::album_key::album_key_for("come on over", None, Some("Shania Twain"), false)
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    /// Regression test for #990: an external writer of `album_profiles.links`
    /// (e.g. luminous-mcp's `update_album_profile` tool) uses `url` instead of
    /// `handle_or_url`, plus extra `title`/`category` fields this struct
    /// doesn't have. That link shape must still deserialize - previously the
    /// whole array silently dropped to empty via `.unwrap_or_default()`.
    #[test]
    fn test_album_profile_links_from_external_writer_shape() {
        let temp_dir_guard = tempfile::Builder::new()
            .prefix("luminous_album_ext_links_test_")
            .tempdir()
            .unwrap();
        let temp_dir = temp_dir_guard.path().to_path_buf();
        let db = Database::new(temp_dir.clone()).unwrap();
        let conn = db.pool.get().unwrap();

        conn.execute(
            "INSERT INTO album_profiles (album_key, artist_key, description, website, links) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "Of Kingdom and Crown",
                Some("Machine Head"),
                None::<String>,
                None::<String>,
                r#"[
                    {"platform":"Spotify","title":"Stream on Spotify","url":"https://open.spotify.com/album/6duwuU8xgK7ShKMCrUxfBi","category":"store"},
                    {"platform":"Live-Metal.com","title":"Live-Metal.com Review","url":"https://live-metal.com/review","category":"review"}
                ]"#,
            ],
        )
        .unwrap();

        let loaded = get_album_profile(&conn, "Of Kingdom and Crown").unwrap();
        assert_eq!(loaded.links.len(), 2);
        assert_eq!(loaded.links[0].platform, "Spotify");
        assert_eq!(
            loaded.links[0].handle_or_url,
            "https://open.spotify.com/album/6duwuU8xgK7ShKMCrUxfBi"
        );
        assert_eq!(loaded.links[1].platform, "Live-Metal.com");
        assert_eq!(
            loaded.links[1].handle_or_url,
            "https://live-metal.com/review"
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
