import { defineScene } from "./types";
import { ensureCustomPlaylist } from "./helpers";

const WIDE_SIDEBAR = { sidebarWidth: 220 };

export const playlistScenes = [
  defineScene({
    name: "playlist",
    file: "playlist.png",
    view: { tab: "playlists", subTab: "custom" },
    layout: WIDE_SIDEBAR,
    run: async (ctx) => ensureCustomPlaylist(ctx, "Road Trip"),
  }),
  defineScene({
    name: "playlist-auto",
    file: "playlist-auto.png",
    view: { tab: "playlists", subTab: "auto" },
    layout: WIDE_SIDEBAR,
    views: { playlistsAuto: "rows" },
  }),
  defineScene({
    name: "playlist-auto-detail",
    file: "playlist-auto-detail.png",
    view: { tab: "playlists", subTab: "auto" },
    layout: WIDE_SIDEBAR,
    position: 79,
    run: async ({ api }) => api.navigate.playlist("recently_added"),
  }),
  defineScene({
    name: "playlist-smart-edit",
    file: "playlist-smart-edit.png",
    view: { tab: "playlists", subTab: "custom" },
    layout: WIDE_SIDEBAR,
    run: async ({ api }) =>
      api.view.openSmartPlaylistBuilder([
        { field: "genre", op: "contains", value: "Rock" },
        { field: "year", op: ">=", value: "1980" },
        { field: "year", op: "<=", value: "1989" },
      ]),
  }),
];
