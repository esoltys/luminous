import { invoke } from "@tauri-apps/api/core";
import { TOAST_DURATION_MS } from "../constants";
import { i18n } from "./i18n.svelte";

type ToastVariant = "info" | "error" | "success" | "milestone" | "warning" | "task";

/** Icon shown on a milestone toast; `checks` is the default. */
type MilestoneIcon = "checks" | "flag" | "star" | "sparkle";

/**
 * What a notification is about, as the user sees it: the unit they can turn off or
 * reprioritise. Plain `toastStore.show()` calls are "confirmations" (feedback on
 * something the user just did); a source that raises notifications on its own takes
 * a typed channel with `toastStore.channel(type)`.
 */
export type NotificationType =
  "confirmations" | "library" | "tasks" | "devices" | "playback" | "celebrations";

export const NOTIFICATION_TYPES: readonly NotificationType[] = [
  "confirmations",
  "library",
  "tasks",
  "devices",
  "playback",
  "celebrations",
];

/**
 * How the user wants a notification type shown:
 * - `default`: as the caller asked (stays until dismissed unless given a duration);
 * - `high`: always stays until dismissed;
 * - `low`: always auto-dismisses (a running task stays until it finishes);
 * - `off`: not shown.
 * Errors and warnings ignore the preference, so a failure can't be hidden by accident.
 */
export type NotificationPreference = "default" | "high" | "low" | "off";

const NOTIFICATION_PREFERENCES: readonly NotificationPreference[] = [
  "default",
  "high",
  "low",
  "off",
];

const PREFERENCES_SETTING_KEY = "notification_preferences";

// Spelled out, not built from the type, so the keys stay greppable.
const TYPE_LABEL_KEYS: Record<NotificationType, string> = {
  confirmations: "notifications.typeConfirmations",
  library: "notifications.typeLibrary",
  tasks: "notifications.typeTasks",
  devices: "notifications.typeDevices",
  playback: "notifications.typePlayback",
  celebrations: "notifications.typeCelebrations",
};

/** Localised name of a notification type, e.g. "Library changes". */
export function notificationTypeLabel(type: NotificationType): string {
  return i18n.t(TYPE_LABEL_KEYS[type]);
}

interface ToastAction {
  label: string;
  onClick: () => void;
  /** Shown before the label; `refresh` marks a retry or restart. */
  icon?: "refresh";
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
  type: NotificationType;
  url?: string;
  action?: ToastAction;
  task?: ToastTaskData;
  milestoneIcon?: MilestoneIcon;
  /** Whether the toast is currently counting down to dismiss itself (after preferences). */
  autoDismisses: boolean;
  /** The caller's own lifetime for the toast's current state; preferences may override it. */
  requestedDurationMs?: number;
  /** Store-owned notices (e.g. "turned off, Undo") that preferences never apply to. */
  isSystemNotice?: boolean;
}

/** The subset of the store a notification source needs, with its type bound. */
interface ToastChannel {
  show(
    text: string,
    variant?: ToastVariant,
    durationMs?: number,
    url?: string,
    action?: ToastAction,
  ): number;
  startBatch(text: string, variant?: ToastVariant): number;
  finishBatch(id: number, text: string, variant?: ToastVariant, durationMs?: number): void;
}

class ToastStore {
  messages = $state<ToastMessage[]>([]);
  /** Per-type preferences; a type missing here is `default`. */
  preferences = $state<Partial<Record<NotificationType, NotificationPreference>>>({});
  private nextId = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  /** A handle that raises notifications of `type`; see `NotificationType`. */
  channel(type: NotificationType): ToastChannel {
    return {
      show: (text, variant = "info", durationMs, url, action) =>
        this.add(type, { text, variant, url, action }, durationMs),
      startBatch: (text, variant = "info") => this.add(type, { text, variant }),
      finishBatch: (id, text, variant = "success", durationMs = TOAST_DURATION_MS) =>
        this.finishBatchAs(type, id, text, variant, durationMs),
    };
  }

  preferenceFor(type: NotificationType): NotificationPreference {
    return this.preferences[type] ?? "default";
  }

  /**
   * Set how `type` is shown from now on, apply it to the toasts already on screen, and
   * save it. Saving is fire-and-forget: a failed write is logged, never surfaced.
   */
  setPreference(type: NotificationType, preference: NotificationPreference) {
    this.preferences = { ...this.preferences, [type]: preference };
    this.applyPreferences();
    invoke("set_app_setting", {
      key: PREFERENCES_SETTING_KEY,
      value: JSON.stringify(this.preferences),
    }).catch((err) => console.error("Failed to save notification preferences:", err));
  }

