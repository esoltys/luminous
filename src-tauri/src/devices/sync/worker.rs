//! Running a planned sync against a device storage: removals, then writes,
//! with progress and a summary of what actually happened.
//!
//! One thread writes to the device (parallel writes slow USB and MTP down);
//! conversions are prepared up to [`LOOKAHEAD`] songs ahead of it on another.
//! Every write goes through `DeviceBackend`, so a stopped or failed write
//! never leaves part of a file, and each completed file is recorded in the
//! manifest, saved as the sync goes, so it stays sync's own.

use super::manifest::{Manifest, ManifestEntry, SongLabel};
use super::plan::{content_fingerprint, Steps, Write, WriteSource};
use crate::devices::layout::{folder_art_jpeg, DevicePlaylist};
use crate::models::Song;
use luminous_devices::{DeviceBackend, DeviceProfile};
use luminous_transcode::{Decision, Prepared, TargetFormat, Transcoder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use std::io;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Songs converted ahead of the one being written. Small: a cache entry is
/// only sure to exist until more are prepared (`luminous_transcode::Prepared`).
const LOOKAHEAD: usize = 2;
/// Longest the manifest goes unsaved while files complete.
const MANIFEST_SAVE_INTERVAL: Duration = Duration::from_secs(5);
/// Shortest gap between progress reports within one step.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
/// The span MB/s is averaged over.
const RATE_WINDOW: Duration = Duration::from_secs(5);

/// Where time comes from, so tests can drive it.
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    /// Unix seconds.
    fn unix_now(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn unix_now(&self) -> i64 {
        chrono::Utc::now().timestamp()
    }
}

/// A clock for tests that moves 100 ms each time it is read, from an
/// arbitrary start, and always says it is `unix` seconds.
#[cfg(test)]
pub(crate) struct TickClock {
    start: Instant,
    ticks: parking_lot::Mutex<u32>,
    unix: i64,
}

#[cfg(test)]
impl TickClock {
    pub(crate) fn new(unix: i64) -> Self {
        Self {
            start: Instant::now(),
            ticks: parking_lot::Mutex::new(0),
            unix,
        }
    }
}

#[cfg(test)]
impl Clock for TickClock {
    fn now(&self) -> Instant {
        let mut ticks = self.ticks.lock();
        *ticks += 1;
        self.start + Duration::from_millis(100) * *ticks
    }
    fn unix_now(&self) -> i64 {
        self.unix
    }
}

/// What the sync is doing right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncStep {
    Removing,
    Converting {
        song: SongLabel,
        format: TargetFormat,
        bitrate_kbps: u32,
    },
    Copying {
        song: SongLabel,
    },
    WritingPlaylists,
    WritingFolderArt,
    Finishing,
}

/// `device-sync-progress`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub device_id: String,
    pub step: SyncStep,
    /// Songs copied or updated so far, of `songs_total`.
    pub songs_done: usize,
    pub songs_total: usize,
    /// Bytes written so far, of the plan's estimate.
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// Averaged over the last few seconds; `None` until there is a rate.
    pub bytes_per_sec: Option<f64>,
    pub eta_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncOutcome {
    Completed,
    /// The user stopped it. Every file completed before is kept.
    Stopped,
    /// The device went away or its manifest couldn't be written.
    Failed {
        message: String,
    },
}

/// A file that couldn't be written; the sync went on without it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncFailure {
    pub path: String,
    pub song: SongLabel,
    pub message: String,
}

/// What a sync did, next to what its plan said it would (`device-sync-finished`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub device_id: String,
    pub outcome: SyncOutcome,
    /// Unix seconds.
    pub finished_at: i64,
    pub songs_written: usize,
    pub songs_planned: usize,
    pub converted: usize,
    pub removed: usize,
    pub playlist_files: usize,
    pub folder_art: usize,
    pub failures: Vec<SyncFailure>,
    pub planned_bytes: u64,
    pub written_bytes: u64,
    /// The plan's free space afterwards and the storage's real one, `None`
    /// when it couldn't be read.
    pub planned_free_after_bytes: i64,
    pub free_after_bytes: Option<u64>,
}

