import { i18n } from "../stores/i18n.svelte";

/**
 * Returns localized language name for an ISO 639-1 / 639-2 / 639-3 code using Intl.DisplayNames,
 * capitalized, falling back to the raw code if unknown or on error.
 */
export function getLanguageDisplayName(code: string | null | undefined): string {
  if (!code) return "";
  const trimmed = code.trim();
  if (!trimmed) return "";

  try {
    const locale = i18n.currentLocale || "en";
    const dn = new Intl.DisplayNames([locale, "en"], { type: "language", fallback: "none" });
    const name = dn.of(trimmed);
    if (name) {
      return name.charAt(0).toUpperCase() + name.slice(1);
    }
  } catch {
    // Unsupported code or runtime issue, fallback to raw string
  }

  return trimmed;
}
