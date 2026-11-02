import { describe, it, expect, vi, beforeEach } from "vitest";
import { createEventScope, EventScope, type HmrContext } from "./eventScope";
import { listen } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

describe("EventScope", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("forwards listen calls to Tauri's listen function", async () => {
    const unlistenMock = vi.fn();
    vi.mocked(listen).mockResolvedValueOnce(unlistenMock);

    const scope = createEventScope();
    const handler = vi.fn();
    const resultUnlisten = await scope.listen("test-event", handler);

    expect(listen).toHaveBeenCalledWith("test-event", handler);
    expect(resultUnlisten).toBe(unlistenMock);
  });

  it("unregisters all listeners when dispose() is called", async () => {
    const unlisten1 = vi.fn();
    const unlisten2 = vi.fn();
    vi.mocked(listen)
      .mockResolvedValueOnce(unlisten1)
      .mockResolvedValueOnce(unlisten2);

    const scope = createEventScope();
    await scope.listen("event-1", vi.fn());
    await scope.listen("event-2", vi.fn());

    expect(unlisten1).not.toHaveBeenCalled();
    expect(unlisten2).not.toHaveBeenCalled();

    scope.dispose();

    expect(unlisten1).toHaveBeenCalledTimes(1);
    expect(unlisten2).toHaveBeenCalledTimes(1);
  });

  it("handles in-flight listen promises when disposed before resolution", async () => {
    let resolvePromise: (fn: () => void) => void;
    const pendingPromise = new Promise<() => void>((resolve) => {
      resolvePromise = resolve;
    });
    vi.mocked(listen).mockReturnValueOnce(pendingPromise as any);

    const unlistenMock = vi.fn();
    const scope = createEventScope();

    // Start listening but don't await resolution yet
    const listenCall = scope.listen("async-event", vi.fn());

    // Dispose while promise is still pending
    scope.dispose();
    expect(unlistenMock).not.toHaveBeenCalled();

    // Now resolve the promise
    resolvePromise!(unlistenMock);
    await listenCall;

    // The unlistener must have been called as soon as the promise resolved
    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it("does not call Tauri listen if scope is already disposed", async () => {
    const scope = createEventScope();
    scope.dispose();

    const handler = vi.fn();
    const unlisten = await scope.listen("post-dispose-event", handler);

    expect(listen).not.toHaveBeenCalled();
    expect(typeof unlisten).toBe("function");
    // Calling the returned no-op should not throw
    expect(() => unlisten()).not.toThrow();
  });

  it("is idempotent on repeated dispose() calls", async () => {
    const unlistenMock = vi.fn();
    vi.mocked(listen).mockResolvedValueOnce(unlistenMock);

    const scope = createEventScope();
    await scope.listen("event", vi.fn());

    scope.dispose();
    scope.dispose();
    scope.dispose();

    expect(unlistenMock).toHaveBeenCalledTimes(1);
  });

  it("automatically binds to Vite HMR hot.dispose", () => {
    let hmrDisposeCallback: (() => void) | undefined;
    const hotMock: HmrContext = {
      dispose: vi.fn((cb) => {
        hmrDisposeCallback = cb;
      }),
    };

    const scope = createEventScope(hotMock);
    expect(hotMock.dispose).toHaveBeenCalledTimes(1);

    const disposeSpy = vi.spyOn(scope, "dispose");
    expect(disposeSpy).not.toHaveBeenCalled();

    // Trigger HMR disposal callback
    hmrDisposeCallback?.();
    expect(disposeSpy).toHaveBeenCalledTimes(1);
  });

  it("continues disposing other unlisteners if one unlistener throws", async () => {
    const brokenUnlisten = vi.fn(() => {
      throw new Error("Broken unlistener");
    });
    const goodUnlisten = vi.fn();

    vi.mocked(listen)
      .mockResolvedValueOnce(brokenUnlisten)
      .mockResolvedValueOnce(goodUnlisten);

    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const scope = createEventScope();
    await scope.listen("event-1", vi.fn());
    await scope.listen("event-2", vi.fn());

    scope.dispose();

    expect(brokenUnlisten).toHaveBeenCalledTimes(1);
    expect(goodUnlisten).toHaveBeenCalledTimes(1);
    expect(consoleErrorSpy).toHaveBeenCalled();

    consoleErrorSpy.mockRestore();
  });

  it("disposes child scopes when parent scope is disposed", async () => {
    const parentUnlisten = vi.fn();
    const childUnlisten = vi.fn();

    vi.mocked(listen)
      .mockResolvedValueOnce(parentUnlisten)
      .mockResolvedValueOnce(childUnlisten);

    const parent = createEventScope();
    const child = parent.createChild();

    await parent.listen("parent-event", vi.fn());
    await child.listen("child-event", vi.fn());

    parent.dispose();

    expect(parentUnlisten).toHaveBeenCalledTimes(1);
    expect(childUnlisten).toHaveBeenCalledTimes(1);
  });

  it("allows child scope to be disposed independently without disposing parent", async () => {
    const parentUnlisten = vi.fn();
    const childUnlisten = vi.fn();

    vi.mocked(listen)
      .mockResolvedValueOnce(parentUnlisten)
      .mockResolvedValueOnce(childUnlisten);

    const parent = createEventScope();
    const child = parent.createChild();

    await parent.listen("parent-event", vi.fn());
    await child.listen("child-event", vi.fn());

    child.dispose();

    expect(childUnlisten).toHaveBeenCalledTimes(1);
    expect(parentUnlisten).not.toHaveBeenCalled();

    parent.dispose();

    expect(parentUnlisten).toHaveBeenCalledTimes(1);
    // Child unlisten should not be called again
    expect(childUnlisten).toHaveBeenCalledTimes(1);
  });

  it("prunes child scope teardown from parent when child is disposed early", () => {
    const parent = createEventScope() as any;
    const child1 = parent.createChild();
    const child2 = parent.createChild();

    expect(parent.unlisteners.length).toBe(2);

    child1.dispose();
    expect(parent.unlisteners.length).toBe(1);

    child2.dispose();
    expect(parent.unlisteners.length).toBe(0);
  });
});
