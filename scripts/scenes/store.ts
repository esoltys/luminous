import { defineScene } from "./types";

// Microsoft Store listing captures that differ from the guide's, dark only. esoltys/luminous-store
// composites these and the guide's own captures into the Store screenshots.

export const storeScenes = [
  defineScene({
    name: "store-equalizer",
    file: "store-equalizer.png",
    view: { tab: "settings", settings: "equalizer" },
    schemes: ["dark"],
    // The guide's parametric scene opens the preset menu; the Store shows just the curve.
    run: async ({ api }) => {
      await api.screens.setEqualizerMode("parametric");
      await api.screens.selectEqualizerPreset("Rock");
    },
    cleanup: async ({ api }) => api.screens.setEqualizerMode("graphic10"),
  }),
];
