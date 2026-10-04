import type { ThemeColors } from "../stores/theme.svelte";

/**
 * Add-ons this build can offer (#1417). Ids match `KNOWN_ADDONS` in
 * `src-tauri/src/addons/entitlement.rs`.
 *
 * Everything here is public marketing material, so it can show before the
 * add-on is owned: the overlay and its art stay inside the encrypted bundle.
 * The palette is also what the app paints, without the overlay, while a saved
 * add-on theme waits for the Store to confirm ownership at launch (#1438).
 * Once an add-on is owned, the registered theme (`addonsStore.themes`) is the
 * source of truth, so this list only has to be right until then.
 */
export interface AddonCatalogEntry {
  id: string;
  name: string;
  /** Locale key for the one-line description under the palette. */
  descriptionKey: string;
  /** The add-on's palette, matching its manifest's `colors`. */
  colors: ThemeColors;
  /** Public Store image that perches on the palette strip. Pre-scaled to 2x its 104x102 CSS px display size. */
  art: string;
}

export const ADDON_CATALOG: readonly AddonCatalogEntry[] = [
  {
    id: "mothman",
    name: "Mothman",
    descriptionKey: "settings.addonDescriptionMothman",
    colors: {
      "bg-main": "#2a535e",
      "bg-sidebar": "#000308",
      "bg-playerbar": "#2a535e",
      "color-accent": "#c6133d",
      "color-accent-hover": "#e2bd86",
      "color-text-primary": "#ffffff",
      "color-text-secondary": "#e2e8f0",
      "color-border": "#904d46"
    },
    art: "/addons/mothman-store.png"
  }
];

/** Order of the theme colours shown as swatches. */
export const SWATCH_KEYS = [
  "bg-main",
  "bg-sidebar",
  "bg-playerbar",
  "color-accent",
  "color-accent-hover",
  "color-border"
] as const;
