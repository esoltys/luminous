use super::*;
use crate::devices::sync::manifest::{EntryKind, SourceStamp};
use luminous_devices::FsBackend;
use parking_lot::Mutex;
use std::path::{Path, PathBuf};

/// A clock that moves 100 ms each time it is read.
struct TickClock {
    start: Instant,
    ticks: Mutex<u32>,
}

impl TickClock {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            ticks: Mutex::new(0),
        }
    }
}

impl Clock for TickClock {
    fn now(&self) -> Instant {
        let mut ticks = self.ticks.lock();
        *ticks += 1;
        self.start + Duration::from_millis(100) * *ticks
    }
    fn unix_now(&self) -> i64 {
        1_790_000_000
    }
}

struct Setup {
    library: tempfile::TempDir,
    device: tempfile::TempDir,
    cache: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        Self {
            library: tempfile::tempdir().unwrap(),
            device: tempfile::tempdir().unwrap(),
            cache: tempfile::tempdir().unwrap(),
        }
    }

    fn wav(&self, name: &str) -> PathBuf {
        let path = self.library.path().join(name);
        luminous_audio::write_test_pcm16_wav(&path, 44_100, &[vec![0; 4_410], vec![0; 4_410]]);
        path
    }

    fn device_file(&self, path: &str) -> PathBuf {
        self.device.path().join(path)
    }

    fn job(&self, steps: Steps, manifest: Manifest) -> Job {
        Job {
            device_id: "vol-0088-5572".into(),
            storage: Box::new(FsBackend::new(self.device.path())),
            profile: profile(),
            manifest,
            steps,
            planned_free_after_bytes: 0,
        }
    }

    fn run(&self, job: Job, cancel_after_songs: Option<usize>) -> (SyncSummary, Vec<SyncProgress>) {
        let transcoder = Transcoder::new(self.cache.path().to_path_buf());
        let cancel = AtomicBool::new(false);
        let mut reports = Vec::new();
        let summary = run(job, &transcoder, &cancel, &TickClock::new(), &mut |p| {
            if cancel_after_songs.is_some_and(|n| p.songs_done >= n) {
                cancel.store(true, Ordering::Relaxed);
            }
            reports.push(p.clone());
        });
        (summary, reports)
    }

    fn manifest(&self) -> Manifest {
        Manifest::read(&FsBackend::new(self.device.path())).unwrap()
    }
}

fn profile() -> DeviceProfile {
    DeviceProfile {
        music_path: "Music".into(),
        supported_formats: vec!["wav".into()],
        max_sample_rate: 192_000,
        max_bit_depth: 24,
        ..DeviceProfile::default()
    }
}

fn track_write(source: &Path, device_path: &str) -> Write {
    let size = std::fs::metadata(source).unwrap().len();
    Write {
        source: WriteSource::Track {
            source: source.to_path_buf(),
            converts: false,
        },
        entry: ManifestEntry {
            path: device_path.into(),
            kind: EntryKind::Track,
            size,
            source: Some(SourceStamp {
                path: source.to_string_lossy().into_owned(),
                size,
                mtime: None,
            }),
            conversion: None,
            fingerprint: None,
            sources: Vec::new(),
            label: SongLabel {
                title: Some(device_path.into()),
                ..SongLabel::default()
            },
        },
        estimate: size,
    }
}

fn steps(write: Vec<Write>, remove: Vec<String>) -> Steps {
    Steps {
        record: Vec::new(),
        remove,
        write,
        roots: vec!["Music".into(), String::new()],
    }
}

#[test]
fn copies_files_and_records_them_in_the_manifest() {
    let setup = Setup::new();
    let a = setup.wav("a.wav");
    let b = setup.wav("b.wav");
    let playlist = Write {
        source: WriteSource::Bytes(b"#EXTM3U\n".to_vec()),
        entry: ManifestEntry {
            path: "Music/Mix.m3u8".into(),
            kind: EntryKind::Playlist,
            size: 8,
            source: None,
            conversion: None,
            fingerprint: Some("f".into()),
            sources: Vec::new(),
            label: SongLabel::default(),
        },
        estimate: 8,
    };
    let job = setup.job(
        steps(
            vec![
                track_write(&a, "Music/A/a.wav"),
                track_write(&b, "Music/B/b.wav"),
                playlist,
            ],
            vec![],
        ),
        Manifest::default(),
    );
    let (summary, reports) = setup.run(job, None);

    assert_eq!(summary.outcome, SyncOutcome::Completed);
    assert_eq!(summary.songs_written, 2);
    assert_eq!(summary.playlist_files, 1);
    assert!(summary.failures.is_empty());
    assert_eq!(summary.written_bytes, summary.planned_bytes);
    assert_eq!(summary.finished_at, 1_790_000_000);
    assert_eq!(
        std::fs::read(setup.device_file("Music/A/a.wav")).unwrap(),
        std::fs::read(&a).unwrap()
    );
    let manifest = setup.manifest();
    assert_eq!(manifest.entries().count(), 3);
    assert!(manifest.get("music/b/b.wav").is_some());

    let last = reports.last().unwrap();
    assert_eq!(last.step, SyncStep::Finishing);
    assert_eq!((last.songs_done, last.songs_total), (2, 2));
    assert_eq!(last.bytes_done, last.bytes_total);
    assert!(reports
        .iter()
        .any(|r| matches!(r.step, SyncStep::Copying { .. })));
    assert!(last.bytes_per_sec.is_some_and(|r| r > 0.0));
}

