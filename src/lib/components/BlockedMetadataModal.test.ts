import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import BlockedMetadataModal from "./BlockedMetadataModal.svelte";
import type { MetadataRejection } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const mockRejections: MetadataRejection[] = [
  {
    id: 1,
    entity_type: "song",
    entity_id: "42",
    entity_label: "Radiohead - Paranoid Android",
    category: "lyrics",
    provider: "lrclib",
    rejected_identifier: "*",
    created_at: "2026-11-10T12:00:00",
  },
  {
    id: 2,
    entity_type: "album",
    entity_id: "Radiohead - OK Computer",
    category: "cover_art",
    provider: "itunes",
    rejected_identifier: "*",
    created_at: "2026-11-10T12:05:00",
  },
];

describe("BlockedMetadataModal.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders empty state when there are no rejections", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([]);

    const { getByText } = render(BlockedMetadataModal, { onClose: () => {} });

    await waitFor(() => {
      expect(getByText("No blocked metadata matches.")).toBeInTheDocument();
    });
  });

  it("renders rejection list and allows unblocking an entry", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "list_metadata_rejections") return [...mockRejections];
      if (cmd === "unblock_metadata_rejection") return null;
      return null;
    });

    const { getByText, getAllByText, queryByText } = render(BlockedMetadataModal, { onClose: () => {} });

    await waitFor(() => {
      expect(getByText("lrclib")).toBeInTheDocument();
      expect(getByText("itunes")).toBeInTheDocument();
      expect(getByText("Radiohead - Paranoid Android")).toBeInTheDocument();
    });

    const unblockButtons = getAllByText("Unblock");
    expect(unblockButtons.length).toBe(2);

    await fireEvent.click(unblockButtons[0]);

    expect(invoke).toHaveBeenCalledWith("unblock_metadata_rejection", { rejectionId: 1 });

    await waitFor(() => {
      expect(queryByText("lrclib")).not.toBeInTheDocument();
      expect(getByText("itunes")).toBeInTheDocument();
    });
  });
});
