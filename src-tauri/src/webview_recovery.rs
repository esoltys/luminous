//! Automatic crash detection and recovery for the WebView2 rendering engine (#1738).
//!
//! When Microsoft Edge WebView2's render process terminates or hangs on Windows
//! (e.g. GPU crash, canvas command overflow, memory pressure, or profile lock
//! collision on quick relaunch), WebView2 removes the child rendering HWND.
//! Because Luminous's audio engine runs on native OS threads, playback continues
//! while the window remains visibly blank.
//!
//! This module registers an [`ICoreWebView2ProcessFailedEventHandler`]. For
//! recoverable process failures (renderer crash or unresponsive), it logs the
//! event details to `crash.log` and reloads the webview via `CoreWebView2.Reload()`,
//! seamlessly reviving the UI. A rate-limiting circuit breaker prevents infinite
//! reload loops if a catastrophic failure persists across reloads.

use std::path::Path;
use std::time::{Duration, Instant};

/// Maximum automatic recovery reloads permitted within [`CIRCUIT_BREAKER_WINDOW`]
/// before halting automatic recovery to avoid an infinite loop.
pub const MAX_FAILURES_IN_WINDOW: usize = 3;

/// Time window within which repeated failures trip the circuit breaker.
pub const CIRCUIT_BREAKER_WINDOW: Duration = Duration::from_secs(10);

/// Protects against infinite crash/reload loops by limiting the number of
/// automatic recovery reloads within a sliding time window.
#[derive(Debug)]
pub(crate) struct CrashCircuitBreaker {
    recent_failures: Vec<Instant>,
    max_failures: usize,
    window: Duration,
}

impl CrashCircuitBreaker {
    pub(crate) fn new(max_failures: usize, window: Duration) -> Self {
        Self {
            recent_failures: Vec::with_capacity(max_failures + 1),
            max_failures,
            window,
        }
    }

    /// Records a failure at the given timestamp and determines whether recovery
    /// is permitted. Returns `true` if recovery should proceed, or `false` if the
    /// circuit breaker has tripped (too many failures within the sliding window).
    pub(crate) fn try_acquire_recovery(&mut self, now: Instant) -> bool {
        self.recent_failures
            .retain(|&t| now.duration_since(t) < self.window);
        self.recent_failures.push(now);
        self.recent_failures.len() <= self.max_failures
    }
}

impl Default for CrashCircuitBreaker {
    fn default() -> Self {
        Self::new(MAX_FAILURES_IN_WINDOW, CIRCUIT_BREAKER_WINDOW)
    }
}

#[cfg(target_os = "windows")]
use parking_lot::Mutex;
#[cfg(target_os = "windows")]
use std::sync::Arc;
#[cfg(target_os = "windows")]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2ProcessFailedEventArgs2, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_BROKER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_PLUGIN_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
        COREWEBVIEW2_PROCESS_FAILED_KIND_SANDBOX_HELPER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_REASON,
    },
    ProcessFailedEventHandler,
};
#[cfg(target_os = "windows")]
use windows::core::Interface;

/// Recovery action to take for a specific WebView2 process failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessFailureAction {
    /// Render or GPU process failed/hung: UI is blank or frozen, reload restores it.
    Reload,
    /// Utility or background worker process exited: WebView2 automatically recreates it.
    SelfHealing,
    /// The parent browser process exited: webview object is destroyed, manual app restart required.
    Fatal,
}

#[cfg(target_os = "windows")]
pub(crate) fn format_process_failed_kind(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> &'static str {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => "BrowserProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => "RenderProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => "RenderProcessUnresponsive",
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => "FrameRenderProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => "GpuProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED => "UtilityProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_SANDBOX_HELPER_PROCESS_EXITED => {
            "SandboxHelperProcessExited"
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_PLUGIN_PROCESS_EXITED => "PpapiPluginProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_BROKER_PROCESS_EXITED => "PpapiBrokerProcessExited",
        COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED => "UnknownProcessExited",
        _ => "OtherProcessFailed",
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn classify_process_failure(
    kind: COREWEBVIEW2_PROCESS_FAILED_KIND,
) -> ProcessFailureAction {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
        | COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => ProcessFailureAction::Reload,

        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_SANDBOX_HELPER_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_PLUGIN_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_PPAPI_BROKER_PROCESS_EXITED => {
            ProcessFailureAction::SelfHealing
        }

        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
        | COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED
        | _ => ProcessFailureAction::Fatal,
    }
}

