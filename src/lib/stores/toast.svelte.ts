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

  show(text: string, variant: ToastVariant = "info", durationMs: number = TOAST_DURATION_MS) {
    const id = this.nextId++;
    this.messages.push({ id, text, variant });
    setTimeout(() => this.dismiss(id), durationMs);
  }

  dismiss(id: number) {
    this.messages = this.messages.filter((m) => m.id !== id);
  }
}

export const toastStore = new ToastStore();
