// @vitest-environment node
import { describe, expect, it } from "vitest";
import path from "node:path";
import { clampPosition, outputPath, planPasses, runScenes, summarize } from "./scene-runner";
import type { RemoteApi, Scene } from "./scenes/types";

const scene = (over: Partial<Scene> & { name: string }): Scene => ({ file: `${over.name}.png`, ...over });

describe("planPasses", () => {
  const scenes = [
    scene({ name: "a" }),
    scene({ name: "mini", layout: { miniplayer: true } }),
    scene({ name: "b", locales: ["fr"] }),
    scene({ name: "welcome", stage: "fresh" }),
    scene({ name: "dyn", outputSubdir: "dynamic", schemes: ["dark"] }),
  ];

  it("runs fresh passes before library passes", () => {
    const stages = planPasses(scenes, { locales: ["en-CA"] }).map((p) => p.stage);
    expect(stages).toEqual(["fresh", "fresh", "library", "library"]);
  });

  it("crosses locales with schemes and applies per-scene narrowing", () => {
    const passes = planPasses(scenes, { locales: ["en-CA", "fr"], stage: "library" });
    const key = (p: (typeof passes)[number]) => `${p.locale}/${p.scheme}:${p.scenes.map((s) => s.name).join(",")}`;
    expect(passes.map(key)).toEqual([
      "en-CA/light:a,mini",
      "en-CA/dark:a,dyn,mini",
      "fr/light:a,b,mini",
      "fr/dark:a,b,dyn,mini",
    ]);
  });

  it("runs miniplayer scenes last in a pass", () => {
    const [pass] = planPasses([scene({ name: "mini", layout: { miniplayer: true } }), scene({ name: "a" })], { locales: ["en-CA"] });
    expect(pass.scenes.map((s) => s.name)).toEqual(["a", "mini"]);
  });

  it("runs a subfolder scene once per locale even when both schemes are requested", () => {
    const passes = planPasses([scene({ name: "dyn", outputSubdir: "dynamic" })], { locales: ["en-CA"] });
    expect(passes.length).toBe(1);
  });

  it("filters by name and lists valid names for an unknown one", () => {
    expect(planPasses(scenes, { locales: ["en-CA"], name: "a" }).every((p) => p.scenes.length === 1)).toBe(true);
    expect(() => planPasses(scenes, { locales: ["en-CA"], name: "nope" })).toThrow(/Valid names: a, mini/);
  });
});

describe("outputPath", () => {
  it("uses the scheme folder unless the scene names its own", () => {
    expect(outputPath("/o", scene({ name: "x" }), "fr", "dark")).toBe(path.join("/o", "fr", "screenshots", "dark", "x.png"));
    expect(outputPath("/o", scene({ name: "x", outputSubdir: "dynamic" }), "en-CA", "dark")).toBe(
      path.join("/o", "en-CA", "screenshots", "dynamic", "x.png")
    );
  });
});

describe("clampPosition", () => {
  it("keeps the position when the song is long enough and halves into short songs", () => {
    expect(clampPosition(100, 300e9)).toBe(100);
    expect(clampPosition(176, 120e9)).toBe(60);
    expect(clampPosition(50, undefined)).toBe(50);
  });
});

describe("runScenes", () => {
  function fake() {
    const calls: string[] = [];
    const recorder = (prefix: string): any =>
      new Proxy(() => {}, {
        get: (_t, key) => (key === "then" ? undefined : recorder(`${prefix}${String(key)}.`)),
        apply: (_t, _this, args) => {
          calls.push(`${prefix.slice(0, -1)}(${JSON.stringify(args)})`);
          return Promise.resolve(prefix === "playback.play." ? { length_nanosec: 300e9 } : undefined);
        },
      });
    const shots: string[] = [];
    const driver: any = {
      evaluate: async (fn: unknown, ...args: unknown[]) => (args.length ? { x: 1, y: 2, width: 3, height: 4 } : undefined),
      screenshot: async (o: { path: string }) => {
        shots.push(o.path);
        return Buffer.alloc(0);
      },
      setWindowSize: async (w: number, h: number) => void calls.push(`size(${w}x${h})`),
      invoke: async (cmd: string) => void calls.push(`invoke(${cmd})`),
    };
    return { calls, shots, driver, api: recorder("") as RemoteApi };
  }

  const base = { outRoot: "/o", translate: (_l: string, k: string) => k, log: () => {}, sleep: async () => {} };

  it("sets locale and scheme once per pass and captures each scene", async () => {
    const { calls, shots, driver, api } = fake();
    const report = await runScenes({
      ...base,
      driver,
      api,
      scenes: [scene({ name: "a", view: { tab: "home" } }), scene({ name: "b", view: { tab: "stats" } })],
      filters: { locales: ["fr"], schemes: ["dark"] },
    });
    expect(report.saved.length).toBe(2);
    expect(shots[0]).toBe(path.join("/o", "fr", "screenshots", "dark", "a.png"));
    expect(calls.filter((c) => c.startsWith("appearance.setLocale")).length).toBe(1);
    expect(calls.filter((c) => c.startsWith("appearance.setColorScheme"))).toEqual(['appearance.setColorScheme(["dark"])']);
    // Both scenes feature the default song: it is played once, then only re-seeked.
    expect(calls.filter((c) => c.startsWith("playback.play")).length).toBe(1);
    expect(calls.filter((c) => c.startsWith("playback.seek")).length).toBe(2);
  });

  it("records a failed scene and keeps going", async () => {
    const { driver, api } = fake();
    const report = await runScenes({
      ...base,
      driver,
      api,
      scenes: [
        scene({ name: "bad", run: async () => { throw new Error("boom"); } }),
        scene({ name: "good" }),
      ],
      filters: { locales: ["en-CA"], schemes: ["light"] },
    });
    expect(report.failures.map((f) => `${f.scene}: ${f.error}`)).toEqual(["bad: boom"]);
    expect(report.saved.length).toBe(1);
    expect(summarize(report)).toContain("1 failed");
  });

  it("prepares the library once, after the fresh passes and before the library passes", async () => {
    const { calls, driver, api } = fake();
    let prepared = 0;
    await runScenes({
      ...base,
      driver,
      api,
      scenes: [scene({ name: "w", stage: "fresh" }), scene({ name: "a" })],
      filters: { locales: ["en-CA"], schemes: ["light", "dark"] },
      async prepareLibrary() {
        prepared++;
        calls.push("PREPARE");
      },
    });
    expect(prepared).toBe(1);
    const prepareAt = calls.indexOf("PREPARE");
    expect(calls.slice(0, prepareAt).some((c) => c.startsWith("playback.play"))).toBe(false);
    expect(calls.slice(prepareAt).some((c) => c.startsWith("playback.play"))).toBe(true);
  });

  it("clips to the element a scene names", async () => {
    const { driver, api } = fake();
    const shotOptions: unknown[] = [];
    driver.screenshot = async (o: unknown) => (shotOptions.push(o), Buffer.alloc(0));
    await runScenes({ ...base, driver, api, scenes: [scene({ name: "bar", clip: "footer" })], filters: { locales: ["en-CA"], schemes: ["light"] } });
    expect(shotOptions[0]).toMatchObject({ clip: { width: 3, height: 4 } });
  });
});
