// Captures the Dynamic Artwork theme slideshow from the REAL running app, not the
// mocked IPC bridge: it plays each song for real (real waveform, real cover-art
// colors from the track-changed path) and screenshots that song's album detail.
// Start `bun run tauri dev` first (remote devtools on :9222), and nothing else
// that should keep playing — this REPLACES THE QUEUE and moves playback.
// Entries are the `liveApp` rows in mock-config.json. It switches the app to the
// Dynamic Artwork ("✨ Luminous") theme itself and restores your theme afterwards.
// Usage: bunx tsx scripts/capture-dynamic-themes.ts [--name=theme-dynamic-<artist>]
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { DEFAULT_VIEWPORT, loadMockConfig } from "./mock-library";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, "../docs/user-guide/assets/en-CA/screenshots/dynamic");
const DEVTOOLS_URL = "http://127.0.0.1:9222";

interface Song {
  id: number;
  title: string;
  artist: string;
  album: string;
}

async function main() {
  const nameFilter = process.argv.find((a) => a.startsWith("--name="))?.slice("--name=".length);
  const entries = (loadMockConfig().screenshots ?? []).filter(
    (s) => s.liveApp && (!nameFilter || s.name === nameFilter)
  );
  if (entries.length === 0) throw new Error("No theme-dynamic-* entries to capture.");

  const { chromium } = await import("playwright");
  const browser = await chromium.connectOverCDP(DEVTOOLS_URL);
  const page = browser.contexts()[0]?.pages()[0];
  if (!page) throw new Error(`No app page found at ${DEVTOOLS_URL}; is "bun run tauri dev" running?`);

  const invoke = <T>(cmd: string, args: Record<string, unknown> = {}) =>
    page.evaluate(
      ([c, a]) => (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string, a: unknown) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke(c as string, a),
      [cmd, args] as const
    ) as Promise<T>;

  // The theme is saved in the app's settings and read on load, so set it there
  // and reload; startup applies it exactly as a relaunch would.
  const DYNAMIC_THEME_ID = "dynamic-artwork";
  const setTheme = async (id: string) => {
    await invoke("set_app_setting", { key: "active_theme_id", value: id });
    await page.reload();
    await page.getByRole("button", { name: /^Albums/ }).first().waitFor({ timeout: 60000 });
  };
  const settings = await invoke<Record<string, string>>("get_all_app_settings");
  const originalThemeId = settings.active_theme_id ?? "system";
  if (originalThemeId !== DYNAMIC_THEME_ID) await setTheme(DYNAMIC_THEME_ID);

  // Album Info starts collapsed so the track list gets the room; it is a saved
  // layout preference, so put it back the way it was afterwards.
  const OVERVIEW_KEY = "layout_isOverviewExpanded";
  const originalOverview = await page.evaluate((k) => localStorage.getItem(k), OVERVIEW_KEY);
  const setOverview = async (value: string | null) => {
    await page.evaluate(([k, v]) => (v === null ? localStorage.removeItem(k) : localStorage.setItem(k, v)), [OVERVIEW_KEY, value] as const);
    await page.reload();
    await page.getByRole("button", { name: /^Albums/ }).first().waitFor({ timeout: 60000 });
  };
  if (originalOverview !== "false") await setOverview("false");

  // Online services off keeps enrichment toasts and fetched panels out of the
  // frame; the original setting is restored afterwards.
  const originalOnline = await invoke<boolean>("is_context_enrichment_enabled");
  if (originalOnline) await invoke("set_online_enabled", { enabled: false });

  fs.mkdirSync(OUT_DIR, { recursive: true });
  // A connected real window reports no viewport, so undo the override via CDP, not setViewportSize.
  const cdp = await page.context().newCDPSession(page);
  try {
    for (const entry of entries) {
      console.log(`Capturing ${entry.name}...`);
      await page.setViewportSize({ width: entry.viewportWidth ?? DEFAULT_VIEWPORT.width, height: entry.viewportHeight ?? DEFAULT_VIEWPORT.height });

      const results = await invoke<Song[]>("search_songs", { query: entry.featuredSong, limit: 200 });
      const song = results.find((s) => s.title === entry.featuredSong && s.artist === entry.featuredArtist);
      if (!song) throw new Error(`Song "${entry.featuredSong}" by ${entry.featuredArtist} not found in the library.`);

      // Play first: the theme re-extracts from the playing song's own cover on
      // track-changed, which is what a user sees after pressing Play.
      await invoke("play_song", { songId: song.id });
      // Wait on the backend's own state: matching the title in the DOM also hits hidden views.
      const deadline = Date.now() + 20000;
      for (;;) {
        const playback = await invoke<{ state: string; current_song?: { id: number } }>("get_playback_state");
        if (playback.state === "playing" && playback.current_song?.id === song.id) break;
        if (Date.now() > deadline) throw new Error(`"${song.title}" never started playing.`);
        await page.waitForTimeout(250);
      }
      await invoke("seek_to", { positionNanosec: Math.round((entry.positionSeconds ?? 60) * 1e9) });

      // Navigate without reloading (a reload would re-run the startup theme
      // path): search for the album and open its suggestion. Independent of the
      // Albums grid/rows view mode and of where the album sits in the list.
      const search = page.locator("header input[type='text']").first();
      const albumHeading = page.locator("h1:visible", { hasText: song.album }).first();
      // The open can race the search box's own state, so verify the album view
      // is showing with an empty search and retry a few times.
      for (let attempt = 1; ; attempt++) {
        await search.fill(song.album);
        await page.getByText(song.album, { exact: true }).first().click();
        await albumHeading.waitFor({ timeout: 15000 });
        await search.fill("");
        await page.keyboard.press("Escape");
        await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
        await page.waitForTimeout(800);
        if ((await albumHeading.isVisible()) && (await search.inputValue()) === "") break;
        if (attempt >= 3) throw new Error(`Couldn't open the album view for "${song.album}".`);
      }
      for (const toast of await page.getByLabel("Dismiss notification").all()) await toast.click().catch(() => {});
      await page.waitForTimeout(1500); // let the waveform and theme crossfade settle

      await page.screenshot({ path: path.join(OUT_DIR, entry.filename) });
      console.log(`Saved ${path.relative(path.join(__dirname, ".."), path.join(OUT_DIR, entry.filename))}`);
    }
  } finally {
    await cdp.send("Emulation.clearDeviceMetricsOverride");
    if (originalOnline) await invoke("set_online_enabled", { enabled: true });
    if (originalOverview !== "false") await setOverview(originalOverview);
    if (originalThemeId !== DYNAMIC_THEME_ID) await setTheme(originalThemeId);
    await browser.close();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
