import { invoke } from "@tauri-apps/api/core";
import type { Song } from "../types";

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

export interface ExtractedColors {
  primary: string;
  sidebar: string;
  playerbar: string;
  accent: string;
  accentHover: string;
  border: string;
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
    id: "ruby-red",
    name: "Ruby Red",
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
  },
  {
    id: "dynamic-artwork",
    name: "Dynamic Artwork 💿",
    colors: {
      "bg-main": "var(--color-artwork-primary)",
      "bg-sidebar": "var(--color-artwork-sidebar)",
      "bg-playerbar": "var(--color-artwork-playerbar)",
      "color-accent": "var(--color-artwork-accent)",
      "color-accent-hover": "var(--color-artwork-accent-hover)",
      "color-text-primary": "#ffffff",
      "color-text-secondary": "#e2e8f0",
      "color-border": "var(--color-artwork-border)"
    }
  }
];

export function extractColorsFromImage(imgUrl: string): Promise<ExtractedColors> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = "Anonymous";
    img.onload = () => {
      try {
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          resolve(getFallbackColors());
          return;
        }

        canvas.width = 40;
        canvas.height = 40;
        ctx.drawImage(img, 0, 0, 40, 40);

        const imgData = ctx.getImageData(0, 0, 40, 40);
        const data = imgData.data;

        const colorBuckets = new Map<string, number>();
        let count = 0;

        for (let i = 0; i < data.length; i += 4) {
          const r = data[i];
          const g = data[i + 1];
          const b = data[i + 2];
          const a = data[i + 3];

          if (a < 200) continue; // skip transparent

          count++;

          // Quantize color
          const qr = Math.floor(r / 16) * 16;
          const qg = Math.floor(g / 16) * 16;
          const qb = Math.floor(b / 16) * 16;
          const key = `${qr},${qg},${qb}`;

          colorBuckets.set(key, (colorBuckets.get(key) || 0) + 1);
        }

        if (count === 0) {
          resolve(getFallbackColors());
          return;
        }

        const sortedColors = Array.from(colorBuckets.entries())
          .sort((a, b) => b[1] - a[1])
          .map(([key]) => {
            const [r, g, b] = key.split(",").map(Number);
            return { r, g, b };
          });

        const dominant = sortedColors[0] || { r: 139, g: 92, b: 246 };

        // Find a saturated accent color
        let accent = dominant;
        let maxSaturation = 0;

        for (const color of sortedColors) {
          const { r, g, b } = color;
          const max = Math.max(r, g, b);
          const min = Math.min(r, g, b);
          const chroma = max - min;
          const saturation = max === 0 ? 0 : chroma / max;
          const brightness = max / 255;

          if (saturation > maxSaturation && brightness > 0.3 && brightness < 0.85) {
            maxSaturation = saturation;
            accent = color;
          }
        }

        if (maxSaturation < 0.15) {
          accent = dominant;
        }

        const toHex = (c: number) => {
          const hex = Math.min(255, Math.max(0, Math.round(c))).toString(16);
          return hex.length === 1 ? "0" + hex : hex;
        };

        const rgbToHex = (r: number, g: number, b: number) => `#${toHex(r)}${toHex(g)}${toHex(b)}`;

        const adjustBrightness = (color: { r: number, g: number, b: number }, factor: number) => {
          return {
            r: Math.min(255, Math.max(0, color.r * factor)),
            g: Math.min(255, Math.max(0, color.g * factor)),
            b: Math.min(255, Math.max(0, color.b * factor))
          };
        };

        // Darken primary background for dark-mode readability
        let primaryColor = { ...dominant };
        const primaryBrightness = (primaryColor.r * 0.299 + primaryColor.g * 0.587 + primaryColor.b * 0.114) / 255;
        if (primaryBrightness > 0.15) {
          primaryColor = adjustBrightness(primaryColor, 0.10 / primaryBrightness);
        } else if (primaryBrightness < 0.05) {
          primaryColor = adjustBrightness(primaryColor, 1.5);
        }

        const sidebarColor = adjustBrightness(primaryColor, 0.6);
        const playerbarColor = adjustBrightness(primaryColor, 0.8);

        // Normalize accent brightness
        let accentColor = { ...accent };
        const accentBrightness = (accentColor.r * 0.299 + accentColor.g * 0.587 + accentColor.b * 0.114) / 255;
        if (accentBrightness < 0.45) {
          accentColor = adjustBrightness(accentColor, 0.6 / Math.max(0.1, accentBrightness));
        }

        const accentHoverColor = adjustBrightness(accentColor, 1.2);
        const borderColor = adjustBrightness(primaryColor, 2.2);

        resolve({
          primary: rgbToHex(primaryColor.r, primaryColor.g, primaryColor.b),
          sidebar: rgbToHex(sidebarColor.r, sidebarColor.g, sidebarColor.b),
          playerbar: rgbToHex(playerbarColor.r, playerbarColor.g, playerbarColor.b),
          accent: rgbToHex(accentColor.r, accentColor.g, accentColor.b),
          accentHover: rgbToHex(accentHoverColor.r, accentHoverColor.g, accentHoverColor.b),
          border: rgbToHex(borderColor.r, borderColor.g, borderColor.b)
        });
      } catch (e) {
        console.error("Failed to process image colors:", e);
        resolve(getFallbackColors());
      }
    };

    img.onerror = () => {
      resolve(getFallbackColors());
    };

    img.src = imgUrl;
  });
}

