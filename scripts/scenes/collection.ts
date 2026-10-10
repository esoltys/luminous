import { defineScene } from "./types";

const WIDE_SIDEBAR = { sidebarWidth: 220 };

export const collectionScenes = [
  defineScene({ name: "home", file: "home.png", view: { tab: "home" }, layout: WIDE_SIDEBAR }),
  defineScene({ name: "albums", file: "albums.png", view: { tab: "collection", subTab: "albums" }, layout: WIDE_SIDEBAR }),
];
