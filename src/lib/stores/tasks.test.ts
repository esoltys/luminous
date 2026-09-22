import { describe, it, expect, beforeEach, vi } from "vitest";
import { tasksStore } from "./tasks.svelte";

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
});
