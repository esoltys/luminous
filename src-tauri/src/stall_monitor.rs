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
//! [`instrument`] wraps the invoke handler so a stall warning can name the
//! IPC commands dispatched around it, and times synchronous commands — which
//! Tauri runs inline on the main thread, freezing the UI while they run.
//!
//! This is a cheap first signal, not a substitute for `tokio-console`
//! (see docs/PERFORMANCE.md), which can name the exact task holding a worker.

use parking_lot::Mutex;
use std::collections::{BTreeMap, VecDeque};
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

struct Sample {
    start: Instant,
    end: Instant,
}

static IPC_DISPATCHES: Mutex<VecDeque<(Instant, String)>> = Mutex::new(VecDeque::new());
static OS_PROBE_SAMPLES: Mutex<VecDeque<Sample>> = Mutex::new(VecDeque::new());

fn push_bounded<T>(queue: &Mutex<VecDeque<T>>, item: T, cap: usize) {
    let mut q = queue.lock();
    if q.len() == cap {
        q.pop_front();
    }
    q.push_back(item);
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
            &IPC_DISPATCHES,
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
        .spawn(|| loop {
            let start = Instant::now();
            std::thread::sleep(PROBE_INTERVAL);
            let end = Instant::now();
            push_bounded(&OS_PROBE_SAMPLES, Sample { start, end }, OS_PROBE_HISTORY);
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
    let os_overshoot = os_probe_overshoot(start, end);
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
        classify(overshoot, os_overshoot, !commands.is_empty()),
        if commands.is_empty() {
            "none"
        } else {
            &commands
        },
        metrics
    );
}

/// Largest overshoot of any OS-probe sample that overlaps `[start, end]`.
/// `None` when no sample overlaps — the OS probe thread itself never got to
/// run, which is itself evidence of process-wide starvation.
fn os_probe_overshoot(start: Instant, end: Instant) -> Option<Duration> {
    OS_PROBE_SAMPLES
        .lock()
        .iter()
        .filter(|s| s.end >= start && s.start <= end)
        .map(|s| (s.end - s.start).saturating_sub(PROBE_INTERVAL))
        .max()
}

/// Names the likely cause of a Tokio stall of `overshoot`, given the
/// OS-thread probe's worst overshoot over the same window.
fn classify(overshoot: Duration, os_overshoot: Option<Duration>, had_ipc: bool) -> String {
    match os_overshoot {
        Some(os) if os * 2 >= overshoot => format!(
            "whole process was starved (a plain OS thread was also {}ms late) — CPU \
             contention, system suspend or a debugger pause, not a blocked Tokio task",
            os.as_millis()
        ),
        None => "whole process was starved (the OS-thread probe didn't run at all during \
                 the window) — CPU contention, system suspend or a debugger pause"
            .into(),
        Some(_) if had_ipc => "a Tokio worker was blocked (OS-thread probe was on time) — \
             likely blocking work (sync lock, DB query, file I/O) inside one of the async \
             commands below; run with tokio-console to confirm which"
            .into(),
        Some(_) => "a Tokio worker was blocked (OS-thread probe was on time) with no IPC \
             nearby — likely a background task (scan, file watcher, cover/metadata fetch, \
             analysis) doing blocking work outside `spawn_blocking`"
            .into(),
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

    #[test]
    fn late_os_probe_blames_the_process_not_tokio() {
        assert!(classify(ms(100), Some(ms(60)), true).starts_with("whole process"));
        assert!(classify(ms(100), None, false).starts_with("whole process"));
    }

    #[test]
    fn on_time_os_probe_blames_a_worker_and_points_at_ipc_or_background() {
        let with_ipc = classify(ms(100), Some(ms(5)), true);
        assert!(with_ipc.contains("async commands below"), "{with_ipc}");
        let without = classify(ms(100), Some(ms(5)), false);
        assert!(without.contains("background task"), "{without}");
    }
}
