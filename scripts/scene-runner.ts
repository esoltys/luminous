// Runs scenes (see scenes/types.ts) against the real app over CDP.
//
// The app is booted once. The runner loops locale × color scheme, sets each once
// per pass, then runs every applicable scene without restarting anything. Each
// scene is reset to a known state first, so a scene only declares what differs.
import * as path from "node:path";
import type { DevtoolsDriver } from "./devtools-driver";
import type { ColorScheme, Featured, RemoteApi, Scene, SceneContext, SceneLayout } from "./scenes/types";

/** The slice of DevtoolsDriver the runner uses, so tests can fake it. */
type SceneDriver = Pick<DevtoolsDriver, "evaluate" | "screenshot" | "setWindowSize" | "invoke">;

export const ALL_SCHEMES: ColorScheme[] = ["light", "dark"];

/** What a scene gets when it doesn't say otherwise. */
const SCENE_DEFAULTS = {
  theme: "system",
  layout: { sidebarOpen: true, rightPanelOpen: false, sidebarWidth: 64, immersive: false, miniplayer: false } satisfies SceneLayout,
  viewport: { width: 1280, height: 800 },
  position: 176,
  featured: { song: "I Get Weak", artist: "Cannons", album: "Everything Glows" } satisfies Featured,
  settleMs: 400,
};

export interface Pass {
  stage: "fresh" | "library";
  locale: string;
  scheme: ColorScheme;
  scenes: Scene[];
}

export interface PlanFilters {
  locales: string[];
  schemes?: ColorScheme[];
  name?: string;
  stage?: "fresh" | "library";
}

/**
 * Expands scenes into passes: fresh-stage passes first (no library yet), then
 * library passes. A scene writing to its own subfolder runs once, in the first
 * scheme pass of each locale. Scenes that switch to the miniplayer window run
 * last in a pass so the full window is never restored mid-pass.
 */
export function planPasses(scenes: Scene[], filters: PlanFilters): Pass[] {
  const selected = filters.name ? scenes.filter((s) => s.name === filters.name) : scenes;
  if (filters.name && selected.length === 0) {
    throw new Error(`No scene named "${filters.name}". Valid names: ${scenes.map((s) => s.name).join(", ")}`);
  }
  const schemes = filters.schemes ?? ALL_SCHEMES;
  const passes: Pass[] = [];
  for (const stage of ["fresh", "library"] as const) {
    if (filters.stage && filters.stage !== stage) continue;
    const inStage = selected.filter((s) => (s.stage ?? "library") === stage);
    for (const locale of filters.locales) {
      const firstSchemeSeen = new Set<string>();
      for (const scheme of schemes) {
        const applicable = inStage.filter((s) => {
          if (s.locales && !s.locales.includes(locale)) return false;
          if (s.schemes && !s.schemes.includes(scheme)) return false;
          if (s.outputSubdir) {
            if (firstSchemeSeen.has(s.name)) return false;
            firstSchemeSeen.add(s.name);
          }
          return true;
        });
        if (applicable.length === 0) continue;
        const ordered = [...applicable.filter((s) => !s.layout?.miniplayer), ...applicable.filter((s) => s.layout?.miniplayer)];
        passes.push({ stage, locale, scheme, scenes: ordered });
      }
    }
  }
  return passes;
}

export function outputPath(outRoot: string, scene: Scene, locale: string, scheme: ColorScheme): string {
  return path.join(outRoot, locale, "screenshots", scene.outputSubdir ?? scheme, scene.file);
}

interface SceneFailure {
  scene: string;
  locale: string;
  scheme: ColorScheme;
  error: string;
}

interface SceneTiming {
  scene: string;
  locale: string;
  scheme: ColorScheme;
  ms: number;
}

export interface RunReport {
  saved: string[];
  failures: SceneFailure[];
  timings: SceneTiming[];
  totalMs: number;
}

export interface RunOptions {
  driver: SceneDriver;
  /** The driver again, typed as a full DevtoolsDriver for scenes' escape hatch. */
  sceneDriver?: DevtoolsDriver;
  api: RemoteApi;
  outRoot: string;
  filters: PlanFilters;
  scenes: Scene[];
  /** Looks up a UI string in a locale. */
  translate(locale: string, keyPath: string): string;
  /** Runs between the fresh and library phases: add the library, seed startup prefs. */
  prepareLibrary?(): Promise<void>;
  log?(line: string): void;
  sleep?(ms: number): Promise<void>;
}

const realSleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

// Runs in the page: waits for every <img> (forcing lazy ones to load) and two frames.
const SETTLE_PAGE = async () => {
  const imgs = Array.from(document.images);
  await Promise.all(
    imgs.map((img) => {
      if (img.loading === "lazy") img.loading = "eager";
      if (img.complete) return undefined;
      return new Promise((resolve) => {
        img.addEventListener("load", resolve, { once: true });
        img.addEventListener("error", resolve, { once: true });
      });
    })
  );
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  await window.__LUMINOUS_SCRIPT__!.wait.settled();
};

const ELEMENT_RECT = (selector: string) => {
  const el = document.querySelector(selector);
  if (!el) return null;
  const r = el.getBoundingClientRect();
  return { x: r.x, y: r.y, width: r.width, height: r.height };
};

/** Keeps a scene's seek target inside the song it plays, so a short song doesn't end. */
export function clampPosition(position: number, lengthNanosec: number | undefined): number {
  if (!lengthNanosec) return position;
  const seconds = lengthNanosec / 1e9;
  return position > seconds - 10 ? Math.max(0, Math.floor(seconds / 2)) : position;
}