/// A planned sync, ready to run.
pub struct Job {
    pub device_id: String,
    pub storage: Box<dyn DeviceBackend>,
    pub profile: DeviceProfile,
    /// The manifest the plan was made from.
    pub manifest: Manifest,
    pub steps: Steps,
    pub planned_free_after_bytes: i64,
}

/// Runs `job` to the end, or until `cancel` is raised or the device goes
/// away, reporting progress as it goes. Blocks; run it on its own thread.
pub fn run(
    job: Job,
    transcoder: &Transcoder,
    cancel: &AtomicBool,
    clock: &dyn Clock,
    report: &mut dyn FnMut(&SyncProgress),
) -> SyncSummary {
    let Job {
        device_id,
        storage,
        profile,
        manifest,
        steps,
        planned_free_after_bytes,
    } = job;
    let target = transcoder.conversion_target(&profile);
    let mut run = Run {
        storage: &*storage,
        manifest,
        clock,
        report,
        cancel,
        last_save: clock.now(),
        dirty: false,
        rate: Rate::default(),
        last_report: None,
        progress: SyncProgress {
            device_id: device_id.clone(),
            step: SyncStep::Removing,
            songs_done: 0,
            songs_total: steps.song_writes(),
            bytes_done: 0,
            bytes_total: steps.write.iter().map(|w| w.estimate).sum(),
            bytes_per_sec: None,
            eta_secs: None,
        },
        summary: SyncSummary {
            device_id,
            outcome: SyncOutcome::Completed,
            finished_at: 0,
            songs_written: 0,
            songs_planned: steps.song_writes(),
            converted: 0,
            removed: 0,
            playlist_files: 0,
            folder_art: 0,
            failures: Vec::new(),
            planned_bytes: steps.write.iter().map(|w| w.estimate).sum(),
            written_bytes: 0,
            planned_free_after_bytes,
            free_after_bytes: None,
        },
    };

    let result = (|| {
        for entry in &steps.record {
            run.manifest.insert(entry.clone());
            run.dirty = true;
        }
        run.remove_all(&steps.remove, &steps.roots)?;
        run.write_all(&steps.write, transcoder, &profile, target)
    })();
    run.finish(result)
}

/// Why a sync stopped before its end.
enum Halt {
    Cancelled,
    Failed(String),
}

struct Run<'a> {
    storage: &'a dyn DeviceBackend,
    manifest: Manifest,
    clock: &'a dyn Clock,
    report: &'a mut dyn FnMut(&SyncProgress),
    cancel: &'a AtomicBool,
    last_save: Instant,
    /// The manifest changed since it was last saved.
    dirty: bool,
    rate: Rate,
    last_report: Option<Instant>,
    progress: SyncProgress,
    summary: SyncSummary,
}

