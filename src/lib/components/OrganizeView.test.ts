import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { organizeStore } from "../stores/organizer.svelte";
import OrganizeView from "./OrganizeView.svelte";

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
      return Promise.resolve([]);
    }
    if (cmd === "apply_organize") {
      return Promise.resolve({
        moved_count: 0,
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

describe("OrganizeView header status", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation((cmd: string) => defaultInvokeHandler(cmd));
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(600);
    await organizeStore.init();
  });

  it("renders nothingToOrganize with celebration milestone star when ready and pending counts are both 0", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([]);
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText } = render(OrganizeView);

    expect(await findByText("Nothing to Organize!")).toBeInTheDocument();
  });

  it("renders summaryWaitingOnCuration with broom icon when ready count is 0 and pending count > 0", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([
          {
            song_id: 1,
            from_path: "/music/track1.mp3",
            to_path: "/music/Rock/track1.mp3",
            status: "missing_tag",
            error_message: "Missing title",
            title: null,
            artist: "Artist",
            album: "Album",
            missing_fields: ["title"],
          },
        ]);
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText } = render(OrganizeView);

    await waitFor(async () => {
      expect(await findByText("1 song waiting on tags or review")).toBeInTheDocument();
    });
  });

  it("renders summaryReady with broom icon when ready count > 0", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "preview_organize") {
        return Promise.resolve([
          {
            song_id: 1,
            from_path: "/music/track1.mp3",
            to_path: "/music/Rock/track1.mp3",
            status: "ok",
            error_message: null,
            title: "Track 1",
            artist: "Artist",
            album: "Album",
            missing_fields: [],
          },
          {
            song_id: 2,
            from_path: "/music/track2.mp3",
            to_path: "/music/Rock/track2.mp3",
            status: "missing_tag",
            error_message: "Missing title",
            title: null,
            artist: "Artist",
            album: "Album",
            missing_fields: ["title"],
          },
        ]);
      }
      return defaultInvokeHandler(cmd);
    });

    const { findByText } = render(OrganizeView);

    await waitFor(async () => {
      expect(await findByText("1 unorganized song")).toBeInTheDocument();
    });
  });
});
