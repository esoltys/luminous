import { TOAST_DURATION_MS } from "../constants";
import { soundCues } from "../utils/soundCues";

export type ToastVariant = "info" | "error" | "success" | "milestone" | "warning";

export interface ToastMessage {
  id: number;
  text: string;
  variant: ToastVariant;
}

class ToastStore {
  messages = $state<ToastMessage[]>([]);
  private nextId = 0;

  show(text: string, variant: ToastVariant = "info", durationMs?: number) {
    const id = this.nextId++;
    this.messages.push({ id, text, variant });

    // Sound cues matching tier
    if (variant === "success") {
      soundCues.playSuccess();
    } else if (variant === "milestone") {
      soundCues.playMilestone();
    } else if (variant === "warning" || variant === "error") {
      soundCues.playWarning();
    } else if (variant === "info") {
      soundCues.playNew();
    }

    if (durationMs && durationMs > 0) {
      setTimeout(() => this.dismiss(id), durationMs);
    }
  }

  dismiss(id: number) {
    this.messages = this.messages.filter((m) => m.id !== id);
  }
}

export const toastStore = new ToastStore();