/// Configures automatic crash detection and recovery for the application's
/// webview rendering engine on Windows.
///
/// Hooks WebView2's `ProcessFailed` event on the main webview. When a
/// renderer or GPU process terminates or hangs, automatically logs the failure
/// reason and reloads the webview to restore the UI seamlessly. Includes a
/// circuit breaker to guard against runaway crash-reload loops.
///
/// On non-Windows platforms, this is an inert no-op.
#[cfg(target_os = "windows")]
pub fn setup_webview_recovery(app: &tauri::AppHandle, app_data_dir: &Path) {
    use tauri::Manager;

    let Some(webview) = app.get_webview("main") else {
        log::debug!("Main webview not found during webview recovery setup");
        return;
    };

    let breaker = Arc::new(Mutex::new(CrashCircuitBreaker::default()));
    let app_data_dir_buf = app_data_dir.to_path_buf();

    let res = webview.with_webview(move |platform_webview| {
        let controller = platform_webview.controller();
        let core = match unsafe { controller.CoreWebView2() } {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to get CoreWebView2 from controller: {e}");
                return;
            }
        };

        let app_data_dir_clone = app_data_dir_buf.clone();
        let breaker_clone = Arc::clone(&breaker);

        let handler = ProcessFailedEventHandler::create(Box::new(move |core_opt, args_opt| {
            let Some(args) = args_opt else {
                log::warn!("WebView2 ProcessFailed invoked with null args");
                return Ok(());
            };

            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
            unsafe {
                let _ = args.ProcessFailedKind(&mut kind);
            }

            let kind_str = format_process_failed_kind(kind);
            let (failure_reason, exit_code) =
                if let Ok(args2) = args.cast::<ICoreWebView2ProcessFailedEventArgs2>() {
                    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON(0);
                    let mut code = 0i32;
                    unsafe {
                        let _ = args2.Reason(&mut reason);
                        let _ = args2.ExitCode(&mut code);
                    }
                    (Some(reason.0), Some(code))
                } else {
                    (None, None)
                };

            let detail = format!(
                "WebView2 process failed: kind={kind_str} (code={}), reason={failure_reason:?}, exit_code={exit_code:?}",
                kind.0
            );

            log::warn!("{detail}");
            crate::diagnostics::log_webview_error(&app_data_dir_clone, &detail);

            match classify_process_failure(kind) {
                ProcessFailureAction::Reload => {
                    let mut guard = breaker_clone.lock();
                    if guard.try_acquire_recovery(Instant::now()) {
                        log::info!("Attempting automatic WebView2 reload recovery ({kind_str})...");
                        if let Some(core_ref) = core_opt {
                            unsafe {
                                if let Err(e) = core_ref.Reload() {
                                    log::error!(
                                        "Failed to reload WebView2 after process failure: {e}"
                                    );
                                } else {
                                    log::info!("CoreWebView2.Reload() dispatched successfully");
                                }
                            }
                        }
                    } else {
                        let alert = "WebView2 crash circuit breaker tripped (>3 failures in 10s); halting automatic reload";
                        log::error!("{alert}");
                        crate::diagnostics::log_webview_error(&app_data_dir_clone, alert);
                    }
                }
                ProcessFailureAction::SelfHealing => {
                    log::info!(
                        "WebView2 background process exited ({kind_str}); lifecycle managed automatically by WebView2"
                    );
                }
                ProcessFailureAction::Fatal => {
                    let alert = format!(
                        "Unrecoverable WebView2 process failure ({kind_str}); manual restart required"
                    );
                    log::error!("{alert}");
                    crate::diagnostics::log_webview_error(&app_data_dir_clone, &alert);
                }
            }

            Ok(())
        }));

        let mut token = 0i64;
        unsafe {
            if let Err(e) = core.add_ProcessFailed(&handler, &mut token) {
                log::warn!("Failed to register WebView2 ProcessFailed event handler: {e}");
            } else {
                log::debug!("Successfully registered WebView2 ProcessFailed event handler");
            }
        }
    });

    if let Err(e) = res {
        log::debug!("Could not attach webview recovery handler: {e}");
    }
}

/// Non-Windows stub for webview crash recovery.
#[cfg(not(target_os = "windows"))]
pub fn setup_webview_recovery(_app: &tauri::AppHandle, _app_data_dir: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_allows_up_to_max_failures() {
        let mut breaker = CrashCircuitBreaker::new(3, Duration::from_secs(10));
        let start = Instant::now();

        assert!(breaker.try_acquire_recovery(start));
        assert!(breaker.try_acquire_recovery(start + Duration::from_secs(1)));
        assert!(breaker.try_acquire_recovery(start + Duration::from_secs(2)));

        // 4th failure within the 10-second window should trip
        assert!(!breaker.try_acquire_recovery(start + Duration::from_secs(3)));
        // 5th failure still tripped
        assert!(!breaker.try_acquire_recovery(start + Duration::from_secs(4)));
    }

    #[test]
    fn test_circuit_breaker_slides_window() {
        let mut breaker = CrashCircuitBreaker::new(2, Duration::from_secs(5));
        let start = Instant::now();

        assert!(breaker.try_acquire_recovery(start));
        assert!(breaker.try_acquire_recovery(start + Duration::from_secs(1)));
        // 3rd failure at sec 2 trips
        assert!(!breaker.try_acquire_recovery(start + Duration::from_secs(2)));

        // Advance beyond window (start + 6s) — earlier failures (at 0s and 1s) drop off
        assert!(breaker.try_acquire_recovery(start + Duration::from_secs(6)));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_windows_process_failure_classification() {
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED),
            ProcessFailureAction::Reload
        );
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE),
            ProcessFailureAction::Reload
        );
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED),
            ProcessFailureAction::Reload
        );
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED),
            ProcessFailureAction::Reload
        );
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED),
            ProcessFailureAction::SelfHealing
        );
        assert_eq!(
            classify_process_failure(
                COREWEBVIEW2_PROCESS_FAILED_KIND_SANDBOX_HELPER_PROCESS_EXITED
            ),
            ProcessFailureAction::SelfHealing
        );
        assert_eq!(
            classify_process_failure(COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED),
            ProcessFailureAction::Fatal
        );
    }
}
