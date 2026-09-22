import { toastStore } from "./toast.svelte";

type TaskStatus = "pending" | "running" | "done" | "failed" | "cancelled";

interface TrackedTask {
  id: string;
  label: string;
  status: TaskStatus;
  progress?: number; // 0..1 (determinate fraction)
  current?: number;
  total?: number;
  contextName?: string;
  error?: string;
  createdAt: number;
  finishedAt?: number;
}

interface StartTaskOptions {
  id?: string;
  label: string;
  taskName?: string;
  total?: number;
  contextName?: string;
}

interface UpdateTaskOptions {
  progress?: number;
  current?: number;
  total?: number;
  label?: string;
  status?: TaskStatus;
  error?: string;
}

const DEFAULT_AGE_OUT_MS = 8000;

class TasksStore {
  tasks = $state<TrackedTask[]>([]);
  private ageOutTimers = new Map<string, ReturnType<typeof setTimeout>>();

  activeTasks = $derived.by(() => {
    return this.tasks.filter((t) => t.status === "running" || t.status === "pending");
  });

  hasActiveTasks = $derived.by(() => {
    return this.activeTasks.length > 0;
  });

  recentTasks = $derived.by(() => {
    return this.tasks;
  });

  overallProgress = $derived.by(() => {
    const runningWithProgress = this.activeTasks.filter(
      (t) => typeof t.progress === "number" && !isNaN(t.progress)
    );
    if (runningWithProgress.length === 0) return null;
    const sum = runningWithProgress.reduce((acc, t) => acc + (t.progress ?? 0), 0);
    return sum / runningWithProgress.length;
  });

  startTask(options: StartTaskOptions): string {
    const id = options.id || `task-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    this.clearAgeOutTimer(id);

    const existingIndex = this.tasks.findIndex((t) => t.id === id);
    const task: TrackedTask = {
      id,
      label: options.label,
      status: "running",
      total: options.total,
      current: options.total !== undefined ? 0 : undefined,
      progress: options.total !== undefined && options.total > 0 ? 0 : undefined,
      contextName: options.contextName,
      createdAt: Date.now(),
    };

    if (existingIndex >= 0) {
      this.tasks[existingIndex] = task;
    } else {
      this.tasks.push(task);
    }
    const taskName = options.taskName || options.label.replace(/\s*\([^)]*\)\s*$/, "").trim() || options.label;
    toastStore.startTask({
      taskId: id,
      text: options.label,
      taskName,
      total: options.total,
      current: task.current,
      progress: task.progress,
    });
    return id;
  }

  updateTask(id: string, update: UpdateTaskOptions) {
    const task = this.tasks.find((t) => t.id === id);
    if (!task) return;

    if (update.label !== undefined) task.label = update.label;
    if (update.status !== undefined) task.status = update.status;
    if (update.error !== undefined) task.error = update.error;
    if (update.current !== undefined) task.current = update.current;
    if (update.total !== undefined) task.total = update.total;

    if (update.progress !== undefined) {
      task.progress = Math.max(0, Math.min(1, update.progress));
    } else if (task.total !== undefined && task.total > 0 && task.current !== undefined) {
      task.progress = Math.max(0, Math.min(1, task.current / task.total));
    }

    toastStore.updateTask(id, {
      text: task.label,
      current: task.current,
      total: task.total,
      progress: task.progress,
    });

    if (task.status === "done" || task.status === "failed" || task.status === "cancelled") {
      if (!task.finishedAt) task.finishedAt = Date.now();
      this.scheduleAgeOut(id);
    }
  }

  completeTask(id: string, label?: string) {
    const task = this.tasks.find((t) => t.id === id);
    if (!task) return;
    if (label && label !== "Done") task.label = label;
    task.status = "done";
    task.progress = 1;
    if (task.total !== undefined) task.current = task.total;
    task.finishedAt = Date.now();
    toastStore.completeTask(id, label);
    this.scheduleAgeOut(id);
  }

  failTask(id: string, error?: string) {
    const task = this.tasks.find((t) => t.id === id);
    if (!task) return;
    task.status = "failed";
    if (error) task.error = error;
    task.finishedAt = Date.now();
    toastStore.failTask(id, error || "Task failed");
    this.scheduleAgeOut(id);
  }

  cancelTask(id: string) {
    const task = this.tasks.find((t) => t.id === id);
    if (!task) return;
    task.status = "cancelled";
    task.finishedAt = Date.now();
    toastStore.dismissTask(id);
    this.scheduleAgeOut(id);
  }

  clearTask(id: string) {
    this.clearAgeOutTimer(id);
    toastStore.dismissTask(id);
    this.tasks = this.tasks.filter((t) => t.id !== id);
  }

  clearFinished() {
    const finishedIds = this.tasks
      .filter((t) => t.status === "done" || t.status === "failed" || t.status === "cancelled")
      .map((t) => t.id);
    for (const id of finishedIds) {
      this.clearAgeOutTimer(id);
    }
    this.tasks = this.tasks.filter((t) => t.status === "running" || t.status === "pending");
  }

  isTaskActive(id: string): boolean {
    const task = this.tasks.find((t) => t.id === id);
    return task?.status === "running" || task?.status === "pending";
  }

  scheduleAgeOut(id: string, delayMs: number = DEFAULT_AGE_OUT_MS) {
    this.clearAgeOutTimer(id);
    const timer = setTimeout(() => {
      this.clearTask(id);
    }, delayMs);
    this.ageOutTimers.set(id, timer);
  }

  private clearAgeOutTimer(id: string) {
    const timer = this.ageOutTimers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.ageOutTimers.delete(id);
    }
  }
}

export const tasksStore = new TasksStore();
