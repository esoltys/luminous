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
  authMode: "token",
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

/** Route mocked IPC by command so the debounced auth-support lookup never
 * consumes a response meant for the test or save call. */
function mockCommands(handlers: Record<string, (args: any) => unknown>, apiKey = false) {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
    if (cmd === "get_subsonic_auth_support") return { apiKey };
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unexpected command ${cmd}`);
    return handler(args);
  });
}

function callsOf(cmd: string) {
  return vi.mocked(invoke).mock.calls.filter(([c]) => c === cmd);
}

async function fillNewServer(getByPlaceholderText: (t: string) => HTMLElement, password = "pw", url = "https://music.example.com") {
  await fireEvent.input(getByPlaceholderText("e.g. Home Navidrome"), { target: { value: "Home" } });
  await fireEvent.input(getByPlaceholderText("https://music.example.com"), { target: { value: url } });
  await fireEvent.input(getByPlaceholderText("Username"), { target: { value: "me" } });
  if (password) {
    await fireEvent.input(getByPlaceholderText("Password"), { target: { value: password } });
  }
}

describe("SubsonicModal.svelte", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    mockCommands({});
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
    mockCommands({
      test_subsonic_connection: () => ({ serverType: "navidrome", serverVersion: "0.53.3", extensions: [] }),
    });
    const { getByPlaceholderText, getByRole, findByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Test Connection" }));

    expect(invoke).toHaveBeenCalledWith("test_subsonic_connection", {
      url: "https://music.example.com",
      username: "me",
      password: "pw",
      authMode: "token",
      id: null,
    });
    expect(await findByTestId("subsonic-test-success")).toHaveTextContent("Connected to navidrome 0.53.3.");
  });

  it("shows the backend error when the connection test fails", async () => {
    mockCommands({
      test_subsonic_connection: () => {
        throw { message: "Wrong username or password", code: 40 };
      },
    });
    const { getByPlaceholderText, getByRole, findByTestId, queryByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Test Connection" }));

    expect(await findByTestId("subsonic-test-failed")).toHaveTextContent(
      "Connection failed: Wrong username or password"
    );
    expect(queryByTestId("subsonic-use-password")).toBeNull();
  });

  it("offers Password sign-in when the server rejects token auth with error 41", async () => {
    mockCommands({
      test_subsonic_connection: () => {
        throw { message: "This server doesn't support token sign-in", code: 41 };
      },
    });
    const { getByPlaceholderText, getByRole, findByTestId, getByLabelText, queryByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText);
    await fireEvent.click(getByRole("button", { name: "Test Connection" }));
    await fireEvent.click(await findByTestId("subsonic-use-password"));

    expect((getByLabelText("Sign-in method") as HTMLSelectElement).value).toBe("password");
    expect(queryByTestId("subsonic-test-failed")).toBeNull();
  });

  it("warns that Password sign-in over plain http:// is unencrypted", async () => {
    const { getByPlaceholderText, getByLabelText, queryByTestId } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await fillNewServer(getByPlaceholderText, "pw", "http://192.168.1.10:4533");
    expect(queryByTestId("subsonic-http-warning")).toBeNull();

    await fireEvent.change(getByLabelText("Sign-in method"), { target: { value: "password" } });
    expect(queryByTestId("subsonic-http-warning")).not.toBeNull();

    await fireEvent.input(getByPlaceholderText("https://music.example.com"), {
      target: { value: "https://music.example.com" },
    });
    expect(queryByTestId("subsonic-http-warning")).toBeNull();
  });

  it("offers API key sign-in only when the server advertises it", async () => {
    const { getByPlaceholderText, getByLabelText, queryByRole, findByRole } = render(SubsonicModal, {
      props: { server: null, onClose: vi.fn(), onSaved: vi.fn() },
    });
    await fillNewServer(getByPlaceholderText);
    await waitFor(() => expect(callsOf("get_subsonic_auth_support").length).toBeGreaterThan(0));
    expect(queryByRole("option", { name: "API key" })).toBeNull();

    mockCommands({}, true);
    await fireEvent.input(getByPlaceholderText("https://music.example.com"), {
      target: { value: "https://music.example.com/" },
    });
    await findByRole("option", { name: "API key" });

    const select = getByLabelText("Sign-in method") as HTMLSelectElement;
    await fireEvent.change(select, { target: { value: "apiKey" } });
    expect(select.value).toBe("apiKey");
  });

  it("signs in with only an API key, sending no username", async () => {
    mockCommands(
      { save_subsonic_server: () => ({ ...existing, authMode: "apiKey", username: "" }) },
      true
    );
    const onSaved = vi.fn();
    const { getByPlaceholderText, getByLabelText, getByRole, findByRole, queryByPlaceholderText } = render(
      SubsonicModal,
      { props: { server: null, onClose: vi.fn(), onSaved } }
    );

    await fillNewServer(getByPlaceholderText, "");
    await findByRole("option", { name: "API key" });
    await fireEvent.change(getByLabelText("Sign-in method"), { target: { value: "apiKey" } });

    expect(queryByPlaceholderText("Username")).toBeNull();
    expect(getByRole("button", { name: "Add Server" })).toBeDisabled();
    await fireEvent.input(getByPlaceholderText("API key"), { target: { value: "key-123" } });
    await fireEvent.click(getByRole("button", { name: "Add Server" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [, args] = callsOf("save_subsonic_server")[0] as [string, { input: Record<string, unknown> }];
    expect(args.input).toMatchObject({ username: "", password: "key-123", authMode: "apiKey" });
  });

  it("shows save errors instead of closing", async () => {
    mockCommands({
      save_subsonic_server: () => {
        throw "URL must start with http:// or https://";
      },
    });
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
    mockCommands({ save_subsonic_server: () => existing });
    const onSaved = vi.fn();
    const { getByPlaceholderText, getByRole } = render(SubsonicModal, {
      props: { server: existing, onClose: vi.fn(), onSaved },
    });

    const password = getByPlaceholderText("Leave blank to keep the current password") as HTMLInputElement;
    expect(password.value).toBe("");

    await fireEvent.click(getByRole("button", { name: "Save Changes" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    const [, args] = callsOf("save_subsonic_server")[0] as [string, { input: Record<string, unknown> }];
    expect(args.input).toMatchObject({ id: 4, name: "Home Navidrome", username: "me", authMode: "token" });
    expect(args.input.password ?? null).toBeNull();
  });

  it("requires a new API key when switching an existing server to API key sign-in", async () => {
    mockCommands({}, true);
    const { getByLabelText, getByRole, findByRole, getByPlaceholderText } = render(SubsonicModal, {
      props: { server: existing, onClose: vi.fn(), onSaved: vi.fn() },
    });

    await findByRole("option", { name: "API key" });
    await fireEvent.change(getByLabelText("Sign-in method"), { target: { value: "apiKey" } });

    expect(getByRole("button", { name: "Save Changes" })).toBeDisabled();
    await fireEvent.input(getByPlaceholderText("API key"), { target: { value: "key-123" } });
    expect(getByRole("button", { name: "Save Changes" })).toBeEnabled();
  });
});
