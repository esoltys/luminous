import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { toastStore } from "../stores/toast.svelte";
import { openInPicard } from "./picard";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("../stores/toast.svelte", () => ({
  toastStore: { show: vi.fn() },
}));

describe("openInPicard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("invokes open_in_picard with the given song ids", async () => {
    (invoke as any).mockResolvedValue(undefined);
    await openInPicard([1, 2, 3]);
    expect(invoke).toHaveBeenCalledWith("open_in_picard", { songIds: [1, 2, 3] });
    expect(toastStore.show).not.toHaveBeenCalled();
  });

  it("does nothing for an empty selection", async () => {
    await openInPicard([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("surfaces a launch failure (e.g. Picard not found) as an error toast", async () => {
    (invoke as any).mockRejectedValue("MusicBrainz Picard not found.");
    await openInPicard([1]);
    expect(toastStore.show).toHaveBeenCalledWith("MusicBrainz Picard not found.", "error");
  });
});
