import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toastStore } from "./toast.svelte";
import { TOAST_DURATION_MS } from "../constants";

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
    toastStore.show("Update ready", "success", undefined, undefined, {
      label: "Restart",
      onClick: vi.fn(),
    });

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

describe("ToastStore notification preferences", () => {
  const devices = toastStore.channel("devices");
  const texts = () => toastStore.messages.map((m) => m.text);

  beforeEach(() => {
    vi.useFakeTimers();
    toastStore.messages = [];
    toastStore.preferences = {};
    vi.mocked(invoke).mockClear();
  });

  it("tags plain toasts as confirmations and channel toasts with the channel's type", () => {
    toastStore.show("Added to Queue");
    devices.show("Synced Walkman", "success");
    toastStore.celebrate("Your Queue is done", "checks");
    toastStore.startTask({ taskId: "scan", text: "Scanning" });

    expect(toastStore.messages.map((m) => m.type)).toEqual([
      "confirmations",
      "devices",
      "celebrations",
      "tasks",
    ]);
  });

  it("hides a type that is off, and later calls on the hidden toast's id do nothing", () => {
    toastStore.setPreference("devices", "off");

    const id = devices.startBatch("Syncing (0/3)");
    toastStore.updateBatch(id, "Syncing (1/3)");
    devices.finishBatch(id, "Synced 3 songs");
    toastStore.show("Added to Queue");

    expect(texts()).toEqual(["Added to Queue"]);
  });

  it("removes toasts already on screen when their type is turned off", () => {
    devices.show("Synced Walkman", "success");
    devices.show("Walkman is full", "error");
    toastStore.show("Added to Queue");

    toastStore.setPreference("devices", "off");

    expect(texts()).toEqual(["Walkman is full", "Added to Queue"]);
  });

  it("never hides or times out errors and warnings", () => {
    toastStore.setPreference("devices", "off");
    devices.show("Sync failed", "error");
    toastStore.setPreference("confirmations", "low");
    toastStore.show("Duplicates found", "warning");

    vi.advanceTimersByTime(TOAST_DURATION_MS * 10);
    expect(texts()).toEqual(["Sync failed", "Duplicates found"]);
    expect(toastStore.messages.every((m) => !m.autoDismisses)).toBe(true);
  });

  it("low priority makes a toast that would stay auto-dismiss, keeping a caller's own duration", () => {
    toastStore.setPreference("devices", "low");
    devices.show("Synced Walkman", "success");
    devices.show("Ejected Walkman", "success", 1000);

    expect(toastStore.messages.every((m) => m.autoDismisses)).toBe(true);
    vi.advanceTimersByTime(1000);
    expect(texts()).toEqual(["Synced Walkman"]);
    vi.advanceTimersByTime(TOAST_DURATION_MS - 1000);
    expect(texts()).toEqual([]);
  });

  it("low priority keeps a running task up and auto-dismisses it once it completes", () => {
    toastStore.setPreference("tasks", "low");
    toastStore.startTask({ taskId: "scan", text: "Scanning" });

    vi.advanceTimersByTime(TOAST_DURATION_MS * 2);
    expect(texts()).toEqual(["Scanning"]);

    toastStore.completeTask("scan", "Scan complete");
    vi.advanceTimersByTime(TOAST_DURATION_MS);
    expect(texts()).toEqual([]);
  });

  it("high priority keeps a toast that would auto-dismiss until the user closes it", () => {
    toastStore.setPreference("confirmations", "high");
    toastStore.show("Theme exported", "success", 1000);

    vi.advanceTimersByTime(TOAST_DURATION_MS * 10);
    expect(texts()).toEqual(["Theme exported"]);
    expect(toastStore.messages[0].autoDismisses).toBe(false);
  });

  it("applies a new preference to toasts already on screen", () => {
    toastStore.show("Added to Queue");
    expect(toastStore.messages[0].autoDismisses).toBe(false);

    toastStore.setPreference("confirmations", "low");
    expect(toastStore.messages[0].autoDismisses).toBe(true);

    toastStore.setPreference("confirmations", "high");
    vi.advanceTimersByTime(TOAST_DURATION_MS * 2);
    expect(texts()).toEqual(["Added to Queue"]);
  });

  it("turnOff leaves a short notice whose Undo restores the previous preference", () => {
    toastStore.setPreference("devices", "low");
    devices.show("Synced Walkman", "success");

    toastStore.turnOff("devices");

    expect(toastStore.preferenceFor("devices")).toBe("off");
    expect(toastStore.messages).toHaveLength(1);
    const notice = toastStore.messages[0];
    expect(notice.text).toContain("Devices and sync");
    expect(notice.autoDismisses).toBe(true);

    notice.action!.onClick();
    expect(toastStore.preferenceFor("devices")).toBe("low");
    expect(toastStore.messages).toEqual([]);
  });

  it("saves the preferences as one app setting", () => {
    toastStore.setPreference("library", "low");
    toastStore.setPreference("celebrations", "off");

    expect(invoke).toHaveBeenLastCalledWith("set_app_setting", {
      key: "notification_preferences",
      value: JSON.stringify({ library: "low", celebrations: "off" }),
    });
  });

  it("restores saved preferences, ignoring unknown entries, and applies them to toasts already up", async () => {
    toastStore.channel("library").show("3 songs added", "success");
    vi.mocked(invoke).mockResolvedValueOnce({
      notification_preferences: JSON.stringify({
        library: "off",
        tasks: "loud",
        weather: "low",
      }),
    });

    await toastStore.loadPreferences();

    expect(toastStore.preferences).toEqual({ library: "off" });
    expect(texts()).toEqual([]);
  });

  it("treats a missing or corrupt saved value as all defaults", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ notification_preferences: "{not json" });
    await toastStore.loadPreferences();
    expect(toastStore.preferences).toEqual({});
  });
});
