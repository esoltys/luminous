import { themeStore } from "../stores/theme.svelte";
import { i18n } from "../stores/i18n.svelte";
import { isLocale, LOCALES } from "../locales";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import type { ScriptAppearanceApi, ScriptWaitApi } from "./types";

/**
 * Creates the appearance controller.
 * Manages themes, light/dark color schemes, localization, and window panel layout.
 */
export function createAppearanceController(wait: ScriptWaitApi): ScriptAppearanceApi {
  return {
    async setTheme(themeId: string): Promise<void> {
      await themeStore.setTheme(themeId);
      await wait.settled();
      await wait.forState(() => themeStore.activeThemeId === themeId, 3000);
    },

    async setColorScheme(scheme: "light" | "dark" | "system"): Promise<void> {
      await themeStore.setColorSchemeMode(scheme);
      await wait.settled();
      await wait.forState(() => themeStore.colorSchemeMode === scheme, 3000);
    },

    async setLocale(locale: string): Promise<void> {
      if (!isLocale(locale)) {
        throw new Error(`Unknown locale "${locale}"; supported: ${LOCALES.map((l) => l.tag).join(", ")}.`);
      }
      await i18n.setLocale(locale);
      await wait.settled();
      await wait.forState(() => i18n.currentLocale === locale, 3000);
    },

    async setLayout(layout: {
      sidebarOpen?: boolean;
      rightPanelOpen?: boolean;
      sidebarWidth?: number;
      miniplayer?: boolean;
      immersive?: boolean;
    }): Promise<void> {
      if (layout.sidebarOpen !== undefined) {
        windowLayoutStore.sidebarOpen = layout.sidebarOpen;
      }
      if (layout.rightPanelOpen !== undefined) {
        windowLayoutStore.rightPanelOpen = layout.rightPanelOpen;
      }
      if (layout.sidebarWidth !== undefined) {
        windowLayoutStore.sidebarWidth = layout.sidebarWidth;
      }
      if (layout.miniplayer !== undefined && layout.miniplayer !== windowLayoutStore.isMiniplayer) {
        // The window resize behind the toggle is async, and an overlapping toggle is dropped.
        await windowLayoutStore.toggleMiniplayerMode();
      }
      if (layout.immersive !== undefined && layout.immersive !== windowLayoutStore.immersiveMode) {
        windowLayoutStore.toggleImmersiveMode();
      }
      await wait.settled();
    },
  };
}
