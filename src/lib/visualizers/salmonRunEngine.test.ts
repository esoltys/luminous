import { describe, expect, it } from "vitest";
import { LoneSalmon, RiverAudio, SeededRandom, seedFromKey } from "./salmonRunEngine";
import type { VisualizerFrame } from "./types";

const QUIET = new Array(32).fill(0.2);
const LOUD_TREBLE = [...new Array(20).fill(0.2), ...new Array(12).fill(1)];
const FRAME_MS = 1000 / 60;

function frame(bands: readonly number[], onset = 0, playing = true): VisualizerFrame {
  return { bands, onset, playing, scope: null };
}

/** Steps `scene` for `seconds` at 60 fps; returns the last step's result. */
function run(scene: LoneSalmon, seconds: number, reducedMotion = false): boolean {
  let moving = true;
  for (let t = 0; t < seconds * 60; t++) moving = scene.step(FRAME_MS, reducedMotion);
  return moving;
}

const LONE = { fishLength: 1.1, leafSize: 0.7, leafCrossS: 6 };

describe("SeededRandom", () => {
  it("repeats the same sequence for the same seed", () => {
    const a = new SeededRandom(seedFromKey("track-1"));
    const b = new SeededRandom(seedFromKey("track-1"));
    for (let i = 0; i < 20; i++) expect(a.next()).toBe(b.next());
  });

  it("keeps int() within its inclusive bounds", () => {
    const rng = new SeededRandom(7);
    const seen = new Set<number>();
    for (let i = 0; i < 500; i++) seen.add(rng.int(2, 5));
    expect([...seen].sort()).toEqual([2, 3, 4, 5]);
  });
});

describe("RiverAudio", () => {
  it("hands out a treble transient once, then holds off for its cooldown", () => {
    const audio = new RiverAudio();
    audio.push(frame(QUIET));
    for (let t = 0; t < 60; t++) audio.step(FRAME_MS / 1000);
    audio.push(frame(LOUD_TREBLE));
    audio.step(FRAME_MS / 1000);
    expect(audio.transient).toBeGreaterThan(0.7);
    audio.step(FRAME_MS / 1000);
    expect(audio.transient).toBe(0);
    audio.push(frame(QUIET));
    audio.push(frame(LOUD_TREBLE, 1));
    audio.step(FRAME_MS / 1000);
    expect(audio.transient).toBe(0);
    expect(audio.onset).toBe(1);
  });

  it("takes the first data after resuming as the baseline, not a transient", () => {
    const audio = new RiverAudio();
    audio.push(frame(LOUD_TREBLE));
    audio.step(FRAME_MS / 1000);
    expect(audio.transient).toBe(0);
  });

  it("eases activity in on play and out on pause", () => {
    const audio = new RiverAudio();
    audio.push(frame(QUIET));
    audio.step(0.1);
    expect(audio.activity).toBeGreaterThan(0);
    expect(audio.activity).toBeLessThan(0.5);
    for (let t = 0; t < 300; t++) audio.step(FRAME_MS / 1000);
    expect(audio.activity).toBe(1);
    audio.push(frame(QUIET, 0, false));
    for (let t = 0; t < 600; t++) audio.step(FRAME_MS / 1000);
    expect(audio.activity).toBe(0);
    expect(audio.stirring).toBe(false);
  });
});

describe("LoneSalmon", () => {
  it("sends a leaf past on a track change but not for the first track", () => {
    const scene = new LoneSalmon(LONE);
    scene.setTrack("a");
    expect(scene.leafActive).toBe(false);
    scene.setTrack("a");
    expect(scene.leafActive).toBe(false);
    scene.setTrack("b");
    expect(scene.leafActive).toBe(true);
    run(scene, LONE.leafCrossS + 1);
    expect(scene.leafActive).toBe(false);
  });

  it("keeps the salmon in frame, facing into the current", () => {
    const scene = new LoneSalmon(LONE);
    scene.setAspect(96 / 28);
    scene.setTrack("a");
    for (let s = 0; s < 30; s++) {
      scene.push(frame(s % 3 === 0 ? LOUD_TREBLE : QUIET, s % 2));
      run(scene, 1);
      expect(scene.fishX).toBeGreaterThan(0);
      expect(scene.fishX).toBeLessThan(scene.aspect);
      expect(scene.fishY).toBeGreaterThan(0);
      expect(scene.fishY).toBeLessThan(1);
    }
  });

  it("darts forward on a transient, but not under reduced motion", () => {
    for (const reduced of [false, true]) {
      const scene = new LoneSalmon(LONE);
      scene.setTrack("a");
      scene.push(frame(QUIET));
      run(scene, 2, reduced);
      scene.push(frame(LOUD_TREBLE));
      scene.step(FRAME_MS, reduced);
      expect(scene.fishSurge > 0).toBe(!reduced);
    }
  });

  it("calms and stops asking for frames once paused", () => {
    const scene = new LoneSalmon(LONE);
    scene.setTrack("a");
    scene.push(frame(QUIET));
    expect(run(scene, 2)).toBe(true);
    scene.push(frame(QUIET, 0, false));
    expect(run(scene, 5)).toBe(false);
  });
});
