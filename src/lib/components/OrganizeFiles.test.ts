import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { organizeStore } from "../stores/organizer.svelte";
import OrganizeFiles from "./OrganizeFiles.svelte";

vi.mock("svelte-virtual-list-ts", async () => {
  const mod = await import("./__mocks__/VirtualList.svelte");
  return { VirtualList: mod.default };
});

vi.mock("../utils/motion", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../utils/motion")>();
  return {
    ...actual,
    slide: (node: Element, params: any = {}) => actual.slide(node, { ...params, duration: 0 }),
  };
});

const { defaultInvokeHandler } = vi.hoisted(() => {
  const defaultInvokeHandler = (cmd: string) => {
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
          title: "Track A",
          artist: "Artist A",
          album: "Album A",
          missing_fields: [],
        },
        {
          song_id: 2,
          from_path: "/music/b.mp3",
          to_path: "/music/Rock/b.mp3",
          status: "ok",
          error_message: null,
          title: "Track B",
          artist: "Artist B",
          album: "Album B",
          missing_fields: [],
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
  };
  return { defaultInvokeHandler };
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => defaultInvokeHandler(cmd)),
}));

describe("OrganizeFiles safeguards", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((cmd: string) => defaultInvokeHandler(cmd));
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
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
        content.includes("You are about to move 2 files into 1 folder."),
      ),
    ).toBeInTheDocument();

    expect(
      await findByText((content) => content.includes("Empty source folders will be deleted.")),
    ).toBeInTheDocument();

    expect(
      await findByText((content) =>
        content.includes("Companion files (artwork, lyrics, and cue sheets) will also be moved."),
      ),
    ).toBeInTheDocument();

    // Cancel closes dialog without applying
    const cancelBtns = await findAllByText("Cancel");
    const dialogCancelBtn = cancelBtns[cancelBtns.length - 1];
    await fireEvent.click(dialogCancelBtn);

    await waitFor(() => {
      expect(
        queryByText((content) => content.includes("You are about to move 2 files into 1 folder.")),
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
      }),
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
        content.includes("You are about to move 2 files into 1 folder."),
      ),
    ).toBeInTheDocument();
  });

  it("describes presets as a folder/file outline instead of the raw pattern", async () => {
    const { findByRole } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1],
    });

    const defaultCard = await findByRole("button", { name: "Default" });
    expect(defaultCard).toHaveAccessibleDescription(
      "Folder: Album artist. Folder: Year - Album. File name: Track and title.",
    );

    const alternativeCard = await findByRole("button", { name: "Alternative" });
    expect(alternativeCard).toHaveAccessibleDescription(
      "Folder: Artist. Folder: Album (Year). Folder: Disc (multi-disc only). File name: Track, artist and title.",
    );

    // No pattern syntax anywhere on the preset cards.
    for (const card of [defaultCard, alternativeCard]) {
      expect(card.textContent).not.toContain("%");
    }
    // Custom isn't selected, so its pattern field isn't rendered either.
    expect(document.querySelector("#template-input")).toBeNull();
  });

  it("seeds the Custom field with the previously selected preset's pattern", async () => {
    const { findByRole } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1],
    });

    await fireEvent.click(await findByRole("button", { name: "Alternative" }));
    await fireEvent.click(await findByRole("button", { name: "Custom" }));

    await waitFor(() => {
      expect(document.querySelector<HTMLInputElement>("#template-input")?.value).toBe(
        "%artist/%album (%year)/{CD %disc/}{%track-}%artist-%title",
      );
    });
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
          {
            song_id: 1,
            from_path: "/music/a.mp3",
            to_path: "/music/Rock/a.mp3",
            status: "ok",
            error_message: null,
          },
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

  it("filters preview rows with interactive filter pills and hides zero-count pills", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([
          {
            song_id: 1,
            from_path: "/music/track1.mp3",
            to_path: "/music/Rock/track1.mp3",
            status: "ok",
            error_message: null,
            title: "Song Ready",
            artist: "Artist Alpha",
            album: "Album Alpha",
            missing_fields: [],
          },
          {
            song_id: 2,
            from_path: "/music/track2.mp3",
            to_path: "/music/Rock/track2.mp3",
            status: "missing_tag",
            error_message: "Missing title, artist",
            title: null,
            artist: null,
            album: "Album Alpha",
            missing_fields: ["title", "artist"],
          },
        ]);
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText, findAllByText, queryByText, findByRole } = render(OrganizeFiles, {
      isOpen: true,
      embedded: false,
      songIds: [1, 2],
    });

    // Check filter pills
    expect(await findByText("All (2)")).toBeInTheDocument();
    expect(await findByText("Ready to Move (1)")).toBeInTheDocument();
    expect(await findByText("Needs Tags (1)")).toBeInTheDocument();

    // 0-count pills should not be rendered
    expect(queryByText(/Duplicate Copies/)).not.toBeInTheDocument();
    expect(queryByText(/Errors/)).not.toBeInTheDocument();

    // Both songs displayed under "All"
    expect(await findByText("Song Ready")).toBeInTheDocument();
    expect(await findByText("track2")).toBeInTheDocument();
    expect(await findByText("Missing Tag: Title, Artist")).toBeInTheDocument();

    // Context menu includes Edit Tags
    await fireEvent.contextMenu(await findByText("track2"));
    const editTagsOptions = await findAllByText("Edit Tags");
    expect(editTagsOptions.length).toBeGreaterThanOrEqual(1);

    // Filter by "Needs Tags"
    const needsTagsBtn = await findByRole("button", { name: "Needs Tags (1)" });
    await fireEvent.click(needsTagsBtn);

    await waitFor(() => {
      expect(queryByText("Song Ready")).not.toBeInTheDocument();
      expect(queryByText("track2")).toBeInTheDocument();
    });

    // Filter by "Ready to Move"
    const readyBtn = await findByRole("button", { name: "Ready to Move (1)" });
    await fireEvent.click(readyBtn);

    await waitFor(() => {
      expect(queryByText("Song Ready")).toBeInTheDocument();
      expect(queryByText("track2")).not.toBeInTheDocument();
    });
  });

  it("holds back untagged tracks by default and includes them when toggle is checked", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([
          {
            song_id: 1,
            from_path: "/music/track1.mp3",
            to_path: "/music/Rock/track1.mp3",
            status: "ok",
            error_message: null,
            title: "Song Ready",
            artist: "Artist Alpha",
            album: "Album Alpha",
            missing_fields: [],
          },
          {
            song_id: 2,
            from_path: "/music/track2.mp3",
            to_path: "/music/Unknown/track2.mp3",
            status: "missing_tag",
            error_message: "Missing title, artist",
            title: null,
            artist: null,
            album: null,
            missing_fields: ["title", "artist"],
          },
        ]);
      }
      if (cmd === "apply_organize") {
        return Promise.resolve({ moved_count: 1, skipped_count: 0, errors: [] });
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText, findAllByText, findByLabelText } = render(OrganizeFiles, {
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

    // Default: only 1 file is about to move (un-tagged track held back)
    expect(
      await findByText((content) =>
        content.includes("You are about to move 1 file into 1 folder."),
      ),
    ).toBeInTheDocument();

    // Cancel dialog
    const cancelBtns = await findAllByText("Cancel");
    await fireEvent.click(cancelBtns[cancelBtns.length - 1]);

    // Turn on the "Include songs with missing tags" toggle
    const toggle = await findByLabelText("Include songs with missing tags");
    await fireEvent.click(toggle);

    // Click Organize Music again
    await waitFor(() => {
      expect(buttonEl).not.toBeDisabled();
    });
    await fireEvent.click(buttonEl);

    // Now 2 files are included
    expect(
      await findByText((content) => content.includes("You are about to move 2 files")),
    ).toBeInTheDocument();

    // Confirm dialog applies both items
    const confirmBtns = await findAllByText("Organize Files");
    const confirmBtn = confirmBtns.find((el) => el.tagName === "BUTTON")!;
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("apply_organize", {
        items: [
          { song_id: 1, from_path: "/music/track1.mp3", to_path: "/music/Rock/track1.mp3" },
          { song_id: 2, from_path: "/music/track2.mp3", to_path: "/music/Unknown/track2.mp3" },
        ],
        cleanEmptyDirs: true,
        moveExtraFiles: true,
      });
    });
  });

  it("collapses template and destination in embedded mode and expands on Customize", async () => {
    const { findByText, findAllByText, queryByText, findByRole } = render(OrganizeFiles, {
      embedded: true,
      songIds: [],
    });

    // Summary bar displays active template and destination
    expect(await findByText("Template:")).toBeInTheDocument();
    expect(await findByText("Destination:")).toBeInTheDocument();

    // Full template section is collapsed by default in embedded mode
    expect(queryByText("Sample Output")).not.toBeInTheDocument();

    // Click "Customize…" button
    const customizeBtn = await findByRole("button", { name: "Customize…" });
    await fireEvent.click(customizeBtn);

    // Full section should now be visible
    expect(await findByText("Sample Output")).toBeInTheDocument();
    expect(await findByText("Done")).toBeInTheDocument();

    // Click "Done" collapses it back
    const doneBtn = await findByRole("button", { name: "Done" });
    await fireEvent.click(doneBtn);

    await waitFor(() => {
      expect(queryByText("Sample Output")).not.toBeInTheDocument();
    });
  });

  it("hides the embedded preview card when no file would move", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([
          {
            song_id: 1,
            from_path: "/music/Artist/Album/01 Song.mp3",
            to_path: "/music/Artist/Album/01 Song.mp3",
            status: "unchanged",
            error_message: null,
          },
        ]);
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText, queryByRole } = render(OrganizeFiles, { embedded: true, songIds: [] });

    // The summary bar renders, and the preview has had time to resolve.
    await findByText("Template:");
    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("preview_organize", expect.anything()),
    );
    expect(queryByRole("button", { name: "Refresh Preview" })).not.toBeInTheDocument();
  });

  it("shows the embedded preview card when files would move", async () => {
    const { findByRole } = render(OrganizeFiles, { embedded: true, songIds: [] });

    // defaultInvokeHandler's preview moves two files.
    expect(await findByRole("button", { name: "Refresh Preview" })).toBeInTheDocument();
  });

  it("summarizes a preset as its outline and shows the raw pattern only for Custom", async () => {
    const { findByText, findByRole, queryByText } = render(OrganizeFiles, {
      embedded: true,
      songIds: [],
    });

    const summaryBar = (await findByText("Template:")).parentElement!;
    expect(summaryBar).toHaveTextContent("Default");
    expect(summaryBar).toHaveTextContent("Album artist");
    expect(summaryBar).toHaveTextContent("Track and title");
    expect(summaryBar.textContent).not.toContain("%");

    await fireEvent.click(await findByRole("button", { name: "Customize…" }));
    await fireEvent.click(await findByRole("button", { name: "Custom" }));
    await fireEvent.click(await findByRole("button", { name: "Done" }));
    await waitFor(() => expect(queryByText("Sample Output")).not.toBeInTheDocument());

    const customBar = (await findByText("Template:")).parentElement!;
    expect(customBar.querySelector("code")).toHaveTextContent(
      "%albumartist/{%year - }{%album/}{%disc-}{%track }%title",
    );
  });
});