function getFallbackColors(): ExtractedColors {
  return {
    primary: "#0d0b18",
    sidebar: "#07050e",
    playerbar: "#0a0813",
    accent: "#8b5cf6",
    accentHover: "#a78bfa",
    border: "#1f1b2e"
  };
}

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

  async updateArtworkColors(song: Song | undefined) {
    if (!song) {
      this.resetArtworkColors();
      return;
    }

    let url: string | null = null;
    if (song.art_manual) {
      url = `luminous-art://${song.art_manual}`;
    } else if (song.art_automatic) {
      if (song.art_automatic.startsWith("album-")) {
        url = `luminous-art://${song.art_automatic}`;
      } else {
        url = `luminous-art://local/${song.art_automatic}`;
      }
    } else if (song.art_embedded) {
      try {
        const uri = await invoke<string | null>("get_cover_art_uri", { songId: song.id });
        if (uri) {
          url = uri;
        }
      } catch (e) {
        console.error("Failed to query cover art URI in themeStore:", e);
      }
    }

    if (!url) {
      this.resetArtworkColors();
      return;
    }

    try {
      const colors = await extractColorsFromImage(url);
      this.applyArtworkColors(colors);
    } catch (e) {
      console.error("Failed to extract artwork colors:", e);
      this.resetArtworkColors();
    }
  }

  applyArtworkColors(colors: ExtractedColors) {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.style.setProperty("--color-artwork-primary", colors.primary);
    root.style.setProperty("--color-artwork-sidebar", colors.sidebar);
    root.style.setProperty("--color-artwork-playerbar", colors.playerbar);
    root.style.setProperty("--color-artwork-accent", colors.accent);
    root.style.setProperty("--color-artwork-accent-hover", colors.accentHover);
    root.style.setProperty("--color-artwork-border", colors.border);
  }

  resetArtworkColors() {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    root.style.setProperty("--color-artwork-primary", "#0d0b18");
    root.style.setProperty("--color-artwork-sidebar", "#07050e");
    root.style.setProperty("--color-artwork-playerbar", "#0a0813");
    root.style.setProperty("--color-artwork-accent", "#8b5cf6");
    root.style.setProperty("--color-artwork-accent-hover", "#a78bfa");
    root.style.setProperty("--color-artwork-border", "#1f1b2e");
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
