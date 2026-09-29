import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { isBenignFrontendError, FrontendErrorReporter } from './frontendError';

describe('isBenignFrontendError', () => {
  it('identifies benign ResizeObserver loop notices', () => {
    expect(
      isBenignFrontendError('ResizeObserver loop completed with undelivered notifications.')
    ).toBe(true);
    expect(isBenignFrontendError('ResizeObserver loop limit exceeded')).toBe(true);
    expect(
      isBenignFrontendError('Error: ResizeObserver loop completed with undelivered notifications.')
    ).toBe(true);
  });

  it('identifies benign ViewTransition abort errors', () => {
    expect(
      isBenignFrontendError('Transition was skipped. New ViewTransition started')
    ).toBe(true);
    expect(
      isBenignFrontendError('Transition was skipped: skipTransition() called.')
    ).toBe(true);
    expect(isBenignFrontendError('skipTransition() called')).toBe(true);

    const abortError = new DOMException('Transition was skipped', 'AbortError');
    expect(isBenignFrontendError('', abortError)).toBe(true);
  });

  it('does not filter actionable real errors', () => {
    expect(isBenignFrontendError('TypeError: Cannot read properties of undefined (reading "id")')).toBe(false);
    expect(isBenignFrontendError('ReferenceError: player is not defined')).toBe(false);
    expect(isBenignFrontendError('Unhandled Promise Rejection: network error')).toBe(false);
    expect(isBenignFrontendError('', new Error('Database locked'))).toBe(false);
    expect(isBenignFrontendError(undefined, undefined)).toBe(false);
  });
});

describe('FrontendErrorReporter', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('drops benign errors without invoking send', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    reporter.report('ResizeObserver loop completed with undelivered notifications.');
    reporter.report('Transition was skipped. New ViewTransition started');

    expect(send).not.toHaveBeenCalled();
  });

  it('forwards normal errors to send', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    reporter.report('Failed to load track', 'at player.ts:42');
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith('Failed to load track', 'at player.ts:42');
  });

  it('bounds calls during a burst of identical errors to milestones', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    // Send 100 identical errors
    for (let i = 0; i < 100; i++) {
      reporter.report('Render loop failed', 'at component.svelte:10');
    }

    // Milestones hit: 1 (first), 10, 100 => 3 total calls instead of 100!
    expect(send).toHaveBeenCalledTimes(3);
    expect(send).toHaveBeenNthCalledWith(1, 'Render loop failed', 'at component.svelte:10');
    expect(send).toHaveBeenNthCalledWith(2, '(message repeated 10 times)');
    expect(send).toHaveBeenNthCalledWith(3, '(message repeated 100 times)');
  });

  it('flushes pending repeats when a different error arrives', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    // 5 identical errors
    for (let i = 0; i < 5; i++) {
      reporter.report('First error');
    }
    // Only 1 call so far (first error)
    expect(send).toHaveBeenCalledTimes(1);

    // Different error arrives
    reporter.report('Second error');

    // Should flush the repeat count for the first error, then send second error
    expect(send).toHaveBeenCalledTimes(3);
    expect(send).toHaveBeenNthCalledWith(2, '(message repeated 5 times)');
    expect(send).toHaveBeenNthCalledWith(3, 'Second error', undefined);
  });

  it('flushes pending repeats on debounce timeout', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    for (let i = 0; i < 4; i++) {
      reporter.report('Flaky network call');
    }
    expect(send).toHaveBeenCalledTimes(1);

    // Fast-forward debounce timer
    vi.advanceTimersByTime(1500);

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenNthCalledWith(2, '(message repeated 4 times)');
  });

  it('handles window ErrorEvent and unhandledrejection', () => {
    const send = vi.fn().mockResolvedValue(undefined);
    const reporter = new FrontendErrorReporter(send);

    const errorEvent = {
      message: 'Uncaught TypeError: test',
      error: new Error('test'),
    } as unknown as ErrorEvent;
    reporter.handleWindowError(errorEvent);

    const rejectionEvent = {
      reason: new Error('Promise rejected'),
    } as unknown as PromiseRejectionEvent;
    reporter.handleUnhandledRejection(rejectionEvent);

    expect(send).toHaveBeenCalledTimes(2);
  });
});