#[test]
fn removals_prune_only_folders_they_empty() {
    let setup = Setup::new();
    for path in [
        "Music/Gone/x.wav",
        "Music/Shared/y.wav",
        "Music/Shared/mine.txt",
    ] {
        let file = setup.device_file(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, b"1").unwrap();
    }
    let entry = |path: &str| ManifestEntry {
        path: path.into(),
        kind: EntryKind::Track,
        size: 1,
        source: None,
        conversion: None,
        fingerprint: None,
        sources: Vec::new(),
        label: SongLabel::default(),
    };
    let manifest: Manifest = [entry("Music/Gone/x.wav"), entry("Music/Shared/y.wav")]
        .into_iter()
        .collect();
    let job = setup.job(
        steps(
            vec![],
            vec!["Music/Gone/x.wav".into(), "Music/Shared/y.wav".into()],
        ),
        manifest,
    );
    let (summary, _) = setup.run(job, None);

    assert_eq!(summary.removed, 2);
    assert!(!setup.device_file("Music/Gone").exists());
    assert!(setup.device_file("Music/Shared/mine.txt").exists());
    assert!(
        setup.device_file("Music").exists(),
        "the music folder stays"
    );
    assert_eq!(setup.manifest().entries().count(), 0);
}

#[test]
fn stopping_keeps_completed_files_and_their_manifest_entries() {
    let setup = Setup::new();
    let writes = (0..4)
        .map(|i| {
            let source = setup.wav(&format!("{i}.wav"));
            track_write(&source, &format!("Music/{i}.wav"))
        })
        .collect();
    let job = setup.job(steps(writes, vec![]), Manifest::default());
    let (summary, _) = setup.run(job, Some(1));

    assert_eq!(summary.outcome, SyncOutcome::Stopped);
    assert!(summary.songs_written >= 1 && summary.songs_written < 4);
    let manifest = setup.manifest();
    assert_eq!(manifest.entries().count(), summary.songs_written);
    for entry in manifest.entries() {
        assert!(setup.device_file(&entry.path).exists());
    }
    let leftovers: Vec<_> = std::fs::read_dir(setup.device_file("Music"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("partial"))
        .collect();
    assert!(leftovers.is_empty(), "no partial files");
}

#[test]
fn a_file_that_fails_is_reported_and_the_sync_goes_on() {
    let setup = Setup::new();
    let good = setup.wav("good.wav");
    let missing = setup.library.path().join("missing.wav");
    let mut lost = track_write(&good, "Music/missing.wav");
    lost.source = WriteSource::Track {
        source: missing,
        converts: false,
    };
    let job = setup.job(
        steps(vec![lost, track_write(&good, "Music/good.wav")], vec![]),
        Manifest::default(),
    );
    let (summary, _) = setup.run(job, None);

    assert_eq!(summary.outcome, SyncOutcome::Completed);
    assert_eq!(summary.songs_written, 1);
    assert_eq!(summary.failures.len(), 1);
    assert_eq!(summary.failures[0].path, "Music/missing.wav");
    assert!(summary.written_bytes < summary.planned_bytes);
}

#[test]
fn rate_averages_over_its_window() {
    let start = Instant::now();
    let mut rate = Rate::default();
    assert_eq!(rate.bytes_per_sec(), None);
    rate.sample(start, 0);
    rate.sample(start + Duration::from_secs(2), 4_000_000);
    assert_eq!(rate.bytes_per_sec(), Some(2_000_000.0));
    rate.sample(start + Duration::from_secs(10), 4_000_000);
    rate.sample(start + Duration::from_secs(11), 5_000_000);
    assert_eq!(rate.bytes_per_sec(), Some(1_000_000.0));
}
