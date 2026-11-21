import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import { listen } from "@tauri-apps/api/event";
import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";
import { playerStore } from "../stores/player.svelte";
import type { SpectrumFrame } from "../utils/spectrumEnable";

let reducedMotion = false;
vi.mock("../utils/motion", () => ({ prefersReducedMotion: () => reducedMotion }));

/** Renders the playing logo and returns a way to send it spectrum frames. */
async function renderPlaying() {
  let send: ((frame: SpectrumFrame) => void) | undefined;
  vi.mocked(listen).mockImplementation(async (event, cb) => {
    if (event === "spectrum-data") {
      send = (frame) => (cb as (e: { payload: SpectrumFrame }) => void)({ payload: frame });
    }
    return () => {};
  });
  playerStore.state = "playing";
  const view = render(ReactiveLogoBrand, { props: { size: "lg" } });
  await tick();
  await tick();
  const burstLayer = () =>
    view.container.querySelector('circle[data-layer="burst"]')
      ?.parentElement as unknown as SVGElement;
  const emit = async (frame: SpectrumFrame) => {
    send?.(frame);
    await tick();
  };
  return { emit, burstLayer };
}

describe("ReactiveLogoBrand.svelte", () => {
  beforeEach(() => {
    localStorage.clear();
    playerStore.state = "stopped";
    reducedMotion = false;
  });

  it("flares the coronal burst on a bass onset, then lets it fade", async () => {
    const { emit, burstLayer } = await renderPlaying();
    const quiet = new Array(32).fill(0);
    await emit({ bands: quiet, onset: 0 });
    const restOpacity = Number(burstLayer().style.opacity);
    const restTransform = burstLayer().style.transform;

    await emit({ bands: quiet, onset: 1 });
    expect(Number(burstLayer().style.opacity)).toBeGreaterThan(restOpacity + 0.5);
    expect(burstLayer().style.transform).not.toBe(restTransform);

    for (let i = 0; i < 10; i++) await emit({ bands: quiet, onset: 0 });
    expect(Number(burstLayer().style.opacity)).toBeCloseTo(restOpacity);
  });

  it("under reduced motion, a kick brightens the burst without growing it", async () => {
    reducedMotion = true;
    const { emit, burstLayer } = await renderPlaying();
    const quiet = new Array(32).fill(0);
    await emit({ bands: quiet, onset: 0 });
    const restTransform = burstLayer().style.transform;
    const restOpacity = Number(burstLayer().style.opacity);

    await emit({ bands: quiet, onset: 1 });
    expect(burstLayer().style.transform).toBe(restTransform);
    expect(Number(burstLayer().style.opacity)).toBeGreaterThan(restOpacity + 0.5);
  });

  it("renders the crisp authentic Luminous brand mark in resting state when playback is stopped", () => {
    playerStore.state = "stopped";
    const { container } = render(ReactiveLogoBrand, { props: { size: "lg" } });

    // Inner rim (r=77)
    const innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim).not.toBeNull();
    expect(innerRim?.getAttribute("stroke")).toBe("#626FE8");
    expect(innerRim?.getAttribute("stroke-width")).toBe("14");
    expect(innerRim?.getAttribute("opacity")).toBe("1");
    expect(innerRim?.getAttribute("filter")).toBeNull();

    // Eclipse ring (r=92)
    const eclipseRing = container.querySelector('circle[r="92"]');
    expect(eclipseRing).not.toBeNull();
    expect(eclipseRing?.getAttribute("stroke")).toBe("#FFB648");
    expect(eclipseRing?.getAttribute("stroke-width")).toBe("8");
    expect(eclipseRing?.getAttribute("opacity")).toBe("1");
    expect(eclipseRing?.getAttribute("filter")).toBeNull();

    // Planet silhouette disc (r=68)
    const disc = container.querySelector('circle[r="68"]');
    expect(disc).not.toBeNull();
    expect(disc?.getAttribute("fill")).toBe("#0A0A0D");

    // Coronal burst core (cx=150.6, cy=59.6, r=17.4)
    const burstCore = container.querySelector('circle[cx="150.6"][cy="59.6"][r="17.4"]');
    expect(burstCore).not.toBeNull();
    expect(burstCore?.getAttribute("fill")).toBe("#FFFFFF");

    // No glow, burst halo or blur filters at rest
    expect(container.querySelector("[data-layer]")).toBeNull();
    expect(container.querySelector("filter")).toBeNull();
  });

  it("renders the crisp authentic brand logo in resting state when playback is paused", () => {
    playerStore.state = "paused";
    const { container } = render(ReactiveLogoBrand, { props: { size: "md" } });

    const innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim?.getAttribute("stroke")).toBe("#626FE8");
    expect(innerRim?.getAttribute("stroke-width")).toBe("14");
    expect(innerRim?.getAttribute("filter")).toBeNull();

    const eclipseRing = container.querySelector('circle[r="92"]');
    expect(eclipseRing?.getAttribute("stroke")).toBe("#FFB648");
    expect(eclipseRing?.getAttribute("stroke-width")).toBe("8");
    expect(eclipseRing?.getAttribute("filter")).toBeNull();
  });

  it("activates audio-reactive visualizer with theme accents and blur filters while playing", () => {
    playerStore.state = "playing";
    const { container } = render(ReactiveLogoBrand, { props: { size: "lg" } });

    // Inner rim re-targets to the theme accent, as a blurred thin/thick pair
    const rims = container.querySelectorAll('circle[data-layer="rim"]');
    expect([...rims].map((c) => c.getAttribute("stroke-width"))).toEqual(["12", "24"]);
    for (const rim of rims) {
      expect(rim.getAttribute("stroke")).toBe("var(--color-accent)");
      expect(rim.getAttribute("filter")).toMatch(/^url\(#rimBlur-/);
    }

    // Eclipse ring re-targets to the theme accent-hover, same treatment
    const rings = container.querySelectorAll('circle[data-layer="ring"]');
    expect([...rings].map((c) => c.getAttribute("stroke-width"))).toEqual(["5", "21"]);
    for (const ring of rings) {
      expect(ring.getAttribute("stroke")).toBe("var(--color-accent-hover)");
      expect(ring.getAttribute("filter")).toMatch(/^url\(#ringBlur-/);
    }

    // Ambient glow and burst halo are active, blurred, visible layers
    const glow = container.querySelector('circle[data-layer="glow"]');
    expect(glow?.getAttribute("filter")).toMatch(/^url\(#glowBlurOuter-/);
    expect(
      parseFloat((glow?.parentElement as unknown as SVGElement).style.opacity),
    ).toBeGreaterThan(0);

    const burst = container.querySelector('circle[data-layer="burst"]');
    expect(burst?.getAttribute("filter")).toMatch(/^url\(#burstBlur-/);
    expect(
      parseFloat((burst?.parentElement as unknown as SVGElement).style.opacity),
    ).toBeGreaterThan(0);
  });

  it("drives the playing layers only through transform and opacity, never SVG geometry", () => {
    playerStore.state = "playing";
    const { container } = render(ReactiveLogoBrand, { props: { size: "lg" } });

    // Blurred content is fixed-size; the audio reaches it only via layer style
    expect(container.querySelector('circle[data-layer="glow"]')?.getAttribute("r")).toBe("112");
    expect(container.querySelector('circle[data-layer="burst"]')?.getAttribute("r")).toBe("31");
    const glowLayer = container.querySelector('circle[data-layer="glow"]')
      ?.parentElement as unknown as SVGElement;
    expect(glowLayer.style.transform).toMatch(/^scale\(/);
  });

  it("returns to resting crisp brand mark when clicking the button to disable pulsing", async () => {
    playerStore.state = "playing";
    const { container, getByRole } = render(ReactiveLogoBrand, { props: { size: "lg" } });

    const button = getByRole("button");
    // Initially playing: active visualizer
    expect(container.querySelector('circle[data-layer="rim"]')?.getAttribute("stroke")).toBe(
      "var(--color-accent)",
    );

    // Toggle pulsing off
    await fireEvent.click(button);

    // Should return to resting crisp brand mark
    const innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim?.getAttribute("stroke")).toBe("#626FE8");
    expect(innerRim?.getAttribute("stroke-width")).toBe("14");
    expect(innerRim?.getAttribute("filter")).toBeNull();

    const eclipseRing = container.querySelector('circle[r="92"]');
    expect(eclipseRing?.getAttribute("stroke")).toBe("#FFB648");
    expect(eclipseRing?.getAttribute("filter")).toBeNull();

    expect(container.querySelector("[data-layer]")).toBeNull();
  });
});
