import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsFolders from "./SettingsFolders.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "list_webdav_servers") {
      return Promise.resolve([
        {
          id: 1,
          name: "Nextcloud Music",
          url: "https://cloud.example.com/remote.php/webdav",
          username: "user1",
          remotePath: "/Music",
          enabled: true,
          syncStatus: "idle",
          lastSyncedAt: 1700000000,
          createdAt: 1700000000,
        },
      ]);
    }
    if (cmd === "get_directories") {
      return Promise.resolve([]);
    }
    if (cmd === "get_library_stats") {
      return Promise.resolve({
        total_songs: 120,
        total_albums: 10,
        total_artists: 5,
        total_filesize_bytes: 1073741824,
      });
    }
    if (cmd === "get_library_snapshot") {
      return Promise.resolve({
        songs: [],
        albums: [],
        artists: [],
        total_duration_ns: 0,
      });
    }
    if (cmd === "get_all_app_settings") {
      return Promise.resolve({});
    }
    return Promise.resolve(null);
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("SettingsFolders.svelte - WebDAV section", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the WebDAV remote libraries card and configured server", async () => {
    const { findByText } = render(SettingsFolders);

    expect(await findByText("Remote WebDAV")).toBeInTheDocument();
    expect(await findByText("https://cloud.example.com/remote.php/webdav/Music")).toBeInTheDocument();
  });

  it("opens WebDavModal when clicking Add WebDAV", async () => {
    const { findByText, getByRole } = render(SettingsFolders);

    const addBtn = await findByText("Add WebDAV");
    await fireEvent.click(addBtn);

    expect(await findByText("Server URL")).toBeInTheDocument();
    expect(await findByText("Test Connection")).toBeInTheDocument();
  });
});
