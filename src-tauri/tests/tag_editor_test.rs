use luminous_lib::db::Database;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Real synthetic audio files (see `scripts/generate_test_fixtures.sh`).
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

/// Writes a minimal valid WAV file with an embedded ID3v2 cover picture, so
/// this exercises `clear_embedded_art` against a real file on disk.
fn write_wav_with_embedded_art(path: &Path) {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::probe::Probe;
    use lofty::tag::Tag;

    let sample_rate = 8_000u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let data = vec![0u8; 800]; // 100ms of silence
    let byte_rate = sample_rate * channels as u32 * (bits_per_sample / 8) as u32;
    let block_align = channels * (bits_per_sample / 8);

    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);

    std::fs::write(path, &wav).expect("failed to write fixture wav");

    let mut tagged_file = Probe::open(path).unwrap().read().unwrap();
    let mut tag = Tag::new(tagged_file.primary_tag_type());
    let picture = Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::Jpeg)
        .build();
    tag.push_picture(picture);
    tagged_file.insert_tag(tag);
    tagged_file
        .save_to_path(path, lofty::config::WriteOptions::default())
        .expect("failed to write fixture tag");
}

#[test]
fn test_edit_track_tags_and_save_back_to_audio_file() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 1i64;

    let song_path = temp_dir.path().join("song_alpha.mp3");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_alpha.mp3"),
        &song_path,
    )
    .expect("failed to copy fixture");

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, source, filetype, unavailable)
         VALUES (?1, ?2, 'Yellow', 'Coldplay', 'Parachutes', 1, 1, 0)",
        rusqlite::params![song_id, song_path.to_string_lossy()],
    )
    .unwrap();

    let new_title = "Yellow (Acoustic)";
    let new_artist = "Coldplay";

    // Write tags back to the audio file on disk via lofty
    luminous_metadata::audio::write_tags(
        &song_path,
        &luminous_metadata::audio::TagWriteRequest {
            title: new_title,
            artist: new_artist,
            album: "Parachutes",
            ..Default::default()
        },
    )
    .expect("write_tags should succeed against a real audio file");

    conn.execute(
        "UPDATE songs SET title = ?1, artist = ?2 WHERE id = ?3",
        rusqlite::params![new_title, new_artist, song_id],
    )
    .unwrap();

    // Verify DB update
    let (db_title, db_artist): (String, String) = conn
        .query_row(
            "SELECT title, artist FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("song not found");
    assert_eq!(db_title, new_title);
    assert_eq!(db_artist, new_artist);

    // Verify lofty read-back from the file on disk
    use lofty::file::TaggedFileExt;
    use lofty::probe::Probe;
    use lofty::tag::Accessor;
    let tagged_file = Probe::open(&song_path).unwrap().read().unwrap();
    let tag = tagged_file.primary_tag().expect("no tag found on file");
    assert_eq!(tag.title().as_deref(), Some(new_title));
    assert_eq!(tag.artist().as_deref(), Some(new_artist));
}

#[test]
fn test_clear_embedded_artwork() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 1i64;

    let song_path = temp_dir.path().join("mistagged.wav");
    write_wav_with_embedded_art(&song_path);

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Alpha', 'Wrong Artist', 'Wrong Album', 1, 0, 1, 1, 0)",
        rusqlite::params![song_id, song_path.to_string_lossy()],
    )
    .unwrap();

    luminous_metadata::audio::clear_embedded_art(&song_path)
        .expect("clear_embedded_art should succeed");

    conn.execute(
        "UPDATE songs SET art_embedded = 0 WHERE id = ?1",
        rusqlite::params![song_id],
    )
    .unwrap();

    // Verify picture removed from file
    use lofty::file::TaggedFileExt;
    use lofty::probe::Probe;
    let tagged_file = Probe::open(&song_path).unwrap().read().unwrap();
    let has_picture = tagged_file
        .primary_tag()
        .is_some_and(|tag| !tag.pictures().is_empty());
    assert!(
        !has_picture,
        "embedded picture should have been removed from the file on disk"
    );

    // Verify DB flag
    let art_emb: bool = conn
        .query_row(
            "SELECT art_embedded FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .expect("art_embedded missing");
    assert!(!art_emb);
}
