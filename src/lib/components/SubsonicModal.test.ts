import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import SubsonicModal from "./SubsonicModal.svelte";
import type { SubsonicServer } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const existing: SubsonicServer = {
  id: 4,
  name: "Home Navidrome",
  url: "https://music.example.com",
  username: "me",
  enabled: true,
  syncStatus: "idle",
  lastSyncedAt: null,
  createdAt: 1700000000,
  autoSyncEnabled: false,
  syncIntervalMinutes: 60,
  reportPlays: true,
  serverType: "navidrome",
  serverVersion: "0.53.3",
  extensions: [],
} as SubsonicServer;

async function fillNewServer(getByPlaceholderText: (t: string) => HTMLElement, password = "pw") {
  await fireEvent.input(getByPlaceholderText("e.g. Home Navidrome"), { target: { value: "Home" } });
  await fireEvent.input(getByPlaceholderText("https://music.example.com"), {
    target: { value: "https://music.example.com" },
  });
  await fireEvent.input(getByPlaceholderText("Username"), { target: { value: "me" } });
  if (password) {
    await fireEvent.input(getByPlaceholderText("Password"), { target: { value: password } });
  }
}

describe("SubsonicModal.svelte", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("keeps Add Server disabled for a new server until a password is entered", async () => {
    const { getByPlaceholderText, getByRole } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText, "");
    expect(getByRole("button", { name: "Add Server" })).toBeDisabled();

    await fireEvent.input(getByPlaceholderText("Password"), { target: { value: "pw" } });
    expect(getByRole("button", { name: "Add Server" })).toBeEnabled();
  });

  it("shows the server type and version after a successful connection test", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ serverType: "navidrome", serverVersion: "0.53.3", extensions: [] });
    const { getByPlaceholderText, getByRole, findByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Test Connection" }));

    expect(invoke).toHaveBeenCalledWith("test_subsonic_connection", {
      url: "https://music.example.com",
      username: "me",
      password: "pw",
      id: null,
    });
    expect(await findByTestId("subsonic-test-success")).toHaveTextContent("Connected to navidrome 0.53.3.");
  });

  it("shows the backend error when the connection test fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Wrong username or password");
    const { getByPlaceholderText, getByRole, findByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Test Connection" }));

    expect(await findByTestId("subsonic-test-failed")).toHaveTextContent(
      "Connection failed: Wrong username or password"
    );
  });

  it("shows save errors instead of closing", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("URL must start with http:// or https://");
    const onSaved = vi.fn();
    const { getByPlaceholderText, getByRole, findByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Add Server" }));

    expect(await findByTestId("subsonic-save-failed")).toHaveTextContent(
      "Couldn't save server: URL must start with http:// or https://"
    );
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("lets an existing server be saved with a blank password and passes its id", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(existing);
    const onSaved = vi.fn();
    const { getByPlaceholderText, getByRole } = render(SubsonicModal, {
      props: { server: existing, onClose: vi.fn(), onSaved },
    });

    const password = getByPlaceholderText("Leave blank to keep the current password") as HTMLInputElement;
    expect(password.value).toBe("");

    await fireEvent.click(getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [cmd, args] = vi.mocked(invoke).mock.calls[0] as [string, { input: Record<string, unknown> }];
    expect(cmd).toBe("save_subsonic_server");
    expect(args.input).toMatchObject({ id: 4, name: "Home Navidrome", username: "me" });
    expect(args.input.password ?? null).toBeNull();
  });
});
