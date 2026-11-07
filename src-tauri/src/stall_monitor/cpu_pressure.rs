//! System-wide CPU contention, read from the OS (#1966).
//!
//! The OS-thread probe alone is a weak witness to contention: on a saturated
//! machine a sleeping thread is still rescheduled within a few ms, while a
//! Tokio timer wake hops through more threads and runs late far more often.
//! So the monitor asks the OS directly how contended the CPU was:
//!
//! - **Linux**: pressure stall information (`/proc/pressure/cpu`) — the share
//!   of wall time at least one runnable task was waiting for a CPU. Absent on
//!   kernels built without PSI, in which case [`Sampler::sample`] stays `None`.
//! - **Windows**: the share of all cores' time spent busy (`GetSystemTimes`).
//!   Windows has no run-queue wait counter, so a near-full CPU stands in for it.
//! - **Elsewhere**: unsupported; [`Sampler::sample`] always returns `None`.

use std::fmt;

/// How contended the CPU was over one sampling interval, as a 0.0–1.0 share
/// whose meaning is platform-specific (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pressure(f32);

impl Pressure {
    #[cfg(test)]
    pub(super) const fn from_share(share: f32) -> Self {
        Self(share)
    }

    /// Whether this is enough contention to explain a Tokio stall. Linux's
    /// threshold is low because an idle desktop sits at a few percent and any
    /// sustained wait means threads are queueing; Windows' is high because
    /// busy share only implies queueing once the cores are nearly full.
    pub(super) fn is_contended(self) -> bool {
        self.0 >= if cfg!(windows) { 0.9 } else { 0.2 }
    }
}

impl fmt::Display for Pressure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let percent = self.0 * 100.0;
        if cfg!(windows) {
            write!(f, "the CPU was {percent:.0}% busy")
        } else {
            write!(f, "tasks waited for a CPU {percent:.0}% of the time")
        }
    }
}

/// Turns the OS's cumulative counters into per-interval [`Pressure`].
pub(super) struct Sampler {
    prev: Option<imp::Counters>,
}

impl Sampler {
    pub(super) fn new() -> Self {
        Self { prev: None }
    }

    /// Contention since the previous call. `None` on the first call, or
    /// whenever the platform doesn't expose a counter.
    pub(super) fn sample(&mut self) -> Option<Pressure> {
        let now = imp::read()?;
        let prev = self.prev.replace(now)?;
        imp::share(&prev, &now).map(|s| Pressure(s.clamp(0.0, 1.0)))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::time::Instant;

    #[derive(Clone, Copy)]
    pub(super) struct Counters {
        at: Instant,
        /// Cumulative µs during which some runnable task waited for a CPU.
        waiting_us: u64,
    }

    pub(super) fn read() -> Option<Counters> {
        let text = std::fs::read_to_string("/proc/pressure/cpu").ok()?;
        Some(Counters {
            at: Instant::now(),
            waiting_us: super::parse_psi_some_total(&text)?,
        })
    }

    pub(super) fn share(prev: &Counters, now: &Counters) -> Option<f32> {
        let wall_us = (now.at - prev.at).as_micros();
        (wall_us > 0)
            .then(|| now.waiting_us.saturating_sub(prev.waiting_us) as f32 / wall_us as f32)
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;

    /// Cumulative 100ns ticks across all cores. Kernel time includes idle.
    #[derive(Clone, Copy)]
    pub(super) struct Counters {
        idle: u64,
        kernel: u64,
        user: u64,
    }

    pub(super) fn read() -> Option<Counters> {
        let zero = || FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut idle, mut kernel, mut user) = (zero(), zero(), zero());
        // SAFETY: three valid, writable FILETIMEs; the call only fills them in.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        let ticks = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        Some(Counters {
            idle: ticks(&idle),
            kernel: ticks(&kernel),
            user: ticks(&user),
        })
    }

    pub(super) fn share(prev: &Counters, now: &Counters) -> Option<f32> {
        let total = now.kernel.saturating_sub(prev.kernel) + now.user.saturating_sub(prev.user);
        let idle = now.idle.saturating_sub(prev.idle);
        (total > 0).then(|| total.saturating_sub(idle) as f32 / total as f32)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod imp {
    #[derive(Clone, Copy)]
    pub(super) enum Counters {}

    pub(super) fn read() -> Option<Counters> {
        None
    }

    pub(super) fn share(prev: &Counters, _: &Counters) -> Option<f32> {
        match *prev {}
    }
}

/// The cumulative `total=` (µs) of the `some` line of a PSI file.
#[cfg(any(target_os = "linux", test))]
fn parse_psi_some_total(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("some "))?
        .split_whitespace()
        .find_map(|field| field.strip_prefix("total="))?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_some_total_and_ignores_full() {
        let psi = "some avg10=46.07 avg60=32.77 avg300=25.24 total=1109540485\n\
                   full avg10=0.00 avg60=0.00 avg300=0.00 total=0\n";
        assert_eq!(parse_psi_some_total(psi), Some(1_109_540_485));
        assert_eq!(parse_psi_some_total("full total=5\n"), None);
        assert_eq!(parse_psi_some_total(""), None);
    }

    #[test]
    fn first_sample_has_nothing_to_compare_against() {
        assert_eq!(Sampler::new().sample(), None);
    }
}
