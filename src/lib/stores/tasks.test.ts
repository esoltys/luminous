import { describe, it, expect, beforeEach, vi } from "vitest";
import { tasksStore } from "./tasks.svelte";
import { toastStore } from "./toast.svelte";

describe("TasksStore", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    tasksStore.tasks = [];
  });

  it("starts a task with defaults and adds to activeTasks", () => {
    const id = tasksStore.startTask({ label: "Scanning library..." });
    expect(tasksStore.tasks.length).toBe(1);
    expect(tasksStore.activeTasks.length).toBe(1);
    expect(tasksStore.hasActiveTasks).toBe(true);
    expect(tasksStore.tasks[0].id).toBe(id);
    expect(tasksStore.tasks[0].status).toBe("running");
    expect(tasksStore.tasks[0].label).toBe("Scanning library...");
    expect(tasksStore.tasks[0].progress).toBeUndefined();
    expect(tasksStore.overallProgress).toBeNull();
  });

  it("calculates progress when total and current are provided", () => {
    const id = tasksStore.startTask({ id: "test-progress", label: "Importing tracks", total: 100 });
    expect(tasksStore.tasks[0].progress).toBe(0);

    tasksStore.updateTask(id, { current: 50 });
    expect(tasksStore.tasks[0].progress).toBe(0.5);
    expect(tasksStore.overallProgress).toBe(0.5);

    tasksStore.updateTask(id, { progress: 0.8 });
    expect(tasksStore.tasks[0].progress).toBe(0.8);
  });

  it("computes overallProgress across multiple active tasks", () => {
    tasksStore.startTask({ id: "task-1", label: "Task 1", total: 10 });
    tasksStore.startTask({ id: "task-2", label: "Task 2", total: 20 });

    tasksStore.updateTask("task-1", { current: 5 }); // 0.5
    tasksStore.updateTask("task-2", { current: 20 }); // 1.0

    // Average of 0.5 and 1.0 is 0.75
    expect(tasksStore.overallProgress).toBe(0.75);
  });

  it("completes a task, sets progress to 1, and auto ages out after timer", () => {
    const id = tasksStore.startTask({ label: "Quick operation" });
    tasksStore.completeTask(id, "Finished operation");

    expect(tasksStore.tasks[0].status).toBe("done");
    expect(tasksStore.tasks[0].progress).toBe(1);
    expect(tasksStore.tasks[0].label).toBe("Finished operation");
    expect(tasksStore.hasActiveTasks).toBe(false);
    expect(tasksStore.tasks.length).toBe(1);

    // Advance time by 8000ms
    vi.advanceTimersByTime(8000);
    expect(tasksStore.tasks.length).toBe(0);
  });

  it("marks failed task and clears on clearFinished", () => {
    const id1 = tasksStore.startTask({ label: "Task that will fail" });
    const id2 = tasksStore.startTask({ label: "Task that is running" });

    tasksStore.failTask(id1, "Network error");
    expect(tasksStore.tasks.find((t) => t.id === id1)?.status).toBe("failed");
    expect(tasksStore.tasks.find((t) => t.id === id1)?.error).toBe("Network error");

    tasksStore.clearFinished();
    expect(tasksStore.tasks.length).toBe(1);
    expect(tasksStore.tasks[0].id).toBe(id2);
  });

  it("cancels task and checks isTaskActive correctly", () => {
    const id = tasksStore.startTask({ label: "Task to cancel" });
    expect(tasksStore.isTaskActive(id)).toBe(true);

    tasksStore.cancelTask(id);
    expect(tasksStore.isTaskActive(id)).toBe(false);
    expect(tasksStore.tasks[0].status).toBe("cancelled");
  });

  it("tracks and updates contextName on tasks", () => {
    const id = tasksStore.startTask({ label: "Scanning...", contextName: "My Library" });
    expect(tasksStore.tasks[0].contextName).toBe("My Library");

    tasksStore.updateTask(id, { contextName: "Fast SSD" });
    expect(tasksStore.tasks[0].contextName).toBe("Fast SSD");
  });

  it("strips parenthetical counts followed by trailing dots or ellipsis when extracting taskName", () => {
    const id1 = tasksStore.startTask({ label: "Processing songs (0/1)..." });
    const id2 = tasksStore.startTask({ label: "Обработка песен (0/2)…" });
    const id3 = tasksStore.startTask({ label: "Titel werden verarbeitet (1/5) ..." });
    const id4 = tasksStore.startTask({ label: "Explicit task name (0/1)...", taskName: "Explicit" });

    // Toast messages verify the extracted taskName passed through
    const msg1 = toastStore.messages.find((m: any) => m.task?.taskId === id1);
    const msg2 = toastStore.messages.find((m: any) => m.task?.taskId === id2);
    const msg3 = toastStore.messages.find((m: any) => m.task?.taskId === id3);
    const msg4 = toastStore.messages.find((m: any) => m.task?.taskId === id4);

    expect(msg1?.task?.taskName).toBe("Processing songs");
    expect(msg2?.task?.taskName).toBe("Обработка песен");
    expect(msg3?.task?.taskName).toBe("Titel werden verarbeitet");
    expect(msg4?.task?.taskName).toBe("Explicit");
  });

  it("coalesces successive task starts under the same id and resets ageOut timer", () => {
    const taskId = "watcher-batch";
    tasksStore.startTask({ id: taskId, label: "Processing songs (0/1)...", total: 1 });
    expect(tasksStore.tasks.length).toBe(1);
    expect(tasksStore.tasks[0].status).toBe("running");

    tasksStore.completeTask(taskId, "1 song updated", 2000);
    expect(tasksStore.tasks[0].status).toBe("done");

    // Advance 1000ms (within age-out window)
    vi.advanceTimersByTime(1000);
    expect(tasksStore.tasks.length).toBe(1);

    // Subsequent batch arrives while previous completed task is aging out
    tasksStore.startTask({ id: taskId, label: "Processing songs (0/2)...", total: 2 });
    expect(tasksStore.tasks.length).toBe(1);
    expect(tasksStore.tasks[0].status).toBe("running");
    expect(tasksStore.tasks[0].total).toBe(2);

    // Advance 1500ms (original 2000ms would have passed, but age-out was cancelled)
    vi.advanceTimersByTime(1500);
    expect(tasksStore.tasks.length).toBe(1);
    expect(tasksStore.tasks[0].status).toBe("running");

    // Complete the second batch
    tasksStore.completeTask(taskId, "2 songs updated", 2000);
    expect(tasksStore.tasks[0].status).toBe("done");

    // Advance 2000ms to allow age-out
    vi.advanceTimersByTime(2000);
    expect(tasksStore.tasks.length).toBe(0);
  });

  it("supports custom ageOutMs on completeTask", () => {
    const id = tasksStore.startTask({ label: "Micro batch", total: 1 });
    tasksStore.completeTask(id, "Done", 2000);

    vi.advanceTimersByTime(1999);
    expect(tasksStore.tasks.length).toBe(1);

    vi.advanceTimersByTime(2);
    expect(tasksStore.tasks.length).toBe(0);
  });
});
