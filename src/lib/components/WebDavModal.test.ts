import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import WebDavModal from "./WebDavModal.svelte";
import type { WebDavServer } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const existing: WebDavServer = {
  id: 1,
  name: "My NAS",
  url: "https://nas.example.com",
  username: "alice",
  remotePath: "/Music",
  enabled: true,
  syncStatus: "idle",
  lastSyncedAt: null,
  createdAt: 1700000000,
  autoSyncEnabled: false,
  syncIntervalMinutes: 60,
  nickname: null,
  icon: "cloud",
  color: null,
};

function callsOf(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd);
}

describe("WebDavModal.svelte", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("defaults auto-sync to on for a new server and sends autoSyncEnabled: true", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(existing);
    const onSaved = vi.fn();
    const { getByRole, container } = render(WebDavModal, {
      props: { server: null, onClose: vi.fn(), onSaved },
    });

    const toggle = getByRole("switch", { name: "Auto-Sync" });
    expect(toggle).toHaveAttribute("aria-checked", "true");

    const nameInput = container.querySelector("#webdav-name") as HTMLInputElement;
    const urlInput = container.querySelector("#webdav-url") as HTMLInputElement;
    await fireEvent.input(nameInput, { target: { value: "My WebDAV" } });
    await fireEvent.input(urlInput, { target: { value: "https://nas.example.com" } });
    await fireEvent.click(getByRole("button", { name: "Add WebDAV" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [, args] = callsOf("save_webdav_server")[0] as [string, { input: Record<string, unknown> }];
    expect(args.input).toMatchObject({ autoSyncEnabled: true });
  });

  it("preserves auto-sync disabled state when editing an existing server with auto-sync off", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(existing);
    const onSaved = vi.fn();
    const { getByRole } = render(WebDavModal, {
      props: { server: existing, onClose: vi.fn(), onSaved },
    });

    const toggle = getByRole("switch", { name: "Auto-Sync" });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [, args] = callsOf("save_webdav_server")[0] as [string, { input: Record<string, unknown> }];
    expect(args.input).toMatchObject({ autoSyncEnabled: false });
  });

  it("preserves auto-sync enabled state when editing an existing server with auto-sync on", async () => {
    const existingWithAutoSync: WebDavServer = {
      ...existing,
      autoSyncEnabled: true,
    };
    vi.mocked(invoke).mockResolvedValueOnce(existingWithAutoSync);
    const onSaved = vi.fn();
    const { getByRole } = render(WebDavModal, {
      props: { server: existingWithAutoSync, onClose: vi.fn(), onSaved },
    });

    const toggle = getByRole("switch", { name: "Auto-Sync" });
    expect(toggle).toHaveAttribute("aria-checked", "true");

    await fireEvent.click(getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [, args] = callsOf("save_webdav_server")[0] as [string, { input: Record<string, unknown> }];
    expect(args.input).toMatchObject({ autoSyncEnabled: true });
  });
});
