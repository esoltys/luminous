import { en } from '../locales/en';
import { fr } from '../locales/fr';
import { invoke } from '@tauri-apps/api/core';

export type Locale = 'en' | 'fr';

const translations = { en, fr };

class I18nStore {
  currentLocale = $state<Locale>('en');

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.language) {
        if (settings.language === "en" || settings.language === "fr") {
          this.currentLocale = settings.language as Locale;
        }
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
    let value: any = translations[this.currentLocale];
    for (const k of keys) {
      if (value && typeof value === 'object') {
        value = value[k];
      } else {
        value = undefined;
        break;
      }
    }

    if (value === undefined) {
      // Fallback to English
      value = translations['en'];
      for (const k of keys) {
        if (value && typeof value === 'object') {
          value = value[k];
        } else {
          value = undefined;
          break;
        }
      }
    }

    if (typeof value !== 'string') {
      return fallback !== undefined ? fallback : key;
    }

    return value.replace(/{(\w+)}/g, (_, name) => {
      return name in vars ? String(vars[name]) : `{${name}}`;
    });
  }
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
