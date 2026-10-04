/**
 * Add-ons this build can offer (#1417). Ids match `KNOWN_ADDONS` in
 * `src-tauri/src/addons/entitlement.rs`.
 *
 * Everything here is public marketing material, so it can show before the
 * add-on is owned: the real palette and overlay stay inside the encrypted
 * bundle. Once an add-on is owned, the card reads its colours from the
 * registered theme instead (`addonsStore.themes`), so this list only has to
 * be right until then.
 */
export interface AddonCatalogEntry {
  id: string;
  name: string;
  /** Locale key for the one-line description under the palette. */
  descriptionKey: string;
  /** Six swatches in the order of every other theme card: main, sidebar, player bar, accent, accent hover, border. */
  swatches: [string, string, string, string, string, string];
  /** Public Store image that perches on the palette strip. Pre-scaled to 2x its 104x102 CSS px display size. */
  art: string;
}

export const ADDON_CATALOG: readonly AddonCatalogEntry[] = [
  {
    id: "mothman",
    name: "Mothman",
    descriptionKey: "settings.addonDescriptionMothman",
    swatches: ["#2a535e", "#000308", "#2a535e", "#c6133d", "#e2bd86", "#904d46"],
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
