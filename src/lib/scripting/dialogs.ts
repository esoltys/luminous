import { collectionStore } from "../stores/collection.svelte";
import { closeSearchIfOpen } from "./screens";
import { toastStore } from "../stores/toast.svelte";
import { welcomeStore } from "../stores/welcome.svelte";
import { walkthroughStore } from "../stores/walkthrough.svelte";
import type { ScriptDialogsApi, ScriptWaitApi } from "./types";

export interface DialogHostControls {
  openShortcuts(): void;
  openTagEditor(songId: number): void;
  closeAll(): void;
  isShortcutsOpen(): boolean;
  isTagEditorOpen(): boolean;
}

let activeHostControls: DialogHostControls | null = null;

/**
 * Registers the layout's active dialog controllers.
 */
export function registerDialogHostControls(controls: DialogHostControls | null): void {
  activeHostControls = controls;
}

/**
 * Creates the dialogs controller.
 * Manages modal visibility, overlays, welcome screen, and walkthrough tours.
 */
export function createDialogsController(wait: ScriptWaitApi): ScriptDialogsApi {
  return {
    async openShortcuts(): Promise<void> {
      if (activeHostControls) {
        activeHostControls.openShortcuts();
      }
      await wait.settled();
    },

    async openTagEditor(songId: number): Promise<void> {
      if (activeHostControls) {
        activeHostControls.openTagEditor(songId);
      }
      await wait.settled();
    },

    async closeAll(): Promise<void> {
      if (activeHostControls) {
        activeHostControls.closeAll();
      }
      collectionStore.closeSmartBuilder();
      closeSearchIfOpen();
      if (walkthroughStore.isActive) {
        walkthroughStore.finish();
      }
      await wait.settled();
    },

    async dismissToasts(): Promise<void> {
      for (const msg of [...toastStore.messages]) toastStore.dismiss(msg.id);
      await wait.settled();
    },

    welcome: {
      show(): void {
        welcomeStore.hasSeen = false;
      },
      dismiss(): void {
        welcomeStore.markSeen();
      },
    },

    walkthrough: {
      start(): void {
        walkthroughStore.start("full");
      },
      stop(): void {
        walkthroughStore.finish();
      },
      next(): void {
        walkthroughStore.next();
      },
    },
  };
}
