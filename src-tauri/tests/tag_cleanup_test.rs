use luminous_lib::collection::SelfWriteTracker;
use luminous_lib::db::Database;
use luminous_lib::tag_cleanup::{self, TagCleanupSelection};
use luminous_metadata::CruftKind;
use std::path::{Path, PathBuf};

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

/// Copies the MP3 fixture into `dir` with an ID3v2 tag holding a real note,
/// a promotional comment's worth of cruft, and an encoder banner.
fn mp3_with_cruft(dir: &Path, name: &str) -> PathBuf {
    use lofty::config::WriteOptions;
    use lofty::id3::v2::{CommentFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame};
    use lofty::tag::TagExt;
    use lofty::TextEncoding;
    use std::borrow::Cow;

    let path = dir.join(name);
    std::fs::copy(PathBuf::from(FIXTURES_DIR).join("song_alpha.mp3"), &path).unwrap();
    let mut tag = Id3v2Tag::new();
    tag.insert(Frame::Comment(CommentFrame::new(
        TextEncoding::UTF8,
        *b"eng",
        "iTunNORM".to_string(),
        " 00000A0B 00000C0D".to_string(),
    )));
    tag.insert(Frame::Comment(CommentFrame::new(
        TextEncoding::UTF8,
        *b"eng",
        String::new(),
        "Downloaded from www.freemp3s.ru".to_string(),
    )));
    tag.insert(Frame::Text(TextInformationFrame::new(
        FrameId::Valid(Cow::Borrowed("TSSE")),
        TextEncoding::UTF8,
        "LAME 3.99r".to_string(),
    )));
    tag.save_to_path(&path, WriteOptions::default()).unwrap();
    path
}

fn insert_song(db: &Database, id: i64, path: &Path, cue_path: Option<&str>) {
    let conn = db.pool.get().unwrap();
    conn.execute(
        "INSERT INTO songs (id, path, title, artist, album, source, filetype, unavailable, cue_path, comment)
         VALUES (?1, ?2, 'Title', 'Artist', 'Album', 1, 1, 0, ?3, ' 00000A0B 00000C0D')",
        rusqlite::params![id, path.to_string_lossy(), cue_path],
    )
    .unwrap();
}

#[test]
fn preview_lists_cruft_skips_cue_tracks_and_apply_refreshes_comment() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
    let plain = mp3_with_cruft(temp_dir.path(), "plain.mp3");
    let cue = mp3_with_cruft(temp_dir.path(), "cue.mp3");
    insert_song(&db, 1, &plain, None);
    insert_song(&db, 2, &cue, Some("/music/album.cue"));

    let progress = std::cell::RefCell::new(Vec::new());
    let preview = tag_cleanup::preview(&db, &[1, 2], |checked, total| {
        progress.borrow_mut().push((checked, total));
        true
    })
    .unwrap();
    assert_eq!(progress.into_inner().last(), Some(&(2, 2)));
    assert_eq!(preview.skipped, 1, "the CUE track can't be cleaned");
    assert_eq!(preview.songs.len(), 1);
    assert_eq!(preview.songs[0].song_id, 1);
    let mut kinds: Vec<_> = preview.songs[0].fields.iter().map(|f| f.kind).collect();
    kinds.sort_by_key(|k| format!("{k:?}"));
    assert_eq!(
        kinds,
        vec![
            CruftKind::Encoder,
            CruftKind::ItunesNormalization,
            CruftKind::PromoComment
        ]
    );

    // Untick the encoder banner; select the CUE track too, which must be ignored.
    let kept_ticked: Vec<_> = preview.songs[0]
        .fields
        .iter()
        .filter(|f| f.kind != CruftKind::Encoder)
        .cloned()
        .collect();
    let selections = vec![
        TagCleanupSelection {
            song_id: 1,
            fields: kept_ticked.clone(),
        },
        TagCleanupSelection {
            song_id: 2,
            fields: kept_ticked,
        },
    ];
    let mut progress = Vec::new();
    let result = tag_cleanup::apply(&db, &SelfWriteTracker::new(), &selections, |done, total| {
        progress.push((done, total))
    })
    .unwrap();
    assert_eq!(progress, vec![(0, 2), (1, 2), (2, 2)]);
    assert_eq!(result.cleaned_songs, 1);
    assert_eq!(result.removed_fields, 2);
    assert!(result.errors.is_empty());

    let after = tag_cleanup::preview(&db, &[1], |_, _| true).unwrap();
    assert_eq!(after.songs[0].fields.len(), 1);
    assert_eq!(after.songs[0].fields[0].kind, CruftKind::Encoder);
    assert_eq!(
        luminous_metadata::find_cruft(&cue).unwrap().len(),
        3,
        "the CUE track's file is untouched"
    );

    let comment: Option<String> = db
        .pool
        .get()
        .unwrap()
        .query_row("SELECT comment FROM songs WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(comment, None, "the stale iTunNORM comment is cleared");
}

#[test]
fn preview_stops_when_progress_says_so() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
    let plain = mp3_with_cruft(temp_dir.path(), "plain.mp3");
    insert_song(&db, 1, &plain, None);
    assert!(tag_cleanup::preview(&db, &[1], |_, _| false).is_err());
}

#[test]
fn empty_selection_is_a_no_op() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Database::new(temp_dir.path().to_path_buf()).unwrap();
    let result = tag_cleanup::apply(&db, &SelfWriteTracker::new(), &[], |_, _| {}).unwrap();
    assert_eq!(result.cleaned_songs, 0);
}
