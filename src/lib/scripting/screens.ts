import type { ScriptScreensApi, ScriptWaitApi } from "./types";

export interface ScreenHooks {
  search(query: string): Promise<void>;
  openAlbumEditor(): void;
  setEqualizerMode(mode: "graphic10" | "parametric"): Promise<void>;
  openEqualizerPresetMenu(): void;
}

const hooks: Partial<ScreenHooks> = {};

/**
 * A screen registers the entry points only it can provide (they live in its
 * component state). Returns an unregister function for the screen's teardown.
 * Screens call this behind `import.meta.env.DEV`, so release builds never load it.
 */
export function registerScreenHook<K extends keyof ScreenHooks>(name: K, hook: ScreenHooks[K]): () => void {
  hooks[name] = hook;
  return () => {
    if (hooks[name] === hook) delete hooks[name];
  };
}

const SCREEN_FOR: Record<keyof ScreenHooks, string> = {
  search: "the top navigation bar",
  openAlbumEditor: "an album detail view (navigate.album first)",
  setEqualizerMode: "Settings → Equalizer (navigate.settings('equalizer') first)",
  openEqualizerPresetMenu: "Settings → Equalizer in parametric mode",
};

function hook<K extends keyof ScreenHooks>(name: K): ScreenHooks[K] {
  const found = hooks[name];
  if (!found) throw new Error(`"${name}" needs ${SCREEN_FOR[name]} on screen.`);
  return found as ScreenHooks[K];
}

/** Creates the controller for component-local screen states. */
export function createScreensController(wait: ScriptWaitApi): ScriptScreensApi {
  return {
    async search(query) {
      await hook("search")(query);
      await wait.settled();
    },
    async openAlbumEditor() {
      hook("openAlbumEditor")();
      await wait.settled();
    },
    async setEqualizerMode(mode) {
      await hook("setEqualizerMode")(mode);
      await wait.settled();
    },
    async openEqualizerPresetMenu() {
      hook("openEqualizerPresetMenu")();
      await wait.settled();
    },
  };
}
