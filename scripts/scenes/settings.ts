import { defineScene } from "./types";

const tall = (height: number) => ({ width: 1280, height });

export const settingsScenes = [
  defineScene({ name: "settings-general", file: "settings-general.png", view: { tab: "settings", settings: "general" } }),
  defineScene({ name: "settings-system", file: "settings-system.png", view: { tab: "settings", settings: "system" }, viewport: tall(1100) }),
  defineScene({ name: "settings-folders", file: "settings-folders.png", view: { tab: "settings", settings: "sources" }, viewport: tall(1100) }),
  defineScene({ name: "settings-integrations", file: "settings-integrations.png", view: { tab: "settings", settings: "integrations" }, viewport: tall(850) }),
  defineScene({ name: "settings-about", file: "settings-about.png", view: { tab: "settings", settings: "about" }, viewport: tall(950) }),
  defineScene({
    name: "themes",
    file: "themes.png",
    view: { tab: "settings", settings: "themes" },
    featured: { song: "Got Your Number", artist: "Serena Ryder" },
  }),
  defineScene({
    name: "equalizer",
    file: "equalizer.png",
    view: { tab: "settings", settings: "equalizer" },
    viewport: tall(1500),
    position: 79,
    run: async ({ api }) => api.screens.setEqualizerMode("graphic10"),
  }),
  defineScene({
    name: "equalizer-parametric",
    file: "equalizer-parametric.png",
    view: { tab: "settings", settings: "equalizer" },
    viewport: tall(1600),
    position: 79,
    // Open the preset actions menu so the guide shows where Import, Export and the user-preset actions live.
    run: async ({ api }) => {
      await api.screens.setEqualizerMode("parametric");
      await api.screens.openEqualizerPresetMenu();
    },
    cleanup: async ({ api }) => api.screens.setEqualizerMode("graphic10"),
  }),
  defineScene({
    name: "organize",
    file: "organize.png",
    view: { tab: "organize" },
    layout: { sidebarWidth: 220 },
    viewport: tall(1600),
    // A template that actually changes this library's paths, so the preview isn't all "Unchanged".
    run: async ({ api }) => api.view.setOrganizeTemplate("%albumartist/{%album/}{Disc %disc/}{%track }%title"),
  }),
  defineScene({ name: "stats", file: "stats.png", view: { tab: "stats" }, layout: { sidebarWidth: 220 } }),
];
