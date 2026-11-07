//! Stall detection with attribution (#1002).
//!
//! Two independent probes run side by side:
//!
//! - a **Tokio probe** — an async task that sleeps [`PROBE_INTERVAL`] and
//!   measures how late it woke up. Late means the runtime's worker threads
//!   were too busy/blocked to poll it, the same condition that delays async
//!   IPC command handlers;
//! - an **OS-thread probe** — a plain `std::thread` doing the same sleep.
//!   It has no Tokio dependency, so if it was late too, the whole process
//!   was starved (CPU contention, system suspend, a debugger pause) and no
//!   Tokio task is to blame.
//!
//! The OS probe is a weak witness to CPU contention, though: a saturated
//! machine still reschedules a sleeping thread within a few ms, while a Tokio
//! timer wake hops through more threads and runs late far more often. So the
//! OS-probe thread also samples system CPU pressure ([`cpu_pressure`]) and
//! remembers when it last saw contention, and a Tokio stall that follows
//! recent contention is reported as inconclusive rather than pinned on a
//! blocked worker (#1966).
//!
//! [`instrument`] wraps the invoke handler so a stall warning can name the
//! IPC commands dispatched around it, and times synchronous commands — which
//! Tauri runs inline on the main thread, freezing the UI while they run.
//!
//! This is a cheap first signal, not a substitute for `tokio-console`
//! (see docs/PERFORMANCE.md), which can name the exact task holding a worker.

mod cpu_pressure;

use cpu_pressure::Pressure;
use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

/// Nominal probe sleep, chosen to match a typical audio-frame window since
/// that's the granularity where a scheduling stall would first become audible.
const PROBE_INTERVAL: Duration = Duration::from_millis(20);
/// Overshoot beyond [`PROBE_INTERVAL`] that counts as a stall. Well above
/// Windows' default ~15.6ms timer resolution, so coarse timers alone never trip it.
const STALL_THRESHOLD: Duration = Duration::from_millis(40);
/// A synchronous IPC command running at least this long froze the UI for a
/// visible number of frames.
const SYNC_COMMAND_THRESHOLD: Duration = Duration::from_millis(32);
/// How far before a stall's probe window to look for IPC dispatches: a
/// command dispatched just before the probe started can still be the one
/// holding a worker when the probe was due.
const DISPATCH_LOOKBACK: Duration = Duration::from_millis(250);
/// Most-recent IPC dispatches kept for attribution. The UI dispatches in
/// bursts (a scroll can fetch dozens of covers), so this sizes for a burst
/// rather than for a steady rate.
const DISPATCH_HISTORY: usize = 256;
/// OS-probe samples kept — ~1.3s at the nominal interval, comfortably longer
/// than any single Tokio probe window we'd need to overlap.
const OS_PROBE_HISTORY: usize = 64;
/// How often the OS-probe thread samples system CPU pressure.
const PRESSURE_INTERVAL: Duration = Duration::from_secs(1);
/// How long seen contention keeps casting doubt on "a Tokio worker was
/// blocked". Contention comes in bursts lasting seconds (a build, a test run),
/// and pressure is only sampled after each interval ends, so the first stalls
/// of a burst are judged against the previous interval's reading.
const CONTENTION_MEMORY: Duration = Duration::from_secs(5);

struct Sample {
    start: Instant,
    end: Instant,
}

impl Sample {
    fn overshoot(&self) -> Duration {
        (self.end - self.start).saturating_sub(PROBE_INTERVAL)
    }
}

/// A sign that the whole machine was short of CPU at some moment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Contention {
    /// An OS-probe sample overshot by this much (over [`STALL_THRESHOLD`]).
    LateOsProbe(Duration),
    /// The OS reported contended CPU pressure over a [`PRESSURE_INTERVAL`].
    CpuPressure(Pressure),
}

impl fmt::Display for Contention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Contention::LateOsProbe(late) => {
                write!(f, "a plain OS thread ran {}ms late", late.as_millis())
            }
            Contention::CpuPressure(pressure) => pressure.fmt(f),
        }
    }
}

/// Everything the OS-probe thread has seen. It is the only witness to
/// process-wide starvation that doesn't depend on Tokio, so it keeps its own
/// record of contention rather than trusting earlier stall verdicts.
struct OsProbe {
    samples: VecDeque<Sample>,
    /// When contention was last seen, and how.
    last_contention: Option<(Instant, Contention)>,
}

