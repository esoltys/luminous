//! Synthetic large-library scan benchmark (#2353).
//!
//! Run manually in release mode to measure `discovery_ms` and `partition_ms` on
//! an unchanged 20k+ file library:
//!
//! ```bash
//! cargo test --release --test scan_perf_test -- --ignored --nocapture
//! ```
//!
//! Optional environment variables:
//! - `LUMINOUS_PERF_FILE_COUNT`: number of synthetic `.mp3` files (default: 20000)
//! - `LUMINOUS_PERF_LIBRARY_DIR`: existing directory (e.g. a NAS/network share)
//!   to scan in place instead of generating synthetic files in a temp directory.

use luminous_lib::{
    collection::{CollectionScanner, ScanReport},
    db::Database,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

const FIXTURE_MP3: &[u8] = include_bytes!("fixtures/audio/song_short.mp3");
const DEFAULT_FILE_COUNT: usize = 20_000;
const UNCHANGED_RUNS: usize = 3;

fn populate_synthetic_library(root: &Path, file_count: usize) {
    let tracks_per_album = 10;
    let albums_per_artist = 10;
    let tracks_per_artist = tracks_per_album * albums_per_artist;

    for i in 0..file_count {
        let artist_idx = i / tracks_per_artist;
        let album_idx = (i % tracks_per_artist) / tracks_per_album;
        let track_idx = i % tracks_per_album;

        let album_dir = root
            .join(format!("artist_{artist_idx:04}"))
            .join(format!("album_{album_idx:02}"));
        if track_idx == 0 {
            std::fs::create_dir_all(&album_dir).expect("failed to create album dir");
            // Add one non-audio file per album to mirror real libraries.
            std::fs::write(album_dir.join("info.txt"), b"synthetic album")
                .expect("failed to write non-audio file");
        }
        std::fs::write(
            album_dir.join(format!("track_{track_idx:02}.mp3")),
            FIXTURE_MP3,
        )
        .expect("failed to write synthetic track");
    }
}

fn print_report(label: &str, report: &ScanReport) {
    println!(
        "[{label}] files_found={} unchanged={} reread={} | discovery_ms={} partition_ms={} (reconcile={} dedup={} stamps={} stat={}) tag_read_ms={} missing_check_ms={} total_ms={}",
        report.files_found,
        report.unchanged,
        report.reread,
        report.discovery_ms,
        report.partition_ms,
        report.partition_reconcile_ms,
        report.partition_dedup_ms,
        report.partition_stamps_ms,
        report.partition_stat_ms,
        report.tag_read_ms,
        report.missing_check_ms,
        report.total_ms,
    );
}

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

#[tokio::test]
#[ignore = "large-library performance benchmark; run explicitly with --ignored --nocapture"]
async fn bench_unchanged_large_library_scan() {
    let profile_dir = tempfile::tempdir().expect("failed to create profile tempdir");
    let db = Arc::new(Database::new(profile_dir.path().to_path_buf()).expect("failed to init db"));
    let scanner = CollectionScanner::new(Arc::clone(&db));
    let app_data_dir = profile_dir.path().join("app_data");

    let mut _synthetic_dir: Option<TempDir> = None;
    let scan_dir: PathBuf = match std::env::var_os("LUMINOUS_PERF_LIBRARY_DIR") {
        Some(dir) => {
            let p = PathBuf::from(dir);
            assert!(p.is_dir(), "LUMINOUS_PERF_LIBRARY_DIR is not a directory");
            p
        }
        None => {
            let file_count = std::env::var("LUMINOUS_PERF_FILE_COUNT")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(DEFAULT_FILE_COUNT);
            let dir = tempfile::tempdir().expect("failed to create synthetic library tempdir");
            let t0 = std::time::Instant::now();
            populate_synthetic_library(dir.path(), file_count);
            println!(
                "Populated {file_count} synthetic tracks in {} ms at {}",
                t0.elapsed().as_millis(),
                dir.path().display()
            );
            let path = dir.path().to_path_buf();
            _synthetic_dir = Some(dir);
            path
        }
    };

    scanner
        .add_directory(&scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    let initial = scanner
        .scan_all_core(app_data_dir.clone(), false, false, false, |_| {})
        .await
        .expect("initial scan failed");
    print_report("initial", &initial);
    assert!(initial.files_found > 0, "expected discovered audio files");

    let mut discovery_samples = Vec::with_capacity(UNCHANGED_RUNS);
    let mut partition_samples = Vec::with_capacity(UNCHANGED_RUNS);
    let mut total_samples = Vec::with_capacity(UNCHANGED_RUNS);

    for run in 1..=UNCHANGED_RUNS {
        let report = scanner
            .scan_all_core(app_data_dir.clone(), false, false, false, |_| {})
            .await
            .expect("unchanged scan failed");
        print_report(&format!("unchanged-{run}"), &report);
        assert_eq!(report.reread, 0, "expected 0 re-reads on unchanged library");
        assert_eq!(
            report.unchanged, report.files_found,
            "expected every file to be skipped as unchanged"
        );
        discovery_samples.push(report.discovery_ms);
        partition_samples.push(report.partition_ms);
        total_samples.push(report.total_ms);
    }

    println!(
        "[median] discovery_ms={} partition_ms={} total_ms={}",
        median(&mut discovery_samples),
        median(&mut partition_samples),
        median(&mut total_samples),
    );
}