export async function runScenes(opts: RunOptions): Promise<RunReport> {
  const { driver, api, outRoot } = opts;
  const log = opts.log ?? ((line: string) => console.log(line));
  const sleep = opts.sleep ?? realSleep;
  const started = Date.now();
  const report: RunReport = { saved: [], failures: [], timings: [], totalMs: 0 };

  const passes = planPasses(opts.scenes, opts.filters);
  const total = passes.reduce((n, p) => n + p.scenes.length, 0);
  let done = 0;

  let viewportKey = "";
  let playingKey = "";
  let onlineNow: boolean | undefined;
  let playingLength: number | undefined;
  let libraryPrepared = false;
  let currentLocale = "";
  let currentScheme = "";

  async function resetAndCapture(scene: Scene, pass: Pass): Promise<string> {
    const featured = { ...SCENE_DEFAULTS.featured, ...scene.featured };
    const layout = { ...SCENE_DEFAULTS.layout, ...scene.layout };
    const ctx: SceneContext = {
      api,
      driver: (opts.sceneDriver ?? driver) as DevtoolsDriver,
      locale: pass.locale,
      scheme: pass.scheme,
      featured,
      t: (keyPath) => opts.translate(pass.locale, keyPath),
    };

    // Leave the miniplayer first: it is a real window change and the rest of the reset assumes the full window.
    await api.appearance.setLayout({ miniplayer: false });
    await api.dialogs.closeAll();

    const viewport = scene.viewport ?? SCENE_DEFAULTS.viewport;
    const key = `${viewport.width}x${viewport.height}`;
    if (key !== viewportKey) {
      await driver.setWindowSize(viewport.width, viewport.height);
      viewportKey = key;
    }

    const online = scene.online ?? false;
    if (online !== onlineNow) {
      await driver.invoke("set_online_enabled", { enabled: online });
      onlineNow = online;
    }

    await api.appearance.setTheme(scene.theme ?? SCENE_DEFAULTS.theme);

    if (pass.stage === "library") {
      const songKey = `${featured.artist}\u0000${featured.song}`;
      if (songKey !== playingKey) {
        const played = await api.playback.play({ title: featured.song, artist: featured.artist });
        playingLength = played.length_nanosec;
        playingKey = songKey;
      }
      await api.playback.seek(clampPosition(scene.position ?? SCENE_DEFAULTS.position, playingLength));
    }

    if (scene.view?.settings) await api.navigate.settings(scene.view.settings as never);
    else if (scene.view) await api.navigate.to(scene.view.tab as never, scene.view.subTab as never);

    const { miniplayer, ...fullWindow } = layout;
    await api.appearance.setLayout(fullWindow);
    if (scene.run) await scene.run(ctx);
    if (miniplayer) await api.appearance.setLayout({ miniplayer: true });

    await api.dialogs.dismissToasts();
    await driver.evaluate(SETTLE_PAGE);
    await sleep(scene.settleMs ?? SCENE_DEFAULTS.settleMs);

    const file = outputPath(outRoot, scene, pass.locale, pass.scheme);
    if (scene.clip) {
      const clip = await driver.evaluate(ELEMENT_RECT, scene.clip);
      if (!clip) throw new Error(`No element matches "${scene.clip}"`);
      await driver.screenshot({ path: file, clip });
    } else {
      await driver.screenshot({ path: file });
    }
    return file;
  }

  for (const pass of passes) {
    if (pass.stage === "library" && !libraryPrepared) {
      await opts.prepareLibrary?.();
      libraryPrepared = true;
      viewportKey = "";
      playingKey = "";
    }
    if (pass.locale !== currentLocale) {
      await api.appearance.setLocale(pass.locale as never);
      currentLocale = pass.locale;
    }
    if (pass.scheme !== currentScheme) {
      await api.appearance.setColorScheme(pass.scheme);
      currentScheme = pass.scheme;
    }
    for (const scene of pass.scenes) {
      done++;
      const t0 = Date.now();
      try {
        const file = await resetAndCapture(scene, pass);
        report.saved.push(file);
        const ms = Date.now() - t0;
        report.timings.push({ scene: scene.name, locale: pass.locale, scheme: pass.scheme, ms });
        log(`[${done}/${total}] ${pass.locale}/${pass.scheme}/${scene.name} ${ms}ms`);
      } catch (err) {
        const error = err instanceof Error ? err.message : String(err);
        report.failures.push({ scene: scene.name, locale: pass.locale, scheme: pass.scheme, error });
        log(`[${done}/${total}] FAILED ${pass.locale}/${pass.scheme}/${scene.name}: ${error}`);
      }
    }
  }

  // Leave the app as found: full window, no viewport override.
  await api.appearance.setLayout({ miniplayer: false }).catch(() => {});
  report.totalMs = Date.now() - started;
  return report;
}

export function summarize(report: RunReport): string {
  const lines = [
    `Saved ${report.saved.length} screenshots in ${(report.totalMs / 1000).toFixed(1)}s` +
      (report.saved.length ? ` (${(report.totalMs / report.saved.length / 1000).toFixed(2)}s each).` : "."),
  ];
  if (report.failures.length) {
    lines.push(`${report.failures.length} failed:`);
    for (const f of report.failures) lines.push(`  ${f.locale}/${f.scheme}/${f.scene}: ${f.error}`);
  }
  return lines.join("\n");
}