/// What the OS probe says about the process around one Tokio stall.
struct OsEvidence {
    /// Largest overshoot of any sample overlapping the stall window. `None`
    /// when no sample overlaps — the OS probe thread itself never got to run,
    /// which is itself evidence of process-wide starvation.
    in_window: Option<Duration>,
    /// How long before the stall ended contention was last seen, and how.
    recent_contention: Option<(Duration, Contention)>,
}

impl OsProbe {
    const fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            last_contention: None,
        }
    }

    fn record(&mut self, sample: Sample) {
        // Over the same bar as a Tokio stall, so Windows' coarse timer alone
        // never makes an idle machine look contended.
        let late = sample.overshoot();
        if late > STALL_THRESHOLD {
            self.last_contention = Some((sample.end, Contention::LateOsProbe(late)));
        }
        push_bounded(&mut self.samples, sample, OS_PROBE_HISTORY);
    }

    /// Records a CPU pressure reading taken at `at`, keeping it only if it
    /// shows contention.
    fn record_pressure(&mut self, at: Instant, pressure: Pressure) {
        if pressure.is_contended() {
            self.last_contention = Some((at, Contention::CpuPressure(pressure)));
        }
    }

    fn evidence(&self, start: Instant, end: Instant) -> OsEvidence {
        OsEvidence {
            in_window: self
                .samples
                .iter()
                .filter(|s| s.end >= start && s.start <= end)
                .map(Sample::overshoot)
                .max(),
            recent_contention: self
                .last_contention
                .map(|(at, what)| (end.saturating_duration_since(at), what)),
        }
    }
}

static IPC_DISPATCHES: Mutex<VecDeque<(Instant, String)>> = Mutex::new(VecDeque::new());
static OS_PROBE: Mutex<OsProbe> = Mutex::new(OsProbe::new());

fn push_bounded<T>(queue: &mut VecDeque<T>, item: T, cap: usize) {
    if queue.len() == cap {
        queue.pop_front();
    }
    queue.push_back(item);
}

/// Wraps the `generate_handler!` dispatcher so every IPC dispatch is
/// recorded for stall attribution, and warns when the dispatcher itself runs
/// long. Async commands only deserialize and spawn inside it, so that timing
/// effectively covers synchronous commands, which Tauri runs inline on the
/// main thread.
pub fn instrument<F>(handler: F) -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static
where
    F: Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static,
{
    move |invoke| {
        let command = invoke.message.command().to_owned();
        let started = Instant::now();
        push_bounded(
            &mut IPC_DISPATCHES.lock(),
            (started, command.clone()),
            DISPATCH_HISTORY,
        );
        let handled = handler(invoke);
        let took = started.elapsed();
        if took >= SYNC_COMMAND_THRESHOLD {
            log::warn!(
                "UI stall: IPC command `{command}` ran {}ms synchronously on the main thread — \
                 the window could not repaint or take input meanwhile. Make it `async` or move \
                 the slow part off the main thread.",
                took.as_millis()
            );
        }
        handled
    }
}

/// Starts both probes. Call once, from inside the Tauri setup hook.
pub fn spawn() {
    std::thread::Builder::new()
        .name("stall-monitor-os-probe".into())
        .spawn(|| {
            let mut pressure = cpu_pressure::Sampler::new();
            let mut last_pressure_read = Instant::now();
            loop {
                let start = Instant::now();
                std::thread::sleep(PROBE_INTERVAL);
                let end = Instant::now();
                // Read the OS counter before locking: a stall report takes the
                // same lock from a Tokio worker.
                let reading = (end - last_pressure_read >= PRESSURE_INTERVAL)
                    .then(|| {
                        last_pressure_read = end;
                        pressure.sample()
                    })
                    .flatten();
                let mut probe = OS_PROBE.lock();
                probe.record(Sample { start, end });
                if let Some(p) = reading {
                    probe.record_pressure(end, p);
                }
            }
        })
        .map_err(|e| log::error!("Failed to start OS-thread stall probe: {e}"))
        .ok();

    tauri::async_runtime::spawn(async move {
        loop {
            let start = Instant::now();
            tokio::time::sleep(PROBE_INTERVAL).await;
            let end = Instant::now();
            let overshoot = (end - start).saturating_sub(PROBE_INTERVAL);
            if overshoot > STALL_THRESHOLD {
                report_tokio_stall(start, end, overshoot);
            }
        }
    });

    // Silent when healthy, so say once that it's watching — otherwise a quiet
    // log can't be told apart from a monitor that never started.
    log::info!(
        "Stall monitor started: warns on Tokio stalls over {}ms and synchronous IPC commands over {}ms",
        STALL_THRESHOLD.as_millis(),
        SYNC_COMMAND_THRESHOLD.as_millis()
    );
}

