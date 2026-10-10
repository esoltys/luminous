import type { ScriptScreensApi, ScriptWaitApi } from "./types";

export interface ScreenHooks {
  search(query: string): Promise<void>;
  setSort(surface: "songs" | "albums" | "artists", field: string, ascending: boolean): void;
  /** Clears the search box and closes its dropdown. */
  closeSearch(): void;
  openAlbumEditor(): void;
  setEqualizerMode(mode: "graphic10" | "parametric"): Promise<void>;
  openEqualizerPresetMenu(): void;
  selectEqualizerPreset(name: string): Promise<void>;
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
  setSort: "the Collection tab (navigate.to('collection') first)",
  closeSearch: "the top navigation bar",
  openAlbumEditor: "an album detail view (navigate.album first)",
  setEqualizerMode: "Settings → Equalizer (navigate.settings('equalizer') first)",
  selectEqualizerPreset: "Settings → Equalizer (navigate.settings('equalizer') first)",
  openEqualizerPresetMenu: "Settings → Equalizer in parametric mode",
};

async function hook<K extends keyof ScreenHooks>(name: K, wait: ScriptWaitApi): Promise<ScreenHooks[K]> {
  // A screen registers its hooks just after it mounts (a dynamic import), so give a screen that
  // was only just opened a moment to appear before saying it isn't there.
  await wait.forState(() => !!hooks[name], 2000).catch(() => {});
  const found = hooks[name];
  if (!found) throw new Error(`"${name}" needs ${SCREEN_FOR[name]} on screen.`);
  return found as ScreenHooks[K];
}

/** Closes the search dropdown if the search box is on screen; nothing to close otherwise. */
export function closeSearchIfOpen(): void {
  hooks.closeSearch?.();
}

/** Creates the controller for component-local screen states. */
export function createScreensController(wait: ScriptWaitApi): ScriptScreensApi {
  return {
    async search(query) {
      await (await hook("search", wait))(query);
      await wait.settled();
    },
    async setSort(surface, field, ascending) {
      (await hook("setSort", wait))(surface, field, ascending);
      await wait.settled();
    },
    async openAlbumEditor() {
      (await hook("openAlbumEditor", wait))();
      await wait.settled();
    },
    async setEqualizerMode(mode) {
      await (await hook("setEqualizerMode", wait))(mode);
      await wait.settled();
    },
    async selectEqualizerPreset(name) {
      await (await hook("selectEqualizerPreset", wait))(name);
      await wait.settled();
    },
    async openEqualizerPresetMenu() {
      (await hook("openEqualizerPresetMenu", wait))();
      await wait.settled();
    },
  };
}
