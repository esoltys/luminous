//! The one thread that reads library files in the background (#2160).
//!
//! Loudness, tempo/key, provenance and integrity analysis each decode whole
//! audio files. They used to run as separate threads that started at launch
//! and never looked at scans, so on a spinning disk a scan that only needs tag
//! headers competed with several full-file decoders at once. The runner owns
//! the scheduling for all of them instead:
//!
//! - **The library comes first.** No reader starts a file until the library
//!   has settled: no scan holds the scan gate and, when scan-on-startup is on,
//!   a full scan has finished this session. The user can browse and play
//!   newly added music before analysis begins.
//! - **Scans always win.** While any scan holds the gate, no reader *starts* a
//!   file. The check runs between files, so a file already being read when a
//!   scan starts is finished first; nothing stops a decode part-way through.
//! - **One file at a time.** Readers take turns in the order they're
//!   registered, one file per turn, with a pause between files.
//! - **No file is read twice in a loop.** A song whose read stored no result
//!   is skipped by that reader for the rest of the session, so an unreadable
//!   file can't be picked over and over. It's tried again after a relaunch,
//!   which also covers a drive that was briefly unavailable.
//!
//! Readers only say which song they want next and how to read it (see
//! [`BackgroundRead`]); pacing and the rules above live here, so a new
//! analyzer gets them by registering with [`BackgroundReadRunner::new`].

use crate::db::Database;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Pause after each file, so background reading never saturates the disk or CPU.
const PAUSE_BETWEEN_FILES: Duration = Duration::from_millis(250);
/// How long to wait before looking again when no reader has anything to do.
const IDLE_POLL: Duration = Duration::from_secs(15);
/// How often to check whether the library has settled (a scan finished).
const SETTLE_POLL: Duration = Duration::from_secs(1);

/// One kind of background analysis over library files.
pub trait BackgroundRead: Send {
    /// Short name for log lines.
    fn name(&self) -> &'static str;

    /// The next song to read, or `None` when nothing is waiting or the feature
    /// is turned off. Must leave out the songs in `skipped` (see
    /// [`Skipped::sql_excluding`]); a reader that offers one anyway loses its
    /// turn.
    fn next(
        &mut self,
        conn: &Connection,
        skipped: &Skipped,
    ) -> rusqlite::Result<Option<(i64, String)>>;

    /// Reads the song's file and stores the outcome. Returns
    /// [`ReadOutcome::NothingStored`] when nothing was stored (the file
    /// couldn't be read, or the result couldn't be written), so the runner
    /// skips that song for the rest of the session instead of reading it again
    /// in a loop.
    fn read(&mut self, song_id: i64, path: &Path) -> ReadOutcome;

    /// Called when `next` finds nothing, for readers that batch notifications
    /// until their backlog drains.
    fn caught_up(&mut self) {}
}

/// What [`BackgroundRead::read`] did with a song.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOutcome {
    /// A result (or a failure marker) is stored, so `next` won't offer the
    /// song again until its file changes.
    Stored,
    /// Nothing was stored; without the skip list `next` would offer it again.
    NothingStored,
}

/// Songs a reader must not be offered again this session.
#[derive(Debug, Default)]
pub struct Skipped(HashSet<i64>);

impl Skipped {
    /// A SQL condition leaving these songs out, for a query's `WHERE` clause:
    /// `column NOT IN (…)`, or `1` when there are none. Safe to splice into
    /// SQL, since the ids are integers.
    pub fn sql_excluding(&self, column: &str) -> String {
        if self.0.is_empty() {
            return "1".to_string();
        }
        let ids: Vec<String> = self.0.iter().map(i64::to_string).collect();
        format!("{column} NOT IN ({})", ids.join(","))
    }

    pub(crate) fn insert(&mut self, song_id: i64) {
        self.0.insert(song_id);
    }

    fn contains(&self, song_id: i64) -> bool {
        self.0.contains(&song_id)
    }
}

/// Schedules every [`BackgroundRead`] on one thread. See the module docs.
pub struct BackgroundReadRunner {
    db: Arc<Database>,
    readers: Vec<Box<dyn BackgroundRead>>,
    /// Per reader, the songs whose read stored nothing.
    skipped: Vec<Skipped>,
    /// Index of the reader whose turn is next.
    turn: usize,
    /// When true, the library isn't settled until a full scan has finished
    /// this session: the frontend starts one at launch (scan-on-startup).
    startup_scan_expected: bool,
    /// Work that also waits for the library but runs once, before any reader.
    once_settled: Vec<Box<dyn FnOnce() + Send>>,
}

