import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import FolderEditModal from "./FolderEditModal.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { toastStore } from "../stores/toast.svelte";
import { i18n } from "../stores/i18n.svelte";
import type { MusicDirectory } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

const directory = {
  id: 7,
  path: "Z:\\Music Library",
  nickname: null,
  icon: "folder",
  color: null,
} as MusicDirectory;

describe("FolderEditModal.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en-CA";
    for (const m of [...toastStore.messages]) {
      toastStore.dismiss(m.id);
    }
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("closes after a successful save", async () => {
    const update = vi.spyOn(collectionStore, "updateDirectoryMetadata").mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(FolderEditModal, { directory, onClose });

    await fireEvent.click(screen.getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(update).toHaveBeenCalledWith(7, { nickname: null, icon: "folder", color: null });
  });

  it("stays open and shows an error toast when the save fails", async () => {
    vi.spyOn(collectionStore, "updateDirectoryMetadata").mockRejectedValue("database is locked");
    vi.spyOn(console, "error").mockImplementation(() => {});
    const onClose = vi.fn();
    render(FolderEditModal, { directory, onClose });

    await fireEvent.click(screen.getByRole("button", { name: "Save Changes" }));

    await waitFor(() =>
      expect(toastStore.messages.map((m) => [m.text, m.variant])).toContainEqual([
        "Failed to save folder details: database is locked",
        "error",
      ]),
    );
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Save Changes" })).toBeEnabled();
  });
  it("re-links the folder from Change Location and closes once it moved (#1403)", async () => {
    const relocate = vi.spyOn(collectionStore, "relocateDirectoryDialog").mockResolvedValue(true);
    const onClose = vi.fn();
    render(FolderEditModal, { directory, onClose });

    await fireEvent.click(screen.getByRole("button", { name: "Change Location…" }));

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(relocate).toHaveBeenCalledWith("Z:\\Music Library");
  });

  it("stays open when Change Location is cancelled", async () => {
    vi.spyOn(collectionStore, "relocateDirectoryDialog").mockResolvedValue(false);
    const onClose = vi.fn();
    render(FolderEditModal, { directory, onClose });

    await fireEvent.click(screen.getByRole("button", { name: "Change Location…" }));

    await waitFor(() => expect(screen.getByRole("button", { name: "Change Location…" })).toBeEnabled());
    expect(onClose).not.toHaveBeenCalled();
  });
});
