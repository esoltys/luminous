import { invoke } from "@tauri-apps/api/core";

export interface ThemeColors {
  "bg-main": string;
  "bg-sidebar": string;
  "bg-playerbar": string;
  "color-accent": string;
  "color-accent-hover": string;
  "color-text-primary": string;
  "color-text-secondary": string;
  "color-border": string;
}

export interface Theme {
  id: string;
  name: string;
  colors: ThemeColors;
  isCustom?: boolean;
}

export const PREDEFINED_THEMES: Theme[] = [
  {
    id: "luminous-violet",
    name: "Luminous Violet",
    colors: {
      "bg-main": "#0d0b18",
      "bg-sidebar": "#07050e",
      "bg-playerbar": "#0a0813",
      "color-accent": "#8b5cf6",
      "color-accent-hover": "#a78bfa",
      "color-text-primary": "#f3f4f6",
      "color-text-secondary": "#9ca3af",
      "color-border": "#1f1b2e"
    }
  },
  {
    id: "strawberry-red",
    name: "Strawberry Red",
    colors: {
      "bg-main": "#1a0f12",
      "bg-sidebar": "#10090a",
      "bg-playerbar": "#150c0e",
      "color-accent": "#e11d48",
      "color-accent-hover": "#f43f5e",
      "color-text-primary": "#f9fafb",
      "color-text-secondary": "#d1d5db",
      "color-border": "#281b1e"
    }
  },
  {
    id: "nordic-blue",
    name: "Nordic Blue",
    colors: {
      "bg-main": "#2e3440",
      "bg-sidebar": "#242933",
      "bg-playerbar": "#2b303c",
      "color-accent": "#88c0d0",
      "color-accent-hover": "#8fbcbb",
      "color-text-primary": "#eceff4",
      "color-text-secondary": "#d8dee9",
      "color-border": "#3b4252"
    }
  },
  {
    id: "retro-amber",
    name: "Retro Amber",
    colors: {
      "bg-main": "#0d0a00",
      "bg-sidebar": "#060500",
      "bg-playerbar": "#0a0800",
      "color-accent": "#d97706",
      "color-accent-hover": "#f59e0b",
      "color-text-primary": "#fef3c7",
      "color-text-secondary": "#b45309",
      "color-border": "#1e1700"
    }
  }
];

class ThemeStore {
  activeThemeId = $state<string>("luminous-violet");
  customThemes = $state<Theme[]>([]);

  constructor() {}

  async init() {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings) {
        if (settings.custom_themes) {
          try {
            this.customThemes = JSON.parse(settings.custom_themes);
          } catch (e) {
            console.error("Failed to parse custom_themes:", e);
          }
        }
        if (settings.active_theme_id) {
          const themeId = settings.active_theme_id;
          if (PREDEFINED_THEMES.some(t => t.id === themeId) || this.customThemes.some(t => t.id === themeId)) {
            this.activeThemeId = themeId;
          }
        }
      }
      this.applyActiveTheme();
    } catch (e) {
      console.error("Failed to init ThemeStore:", e);
      this.applyActiveTheme();
    }
  }

  get currentTheme(): Theme {
    const predefined = PREDEFINED_THEMES.find(t => t.id === this.activeThemeId);
    if (predefined) return predefined;
    const custom = this.customThemes.find(t => t.id === this.activeThemeId);
    return custom || PREDEFINED_THEMES[0];
  }

  async setTheme(themeId: string) {
    if (PREDEFINED_THEMES.some(t => t.id === themeId) || this.customThemes.some(t => t.id === themeId)) {
      this.activeThemeId = themeId;
      this.applyActiveTheme();
      await invoke("set_app_setting", { key: "active_theme_id", value: themeId }).catch(err => {
        console.error("Failed to save active_theme_id:", err);
      });
    }
  }

  async addCustomTheme(theme: Theme) {
    const existingIndex = this.customThemes.findIndex(t => t.id === theme.id);
    if (existingIndex >= 0) {
      this.customThemes[existingIndex] = theme;
    } else {
      this.customThemes.push(theme);
    }
    this.activeThemeId = theme.id;
    this.applyActiveTheme();

    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) }).catch(err => {
      console.error("Failed to save custom_themes:", err);
    });
    await invoke("set_app_setting", { key: "active_theme_id", value: theme.id }).catch(err => {
      console.error("Failed to save active_theme_id:", err);
    });
  }

  async deleteCustomTheme(themeId: string) {
    this.customThemes = this.customThemes.filter(t => t.id !== themeId);
    await invoke("set_app_setting", { key: "custom_themes", value: JSON.stringify(this.customThemes) }).catch(err => {
      console.error("Failed to save custom_themes:", err);
    });

    if (this.activeThemeId === themeId) {
      await this.setTheme("luminous-violet");
    }
  }

  applyActiveTheme() {
    if (typeof document === "undefined") return;
    const theme = this.currentTheme;
    let styleEl = document.getElementById("luminous-theme-style");
    if (!styleEl) {
      styleEl = document.createElement("style");
      styleEl.id = "luminous-theme-style";
      document.head.appendChild(styleEl);
    }

    styleEl.innerHTML = `
      :root {
        --bg-main: ${theme.colors["bg-main"]};
        --bg-sidebar: ${theme.colors["bg-sidebar"]};
        --bg-playerbar: ${theme.colors["bg-playerbar"]};
        --color-accent: ${theme.colors["color-accent"]};
        --color-accent-hover: ${theme.colors["color-accent-hover"]};
        --color-text-primary: ${theme.colors["color-text-primary"]};
        --color-text-secondary: ${theme.colors["color-text-secondary"]};
        --color-border: ${theme.colors["color-border"]};
      }
    `;
  }
}

export const themeStore = new ThemeStore();
