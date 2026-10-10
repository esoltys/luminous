import { defineScene } from "./types";
import { featuredSongId } from "./helpers";

const WIDE_SIDEBAR = { sidebarWidth: 220 };

export const collectionScenes = [
  defineScene({ name: "home", file: "home.png", view: { tab: "home" }, layout: WIDE_SIDEBAR }),
  defineScene({ name: "albums", file: "albums.png", view: { tab: "collection", subTab: "albums" }, layout: WIDE_SIDEBAR }),
  defineScene({ name: "artists", file: "artists.png", view: { tab: "collection", subTab: "artists" }, layout: WIDE_SIDEBAR }),
  defineScene({ name: "songs", file: "songs.png", view: { tab: "collection", subTab: "songs" }, layout: WIDE_SIDEBAR }),
  defineScene({ name: "genres", file: "genres.png", view: { tab: "collection", subTab: "genres" }, layout: WIDE_SIDEBAR }),
  defineScene({
    name: "artist-detail",
    file: "artist-detail.png",
    view: { tab: "collection", subTab: "artists" },
    layout: WIDE_SIDEBAR,
    online: true,
    run: async ({ api, featured }) => api.navigate.artist(featured.artist!),
  }),
  defineScene({
    name: "album-detail",
    file: "album-detail.png",
    view: { tab: "collection", subTab: "albums" },
    theme: "dynamic-artwork",
    layout: { sidebarWidth: 64 },
    position: 81,
    run: async ({ api, featured }) => api.navigate.album(featured.album!),
  }),
  defineScene({
    name: "album-tag-editor",
    file: "album-tag-editor.png",
    view: { tab: "collection", subTab: "albums" },
    layout: WIDE_SIDEBAR,
    run: async ({ api, featured }) => {
      await api.navigate.album(featured.album!);
      await api.screens.openAlbumEditor();
    },
  }),
  defineScene({
    name: "song-tag-editor",
    file: "song-tag-editor.png",
    view: { tab: "collection", subTab: "songs" },
    layout: WIDE_SIDEBAR,
    run: async (ctx) => ctx.api.dialogs.openTagEditor(await featuredSongId(ctx)),
  }),
  defineScene({
    name: "advanced-search",
    file: "advanced-search.png",
    view: { tab: "collection", subTab: "songs" },
    layout: WIDE_SIDEBAR,
    // Reveal the Key column so the table behind the dropdown shows the values "key:d" matches.
    run: async ({ api }) => {
      await api.view.setColumnVisible("initial_key", true);
      await api.screens.search("key:d");
    },
    cleanup: async ({ api }) => api.view.setColumnVisible("initial_key", false),
  }),
  defineScene({
    name: "search",
    file: "search.png",
    view: { tab: "home" },
    run: async ({ api }) => api.screens.search("evan"),
  }),
  defineScene({
    name: "band-waveform",
    file: "band-waveform.png",
    view: { tab: "collection", subTab: "songs" },
    layout: WIDE_SIDEBAR,
    position: 79,
    run: async ({ api }) => api.view.setSeekbarMode("bands"),
    cleanup: async ({ api }) => api.view.setSeekbarMode("waveform"),
  }),
  defineScene({
    name: "lyrics",
    file: "lyrics.png",
    view: { tab: "lyrics" },
    theme: "dynamic-artwork",
    featured: { song: "Bloodsport", artist: "The Warning" },
    layout: { sidebarWidth: 64 },
    position: 163,
    online: true,
  }),
  defineScene({
    name: "now-playing",
    file: "now-playing.png",
    view: { tab: "collection", subTab: "songs" },
    theme: "dynamic-artwork",
    layout: { immersive: true },
    position: 79,
  }),
  defineScene({
    name: "playbar",
    file: "playbar.png",
    view: { tab: "collection", subTab: "songs" },
    layout: WIDE_SIDEBAR,
    viewport: { width: 1640, height: 800 },
    position: 150,
    clip: "footer.bg-brand-playerbar",
  }),
  defineScene({
    name: "searchbar",
    file: "searchbar.png",
    view: { tab: "collection", subTab: "songs" },
    viewport: { width: 1200, height: 800 },
    clip: "header[data-walkthrough-target='top-navigation']",
  }),
];
