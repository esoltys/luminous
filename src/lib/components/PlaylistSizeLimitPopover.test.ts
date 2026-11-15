import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { tick } from "svelte";
import { render } from "@testing-library/svelte";
import PlaylistSizeLimitPopover from "./PlaylistSizeLimitPopover.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue([]) }));

function renderOpen(onClose: () => void) {
  return render(PlaylistSizeLimitPopover, {
    isOpen: true,
    anchorEl: document.body.appendChild(document.createElement("button")),
    limitBytes: null,
    deviceId: null,
    deviceLabel: null,
    refreshReorders: false,
    onApply: vi.fn(),
    onClose,
  });
}

describe("PlaylistSizeLimitPopover outside clicks", () => {
  it("closes on a click outside it", async () => {
    const onClose = vi.fn();
    renderOpen(onClose);
    await new Promise((r) => setTimeout(r, 0));
    document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("ignores the click that opened it, whose target may already be gone from the DOM", async () => {
    // A menu item opens the popover and unmounts itself; the same click then
    // reaches the window listener in the same task as the open.
    const onClose = vi.fn();
    renderOpen(onClose);
    await tick();
    document.body.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(onClose).not.toHaveBeenCalled();
  });
});
