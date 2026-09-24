import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
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
    if (cmd === "list_subsonic_servers") {
      return Promise.resolve([
        {
          id: 7,
          name: "Home Navidrome",
          url: "https://music.example.com",
          username: "me",
          enabled: true,
          syncStatus: "idle",
          lastSyncedAt: 1700000000,
          createdAt: 1700000000,
          autoSyncEnabled: false,
          syncIntervalMinutes: 60,
          reportPlays: true,
          serverType: "navidrome",
          serverVersion: "0.53.3",
          extensions: [],
        },
      ]);
    }
    if (cmd === "check_subsonic_connection") {
      return Promise.reject("Wrong username or password");
    }
    if (cmd === "sync_subsonic_server") {
      return Promise.resolve({ added: 3, updated: 1, removed: 0, errors: 0 });
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

describe("SettingsFolders.svelte - Media servers section", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists configured OpenSubsonic servers with their type and a disconnected status", async () => {
    const { findByText } = render(SettingsFolders);

    expect(await findByText("Media Servers")).toBeInTheDocument();
    expect(await findByText("https://music.example.com · navidrome 0.53.3")).toBeInTheDocument();
    expect(await findByText("Disconnected (Server unreachable?)")).toBeInTheDocument();
  });

  it("syncs a server and reports the stats", async () => {
    const { findByTestId, findByText } = render(SettingsFolders);

    const row = await findByTestId("subsonic-server-row");
    await fireEvent.click(row.querySelector('button[aria-label="Sync Now"]')!);

    expect(invoke).toHaveBeenCalledWith("sync_subsonic_server", { id: 7 });
    expect(
      await findByText("Home Navidrome: 3 added, 1 updated, 0 removed, 0 errors")
    ).toBeInTheDocument();
  });

  it("removes a server only after confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
    const { findByTestId } = render(SettingsFolders);

    const row = await findByTestId("subsonic-server-row");
    const removeBtn = row.querySelector('button[aria-label="Remove media server"]')!;

    await fireEvent.click(removeBtn);
    expect(invoke).not.toHaveBeenCalledWith("delete_subsonic_server", expect.anything());

    await fireEvent.click(removeBtn);
    expect(invoke).toHaveBeenCalledWith("delete_subsonic_server", { id: 7 });
    confirmSpy.mockRestore();
  });

  it("opens SubsonicModal when clicking Add Server", async () => {
    const { findByText } = render(SettingsFolders);

    await fireEvent.click(await findByText("Add Server"));

    expect(await findByText("Report Plays")).toBeInTheDocument();
  });
});
