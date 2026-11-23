import { describe, expect, it, vi } from "vitest";
import { LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS } from "../stores/theme.svelte";
import { VOXEL_ISO_PRESET } from "./voxelIso";

function fakeIsoCtx() {
  return {
    clearRect: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    closePath: vi.fn(),
    fill: vi.fn(),
    fillStyle: "",
  } as unknown as CanvasRenderingContext2D;
}

describe("VOXEL_ISO_PRESET", () => {
  it("renders live waterfall cubes, preview, and settles to rest when paused", () => {
    const instance = VOXEL_ISO_PRESET.create("full");
    instance.setTheme(LUMINOUS_DARK_COLORS);
    instance.setTrack?.({ key: "track-1", accent: "#ff6600" });

    instance.push({
      bands: new Array(32).fill(0.8),
      playing: true,
      onset: 0.5,
      scope: null,
    });
    expect(instance.step(90, false)).toBe(true);

    const ctx = fakeIsoCtx();
    instance.render(ctx, 640, 320, { reducedMotion: false, preview: false });
    expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, 640, 320);
    expect(vi.mocked(ctx.fill).mock.calls.length).toBeGreaterThan(16);

    vi.mocked(ctx.fill).mockClear();
    instance.render(ctx, 640, 320, { reducedMotion: false, preview: true });
    expect(vi.mocked(ctx.fill).mock.calls.length).toBeGreaterThan(10);

    // Paused settles back to rest.
    instance.push({ bands: new Array(32).fill(0.8), playing: false, onset: 0, scope: null });
    let moving = true;
    for (let i = 0; i < 80 && moving; i++) moving = instance.step(100, false);
    expect(moving).toBe(false);
  });

  it("supports mini size, light theme, and reduced motion without errors", () => {
    const mini = VOXEL_ISO_PRESET.create("mini");
    mini.setTheme(LUMINOUS_LIGHT_COLORS);
    mini.push({ bands: new Array(32).fill(0.6), playing: true, onset: 0, scope: null });
    mini.step(16, true);
    const ctx = fakeIsoCtx();
    mini.render(ctx, 96, 28, { reducedMotion: true, preview: false });
    expect(vi.mocked(ctx.fill).mock.calls.length).toBeGreaterThan(0);

    vi.mocked(ctx.fill).mockClear();
    mini.render(ctx, 0, 28, { reducedMotion: false, preview: false });
    expect(ctx.fill).not.toHaveBeenCalled();
  });
});