  /**
   * Turn `type` off and replace its toasts with one short notice offering Undo, which
   * restores the preference the type had before.
   */
  turnOff(type: NotificationType) {
    const previous = this.preferenceFor(type);
    this.setPreference(type, "off");
    const noticeId = this.add(
      type,
      {
        text: i18n.t("notifications.turnedOff", { type: notificationTypeLabel(type) }),
        variant: "info",
        isSystemNotice: true,
        action: {
          label: i18n.t("notifications.undo"),
          onClick: () => {
            this.dismiss(noticeId);
            this.setPreference(type, previous);
          },
        },
      },
      TOAST_DURATION_MS,
    );
  }

  /**
   * Whether saved preferences have been applied (or failed to load). The toast stack
   * paints nothing until then: a toast raised earlier at startup is kept, and filtered
   * by the user's preferences before it can ever show.
   */
  preferencesLoaded = $state(false);

  /** Restore saved preferences, apply them to any toasts raised so far, then let toasts paint. */
  async loadPreferences() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      this.preferences = parsePreferences(settings?.[PREFERENCES_SETTING_KEY]);
      this.applyPreferences();
    } catch (err) {
      console.error("Failed to load notification preferences:", err);
    } finally {
      this.preferencesLoaded = true;
    }
  }

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
    const progress =
      typeof options.progress === "number"
        ? options.progress
        : typeof options.current === "number" &&
            typeof options.total === "number" &&
            options.total > 0
          ? options.current / options.total
          : undefined;

    const taskName =
      options.taskName ||
      existing?.task?.taskName ||
      options.text.replace(/\s*\([^)]*\)\s*(\.{3}|…|\.)?\s*$/, "").trim() ||
      options.text;

    const task: ToastTaskData = {
      taskId: options.taskId,
      taskName,
      status: "running",
      current: options.current,
      total: options.total,
      progress,
    };

    if (existing) {
      existing.text = options.text;
      existing.variant = "task";
      existing.task = task;
      existing.requestedDurationMs = undefined;
      this.applyPolicy(existing);
      return existing.id;
    }

    return this.add("tasks", { text: options.text, variant: "task", task });
  }

  /** Update an in-flight task notification's progress or text. */
  updateTask(
    taskId: string,
    updates: {
      text?: string;
      current?: number;
      total?: number;
      progress?: number;
    },
  ) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) return;
    if (updates.text) msg.text = updates.text;
    const current = updates.current !== undefined ? updates.current : msg.task.current;
    const total = updates.total !== undefined ? updates.total : msg.task.total;
    let progress = updates.progress;
    if (
      progress === undefined &&
      typeof current === "number" &&
      typeof total === "number" &&
      total > 0
    ) {
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
  completeTask(taskId: string, completedText?: string, durationMs?: number) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) {
      if (completedText && completedText !== "Done") {
        this.add("tasks", { text: completedText, variant: "success" }, durationMs);
      }
      return;
    }
    msg.task = {
      ...msg.task,
      status: "done",
      progress: 1,
      completedText:
        completedText && completedText !== "Done" ? completedText : msg.task.completedText,
    };
    msg.variant = "task";
    msg.requestedDurationMs = durationMs;
    this.messages = [...this.messages];
    this.applyPolicy(msg);
  }

  /**
   * Mark a task notification as failed, displaying error state.
   */
  failTask(taskId: string, error: string) {
    const msg = this.messages.find((m) => m.task?.taskId === taskId);
    if (!msg || !msg.task) {
      this.add("tasks", { text: error, variant: "error" });
      return;
    }
    msg.task = {
      ...msg.task,
      status: "failed",
      error,
    };
    msg.text = error;
    msg.requestedDurationMs = undefined;
    this.messages = [...this.messages];
    this.applyPolicy(msg);
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

  /** Show an "Action confirmations" notification; other types go through `channel()`. */
  show(
    text: string,
    variant: ToastVariant = "info",
    durationMs?: number,
    url?: string,
    action?: ToastAction,
  ) {
    return this.add("confirmations", { text, variant, url, action }, durationMs);
  }

  /** Show a milestone-tier celebration toast with the icon that suits the moment. */
  celebrate(text: string, icon: MilestoneIcon, url?: string) {
    return this.add("celebrations", { text, variant: "milestone", url, milestoneIcon: icon });
  }

  /** Update an already-shown toast's text/variant in place, without a new entry. */
  update(id: number, text: string, variant?: ToastVariant) {
    const msg = this.messages.find((m) => m.id === id);
    if (!msg) return;
    msg.text = text;
    if (variant && variant !== msg.variant) {
      msg.variant = variant;
      // Becoming an error or warning lifts the preference; leaving one restores it.
      this.applyPolicy(msg);
    }
  }

  dismiss(id: number) {
    this.clearTimer(id);
    this.messages = this.messages.filter((m) => m.id !== id);
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
    durationMs: number = TOAST_DURATION_MS,
  ) {
    this.finishBatchAs("confirmations", id, text, variant, durationMs);
  }

  private finishBatchAs(
    type: NotificationType,
    id: number,
    text: string,
    variant: ToastVariant,
    durationMs: number,
  ) {
    const msg = this.messages.find((m) => m.id === id);
    if (!msg) {
      this.add(type, { text, variant }, durationMs);
      return;
    }
    msg.text = text;
    msg.variant = variant;
    msg.requestedDurationMs = durationMs;
    this.applyPolicy(msg);
  }

  /**
   * Add a toast of `type`, unless the user turned that type off. Always returns an id;
   * a suppressed toast's id simply matches nothing, so later `update`/`dismiss` calls
   * on it are no-ops and callers never need to know it was suppressed.
   */
  private add(
    type: NotificationType,
    fields: Omit<ToastMessage, "id" | "type" | "autoDismisses" | "requestedDurationMs">,
    durationMs?: number,
  ): number {
    const id = this.nextId++;
    const msg: ToastMessage = {
      ...fields,
      id,
      type,
      autoDismisses: false,
      requestedDurationMs: durationMs,
    };
    if (this.isSuppressed(msg)) return id;
    this.messages.push(msg);
    // Apply to the reactive proxy, so the component sees `autoDismisses` change.
    this.applyPolicy(this.messages[this.messages.length - 1]);
    return id;
  }

  /** Re-apply preferences to every toast on screen, e.g. after a preference change. */
  private applyPreferences() {
    for (const msg of this.messages.filter((m) => this.isSuppressed(m))) {
      this.dismiss(msg.id);
    }
    for (const msg of this.messages) this.applyPolicy(msg);
  }

  private isSuppressed(msg: ToastMessage): boolean {
    return !ignoresPreferences(msg) && this.preferenceFor(msg.type) === "off";
  }

  /** (Re)start or clear `msg`'s dismiss timer from its requested lifetime and its type's preference. */
  private applyPolicy(msg: ToastMessage) {
    const durationMs = this.effectiveDurationMs(msg);
    this.clearTimer(msg.id);
    msg.autoDismisses = durationMs !== undefined;
    if (durationMs !== undefined) {
      this.timers.set(
        msg.id,
        setTimeout(() => this.dismiss(msg.id), durationMs),
      );
    }
  }

  /** How long `msg` stays on screen, in ms; `undefined` means until the user dismisses it. */
  private effectiveDurationMs(msg: ToastMessage): number | undefined {
    const requested =
      msg.requestedDurationMs && msg.requestedDurationMs > 0 ? msg.requestedDurationMs : undefined;
    if (ignoresPreferences(msg)) return requested;
    switch (this.preferenceFor(msg.type)) {
      case "high":
        return undefined;
      case "low":
        return msg.task?.status === "running" ? undefined : (requested ?? TOAST_DURATION_MS);
      default:
        return requested;
    }
  }

  private clearTimer(id: number) {
    const timer = this.timers.get(id);
    if (timer) clearTimeout(timer);
    this.timers.delete(id);
  }
}

/** Errors, warnings, failed tasks and the store's own notices always show as the caller asked. */
function ignoresPreferences(msg: ToastMessage): boolean {
  return (
    msg.isSystemNotice === true ||
    msg.variant === "error" ||
    msg.variant === "warning" ||
    msg.task?.status === "failed"
  );
}

/** Whether the user can change how `msg`'s type is shown from the toast itself. */
export function hasPreferenceMenu(msg: ToastMessage): boolean {
  return !ignoresPreferences(msg);
}

/** Read the saved preference map, dropping unknown types and values (they mean `default`). */
function parsePreferences(
  raw: string | undefined,
): Partial<Record<NotificationType, NotificationPreference>> {
  if (!raw) return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return {};
  }
  if (!parsed || typeof parsed !== "object") return {};
  const result: Partial<Record<NotificationType, NotificationPreference>> = {};
  for (const [type, preference] of Object.entries(parsed)) {
    if (
      NOTIFICATION_TYPES.includes(type as NotificationType) &&
      NOTIFICATION_PREFERENCES.includes(preference as NotificationPreference)
    ) {
      result[type as NotificationType] = preference as NotificationPreference;
    }
  }
  return result;
}

export const toastStore = new ToastStore();
