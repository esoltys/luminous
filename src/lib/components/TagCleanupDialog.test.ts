import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import TagCleanupDialog from "./TagCleanupDialog.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { TagCleanupPreview } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

const preview: TagCleanupPreview = {
  songs: [
    {
      song_id: 7,
      title: "Airbag",
      artist: "Radiohead",
      fields: [
        { kind: "promo_comment", value: "Downloaded from www.freemp3s.ru" },
        { kind: "encoder", value: "LAME 3.99r" },
      ],
    },
  ],
  skipped: 2,
};

describe("TagCleanupDialog.svelte", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("lists each removable field with its value, ticked by default", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(preview);
    const { getByText, getAllByRole } = render(TagCleanupDialog, { songIds: [7], onClose: vi.fn() });

    await waitFor(() => expect(getByText("LAME 3.99r")).toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("preview_tag_cleanup", { songIds: [7] });
    expect(getByText("Downloaded from www.freemp3s.ru")).toBeInTheDocument();
    const boxes = getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes).toHaveLength(2);
    expect(boxes.every((b) => b.checked)).toBe(true);
    expect(getByText(/couldn't be checked/)).toHaveTextContent("2");
  });

  it("sends only the fields still ticked", async () => {
    const onClose = vi.fn();
    vi.mocked(invoke)
      .mockResolvedValueOnce(preview)
      .mockResolvedValueOnce({ cleaned_songs: 1, removed_fields: 1, errors: [] });
    const { getByText, getAllByRole } = render(TagCleanupDialog, { songIds: [7], onClose });

    await waitFor(() => expect(getByText("LAME 3.99r")).toBeInTheDocument());
    await fireEvent.click(getAllByRole("checkbox")[1]);
    await fireEvent.click(getByText("Remove Selected"));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(invoke).toHaveBeenLastCalledWith("apply_tag_cleanup", {
      selections: [{ song_id: 7, fields: [preview.songs[0].fields[0]] }],
    });
  });

  it("disables removal when everything is unticked", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(preview);
    const { getByText, getAllByRole } = render(TagCleanupDialog, { songIds: [7], onClose: vi.fn() });

    await waitFor(() => expect(getByText("LAME 3.99r")).toBeInTheDocument());
    for (const box of getAllByRole("checkbox")) await fireEvent.click(box);
    expect(getByText("Remove Selected").closest("button")).toBeDisabled();
  });

  it("cancels the check when closed before it finishes", async () => {
    vi.mocked(invoke).mockReturnValueOnce(new Promise(() => {}));
    const { unmount } = render(TagCleanupDialog, { songIds: [], onClose: vi.fn() });
    unmount();
    expect(invoke).toHaveBeenLastCalledWith("cancel_tag_cleanup");
  });

  it("says so when there is nothing to clean", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ songs: [], skipped: 0 });
    const { getByText, queryByText } = render(TagCleanupDialog, { songIds: [], onClose: vi.fn() });

    await waitFor(() => expect(getByText("No tag clutter found.")).toBeInTheDocument());
    expect(queryByText("Remove Selected")).not.toBeInTheDocument();
  });
});
