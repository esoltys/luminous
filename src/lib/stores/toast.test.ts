import { describe, it, expect, beforeEach, vi } from "vitest";
import { toastStore } from "./toast.svelte";

describe("ToastStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    for (const m of [...toastStore.messages]) toastStore.dismiss(m.id);
  });

  it("update() mutates an existing toast's text/variant in place without adding a new one", () => {
    const id = toastStore.show("Processing songs (1/3)...", "info");
    expect(toastStore.messages).toHaveLength(1);

    toastStore.update(id, "Processing songs (2/3)...");
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].text).toBe("Processing songs (2/3)...");
    expect(toastStore.messages[0].variant).toBe("info");

    toastStore.update(id, "Done", "success");
    expect(toastStore.messages[0].text).toBe("Done");
    expect(toastStore.messages[0].variant).toBe("success");
  });

  it("update() is a no-op for an id that no longer exists", () => {
    toastStore.update(999999, "should not throw");
    expect(toastStore.messages).toHaveLength(0);
  });

  it("startBatch/updateBatch/finishBatch collapse a whole batch into a single toast", () => {
    const id = toastStore.startBatch("Processing songs (0/5)...");
    expect(toastStore.messages).toHaveLength(1);

    toastStore.updateBatch(id, "Processing songs (3/5)...");
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].text).toBe("Processing songs (3/5)...");

    toastStore.finishBatch(id, "5 song(s) updated", "success", 1000);
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].text).toBe("5 song(s) updated");
    expect(toastStore.messages[0].variant).toBe("success");

    vi.advanceTimersByTime(1000);
    expect(toastStore.messages).toHaveLength(0);
  });

  it("finishBatch shows a fresh toast if the batch toast was already dismissed", () => {
    const id = toastStore.startBatch("Processing songs (0/1)...");
    toastStore.dismiss(id);
    expect(toastStore.messages).toHaveLength(0);

    toastStore.finishBatch(id, "1 song(s) updated", "success", 1000);
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].text).toBe("1 song(s) updated");
  });

  it("dismiss() clears any pending auto-dismiss timer so it doesn't fire on a stale id", () => {
    const id = toastStore.show("temp", "info", 500);
    toastStore.dismiss(id);
    expect(() => vi.advanceTimersByTime(500)).not.toThrow();
    expect(toastStore.messages).toHaveLength(0);
  });

  it("show() stores an optional action and invoking its onClick calls the provided callback", () => {
    const onClick = vi.fn();
    toastStore.show("Update ready", "success", undefined, undefined, { label: "Restart", onClick });

    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].action).toEqual({ label: "Restart", onClick });

    toastStore.messages[0].action!.onClick();
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("a toast shown without durationMs persists indefinitely, action or not", () => {
    toastStore.show("Update ready", "success", undefined, undefined, { label: "Restart", onClick: vi.fn() });

    vi.advanceTimersByTime(10 * 60 * 1000);

    expect(toastStore.messages).toHaveLength(1);
  });

  it("startTask strips trailing dots/ellipsis from label and reactivates existing tasks without duplicate toasts", () => {
    const id = toastStore.startTask({
      taskId: "watcher-batch",
      text: "Processing songs (0/1)...",
      total: 1,
    });

    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].task?.taskName).toBe("Processing songs");
    expect(toastStore.messages[0].task?.status).toBe("running");

    // Complete the task with 2000ms duration
    toastStore.completeTask("watcher-batch", "1 song updated", 2000);
    expect(toastStore.messages[0].task?.status).toBe("done");

    // Advance 1000ms
    vi.advanceTimersByTime(1000);
    expect(toastStore.messages).toHaveLength(1);

    // Restart the task for next batch
    const reactivatedId = toastStore.startTask({
      taskId: "watcher-batch",
      text: "Processing songs (0/2)...",
      total: 2,
    });

    expect(reactivatedId).toBe(id);
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].task?.status).toBe("running");
    expect(toastStore.messages[0].task?.taskName).toBe("Processing songs");
    expect(toastStore.messages[0].text).toBe("Processing songs (0/2)...");

    // Advance 1500ms — dismiss timer was cleared so toast remains visible
    vi.advanceTimersByTime(1500);
    expect(toastStore.messages).toHaveLength(1);
    expect(toastStore.messages[0].task?.status).toBe("running");
  });
});