impl Run<'_> {
    fn remove_all(&mut self, paths: &[String], roots: &[String]) -> Result<(), Halt> {
        if paths.is_empty() {
            return Ok(());
        }
        self.set_step(SyncStep::Removing);
        let mut emptied: BTreeSet<(usize, String)> = BTreeSet::new();
        for path in paths {
            self.check_cancel()?;
            if let Err(e) = self.storage.remove(path) {
                self.check_device(&e)?;
                self.fail(path, SongLabel::default(), &e.to_string());
                continue;
            }
            self.manifest.remove(path);
            self.dirty = true;
            self.summary.removed += 1;
            if let Some(dir) = parent(path) {
                emptied.insert((depth(dir), dir.to_string()));
            }
            self.save_manifest_if_due()?;
        }
        self.prune(emptied, roots)
    }

    /// Removes the folders removals left empty, deepest first, and their
    /// parents as they empty in turn. The storage refuses a folder with
    /// anything in it, so files the user put there stay.
    fn prune(&mut self, mut dirs: BTreeSet<(usize, String)>, roots: &[String]) -> Result<(), Halt> {
        while let Some((_, dir)) = dirs.pop_last() {
            if dir.is_empty() || roots.iter().any(|r| r.eq_ignore_ascii_case(&dir)) {
                continue;
            }
            match self.storage.remove_empty_folder(&dir) {
                Ok(true) => {
                    if let Some(up) = parent(&dir) {
                        dirs.insert((depth(up), up.to_string()));
                    }
                }
                Ok(false) => {}
                Err(e) => self.check_device(&e)?,
            }
        }
        Ok(())
    }

    fn write_all(
        &mut self,
        writes: &[Write],
        transcoder: &Transcoder,
        profile: &DeviceProfile,
        target: luminous_transcode::ConversionTarget,
    ) -> Result<(), Halt> {
        let stop_preparing = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let (tx, rx) =
                mpsc::sync_channel::<Result<Prepared, luminous_transcode::Error>>(LOOKAHEAD);
            let cancel = self.cancel;
            let stop = &stop_preparing;
            scope.spawn(move || {
                for write in writes {
                    let WriteSource::Track { source, .. } = &write.source else {
                        continue;
                    };
                    if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let prepared = transcoder.materialize(source, profile, cancel);
                    if tx.send(prepared).is_err() {
                        return;
                    }
                }
            });

            let result = (|| {
                for write in writes {
                    self.check_cancel()?;
                    match &write.source {
                        WriteSource::Track { source, converts } => {
                            let label = write.entry.label.clone();
                            self.set_step(if *converts {
                                SyncStep::Converting {
                                    song: label.clone(),
                                    format: target.format,
                                    bitrate_kbps: target.bitrate_kbps,
                                }
                            } else {
                                SyncStep::Copying {
                                    song: label.clone(),
                                }
                            });
                            let prepared = match rx.recv() {
                                Ok(prepared) => prepared,
                                Err(_) => return Err(Halt::Cancelled),
                            };
                            self.set_step(SyncStep::Copying { song: label });
                            self.write_track(write, prepared, source, transcoder, profile)?;
                        }
                        WriteSource::Playlist { layout, playlist } => {
                            self.set_step(SyncStep::WritingPlaylists);
                            // A song whose copy failed has no entry at its
                            // path; one whose update failed keeps its old file.
                            let songs: Vec<Song> = playlist
                                .songs
                                .iter()
                                .filter(|song| {
                                    layout
                                        .track_path(song.id)
                                        .is_some_and(|path| self.manifest.get(path).is_some())
                                })
                                .cloned()
                                .collect();
                            let content = layout.playlist_content(&DevicePlaylist {
                                name: &playlist.name,
                                songs: &songs,
                            });
                            let mut entry = write.entry.clone();
                            entry.fingerprint = Some(content_fingerprint(&content));
                            if self.write_bytes(entry, write.estimate, &content)? {
                                self.summary.playlist_files += 1;
                            }
                        }
                        WriteSource::FolderArt { album_track } => {
                            self.set_step(SyncStep::WritingFolderArt);
                            match folder_art_jpeg(album_track, profile.convert.max_art_px) {
                                Some(jpeg) => {
                                    if self.write_bytes(
                                        write.entry.clone(),
                                        write.estimate,
                                        &jpeg,
                                    )? {
                                        self.summary.folder_art += 1;
                                    }
                                }
                                // The album has no art after all.
                                None => {
                                    self.progress.bytes_total =
                                        self.progress.bytes_total.saturating_sub(write.estimate)
                                }
                            }
                        }
                    }
                    self.save_manifest_if_due()?;
                }
                Ok(())
            })();
            stop_preparing.store(true, Ordering::Relaxed);
            // Unblocks the preparing thread if it is waiting to hand one over.
            drop(rx);
            result
        })
    }

    fn write_track(
        &mut self,
        write: &Write,
        prepared: Result<Prepared, luminous_transcode::Error>,
        source: &std::path::Path,
        transcoder: &Transcoder,
        profile: &DeviceProfile,
    ) -> Result<(), Halt> {
        let path = &write.entry.path;
        let label = write.entry.label.clone();
        let mut prepared = match prepared {
            Ok(prepared) => prepared,
            Err(luminous_transcode::Error::Cancelled) => return Err(Halt::Cancelled),
            Err(e) => {
                self.fail(path, label, &e.to_string());
                return Ok(());
            }
        };
        // The plan came from what the library recorded about the file; this
        // is the file itself. Writing anything else than what the preview
        // showed would also leave the manifest disagreeing with every later
        // plan.
        if write.entry.conversion != Some(prepared.decision) {
            self.fail(path, label, "the file doesn't match the library; rescan it");
            return Ok(());
        }
        // A cache entry evicted since it was prepared is prepared again.
        if !prepared.path.exists() {
            prepared = match transcoder.materialize(source, profile, self.cancel) {
                Ok(p) => p,
                Err(luminous_transcode::Error::Cancelled) => return Err(Halt::Cancelled),
                Err(e) => {
                    self.fail(path, label, &e.to_string());
                    return Ok(());
                }
            };
        }
        let mut file = match std::fs::File::open(&prepared.path) {
            Ok(file) => file,
            Err(e) => {
                self.fail(path, label, &e.to_string());
                return Ok(());
            }
        };
        if self.put(path, &mut file, prepared.size, write.estimate, &label)? {
            let mut entry = write.entry.clone();
            entry.size = prepared.size;
            self.record(entry);
            self.summary.songs_written += 1;
            self.progress.songs_done += 1;
            if matches!(prepared.decision, Decision::Convert { .. }) {
                self.summary.converted += 1;
            }
        }
        Ok(())
    }

    /// Writes `content` at `entry`'s path and records `entry` with its real
    /// size. `Ok(false)` when this file failed and the sync goes on.
    fn write_bytes(
        &mut self,
        mut entry: ManifestEntry,
        estimate: u64,
        content: &[u8],
    ) -> Result<bool, Halt> {
        let len = content.len() as u64;
        let written = self.put(
            &entry.path,
            &mut &content[..],
            len,
            estimate,
            &SongLabel::default(),
        )?;
        if written {
            entry.size = len;
            self.record(entry);
        }
        Ok(written)
    }

    /// Writes `len` bytes from `source` at `path`. `Ok(false)` when this
    /// file failed and the sync goes on. Progress counts `estimate` for the
    /// file, whatever its real size, so the bar ends where it was planned to.
    fn put(
        &mut self,
        path: &str,
        source: &mut dyn io::Read,
        len: u64,
        estimate: u64,
        label: &SongLabel,
    ) -> Result<bool, Halt> {
        let start = self.progress.bytes_done;
        let scale = if len == 0 {
            0.0
        } else {
            estimate as f64 / len as f64
        };
        let result = {
            let (storage, cancel) = (self.storage, self.cancel);
            let mut on_progress = |written: u64| {
                if cancel.load(Ordering::Relaxed) {
                    return ControlFlow::Break(());
                }
                self.progress.bytes_done = start + (written as f64 * scale) as u64;
                self.maybe_report();
                ControlFlow::Continue(())
            };
            storage.write(path, source, len, &mut on_progress)
        };
        match result {
            Ok(()) => {
                self.progress.bytes_done = start + estimate;
                self.summary.written_bytes += len;
                self.maybe_report();
                Ok(true)
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Err(Halt::Cancelled),
            Err(e) => {
                self.progress.bytes_done = start + estimate;
                self.check_device(&e)?;
                self.fail(path, label.clone(), &e.to_string());
                Ok(false)
            }
        }
    }

    fn record(&mut self, entry: ManifestEntry) {
        self.manifest.insert(entry);
        self.dirty = true;
    }

    fn fail(&mut self, path: &str, song: SongLabel, message: &str) {
        log::warn!("Sync could not write {path}: {message}");
        self.summary.failures.push(SyncFailure {
            path: path.to_string(),
            song,
            message: message.to_string(),
        });
    }

    fn check_cancel(&self) -> Result<(), Halt> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(Halt::Cancelled)
        } else {
            Ok(())
        }
    }

    /// After a failed operation: a storage that no longer answers is gone,
    /// which ends the sync; otherwise only that file failed.
    fn check_device(&self, error: &io::Error) -> Result<(), Halt> {
        match self.storage.space() {
            Ok(_) => Ok(()),
            Err(_) => Err(Halt::Failed(format!(
                "the device was disconnected ({error})"
            ))),
        }
    }

    fn save_manifest_if_due(&mut self) -> Result<(), Halt> {
        if self.dirty && self.clock.now().duration_since(self.last_save) >= MANIFEST_SAVE_INTERVAL {
            self.save_manifest()?;
        }
        Ok(())
    }

    fn save_manifest(&mut self) -> Result<(), Halt> {
        self.manifest
            .write(self.storage)
            .map_err(|e| Halt::Failed(format!("could not save the sync manifest: {e}")))?;
        self.dirty = false;
        self.last_save = self.clock.now();
        Ok(())
    }

    fn set_step(&mut self, step: SyncStep) {
        if self.progress.step != step {
            self.progress.step = step;
            self.last_report = None;
            self.maybe_report();
        }
    }

    fn maybe_report(&mut self) {
        let now = self.clock.now();
        if self
            .last_report
            .is_some_and(|last| now.duration_since(last) < PROGRESS_INTERVAL)
        {
            return;
        }
        self.rate.sample(now, self.progress.bytes_done);
        self.progress.bytes_per_sec = self.rate.bytes_per_sec();
        self.progress.eta_secs = self.progress.bytes_per_sec.filter(|r| *r > 0.0).map(|r| {
            (self
                .progress
                .bytes_total
                .saturating_sub(self.progress.bytes_done) as f64
                / r)
                .ceil() as u64
        });
        self.last_report = Some(now);
        (self.report)(&self.progress);
    }

    fn finish(mut self, result: Result<(), Halt>) -> SyncSummary {
        self.summary.outcome = match result {
            Ok(()) => SyncOutcome::Completed,
            Err(Halt::Cancelled) => SyncOutcome::Stopped,
            Err(Halt::Failed(message)) => SyncOutcome::Failed { message },
        };
        self.set_step(SyncStep::Finishing);
        if self.dirty {
            if let Err(Halt::Failed(message)) = self.save_manifest() {
                if !matches!(self.summary.outcome, SyncOutcome::Failed { .. }) {
                    self.summary.outcome = SyncOutcome::Failed { message };
                }
            }
        }
        self.summary.free_after_bytes = self.storage.space().ok().map(|s| s.free_bytes);
        self.summary.finished_at = self.clock.unix_now();
        self.summary
    }
}

/// Bytes per second over the last [`RATE_WINDOW`].
#[derive(Default)]
struct Rate {
    samples: VecDeque<(Instant, u64)>,
}

impl Rate {
    fn sample(&mut self, at: Instant, bytes: u64) {
        self.samples.push_back((at, bytes));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| at.duration_since(*t) > RATE_WINDOW)
            && self.samples.len() > 2
        {
            self.samples.pop_front();
        }
    }

    fn bytes_per_sec(&self) -> Option<f64> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        let secs = last.0.duration_since(first.0).as_secs_f64();
        (secs > 0.0).then(|| last.1.saturating_sub(first.1) as f64 / secs)
    }
}

fn parent(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

fn depth(path: &str) -> usize {
    path.matches('/').count()
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod tests;
