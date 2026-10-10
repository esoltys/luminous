import { defineScene } from "./types";

export const firstRunScenes = [
  // Home before any folder is added; runs against the fresh profile.
  defineScene({ name: "home-welcome", file: "home-welcome.png", stage: "fresh", view: { tab: "home" }, layout: { sidebarWidth: 220 } }),
  defineScene({
    name: "walkthrough",
    file: "walkthrough.png",
    view: { tab: "help" },
    layout: { sidebarWidth: 220 },
    viewport: { width: 1640, height: 1090 },
    run: async ({ api }) => api.dialogs.walkthrough.start(),
  }),
];
