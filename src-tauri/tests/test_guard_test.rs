use luminous_lib::db::Database;
use luminous_lib::test_guard::{
    assert_safe_library_write_path, assert_safe_profile_path, check_library_write_path,
    check_profile_path, get_real_music_dirs, get_real_profile_dirs,
    is_prohibited_library_write_path, is_prohibited_profile_path, is_test_mode,
};
use std::path::PathBuf;

#[test]
fn test_is_test_mode_active_under_test_harness() {
    assert!(
        is_test_mode(),
        "is_test_mode() must be true under cargo test"
    );
}

#[test]
fn test_database_initialization_succeeds_in_tempdir() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Database::new(temp_dir.path().to_path_buf());
    assert!(db.is_ok(), "Database::new must succeed in temp directory");
}

#[test]
#[should_panic(expected = "FATAL SAFETY VIOLATION")]
fn test_database_initialization_panics_on_real_profile() {
    let real_dirs = get_real_profile_dirs();
    if let Some(real_dir) = real_dirs.first() {
        let _ = Database::new(real_dir.clone());
    } else {
        panic!("FATAL SAFETY VIOLATION: no real profile dir detected");
    }
}

#[test]
fn test_check_profile_path_detects_real_profiles() {
    let real_dirs = get_real_profile_dirs();
    for real_dir in &real_dirs {
        assert!(
            is_prohibited_profile_path(real_dir),
            "Expected {real_dir:?} to be recognized as a prohibited profile path"
        );
        let nested = real_dir.join("luminous.db");
        assert!(
            is_prohibited_profile_path(&nested),
            "Expected {nested:?} to be recognized as a prohibited profile path"
        );
    }
}

#[test]
fn test_check_profile_path_permits_temp_dirs() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    assert!(
        !is_prohibited_profile_path(temp_dir.path()),
        "Expected temp dir {temp_dir:?} to be allowed"
    );
    assert!(check_profile_path(temp_dir.path()).is_ok());
    assert_safe_profile_path(temp_dir.path());
}

#[test]
fn test_check_library_write_path_refuses_real_music_dirs() {
    let music_dirs = get_real_music_dirs();
    for music_dir in &music_dirs {
        let track_path = music_dir.join("Album").join("track.mp3");
        assert!(
            is_prohibited_library_write_path(&track_path),
            "Expected {track_path:?} to be prohibited for test-mode writes"
        );
        assert!(check_library_write_path(&track_path).is_err());
    }
}

#[test]
fn test_check_library_write_path_refuses_repo_fixtures() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let fixture_path = PathBuf::from(manifest).join("tests/fixtures/audio/song_alpha.mp3");
    assert!(
        is_prohibited_library_write_path(&fixture_path),
        "Expected repo fixture {fixture_path:?} to be protected from direct writes"
    );
    assert!(check_library_write_path(&fixture_path).is_err());
}

#[test]
fn test_check_library_write_path_permits_tempdir_files() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let copied_track = temp_dir.path().join("song_alpha.mp3");
    assert!(
        !is_prohibited_library_write_path(&copied_track),
        "Expected track in tempdir {copied_track:?} to be permitted"
    );
    assert!(check_library_write_path(&copied_track).is_ok());
    assert_safe_library_write_path(&copied_track);
}
