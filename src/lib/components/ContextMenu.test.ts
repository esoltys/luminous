import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import ContextMenu from "./ContextMenu.svelte";
import { playerStore } from "../stores/player.svelte";
import { PLAYER_DOCK_CLEARANCE_PX, VIEWPORT_EDGE_PADDING_PX } from "../constants";

const children = createRawSnippet(() => ({ render: () => "<span>item</span>" }));

function mockMenuHeight(height: number) {
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(height);
}

function menuTop(): number {
  const menu = document.querySelector<HTMLElement>('[role="menu"]')!;
  return parseFloat(menu.style.top);
}

describe("ContextMenu.svelte vertical placement", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        unobserve() {}
        disconnect() {}
      }
    );
    Object.defineProperty(window, "innerHeight", { value: 800, configurable: true });
    playerStore.currentSong = undefined;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("clamps by the measured height", () => {
    mockMenuHeight(600);
    render(ContextMenu, { props: { x: 10, y: 700, onClose: vi.fn(), children } });
    expect(menuTop()).toBe(800 - 600 - VIEWPORT_EDGE_PADDING_PX);
  });

  it("keeps the menu above the player dock when a track is loaded", () => {
    mockMenuHeight(300);
    playerStore.currentSong = { id: 1 } as never;
    render(ContextMenu, { props: { x: 10, y: 790, onClose: vi.fn(), children } });
    expect(menuTop()).toBe(800 - 300 - PLAYER_DOCK_CLEARANCE_PX - VIEWPORT_EDGE_PADDING_PX);
  });

  it("never pushes the top edge off-screen when taller than the window", () => {
    mockMenuHeight(2000);
    render(ContextMenu, { props: { x: 10, y: 500, onClose: vi.fn(), children } });
    expect(menuTop()).toBe(VIEWPORT_EDGE_PADDING_PX);
  });
});
