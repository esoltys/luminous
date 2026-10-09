import {
  BASE_LOCALE,
  catalogChain,
  isLocale,
  legacyLanguageToLocale,
  manualLanguageForLocale,
  type Locale,
  type ManualLanguage,
} from '../locales';
import { invoke } from '@tauri-apps/api/core';

export type { Locale };

/**
 * Marks that `language` holds a BCP 47 tag. Builds before the locale registry saved bare
 * "en"/"fr"; without this marker a saved "fr" could not be told apart from a future
 * France-French tag of the same name, so legacy values are only aliased while it is absent.
 */
const LANGUAGE_TAGS_KEY = "language_tags";

class I18nStore {
  currentLocale = $state<Locale>(BASE_LOCALE);

  /** Language of the user guide the Help view loads. */
  get manualLanguage(): ManualLanguage {
    return manualLanguageForLocale(this.currentLocale);
  }

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      const saved = settings?.language;
      const migrating = settings?.[LANGUAGE_TAGS_KEY] !== "1";
      const locale = saved ? (migrating ? legacyLanguageToLocale(saved) : null) ?? (isLocale(saved) ? saved : null) : null;
      if (locale) this.currentLocale = locale;
      if (migrating) {
        if (locale) void invoke("set_app_setting", { key: "language", value: locale }).catch(() => {});
        void invoke("set_app_setting", { key: LANGUAGE_TAGS_KEY, value: "1" }).catch(() => {});
      }
    } catch (e) {
      console.error("Failed to load language settings:", e);
    } finally {
      if (typeof document !== 'undefined') {
        document.documentElement.lang = this.currentLocale;
      }
    }
  }

  async setLocale(locale: Locale) {
    this.currentLocale = locale;
    if (typeof document !== 'undefined') {
      document.documentElement.lang = locale;
    }
    try {
      await invoke("set_app_setting", { key: "language", value: locale });
    } catch (e) {
      console.error("Failed to save language settings:", e);
    }
  }

  formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
    return formatNumber(value, options);
  }

  t(key: string, vars: Record<string, any> = {}, fallback?: string): string {
    const keys = key.split('.');
    let value: any;
    for (const catalog of catalogChain(this.currentLocale)) {
      value = catalog;
      for (const k of keys) {
        value = value && typeof value === 'object' ? value[k] : undefined;
      }
      if (typeof value === 'string') break;
    }

    if (typeof value !== 'string') {
      return fallback !== undefined ? fallback : key;
    }

    return interpolate(value, vars);
  }

  /**
   * Counted string: the catalog entry is an object keyed by CLDR plural category
   * (`one`, `few`, `many`, `other`...) and the category comes from `Intl.PluralRules` for the
   * current locale, so callers never decide the form. A locale missing the category falls back
   * to its `other`, then to the next catalog in the chain. `{count}` is the locale-formatted
   * `count` unless `vars` supplies its own.
   */
  plural(key: string, count: number, vars: Record<string, any> = {}, fallback?: string): string {
    const category = pluralCategory(this.currentLocale, count);
    const keys = key.split('.');
    for (const catalog of catalogChain(this.currentLocale)) {
      let value: any = catalog;
      for (const k of keys) {
        value = value && typeof value === 'object' ? value[k] : undefined;
      }
      const form = value && typeof value === 'object' ? (value[category] ?? value.other) : undefined;
      if (typeof form === 'string') return interpolate(form, { count: formatNumber(count), ...vars });
    }
    return fallback !== undefined ? fallback : key;
  }
}

function interpolate(template: string, vars: Record<string, any>): string {
  return template.replace(/{(\w+)}/g, (_, name) => {
    return name in vars ? String(vars[name]) : `{${name}}`;
  });
}

const pluralRulesCache = new Map<string, Intl.PluralRules>();

function pluralCategory(locale: string, count: number): Intl.LDMLPluralRule {
  let rules = pluralRulesCache.get(locale);
  if (!rules) {
    rules = new Intl.PluralRules(locale);
    pluralRulesCache.set(locale, rules);
  }
  return rules.select(count);
}

export const i18n = new I18nStore();

const numberFormatCache = new Map<string, Intl.NumberFormat>();

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  if (!Number.isFinite(value)) return String(value);
  const locale = i18n.currentLocale;
  const key = `${locale}:${JSON.stringify(options ?? {})}`;
  let formatter = numberFormatCache.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(locale, options);
    numberFormatCache.set(key, formatter);
  }
  return formatter.format(value);
}
