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

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, MutexGuard};

#[derive(Debug, Default)]
pub struct ScanGate {
    running: Mutex<()>,
    /// True only while a full (unscoped) scan holds `running`.
    full_running: AtomicBool,
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
}

impl Drop for ScanPermit<'_> {
    fn drop(&mut self) {
        // Runs before `_running` is released, so a waiter never sees a stale flag.
        if self.full {
            self.gate.full_running.store(false, Ordering::SeqCst);
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
        assert!(gate.acquire(true).await.is_some());
        assert!(!gate.full_running.load(Ordering::SeqCst));
    }
}
