import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";
import { playerStore } from "../stores/player.svelte";

describe("ReactiveLogoBrand.svelte", () => {
  beforeEach(() => {
    localStorage.clear();
    playerStore.state = "stopped";
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

    // Ambient glow halo and burst halo should have opacity 0 at rest
    const glowCircle = container.querySelector('circle[fill="var(--color-accent)"]');
    expect(glowCircle?.getAttribute("opacity")).toBe("0");
    expect(glowCircle?.getAttribute("filter")).toBeNull();

    const burstHalo = container.querySelector('circle[cx="150.6"][cy="59.6"][r="0"]');
    expect(burstHalo?.getAttribute("opacity")).toBe("0");
    expect(burstHalo?.getAttribute("filter")).toBeNull();
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

    // Inner rim re-targets to theme accent with ringBlur filter
    const innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim?.getAttribute("stroke")).toBe("var(--color-accent)");
    expect(innerRim?.getAttribute("filter")).toBe("url(#ringBlur)");

    // Eclipse ring re-targets to theme accent-hover with ringBlur filter
    const eclipseRing = container.querySelector('circle[r="92"]');
    expect(eclipseRing?.getAttribute("stroke")).toBe("var(--color-accent-hover)");
    expect(eclipseRing?.getAttribute("filter")).toBe("url(#ringBlur)");

    // Ambient glow halo becomes active
    const glowCircle = container.querySelector('circle[fill="var(--color-accent)"]');
    expect(glowCircle?.getAttribute("filter")).toBe("url(#glowBlurOuter)");
    const glowOpacity = parseFloat(glowCircle?.getAttribute("opacity") || "0");
    expect(glowOpacity).toBeGreaterThan(0);

    // Burst halo becomes active
    const burstHalo = container.querySelector('circle[cx="150.6"][cy="59.6"][filter="url(#burstBlur)"]');
    expect(burstHalo).not.toBeNull();
    const burstOpacity = parseFloat(burstHalo?.getAttribute("opacity") || "0");
    expect(burstOpacity).toBeGreaterThan(0);
  });

  it("returns to resting crisp brand mark when clicking the button to disable pulsing", async () => {
    playerStore.state = "playing";
    const { container, getByRole } = render(ReactiveLogoBrand, { props: { size: "lg" } });

    const button = getByRole("button");
    // Initially playing: active visualizer
    let innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim?.getAttribute("stroke")).toBe("var(--color-accent)");

    // Toggle pulsing off
    await fireEvent.click(button);

    // Should return to resting crisp brand mark
    innerRim = container.querySelector('circle[r="77"]');
    expect(innerRim?.getAttribute("stroke")).toBe("#626FE8");
    expect(innerRim?.getAttribute("stroke-width")).toBe("14");
    expect(innerRim?.getAttribute("filter")).toBeNull();

    const eclipseRing = container.querySelector('circle[r="92"]');
    expect(eclipseRing?.getAttribute("stroke")).toBe("#FFB648");
    expect(eclipseRing?.getAttribute("filter")).toBeNull();

    const glowCircle = container.querySelector('circle[fill="var(--color-accent)"]');
    expect(glowCircle?.getAttribute("opacity")).toBe("0");
  });
});
