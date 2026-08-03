import { TOAST_DURATION_MS } from "../constants";

export type ToastVariant = "info" | "error" | "success" | "milestone" | "warning";

export interface ToastMessage {
  id: number;
  text: string;
  variant: ToastVariant;
}

class ToastStore {
  messages = $state<ToastMessage[]>([]);
  private nextId = 0;
  private timers = new Map<number, ReturnType<typeof setTimeout>>();

  show(text: string, variant: ToastVariant = "info", durationMs?: number) {
    const id = this.nextId++;
    this.messages.push({ id, text, variant });
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
