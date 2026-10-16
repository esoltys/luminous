import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { organizeStore } from "../stores/organizer.svelte";
import OrganizeFiles from "./OrganizeFiles.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_organize_config") {
      return Promise.resolve({
        auto_organize: false,
        template: "%albumartist/{%year - }{%album/}{%disc-}{%track }%title",
        preset: "default",
        destination_mode: "original",
        custom_destination_dir: "",
        replace_spaces: false,
        ascii_only: false,
        clean_empty_dirs: true,
        move_extra_files: true,
      });
    }
    if (cmd === "preview_organize") {
      return Promise.resolve([
        {
          song_id: 1,
          from_path: "/music/a.mp3",
          to_path: "/music/Rock/a.mp3",
          status: "ok",
          error_message: null,
        },
        {
          song_id: 2,
          from_path: "/music/b.mp3",
          to_path: "/music/Rock/b.mp3",
          status: "ok",
          error_message: null,
        },
      ]);
    }
    if (cmd === "apply_organize") {
      return Promise.resolve({
        moved_count: 2,
        skipped_count: 0,
        errors: [],
      });
    }
    if (cmd === "set_organize_config") {
      return Promise.resolve(false);
    }
    return Promise.resolve(null);
  }),
}));

describe("OrganizeFiles safeguards", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await organizeStore.init();
  });

  it("does not apply immediately when clicking Organize Music; opens confirmation modal with impact breakdown", async () => {
    const { findByText, findAllByText, queryByText } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1, 2],
    });

    // Wait for preview to settle and apply button to be enabled
    const organizeBtn = await findByText("Organize Music");
    const buttonEl = organizeBtn.closest("button")!;
    await waitFor(() => {
      expect(buttonEl).not.toBeDisabled();
    });
    await fireEvent.click(buttonEl);

    // invoke("apply_organize") should NOT be called yet
    expect(invoke).not.toHaveBeenCalledWith("apply_organize", expect.anything());

    // Confirmation dialog should appear repeating consequences
    const dialogTitles = await findAllByText("Organize Files");
    expect(dialogTitles.length).toBeGreaterThanOrEqual(1);

    expect(
      await findByText((content) =>
        content.includes("You are about to move 2 files into 1 folder.")
      )
    ).toBeInTheDocument();

    expect(
      await findByText((content) =>
        content.includes("Empty source folders will be deleted.")
      )
    ).toBeInTheDocument();

    expect(
      await findByText((content) =>
        content.includes("Companion files (artwork, lyrics, and cue sheets) will also be moved.")
      )
    ).toBeInTheDocument();

    // Cancel closes dialog without applying
    const cancelBtns = await findAllByText("Cancel");
    const dialogCancelBtn = cancelBtns[cancelBtns.length - 1];
    await fireEvent.click(dialogCancelBtn);

    await waitFor(() => {
      expect(
        queryByText((content) =>
          content.includes("You are about to move 2 files into 1 folder.")
        )
      ).not.toBeInTheDocument();
    });
    expect(invoke).not.toHaveBeenCalledWith("apply_organize", expect.anything());
  });

  it("executes apply_organize and persists confirmed config when confirmed in dialog", async () => {
    const updateConfigSpy = vi.spyOn(organizeStore, "updateConfig");

    const { findByText, findAllByText } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1, 2],
    });

    const organizeBtn = await findByText("Organize Music");
    const buttonEl = organizeBtn.closest("button")!;
    await waitFor(() => {
      expect(buttonEl).not.toBeDisabled();
    });
    await fireEvent.click(buttonEl);

    // Click confirm in dialog
    const confirmBtns = await findAllByText("Organize Files");
    const confirmBtn = confirmBtns.find((el) => el.tagName === "BUTTON")!;
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("apply_organize", {
        items: [
          { song_id: 1, from_path: "/music/a.mp3", to_path: "/music/Rock/a.mp3" },
          { song_id: 2, from_path: "/music/b.mp3", to_path: "/music/Rock/b.mp3" },
        ],
        cleanEmptyDirs: true,
        moveExtraFiles: true,
      });
    });

    expect(updateConfigSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        clean_empty_dirs: true,
        move_extra_files: true,
      })
    );
  });

  it("opens confirmation dialog when pressing Enter outside inputs/buttons", async () => {
    const { findByText, findAllByText } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1, 2],
    });

    const organizeBtn = await findByText("Organize Music");
    const buttonEl = organizeBtn.closest("button")!;
    await waitFor(() => {
      expect(buttonEl).not.toBeDisabled();
    });

    // Fire Enter on window/body
    await fireEvent.keyDown(window, { key: "Enter" });

    // Confirm dialog should be open
    const dialogTitles = await findAllByText("Organize Files");
    expect(dialogTitles.length).toBeGreaterThanOrEqual(1);
    expect(
      await findByText((content) =>
        content.includes("You are about to move 2 files into 1 folder.")
      )
    ).toBeInTheDocument();
  });

  it("marks draft active without writing to SQLite when options change, and Revert restores store values", async () => {
    const setConfigSpy = vi.fn();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "set_organize_config") {
        setConfigSpy();
        return Promise.resolve(false);
      }
      if (cmd === "preview_organize") {
        return Promise.resolve([
          { song_id: 1, from_path: "/music/a.mp3", to_path: "/music/Rock/a.mp3", status: "ok", error_message: null },
        ]);
      }
      return Promise.resolve(null);
    });

    const { findByText, findByRole, queryByText } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1],
    });

    // Wait for store initialization
    await findByText("Organize Music");

    expect(organizeStore.isDraftActive).toBe(false);

    // Switch to Custom preset
    const customPresetBtn = await findByRole("button", { name: "Custom" });
    await fireEvent.click(customPresetBtn);

    // isDraftActive should now be true
    await waitFor(() => {
      expect(organizeStore.isDraftActive).toBe(true);
    });

    // Verify set_organize_config was NOT called
    expect(setConfigSpy).not.toHaveBeenCalled();

    // Revert button should be visible
    const revertBtn = await findByText("Revert");
    expect(revertBtn).toBeInTheDocument();

    // Clicking Revert restores store config
    await fireEvent.click(revertBtn);

    await waitFor(() => {
      expect(organizeStore.isDraftActive).toBe(false);
    });
    expect(queryByText("Revert")).not.toBeInTheDocument();
  });
});

