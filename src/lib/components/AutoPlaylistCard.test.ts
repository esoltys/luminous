import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import AutoPlaylistCard from "./AutoPlaylistCard.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

describe("AutoPlaylistCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockResolvedValue([]);
  });

  it("fetches language preview songs with the `language` parameter when no backing playlist row exists (#2136)", async () => {
    render(AutoPlaylistCard, {
      props: {
        label: "Japanese",
        kind: "language",
        selector: "ja",
        trackCount: 12,
        onClick: vi.fn(),
      },
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_songs_by_language", {
        language: "ja",
        limit: 50,
      });
    });
  });
});
