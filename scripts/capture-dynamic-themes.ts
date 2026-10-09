// Captures the Dynamic Artwork theme slideshow from the REAL running app, not the
// mocked IPC bridge: it plays each song for real (real waveform, real cover-art
// colors from the track-changed path) and screenshots that song's album detail.
// Start `bun run tauri dev` first (remote devtools on :9222), and nothing else
// that should keep playing — this REPLACES THE QUEUE and moves playback.
// Entries are the `liveApp` rows in mock-config.json. It switches the app to the
// Dynamic Artwork ("✨ Luminous") theme itself and restores your theme afterwards.
// Usage: bun scripts/capture-dynamic-themes.ts [--name=theme-dynamic-<artist>]
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { DevtoolsDriver } from "./devtools-driver";
import { DEFAULT_VIEWPORT, loadMockConfig } from "./mock-library";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, "../docs/user-guide/assets/en-CA/screenshots/dynamic");

interface Song {
  id: number;
  title: string;
  artist: string;
  album: string;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function main() {
  const nameFilter = process.argv.find((a) => a.startsWith("--name="))?.slice("--name=".length);
  const entries = (loadMockConfig().screenshots ?? []).filter(
    (s) => s.liveApp && (!nameFilter || s.name === nameFilter)
  );
  if (entries.length === 0) throw new Error("No theme-dynamic-* entries to capture.");

  const driver = await DevtoolsDriver.connect();

  const waitForAlbumsReady = async () => {
    await driver.waitForCondition(
      async () => {
        return await driver.evaluate(() =>
          Array.from(document.querySelectorAll("button")).some((b) =>
            /^Albums/.test(b.textContent?.trim() ?? "")
          )
        );
      },
      { timeoutMs: 60000, message: "Albums button not found after reload" }
    );
  };

  // The theme is saved in the app's settings and read on load, so set it there
  // and reload; startup applies it exactly as a relaunch would.
  const DYNAMIC_THEME_ID = "dynamic-artwork";
  const setTheme = async (id: string) => {
    await driver.invoke("set_app_setting", { key: "active_theme_id", value: id });
    await driver.reload();
    await waitForAlbumsReady();
  };
  const settings = await driver.invoke<Record<string, string>>("get_all_app_settings");
  const originalThemeId = settings.active_theme_id ?? "system";
  if (originalThemeId !== DYNAMIC_THEME_ID) await setTheme(DYNAMIC_THEME_ID);

  // Album Info starts collapsed so the track list gets the room; it is a saved
  // layout preference, so put it back the way it was afterwards.
  const OVERVIEW_KEY = "layout_isOverviewExpanded";
  const originalOverview = await driver.evaluate((k) => localStorage.getItem(k), OVERVIEW_KEY);
  const setOverview = async (value: string | null) => {
    await driver.evaluate(
      ([k, v]) => (v === null ? localStorage.removeItem(k) : localStorage.setItem(k, v)),
      [OVERVIEW_KEY, value] as const
    );
    await driver.reload();
    await waitForAlbumsReady();
  };
  if (originalOverview !== "false") await setOverview("false");

  // Online services off keeps enrichment toasts and fetched panels out of the
  // frame; the original setting is restored afterwards.
  const originalOnline = await driver.invoke<boolean>("is_context_enrichment_enabled");
  if (originalOnline) await driver.invoke("set_online_enabled", { enabled: false });

  fs.mkdirSync(OUT_DIR, { recursive: true });

  try {
    for (const entry of entries) {
      console.log(`Capturing ${entry.name}...`);
      await driver.setWindowSize(
        entry.viewportWidth ?? DEFAULT_VIEWPORT.width,
        entry.viewportHeight ?? DEFAULT_VIEWPORT.height
      );

      const results = await driver.invoke<Song[]>("search_songs", {
        query: entry.featuredSong,
        limit: 200,
      });
      const song = results.find(
        (s) => s.title === entry.featuredSong && s.artist === entry.featuredArtist
      );
      if (!song)
        throw new Error(
          `Song "${entry.featuredSong}" by ${entry.featuredArtist} not found in the library.`
        );

      // Play first: the theme re-extracts from the playing song's own cover on
      // track-changed, which is what a user sees after pressing Play.
      await driver.invoke("play_song", { songId: song.id });

      // Wait on the backend's own state: matching the title in the DOM also hits hidden views.
      await driver.waitForCondition(
        async () => {
          const playback = await driver.invoke<{
            state: string;
            current_song?: { id: number };
          }>("get_playback_state");
          return playback.state === "playing" && playback.current_song?.id === song.id;
        },
        { timeoutMs: 20000, message: `"${song.title}" never started playing.` }
      );

      await driver.invoke("seek_to", {
        positionNanosec: Math.round((entry.positionSeconds ?? 60) * 1e9),
      });

      // Navigate without reloading (a reload would re-run the startup theme
      // path): search for the album and open its suggestion. Independent of the
      // Albums grid/rows view mode and of where the album sits in the list.
      for (let attempt = 1; ; attempt++) {
        const opened = await driver.evaluate(async (name) => {
          const search = document.querySelector<HTMLInputElement>("header input[type='text']");
          if (!search) return false;

          search.focus();
          search.value = name;
          search.dispatchEvent(new Event("input", { bubbles: true }));

          await new Promise((r) => setTimeout(r, 400));

          const elements = Array.from(document.querySelectorAll("*"));
          const match =
            elements.find(
              (el) =>
                el.textContent?.trim() === name &&
                (el.getAttribute("role") === "option" ||
                  el.closest("[role='listbox']") ||
                  el.closest("ul"))
            ) ?? elements.find((el) => el.textContent?.trim() === name);

          if (match && match instanceof HTMLElement) {
            match.click();
          }

          await new Promise((r) => setTimeout(r, 400));
          search.value = "";
          search.dispatchEvent(new Event("input", { bubbles: true }));
          search.blur();

          const headings = Array.from(document.querySelectorAll("h1"));
          return headings.some(
            (h) => h.textContent?.includes(name) && (h as HTMLElement).offsetParent !== null
          );
        }, song.album);

        if (opened) break;
        if (attempt >= 3)
          throw new Error(`Couldn't open the album view for "${song.album}".`);
        await sleep(800);
      }

      // Dismiss any notification toasts
      await driver.evaluate(() => {
        const buttons = document.querySelectorAll<HTMLElement>(
          "button[aria-label='Dismiss notification']"
        );
        for (const b of buttons) b.click();
      });

      await sleep(1500); // let the waveform and theme crossfade settle

      await driver.screenshot({ path: path.join(OUT_DIR, entry.filename) });
      console.log(
        `Saved ${path.relative(path.join(__dirname, ".."), path.join(OUT_DIR, entry.filename))}`
      );
    }
  } finally {
    await driver.clearWindowSize();
    if (originalOnline) await driver.invoke("set_online_enabled", { enabled: true });
    if (originalOverview !== "false") await setOverview(originalOverview);
    if (originalThemeId !== DYNAMIC_THEME_ID) await setTheme(originalThemeId);
    await driver.close();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
