import { invoke } from '@tauri-apps/api/core';

/**
 * Returns true for known benign engine warnings/events that should not be forwarded
 * to the crash log (e.g. ResizeObserver loop notices, ViewTransition aborts).
 */
export function isBenignFrontendError(message: string | undefined | null, error?: unknown): boolean {
  if (!message && !error) return false;
  const msg = message || (error instanceof Error ? error.message : String(error ?? ''));

  // Benign ResizeObserver loop notifications (dispatched to window.onerror by browser engine)
  // e.g. "ResizeObserver loop completed with undelivered notifications."
  // or "ResizeObserver loop limit exceeded"
  if (msg.includes('ResizeObserver loop')) {
    return true;
  }

  // Expected ViewTransition aborts from rapid navigation or user interaction
  // e.g. "Transition was skipped. New ViewTransition started" / "skipTransition() called"
  if (msg.includes('Transition was skipped') || msg.includes('skipTransition()')) {
    return true;
  }

  if (error && typeof error === 'object' && 'name' in error && (error as { name?: string }).name === 'AbortError') {
    const errorMsg = 'message' in error ? String((error as { message?: unknown }).message) : '';
    if (
      errorMsg.includes('Transition was skipped') ||
      errorMsg.includes('skipTransition()') ||
      errorMsg.toLowerCase().includes('transition')
    ) {
      return true;
    }
  }

  return false;
}

export type LogFrontendErrorSender = (message: string, stack?: string) => Promise<unknown>;

/**
 * Manages frontend error reporting with filtering of benign browser notices
 * and collapsing of identical consecutive errors to prevent IPC and log flooding (#1261).
 */
export class FrontendErrorReporter {
  private lastMessage: string | null = null;
  private lastStack: string | undefined = undefined;
  private repeatCount = 0;
  private lastReportedCount = 0;
  private flushTimeout: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private send: LogFrontendErrorSender = (message, stack) =>
      invoke('log_frontend_error', { message, stack })
  ) {}

  public report(message: string, stack?: string, error?: unknown) {
    if (isBenignFrontendError(message, error)) {
      return;
    }

    if (this.lastMessage === message && this.lastStack === stack) {
      this.repeatCount++;
      const hitMilestone =
        this.repeatCount === 10 ||
        this.repeatCount === 100 ||
        (this.repeatCount >= 1000 && this.repeatCount % 1000 === 0);

      if (hitMilestone && this.repeatCount > this.lastReportedCount) {
        this.lastReportedCount = this.repeatCount;
        void this.send(`(message repeated ${this.repeatCount} times)`).catch(() => {});
      } else {
        if (this.flushTimeout) clearTimeout(this.flushTimeout);
        this.flushTimeout = setTimeout(() => this.flush(), 1000);
      }
      return;
    }

    // Flush previous repeats if message differs
    this.flush();

    this.lastMessage = message;
    this.lastStack = stack;
    this.repeatCount = 1;
    this.lastReportedCount = 1;

    void this.send(message, stack).catch(() => {});
  }

  public flush() {
    if (this.flushTimeout) {
      clearTimeout(this.flushTimeout);
      this.flushTimeout = undefined;
    }
    if (this.repeatCount > this.lastReportedCount) {
      const count = this.repeatCount;
      this.lastReportedCount = count;
      void this.send(`(message repeated ${count} times)`).catch(() => {});
    }
  }

  public handleWindowError = (e: ErrorEvent) => {
    const message = e.message || (e.error instanceof Error ? e.error.message : String(e.error));
    this.report(message, e.error?.stack, e.error);
  };

  public handleUnhandledRejection = (e: PromiseRejectionEvent) => {
    const reason = e.reason;
    const message = reason instanceof Error ? reason.message : String(reason);
    const stack = reason instanceof Error ? reason.stack : undefined;
    this.report(message, stack, reason);
  };
}
