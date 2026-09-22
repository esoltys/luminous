import { TOAST_DURATION_MS } from "../constants";

type ToastVariant = "info" | "error" | "success" | "milestone" | "warning" | "task";

interface ToastAction {
  label: string;
  onClick: () => void;
}

interface ToastTaskData {
  taskId: string;
  taskName: string;
  status: "running" | "done" | "failed";
  current?: number;
  total?: number;
  progress?: number;
  completedText?: string;
  error?: string;
}

interface ToastMessage {
  id: number;
  text: string;
  variant: ToastVariant;
  url?: string;
  action?: ToastAction;
  task?: ToastTaskData;
}

class ToastStore {
  messages = $state<ToastMessage[]>([]);
  private nextId = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  /**
   * Start or update a long-lived task notification in the toast stack.
   * Stays visible until completion.
   */
  startTask(options: {
    taskId: string;
    text: string;
    taskName?: string;
    total?: number;
    current?: number;
    progress?: number;
  }): number {
    const existing = this.messages.find((m) => m.task?.taskId === options.taskId);
    const progress = typeof options.progress === "number"
      ? options.progress
      : (typeof options.current === "number" && typeof options.total === "number" && options.total > 0)
        ? options.current / options.total
        : undefined;

    const taskName = options.taskName || existing?.task?.taskName || options.text.replace(/\s*\([^)]*\)\s*$/, "").trim() || options.text;

    if (existing) {
      existing.text = options.text;
      existing.variant = "task";
      existing.task = {
        taskId: options.taskId,
        taskName,
        status: "running",
        current: options.current,
        total: options.total,
        progress,
      };
      return existing.id;
    }

    const id = this.nextId++;
    this.messages.push({
      id,
      text: options.text,
      variant: "task",
      task: {
        taskId: options.taskId,
        taskName,
        status: "running",
        current: options.current,
        total: options.total,
        progress,
      },
    });
    return id;
  }

  /** Update an in-flight task notification's progress or text. */
  updateTask(
    taskId: string,
    updates: {
      text?: string;
      current?: number;
      total?: number;
      progress?: number;
    }
  ) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) return;
    if (updates.text) msg.text = updates.text;
    const current = updates.current !== undefined ? updates.current : msg.task.current;
    const total = updates.total !== undefined ? updates.total : msg.task.total;
    let progress = updates.progress;
    if (progress === undefined && typeof current === "number" && typeof total === "number" && total > 0) {
      progress = current / total;
    }
    msg.task = {
      ...msg.task,
      current,
      total,
      progress: progress !== undefined ? Math.max(0, Math.min(1, progress)) : msg.task.progress,
    };
    this.messages = [...this.messages];
  }

  /**
   * Complete a task notification, transitioning it into the success/done state.
   */
  completeTask(
    taskId: string,
    completedText?: string,
    durationMs?: number
  ) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) {
      if (completedText && completedText !== "Done") {
        this.show(completedText, "success", durationMs);
      }
      return;
    }
    msg.task = {
      ...msg.task,
      status: "done",
      progress: 1,
      completedText: completedText && completedText !== "Done" ? completedText : msg.task.completedText,
    };
    msg.variant = "task";
    this.messages = [...this.messages];
    if (durationMs && durationMs > 0) {
      this.scheduleDismiss(msg.id, durationMs);
    }
  }

  /**
   * Mark a task notification as failed, displaying error state.
   */
  failTask(taskId: string, error: string) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) {
      this.show(error, "error");
      return;
    }
    msg.task = {
      ...msg.task,
      status: "failed",
      error,
    };
    msg.text = error;
    this.messages = [...this.messages];
  }

  isTaskActive(taskId: string): boolean {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    return msg?.task?.status === "running";
  }

  dismissTask(taskId: string) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (msg) {
      this.dismiss(msg.id);
    }
  }

  show(
    text: string,
    variant: ToastVariant = "info",
    durationMs?: number,
    url?: string,
    action?: ToastAction
  ) {
    const id = this.nextId++;
    this.messages.push({ id, text, variant, url, action });
    this.scheduleDismiss(id, durationMs);
    return id;
  }

  /** Update an already-shown toast's text/variant in place, without a new entry. */
  update(id: number, text: string, variant?: ToastVariant) {
    const msg = this.messages.find((m) => m.id === id);
    if (!msg) return;
    msg.text = text;
    if (variant) msg.variant = variant;
  }

  dismiss(id: number) {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }
    this.messages = this.messages.filter((m) => m.id !== id);
  }

  private scheduleDismiss(id: number, durationMs?: number) {
    const timer = this.timers.get(id);
    if (timer) clearTimeout(timer);
    this.timers.delete(id);
    if (durationMs && durationMs > 0) {
      this.timers.set(id, setTimeout(() => this.dismiss(id), durationMs));
    }
  }

  /**
   * Start a long-lived toast representing an in-progress batch operation
   * (e.g. processing many songs). Unlike `show`, it has no auto-dismiss
   * timer — call `updateBatch` as progress advances and `finishBatch` once
   * the batch completes, so the whole operation reads as one notification
   * instead of a toast per item (#233).
   */
  startBatch(text: string, variant: ToastVariant = "info"): number {
    return this.show(text, variant);
  }

  /** Update the text/variant of an active batch toast without resetting its lifetime. */
  updateBatch(id: number, text: string, variant?: ToastVariant) {
    this.update(id, text, variant);
  }

  /** Collapse an active batch toast into its final completion message, then auto-dismiss. */
  finishBatch(
    id: number,
    text: string,
    variant: ToastVariant = "success",
    durationMs: number = TOAST_DURATION_MS
  ) {
    if (!this.messages.some((m) => m.id === id)) {
      this.show(text, variant, durationMs);
      return;
    }
    this.update(id, text, variant);
    this.scheduleDismiss(id, durationMs);
  }
}

export const toastStore = new ToastStore();
