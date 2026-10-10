import { defineScene } from "./types";
import { hoverOver, unhover } from "./helpers";

const MINIPLAYER = {
  view: { tab: "collection", subTab: "songs" },
  theme: "dynamic-artwork",
  layout: { miniplayer: true },
  viewport: { width: 340, height: 420 },
  clip: "[role='group'][aria-label]",
} as const;

export const windowScenes = [
  defineScene({ name: "miniplayer", file: "miniplayer.png", ...MINIPLAYER }),
  defineScene({
    name: "miniplayer-hover",
    file: "miniplayer-hover.png",
    ...MINIPLAYER,
    // The hover controls only show while the pointer is over the window.
    run: async (ctx) => hoverOver(ctx, MINIPLAYER.clip),
    cleanup: unhover,
  }),
];