impl BackgroundReadRunner {
    /// `readers` take turns in this order.
    pub fn new(db: Arc<Database>, readers: Vec<Box<dyn BackgroundRead>>) -> Self {
        let startup_scan_expected = db
            .pool
            .get()
            .map(|conn| scan_on_startup(&conn))
            .unwrap_or(false);
        let skipped = readers.iter().map(|_| Skipped::default()).collect();
        Self {
            db,
            readers,
            skipped,
            turn: 0,
            startup_scan_expected,
            once_settled: Vec::new(),
        }
    }

    /// Runs `task` once, on the runner's thread, as soon as the library has
    /// settled and before any reader starts.
    pub fn once_settled(mut self, task: impl FnOnce() + Send + 'static) -> Self {
        self.once_settled.push(Box::new(task));
        self
    }

    /// Starts the runner's thread, which lives for the rest of the app's life.
    pub fn spawn(mut self) {
        std::thread::Builder::new()
            .name("luminous-background-reads".to_string())
            .spawn(move || loop {
                let wait = self.tick();
                std::thread::sleep(wait);
            })
            .expect("failed to spawn background reads thread");
    }

    /// Whether readers may start a file now. See the module docs.
    fn library_settled(&self) -> bool {
        let gate = &self.db.scan_gate;
        !gate.is_scanning() && (!self.startup_scan_expected || gate.full_scans_finished() > 0)
    }

    /// Reads at most one file and returns how long to wait before the next tick.
    fn tick(&mut self) -> Duration {
        if !self.library_settled() {
            return SETTLE_POLL;
        }
        for task in self.once_settled.drain(..) {
            task();
        }

        let Some((index, song_id, path)) = self.pick() else {
            return IDLE_POLL;
        };
        if self.readers[index].read(song_id, Path::new(&path)) == ReadOutcome::NothingStored {
            self.skipped[index].insert(song_id);
        }
        PAUSE_BETWEEN_FILES
    }

    /// The first reader, starting from the one whose turn it is, that has a
    /// song waiting. Advances the turn past it.
    fn pick(&mut self) -> Option<(usize, i64, String)> {
        let conn = match self.db.pool.get() {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("Background reads: no database connection ({e})");
                return None;
            }
        };
        let count = self.readers.len();
        for offset in 0..count {
            let index = (self.turn + offset) % count;
            let reader = &mut self.readers[index];
            match reader.next(&conn, &self.skipped[index]) {
                Ok(Some((song_id, _))) if self.skipped[index].contains(song_id) => {
                    log::warn!(
                        "Background {} analysis offered skipped song {song_id}; passing its turn",
                        reader.name()
                    );
                }
                Ok(Some((song_id, path))) => {
                    self.turn = (index + 1) % count;
                    return Some((index, song_id, path));
                }
                Ok(None) => reader.caught_up(),
                Err(e) => log::warn!(
                    "Background {} analysis couldn't pick its next song: {e}",
                    reader.name()
                ),
            }
        }
        None
    }
}

