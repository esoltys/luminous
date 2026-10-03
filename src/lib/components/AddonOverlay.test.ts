import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import { tick } from "svelte";

const listenMock = vi.fn(async () => () => {});
vi.mock("@tauri-apps/api/event", () => ({ listen: (...a: unknown[]) => (listenMock as any)(...a) }));
const acquire = vi.fn();
const release = vi.fn();
vi.mock("../utils/spectrumEnable", () => ({ acquireSpectrum: () => acquire(), releaseSpectrum: () => release() }));

import AddonOverlay from "./AddonOverlay.svelte";
import { playerStore } from "../stores/player.svelte";
import type { AddonTheme } from "../stores/addons.svelte";

const base: AddonTheme = {
  id: "fixture-addon",
  name: "Fixture",
  colors: {} as AddonTheme["colors"],
  overlayEntry: "overlay.html"
};

function frameOf(container: HTMLElement): HTMLIFrameElement {
  return container.querySelector("iframe") as HTMLIFrameElement;
}

describe("AddonOverlay", () => {
  beforeEach(() => {
    acquire.mockClear();
    release.mockClear();
    listenMock.mockClear();
  });

  it("sandboxes the frame with scripts only, inert and hidden from AT", () => {
    const { container } = render(AddonOverlay, { addon: base });
    const frame = frameOf(container);
    expect(frame.getAttribute("sandbox")).toBe("allow-scripts");
    expect(frame.getAttribute("aria-hidden")).toBe("true");
    expect(frame.className).toContain("pointer-events-none");
    expect(frame.getAttribute("src")).toMatch(/luminous-addon.*\/fixture-addon\/overlay\.html$/);
  });

  it("posts state once the frame loads, then again when playback changes", async () => {
    const { container } = render(AddonOverlay, { addon: base });
    const frame = frameOf(container);
    const post = vi.fn();
    Object.defineProperty(frame, "contentWindow", { value: { postMessage: post } });
    frame.dispatchEvent(new Event("load"));
    await tick();
    expect(post.mock.calls.some(([m]) => m.kind === "size")).toBe(true);
    const states = () => post.mock.calls.map(([m]) => m).filter((m) => m.kind === "state");
    expect(states().at(-1)?.isPlaying).toBe(false);
    playerStore.state = "playing";
    await tick();
    expect(states().at(-1)?.isPlaying).toBe(true);
    expect(states().at(-1)?.luminousAddon).toBe(1);
    playerStore.state = "stopped";
  });

  it("acquires spectrum only when the manifest opts in, and releases on unmount", async () => {
    const plain = render(AddonOverlay, { addon: base });
    await tick();
    expect(acquire).not.toHaveBeenCalled();
    plain.unmount();

    const opted = render(AddonOverlay, { addon: { ...base, capabilities: ["spectrum"] } });
    await tick();
    expect(acquire).toHaveBeenCalledTimes(1);
    expect(listenMock).toHaveBeenCalledWith("spectrum-data", expect.any(Function));
    opted.unmount();
    expect(release).toHaveBeenCalledTimes(1);
  });
});
