import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PinnedNavItem from "./PinnedNavItem.svelte";
import type { NavigablePin } from "../utils/pinnedNav";
import { MusicNotesIcon as Music } from "phosphor-svelte";

describe("PinnedNavItem.svelte", () => {
  const mockPin: NavigablePin = {
    id: "song:42",
    rawItem: { type: "song", song: { id: 42 } } as any,
    type: "song",
    title: "Popstar",
    subtitle: "Tinashe",
    icon: Music,
    coverArt: { songId: 42, artEmbedded: false },
    isActive: false,
    open: vi.fn(),
  };

  it("renders title, subtitle, and artwork in expanded mode", () => {
    const { getByText, getByRole } = render(PinnedNavItem, {
      props: { pin: mockPin, collapsed: false },
    });

    expect(getByText("Popstar")).toBeInTheDocument();
    expect(getByText("Tinashe")).toBeInTheDocument();
    expect(getByRole("button")).toHaveAttribute("title", "Popstar • Tinashe");
  });

  it("renders only artwork/icon in collapsed mode and hides title/subtitle text", () => {
    const { queryByText, getByRole } = render(PinnedNavItem, {
      props: { pin: mockPin, collapsed: true },
    });

    expect(queryByText("Popstar")).not.toBeInTheDocument();
    expect(queryByText("Tinashe")).not.toBeInTheDocument();
    expect(getByRole("button")).toHaveAttribute("title", "Popstar • Tinashe");
  });

  it("calls onclick and oncontextmenu handlers", async () => {
    const onClick = vi.fn();
    const onContextMenu = vi.fn();

    const { getByRole } = render(PinnedNavItem, {
      props: { pin: mockPin, collapsed: false, onclick: onClick, oncontextmenu: onContextMenu },
    });

    const btn = getByRole("button");
    await fireEvent.click(btn);
    expect(onClick).toHaveBeenCalled();

    await fireEvent.contextMenu(btn);
    expect(onContextMenu).toHaveBeenCalled();
  });

  it("applies active styles when pin.isActive is true", () => {
    const activePin = { ...mockPin, isActive: true };
    const { getByRole } = render(PinnedNavItem, {
      props: { pin: activePin, collapsed: false },
    });

    expect(getByRole("button").className).toContain("bg-brand-accent/20");
  });
});
