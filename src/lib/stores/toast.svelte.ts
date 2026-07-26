import { TOAST_DURATION_MS } from "../constants";

export type ToastVariant = "info" | "error";

export interface ToastMessage {
  id: number;
  text: string;
  variant: ToastVariant;
}

class ToastStore {
  messages = $state<ToastMessage[]>([]);
  private nextId = 0;

  show(text: string, variant: ToastVariant = "info") {
    const id = this.nextId++;
    this.messages.push({ id, text, variant });
    setTimeout(() => this.dismiss(id), TOAST_DURATION_MS);
  }

  dismiss(id: number) {
    this.messages = this.messages.filter((m) => m.id !== id);
  }
}

export const toastStore = new ToastStore();