/// The frontend's scan-on-startup setting, read the way it does: the
/// `app_state` value `"true"`, off when missing.
fn scan_on_startup(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT value FROM app_state WHERE key = 'scan_on_startup'",
        [],
        |row| row.get::<_, String>(0),
    )
    .map(|value| value == "true")
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// A reader over a fixed queue of song ids. Records what it read into a
    /// log shared by every fake in a test, so turn order is visible.
    struct FakeReader {
        name: &'static str,
        queue: Vec<i64>,
        /// When false, `read` doesn't remove the song: a reader that never
        /// stores a result.
        stores_result: bool,
        reads: Arc<Mutex<Vec<(&'static str, i64)>>>,
    }

    impl BackgroundRead for FakeReader {
        fn name(&self) -> &'static str {
            self.name
        }
        fn next(
            &mut self,
            _conn: &Connection,
            skipped: &Skipped,
        ) -> rusqlite::Result<Option<(i64, String)>> {
            Ok(self
                .queue
                .iter()
                .find(|id| !skipped.contains(**id))
                .map(|id| (*id, format!("{id}.flac"))))
        }
        fn read(&mut self, song_id: i64, _path: &Path) -> ReadOutcome {
            self.reads.lock().push((self.name, song_id));
            if self.stores_result {
                self.queue.retain(|id| *id != song_id);
                ReadOutcome::Stored
            } else {
                ReadOutcome::NothingStored
            }
        }
    }

    fn fake(
        name: &'static str,
        queue: &[i64],
        reads: &Arc<Mutex<Vec<(&'static str, i64)>>>,
    ) -> Box<dyn BackgroundRead> {
        Box::new(FakeReader {
            name,
            queue: queue.to_vec(),
            stores_result: true,
            reads: Arc::clone(reads),
        })
    }

    fn setup() -> (tempfile::TempDir, Arc<Database>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::new(dir.path().to_path_buf()).unwrap());
        (dir, db)
    }

    fn set_scan_on_startup(db: &Database, value: &str) {
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT OR REPLACE INTO app_state (key, value) VALUES ('scan_on_startup', ?1)",
                [value],
            )
            .unwrap();
    }

    #[test]
    fn readers_take_turns_one_file_at_a_time() {
        let (_dir, db) = setup();
        let reads = Arc::default();
        let mut runner = BackgroundReadRunner::new(
            db,
            vec![
                fake("loudness", &[1, 2], &reads),
                fake("provenance", &[1], &reads),
            ],
        );

        assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        assert_eq!(runner.tick(), IDLE_POLL, "every queue is drained");
        assert_eq!(
            *reads.lock(),
            vec![("loudness", 1), ("provenance", 1), ("loudness", 2)]
        );
    }

    #[tokio::test]
    async fn no_file_is_started_while_a_scan_holds_the_gate() {
        let (_dir, db) = setup();
        let reads = Arc::default();
        let mut runner =
            BackgroundReadRunner::new(Arc::clone(&db), vec![fake("provenance", &[1], &reads)]);

        let scoped = db.scan_gate.acquire(false).await.unwrap();
        assert_eq!(runner.tick(), SETTLE_POLL);
        assert!(reads.lock().is_empty(), "a scoped scan blocks reads too");
        drop(scoped);

        let full = db.scan_gate.acquire(true).await.unwrap();
        assert_eq!(runner.tick(), SETTLE_POLL);
        assert!(reads.lock().is_empty());
        drop(full);

        assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        assert_eq!(*reads.lock(), vec![("provenance", 1)]);
    }

    #[test]
    fn with_scan_on_startup_unset_or_off_reading_starts_at_launch() {
        for value in [None, Some("false")] {
            let (_dir, db) = setup();
            if let Some(value) = value {
                set_scan_on_startup(&db, value);
            }
            let reads = Arc::default();
            let mut runner = BackgroundReadRunner::new(db, vec![fake("integrity", &[1], &reads)]);
            assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES, "setting {value:?}");
        }
    }

    #[tokio::test]
    async fn with_scan_on_startup_reading_waits_for_a_full_scan_to_finish() {
        let (_dir, db) = setup();
        set_scan_on_startup(&db, "true");
        let reads = Arc::default();
        let mut runner =
            BackgroundReadRunner::new(Arc::clone(&db), vec![fake("integrity", &[1], &reads)]);

        assert_eq!(runner.tick(), SETTLE_POLL, "no scan has run yet");

        // A scoped scan doesn't settle the library: only a full scan does.
        drop(db.scan_gate.acquire(false).await.unwrap());
        assert_eq!(runner.tick(), SETTLE_POLL);

        // Stands in for a startup scan that fails part-way: it still counts.
        let failed_scan = async {
            let _permit = db.scan_gate.acquire(true).await;
            Err::<(), &str>("scan failed")
        };
        assert!(failed_scan.await.is_err());
        assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        assert_eq!(*reads.lock(), vec![("integrity", 1)]);
    }

    #[tokio::test]
    async fn once_settled_work_runs_once_after_the_library_settles() {
        let (_dir, db) = setup();
        let runs = Arc::new(Mutex::new(0));
        let counter = Arc::clone(&runs);
        let mut runner = BackgroundReadRunner::new(Arc::clone(&db), Vec::new())
            .once_settled(move || *counter.lock() += 1);

        let full = db.scan_gate.acquire(true).await.unwrap();
        runner.tick();
        assert_eq!(*runs.lock(), 0, "waits for the scan");
        drop(full);

        assert_eq!(runner.tick(), IDLE_POLL);
        runner.tick();
        assert_eq!(*runs.lock(), 1);
    }

    #[test]
    fn a_song_whose_read_stored_nothing_is_not_read_again() {
        let (_dir, db) = setup();
        let reads = Arc::default();
        let unreadable = Box::new(FakeReader {
            name: "integrity",
            queue: vec![7, 8],
            stores_result: false,
            reads: Arc::clone(&reads),
        });
        let mut runner =
            BackgroundReadRunner::new(db, vec![unreadable, fake("provenance", &[1], &reads)]);

        for _ in 0..3 {
            assert_eq!(runner.tick(), PAUSE_BETWEEN_FILES);
        }
        assert_eq!(runner.tick(), IDLE_POLL);
        assert_eq!(
            *reads.lock(),
            vec![("integrity", 7), ("provenance", 1), ("integrity", 8)],
            "each unreadable song is tried once, and the reader moves on"
        );
    }

    #[test]
    fn sql_excluding_lists_skipped_songs() {
        let mut skipped = Skipped::default();
        assert_eq!(skipped.sql_excluding("s.id"), "1");
        skipped.insert(42);
        assert_eq!(skipped.sql_excluding("s.id"), "s.id NOT IN (42)");
    }
}