fn report_tokio_stall(start: Instant, end: Instant, overshoot: Duration) {
    let os = OS_PROBE.lock().evidence(start, end);
    let commands = {
        let q = IPC_DISPATCHES.lock();
        summarize_dispatches(
            q.iter(),
            start.checked_sub(DISPATCH_LOOKBACK).unwrap_or(start),
            end,
        )
    };
    let metrics = tokio::runtime::Handle::try_current()
        .map(|h| {
            let m = h.metrics();
            format!(
                "{} alive tasks, {} queued globally, {} workers",
                m.num_alive_tasks(),
                m.global_queue_depth(),
                m.num_workers()
            )
        })
        .unwrap_or_else(|_| "runtime metrics unavailable".into());

    log::warn!(
        "Tokio stall: {}ms probe woke {}ms late. Cause: {}. IPC around the stall: {}. Runtime: {}.",
        PROBE_INTERVAL.as_millis(),
        overshoot.as_millis(),
        classify(overshoot, &os, !commands.is_empty()),
        if commands.is_empty() {
            "none"
        } else {
            &commands
        },
        metrics
    );
}

/// Likely cause of a Tokio stall, as named in the warning.
#[derive(Debug, PartialEq)]
enum Cause {
    /// The OS probe was late too (`os_late`), or never ran (`None`).
    ProcessStarved { os_late: Option<Duration> },
    /// The OS probe wasn't late enough in this window to explain the stall,
    /// but `what` showed contention `ago` before it ended: the machine is
    /// probably still contended, which delays Tokio wakes far more than it
    /// delays the OS probe.
    RecentContention { ago: Duration, what: Contention },
    /// No sign of contention, so something held a Tokio worker.
    WorkerBlocked { had_ipc: bool },
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cause::ProcessStarved { os_late: Some(os) } => write!(
                f,
                "whole process was starved (a plain OS thread was also {}ms late) — CPU \
                 contention, system suspend or a debugger pause, not a blocked Tokio task",
                os.as_millis()
            ),
            Cause::ProcessStarved { os_late: None } => f.write_str(
                "whole process was starved (the OS-thread probe didn't run at all during \
                 the window) — CPU contention, system suspend or a debugger pause",
            ),
            Cause::RecentContention { ago, what } => write!(
                f,
                "inconclusive — the machine was short of CPU {:.1}s ago ({what}), so likely \
                 CPU contention rather than a blocked Tokio task; only worth chasing if it \
                 recurs on an idle machine",
                ago.as_secs_f32()
            ),
            Cause::WorkerBlocked { had_ipc: true } => f.write_str(
                "a Tokio worker was blocked (OS-thread probe was on time) — likely blocking \
                 work (sync lock, DB query, file I/O) inside one of the async commands below; \
                 run with tokio-console to confirm which",
            ),
            Cause::WorkerBlocked { had_ipc: false } => f.write_str(
                "a Tokio worker was blocked (OS-thread probe was on time) with no IPC nearby — \
                 likely a background task (scan, file watcher, cover/metadata fetch, analysis) \
                 doing blocking work outside `spawn_blocking`",
            ),
        }
    }
}

/// Names the likely cause of a Tokio stall of `overshoot`, given what the
/// OS-thread probe saw around it.
fn classify(overshoot: Duration, os: &OsEvidence, had_ipc: bool) -> Cause {
    match (os.in_window, os.recent_contention) {
        (Some(os_late), _) if os_late * 2 >= overshoot => Cause::ProcessStarved {
            os_late: Some(os_late),
        },
        (None, _) => Cause::ProcessStarved { os_late: None },
        (Some(_), Some((ago, what))) if ago <= CONTENTION_MEMORY => {
            Cause::RecentContention { ago, what }
        }
        (Some(_), _) => Cause::WorkerBlocked { had_ipc },
    }
}

