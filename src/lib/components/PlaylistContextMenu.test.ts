import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import PlaylistContextMenu from "./PlaylistContextMenu.svelte";
import { picardStore } from "../stores/picard.svelte";

describe("PlaylistContextMenu.svelte", () => {
  beforeEach(() => {
    picardStore.path = "/usr/bin/picard";
  });

  it("enables Open in Picard by default", async () => {
    render(PlaylistContextMenu, {
      x: 0,
      y: 0,
      selectedCount: 1,
      onPlay: () => {},
      onRemove: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard when the selection is all remote", async () => {
    render(PlaylistContextMenu, {
      x: 0,
      y: 0,
      selectedCount: 1,
      onPlay: () => {},
      onRemove: () => {},
      onOpenInPicard: () => {},
      allSelectedRemote: true,
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Open in Picard isn't available for songs on a remote server");
  });
});
