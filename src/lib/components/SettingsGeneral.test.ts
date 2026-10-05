import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsGeneral from "./SettingsGeneral.svelte";
import { invoke } from "@tauri-apps/api/core";
import { prefs } from "../stores/prefs.svelte";
import { i18n } from "../stores/i18n.svelte";
import { updaterStore } from "../stores/updater.svelte";
import { flushSync } from "svelte";

const platform = vi.hoisted(() => ({ isWindows: false }));
vi.mock("../platform", () => ({
  get isWindows() {
    return platform.isWindows;
  },
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_commit_hash") {
      return Promise.resolve("048f421");
    }
    if (cmd === "get_minimize_to_tray_enabled" || cmd === "get_autostart_enabled") {
      return Promise.resolve(false);
    }
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.75.0"),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("SettingsGeneral.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
  });

  it("renders the version number and build commit hash", async () => {
    const { findByText } = render(SettingsGeneral);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
    expect(await findByText(/build 048f421/)).toBeInTheDocument();
  });

  it("reflects prefs.autostartEnabled and calls set_autostart_enabled on toggle", async () => {
    prefs.autostartEnabled = false;
    const { findByRole } = render(SettingsGeneral);
    const toggle = await findByRole("switch", { name: "Launch at login" });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(toggle);

    expect(prefs.autostartEnabled).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_autostart_enabled", { enabled: true });
  });

  it("lists every registered UI language by its own name and switches on pick", async () => {
    const { findByLabelText } = render(SettingsGeneral);
    const select = (await findByLabelText(/Language/)) as HTMLSelectElement;
    expect([...select.options].map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ["en-CA", "English (Canada)"],
      ["fr-CA", "Français (Canada)"],
    ]);

    await fireEvent.change(select, { target: { value: "fr-CA" } });

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "language", value: "fr-CA" });
  });

  it("opens Windows Default Apps from the default player row", async () => {
    platform.isWindows = true;
    const { findByRole } = render(SettingsGeneral);

    await fireEvent.click(await findByRole("button", { name: "Open Default Apps" }));

    expect(invoke).toHaveBeenCalledWith("open_default_apps_settings");
  });

  it("hides the default player row outside Windows", async () => {
    platform.isWindows = false;
    const { findByText, queryByText } = render(SettingsGeneral);

    await findByText(/v0\.75\.0/);
    expect(queryByText("Make Luminous the default music player")).not.toBeInTheDocument();
  });

  it("pops the up-to-date check after a manual Check for Updates (#1239)", async () => {
    vi.spyOn(updaterStore, "checkForUpdates").mockImplementation(async () => {
      updaterStore.checkStatus = "up-to-date";
    });
    updaterStore.checkStatus = "idle";
    const { findByRole, container } = render(SettingsGeneral);

    await fireEvent.click(await findByRole("button", { name: "Check for Updates" }));

    await vi.waitFor(() => expect(container.querySelector(".anim-check-pop")).not.toBeNull());
  });

  it("does not pop the check when a background check comes back up-to-date (#1239)", async () => {
    updaterStore.checkStatus = "idle";
    const { findByText, container } = render(SettingsGeneral);
    await findByText(/v0\.75\.0/);

    updaterStore.checkStatus = "checking";
    flushSync();
    updaterStore.checkStatus = "up-to-date";
    flushSync();

    expect(container.querySelector(".anim-check-pop")).toBeNull();
  });
});