/// Collapses dispatches within `[from, to]` into `cmd ×N` pairs, busiest first.
fn summarize_dispatches<'a>(
    dispatches: impl Iterator<Item = &'a (Instant, String)>,
    from: Instant,
    to: Instant,
) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for (at, cmd) in dispatches {
        if *at >= from && *at <= to {
            *counts.entry(cmd.as_str()).or_default() += 1;
        }
    }
    let mut counts: Vec<_> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    counts
        .iter()
        .map(|(cmd, n)| {
            if *n == 1 {
                (*cmd).to_owned()
            } else {
                format!("{cmd} ×{n}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn dispatch_summary_counts_only_the_window_busiest_first() {
        let t0 = Instant::now();
        let d = [
            (t0, "too_early".to_owned()),
            (t0 + ms(100), "get_cover".to_owned()),
            (t0 + ms(110), "get_songs".to_owned()),
            (t0 + ms(120), "get_cover".to_owned()),
            (t0 + ms(500), "too_late".to_owned()),
        ];
        assert_eq!(
            summarize_dispatches(d.iter(), t0 + ms(50), t0 + ms(200)),
            "get_cover ×2, get_songs"
        );
        assert_eq!(
            summarize_dispatches(d.iter(), t0 + ms(200), t0 + ms(300)),
            ""
        );
    }

    fn os_saw(in_window: Option<u64>, recent: Option<(u64, Contention)>) -> OsEvidence {
        OsEvidence {
            in_window: in_window.map(ms),
            recent_contention: recent.map(|(ago, what)| (ms(ago), what)),
        }
    }

    const BUSY: Contention = Contention::CpuPressure(Pressure::from_share(0.95));

    #[test]
    fn late_os_probe_blames_the_process_not_tokio() {
        assert_eq!(
            classify(ms(100), &os_saw(Some(60), None), true),
            Cause::ProcessStarved {
                os_late: Some(ms(60))
            }
        );
        assert_eq!(
            classify(ms(100), &os_saw(None, None), false),
            Cause::ProcessStarved { os_late: None }
        );
    }

    #[test]
    fn on_time_os_probe_blames_a_worker_and_points_at_ipc_or_background() {
        for had_ipc in [true, false] {
            assert_eq!(
                classify(ms(100), &os_saw(Some(5), None), had_ipc),
                Cause::WorkerBlocked { had_ipc }
            );
        }
        let with_ipc = Cause::WorkerBlocked { had_ipc: true }.to_string();
        assert!(with_ipc.contains("async commands below"), "{with_ipc}");
        let without = Cause::WorkerBlocked { had_ipc: false }.to_string();
        assert!(without.contains("background task"), "{without}");
    }

    #[test]
    fn on_time_os_probe_after_recent_contention_is_inconclusive() {
        for had_ipc in [true, false] {
            assert_eq!(
                classify(ms(100), &os_saw(Some(5), Some((2_000, BUSY))), had_ipc),
                Cause::RecentContention {
                    ago: ms(2_000),
                    what: BUSY
                }
            );
        }
    }

    #[test]
    fn contention_older_than_the_memory_no_longer_counts() {
        let long_ago = CONTENTION_MEMORY.as_millis() as u64 + 1;
        assert_eq!(
            classify(ms(100), &os_saw(Some(5), Some((long_ago, BUSY))), false),
            Cause::WorkerBlocked { had_ipc: false }
        );
    }

    #[test]
    fn os_probe_remembers_late_samples_and_contended_pressure_only() {
        let t0 = Instant::now();
        let late = STALL_THRESHOLD + ms(1);
        let mut probe = OsProbe::new();
        probe.record(Sample {
            start: t0,
            end: t0 + PROBE_INTERVAL,
        });
        probe.record_pressure(t0 + PROBE_INTERVAL, Pressure::from_share(0.01));
        assert_eq!(probe.evidence(t0, t0 + ms(100)).recent_contention, None);

        let late_end = t0 + ms(100) + PROBE_INTERVAL + late;
        probe.record(Sample {
            start: t0 + ms(100),
            end: late_end,
        });
        probe.record(Sample {
            start: late_end,
            end: late_end + PROBE_INTERVAL,
        });
        let e = probe.evidence(late_end + ms(1_000), late_end + ms(1_100));
        assert_eq!(
            e.recent_contention,
            Some((ms(1_100), Contention::LateOsProbe(late)))
        );
        assert_eq!(e.in_window, None, "no sample overlaps the later window");

        probe.record_pressure(late_end + ms(1_000), Pressure::from_share(0.95));
        let e = probe.evidence(late_end + ms(1_000), late_end + ms(1_500));
        assert_eq!(e.recent_contention, Some((ms(500), BUSY)));
    }
}
