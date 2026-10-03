import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Theme } from "./theme.svelte";

/**
 * Lifecycle of an advanced add-on theme (#1036). Platform-agnostic: builds
 * with no Store entitlement path never leave `unavailable`.
 */
export type AddonState =
  | "unavailable"
  | "unowned"
  | "purchasing"
  | "downloading"
  | "owned"
  | "error";

const ADDON_STATES: readonly AddonState[] = [
  "unavailable",
  "unowned",
  "purchasing",
  "downloading",
  "owned",
  "error"
];

/** Payload of the backend's `addon-state-changed` event. */
export interface AddonStateChange {
  id: string;
  state: AddonState;
  error?: string;
}

/**
 * A registered add-on: the base palette applies like any other theme, and
 * `overlay` (if any) names what AddonOverlay mounts above the player bar.
 * Definitions arrive from decrypted bundle manifests (#1415) — the registry
 * never holds paid assets itself, only what the manifest declares.
 */
export interface AddonTheme {
  /** Namespaced so it can never collide with a predefined or custom theme id. */
  id: string;
  name: string;
  colors: Theme["colors"];
  /** Entry point the overlay runtime loads (#1413); null for palette-only add-ons. */
  overlayEntry: string | null;
  /** Optional manifest opt-ins; `"spectrum"` streams spectrum data to the overlay. */
  capabilities?: string[];
}

interface AddonStatus {
  state: AddonState;
  error?: string;
}

type Listener = () => void;

export class AddonsStore {
  /** Registered definitions, keyed by id. */
  themes = $state<Record<string, AddonTheme>>({});
  /** Last state reported by the backend, keyed by id. Absent means `unavailable`. */
  statuses = $state<Record<string, AddonStatus>>({});

  private listeners = new Set<Listener>();
  private unlisten: UnlistenFn | null = null;

  /**
   * Subscribe to the backend's `addon-state-changed` event. State only ever
   * changes from this event — never from the result of an `invoke()`.
   */
  async init() {
    if (this.unlisten) return;
    this.unlisten = await listen<AddonStateChange>("addon-state-changed", (event) => {
      this.applyEvent(event.payload);
    });
  }

  destroy() {
    this.unlisten?.();
    this.unlisten = null;
  }

  /** Called by the theme store so a persisted add-on theme is re-applied once it becomes owned. */
  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  register(addon: AddonTheme) {
    this.themes[addon.id] = addon;
    this.notify();
  }

  unregister(id: string) {
    delete this.themes[id];
    delete this.statuses[id];
    this.notify();
  }

  applyEvent(change: AddonStateChange) {
    if (!ADDON_STATES.includes(change.state)) return;
    this.statuses[change.id] = { state: change.state, error: change.error };
    this.notify();
  }

  stateOf(id: string): AddonState {
    return this.statuses[id]?.state ?? "unavailable";
  }

  errorOf(id: string): string | undefined {
    return this.statuses[id]?.error;
  }

  isAddonId(id: string): boolean {
    return id in this.themes;
  }

  /** True only when the add-on is registered and the backend reports it owned. */
  isUsable(id: string): boolean {
    return this.isAddonId(id) && this.stateOf(id) === "owned";
  }

  /** The add-on as a plain Theme, or undefined when it isn't registered. */
  asTheme(id: string): Theme | undefined {
    const addon = this.themes[id];
    if (!addon) return undefined;
    return { id: addon.id, name: addon.name, colors: addon.colors };
  }

  get list(): AddonTheme[] {
    return Object.values(this.themes);
  }

  private notify() {
    for (const listener of this.listeners) listener();
  }
}

export const addonsStore = new AddonsStore();
