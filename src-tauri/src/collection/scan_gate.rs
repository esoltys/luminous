//! One-scan-at-a-time gate for the library (#1873).
//!
//! Every caller builds a throwaway `CollectionScanner`, so the only state they
//! all share is the `Arc<Database>`; the gate lives there. It is taken once, at
//! the top of `CollectionScanner::scan_all_core_ext`, so the Rescan buttons,
//! the Files view, both watcher recovery paths and tests all get the same
//! guarantee with no per-caller code. (The UI's `isScanning` check is only a
//! convenience; it can't see the watcher or the gap before the first event.)
//!
//! Policy, identical for every caller:
//! - a full scan requested while another scan runs **waits**, then runs, so it
//!   picks up changes made while the first one was walking;
//! - a scoped scan requested while a **full** scan runs is **skipped**, since
//!   the running scan walks that folder anyway;
//! - a scoped scan requested while another scoped scan runs waits.
//!
//! Not gated: `rescan_paths` and the tag editor, which write individual files
//! under their own `WatcherPauseGuard` and don't walk the library.
//!
//! The gate is also the one place that knows whether a scan is running, which
//! the background readers (`crate::background_reads`) consult so they never
//! start a file while a scan holds it (#2160).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{Mutex, MutexGuard};

#[derive(Debug, Default)]
pub struct ScanGate {
    running: Mutex<()>,
    /// True only while a full (unscoped) scan holds `running`.
    full_running: AtomicBool,
    /// Full scans that have released the gate since launch, however they
    /// ended (finished, failed or returned early).
    full_scans_finished: AtomicU64,
}

/// Held for the duration of one scan; releases the gate on drop, including on
/// early return, error and panic.
#[derive(Debug)]
pub struct ScanPermit<'a> {
    gate: &'a ScanGate,
    full: bool,
    _running: MutexGuard<'a, ()>,
}

impl ScanGate {
    /// Waits for the gate and returns a permit, or `None` when the requested
    /// scan is redundant (see the module docs). `full` is true for a scan of
    /// every watched folder.
    ///
    /// A scoped scan checks for a running full scan only on entry: if it queues
    /// behind another scoped scan and a full scan then takes the gate first, it
    /// still runs afterwards. That is redundant but harmless (it is mtime-skipped).
    pub async fn acquire(&self, full: bool) -> Option<ScanPermit<'_>> {
        if !full && self.full_running.load(Ordering::SeqCst) {
            return None;
        }
        let running = self.running.lock().await;
        if full {
            self.full_running.store(true, Ordering::SeqCst);
        }
        Some(ScanPermit {
            gate: self,
            full,
            _running: running,
        })
    }

    /// Whether a scan, full or scoped, holds the gate right now. A scan
    /// still waiting for the gate doesn't count.
    pub fn is_scanning(&self) -> bool {
        self.running.try_lock().is_err()
    }

    /// How many full scans have released the gate since launch.
    pub fn full_scans_finished(&self) -> u64 {
        self.full_scans_finished.load(Ordering::SeqCst)
    }
}

impl Drop for ScanPermit<'_> {
    fn drop(&mut self) {
        // Runs before `_running` is released, so a waiter never sees a stale flag.
        if self.full {
            self.gate.full_running.store(false, Ordering::SeqCst);
            self.gate.full_scans_finished.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn second_full_scan_waits_for_the_first() {
        let gate = Arc::new(ScanGate::default());
        let first = gate.acquire(true).await.unwrap();

        let g = Arc::clone(&gate);
        let second = tokio::spawn(async move { g.acquire(true).await.is_some() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "second scan must wait");

        drop(first);
        assert!(second.await.unwrap());
    }

    #[tokio::test]
    async fn scoped_scan_is_skipped_while_a_full_scan_runs() {
        let gate = ScanGate::default();
        let full = gate.acquire(true).await.unwrap();
        assert!(gate.acquire(false).await.is_none());
        drop(full);
        assert!(gate.acquire(false).await.is_some());
    }

    #[tokio::test]
    async fn scoped_scan_waits_for_another_scoped_scan() {
        let gate = Arc::new(ScanGate::default());
        let first = gate.acquire(false).await.unwrap();

        let g = Arc::clone(&gate);
        let second = tokio::spawn(async move { g.acquire(false).await.is_some() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished());

        drop(first);
        assert!(second.await.unwrap());
    }

    #[tokio::test]
    async fn gate_is_released_when_the_permit_is_dropped_early() {
        let gate = ScanGate::default();
        // Stands in for a scan that bails out with `?` part-way through.
        let attempt = || async {
            let _permit = gate.acquire(true).await;
            Err::<(), &str>("scan failed")
        };
        assert!(attempt().await.is_err());
        assert_eq!(
            gate.full_scans_finished(),
            1,
            "a failed full scan still finished"
        );
        assert!(gate.acquire(true).await.is_some());
        assert!(!gate.full_running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn reports_scanning_while_any_permit_is_held() {
        let gate = ScanGate::default();
        assert!(!gate.is_scanning());

        let scoped = gate.acquire(false).await.unwrap();
        assert!(gate.is_scanning());
        drop(scoped);
        assert!(!gate.is_scanning());
        assert_eq!(
            gate.full_scans_finished(),
            0,
            "a scoped scan isn't a full one"
        );

        let full = gate.acquire(true).await.unwrap();
        assert!(gate.is_scanning());
        drop(full);
        assert!(!gate.is_scanning());
        assert_eq!(gate.full_scans_finished(), 1);
    }
}
