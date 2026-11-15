import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import ContextMenuSubmenu from "./ContextMenuSubmenu.svelte";
import ContextMenu from "./ContextMenu.svelte";
import { playerStore } from "../stores/player.svelte";
import {
  CONTEXT_SUBMENU_WIDTH_PX,
  PLAYER_DOCK_CLEARANCE_PX,
  VIEWPORT_EDGE_PADDING_PX,
} from "../constants";

const Icon = (() => {}) as never;
const items = createRawSnippet(() => ({ render: () => '<button id="flyout-item">item</button>' }));

function mockTriggerRect(rect: Partial<DOMRect>) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    width: 0,
    height: 0,
    x: 0,
    y: 0,
    toJSON: () => ({}),
    ...rect,
  });
}

function flyout(): HTMLElement {
  return document.querySelector<HTMLElement>("[data-context-menu-layer]")!;
}

async function openFlyout(utils: { getByRole: (role: string) => HTMLElement }) {
  await fireEvent.mouseEnter(utils.getByRole("menuitem").parentElement!);
}

describe("ContextMenuSubmenu.svelte placement", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", { value: 1000, configurable: true });
    Object.defineProperty(window, "innerHeight", { value: 600, configurable: true });
    playerStore.currentSong = undefined;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("opens to the right of the trigger, aligned to its top", async () => {
    mockTriggerRect({ top: 100, left: 100, right: 300 });
    const utils = render(ContextMenuSubmenu, {
      props: { icon: Icon, label: "More", children: items },
    });
    await openFlyout(utils);
    expect(flyout().style.left).toBe("298px");
    expect(flyout().style.top).toBe("100px");
    expect(flyout().style.width).toBe(`${CONTEXT_SUBMENU_WIDTH_PX}px`);
  });

  it("opens to the left when there is no room on the right", async () => {
    mockTriggerRect({ top: 100, left: 700, right: 900 });
    const utils = render(ContextMenuSubmenu, {
      props: { icon: Icon, label: "More", children: items },
    });
    await openFlyout(utils);
    expect(flyout().style.left).toBe(`${702 - CONTEXT_SUBMENU_WIDTH_PX}px`);
  });

  it("caps its height to the viewport above the dock so it scrolls instead of clipping", async () => {
    playerStore.currentSong = { id: 1 } as never;
    mockTriggerRect({ top: 100, left: 100, right: 300 });
    const utils = render(ContextMenuSubmenu, {
      props: { icon: Icon, label: "More", children: items },
    });
    await openFlyout(utils);
    expect(flyout().style.maxHeight).toBe(
      `${600 - PLAYER_DOCK_CLEARANCE_PX - 2 * VIEWPORT_EDGE_PADDING_PX}px`,
    );
  });
});

describe("ContextMenu.svelte scrolling and flyouts", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", { value: 1000, configurable: true });
    Object.defineProperty(window, "innerHeight", { value: 400, configurable: true });
    playerStore.currentSong = undefined;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("caps its height to the viewport and scrolls", () => {
    render(ContextMenu, { props: { x: 10, y: 10, onClose: vi.fn(), children: items } });
    const menu = document.querySelector<HTMLElement>('[role="menu"]')!;
    expect(menu.style.maxHeight).toBe(`${400 - 2 * VIEWPORT_EDGE_PADDING_PX}px`);
    expect(menu).toHaveClass("overflow-y-auto");
  });

  it("does not dismiss when a portaled flyout is clicked, but does for outside clicks", async () => {
    const onClose = vi.fn();
    const outer = createRawSnippet(() => ({ render: () => "<span>menu</span>" }));
    render(ContextMenu, { props: { x: 10, y: 10, onClose, children: outer } });
    const layer = document.createElement("div");
    layer.setAttribute("data-context-menu-layer", "");
    const inner = document.createElement("button");
    layer.appendChild(inner);
    document.body.appendChild(layer);

    await fireEvent.mouseDown(inner);
    expect(onClose).not.toHaveBeenCalled();

    await fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
    layer.remove();
  });
});
