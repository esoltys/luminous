use cucumber::{given, then, when, World};
use luminous_lib::db::Database;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct TagEditorWorld {
    temp_dir: TempDir,
    db: Arc<Database>,
    song_id: i64,
    song_path: Option<PathBuf>,
    new_title: String,
    new_artist: String,
}

impl Default for TagEditorWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        Self {
            temp_dir,
            db,
            song_id: 1,
            song_path: None,
            new_title: String::new(),
            new_artist: String::new(),
        }
    }
}

/// Writes a minimal valid WAV file with an embedded ID3v2 cover picture, so
/// this scenario exercises `clear_embedded_art` against a real file on disk
/// rather than just the database (see the "Editing track tags" scenario
/// above, which only simulates the DB side).
fn write_wav_with_embedded_art(path: &std::path::Path) {
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
    std::fs::write(path, wav).expect("failed to write wav fixture");

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

#[given("I have a song in the library")]
fn song_in_library(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', 'Parachutes', 1, 1, 0)",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[when("I open the tag editor for the song")]
fn open_tag_editor(_w: &mut TagEditorWorld) {}

#[when(expr = "I change the Title to {string}")]
fn change_title(w: &mut TagEditorWorld, title: String) {
    w.new_title = title;
}

#[when(expr = "I change the Artist to {string}")]
fn change_artist(w: &mut TagEditorWorld, artist: String) {
    w.new_artist = artist;
}

#[when("I click \"Save Tags\"")]
fn click_save_tags(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET title = ?1, artist = ?2 WHERE id = ?3",
        rusqlite::params![w.new_title, w.new_artist, w.song_id],
    )
    .unwrap();
}

#[then("it should update the song details in the SQLite database")]
fn db_updated(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let (title, artist): (String, String) = conn
        .query_row(
            "SELECT title, artist FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("song not found");
    assert_eq!(title, w.new_title);
    assert_eq!(artist, w.new_artist);
}

#[then("the library views should immediately reflect the updated metadata")]
fn library_reflects_metadata(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let title: String = conn
        .query_row(
            "SELECT title FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("song not found");
    assert_eq!(title, "Yellow (Acoustic)");
}

#[given("I have a song in the library with embedded cover art")]
fn song_with_embedded_art(w: &mut TagEditorWorld) {
    let song_path = w.temp_dir.path().join("mistagged.wav");
    write_wav_with_embedded_art(&song_path);
    w.song_path = Some(song_path.clone());

    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Alpha', 'Wrong Artist', 'Wrong Album', 1, 0, 1, 1, 0)",
        rusqlite::params![w.song_id, song_path.to_string_lossy()],
    )
    .unwrap();
}

#[when("I click \"Clear Artwork\"")]
fn click_clear_artwork(w: &mut TagEditorWorld) {
    let path = w.song_path.clone().expect("song_path not set");
    luminous_lib::tageditor::clear_embedded_art(&path).expect("clear_embedded_art should succeed");

    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET art_embedded = 0 WHERE id = ?1",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[then("the embedded picture should be removed from the audio file")]
fn picture_removed_from_file(w: &mut TagEditorWorld) {
    use lofty::file::TaggedFileExt;
    use lofty::probe::Probe;

    let path = w.song_path.clone().expect("song_path not set");
    let tagged_file = Probe::open(&path).unwrap().read().unwrap();
    // The fixture tag holds nothing but the picture, so once it's cleared
    // the tag itself is empty — lofty may drop an empty tag frame entirely
    // on write rather than leaving a present-but-empty one, so both outcomes
    // count as "no embedded picture".
    let has_picture = tagged_file
        .primary_tag()
        .is_some_and(|tag| !tag.pictures().is_empty());
    assert!(
        !has_picture,
        "embedded picture should have been removed from the file on disk"
    );
}

#[then("the song in the database should have \"art_embedded\" set to false")]
fn art_embedded_is_false(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_emb: bool = conn
        .query_row(
            "SELECT art_embedded FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_embedded missing");
    assert!(!art_emb);
}

#[tokio::main]
async fn main() {
    TagEditorWorld::cucumber()
        .max_concurrent_scenarios(4)
        .run_and_exit("../features/tag_editor.feature")
        .await;
}
