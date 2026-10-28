import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsSystem from "./SettingsSystem.svelte";
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
    if (cmd === "get_audio_output_devices") {
      return Promise.resolve([
        { id: "def-1", name: "Speakers (Realtek Audio)", backend: "cpal", is_default: true },
        { id: "usb-1", name: "USB DAC", backend: "cpal", is_default: false },
        { id: "def-1", name: "Speakers (Realtek Audio)", backend: "wasapi_exclusive", is_default: true },
        { id: "usb-1", name: "USB DAC", backend: "wasapi_exclusive", is_default: false },
        { id: "asio-1", name: "FlexASIO", backend: "asio", is_default: false },
      ]);
    }
    if (cmd === "get_audio_output_settings") {
      return Promise.resolve({ backend: "cpal", device_id: null });
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

describe("SettingsSystem.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
  });

  it("renders the version number and build commit hash", async () => {
    const { findByText } = render(SettingsSystem);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
    expect(await findByText(/build 048f421/)).toBeInTheDocument();
  });

  it("reflects prefs.autostartEnabled and calls set_autostart_enabled on toggle", async () => {
    prefs.autostartEnabled = false;
    const { findByRole } = render(SettingsSystem);
    const toggle = await findByRole("switch", { name: "Launch at login" });
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await fireEvent.click(toggle);

    expect(prefs.autostartEnabled).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_autostart_enabled", { enabled: true });
  });

  it("opens Windows Default Apps from the default player row", async () => {
    platform.isWindows = true;
    const { findByRole } = render(SettingsSystem);

    await fireEvent.click(await findByRole("button", { name: "Open Default Apps" }));

    expect(invoke).toHaveBeenCalledWith("open_default_apps_settings");
  });

  it("hides the default player row outside Windows", async () => {
    platform.isWindows = false;
    const { findByText, queryByText } = render(SettingsSystem);

    await findByText(/v0\.75\.0/);
    expect(queryByText("Make Luminous the default music player")).not.toBeInTheDocument();
  });

  it("pops the up-to-date check after a manual Check for Updates (#1239)", async () => {
    vi.spyOn(updaterStore, "checkForUpdates").mockImplementation(async () => {
      updaterStore.checkStatus = "up-to-date";
    });
    updaterStore.checkStatus = "idle";
    const { findByRole, container } = render(SettingsSystem);

    await fireEvent.click(await findByRole("button", { name: "Check for Updates" }));

    await vi.waitFor(() => expect(container.querySelector(".anim-check-pop")).not.toBeNull());
  });

  it("does not pop the check when a background check comes back up-to-date (#1239)", async () => {
    updaterStore.checkStatus = "idle";
    const { findByText, container } = render(SettingsSystem);
    await findByText(/v0\.75\.0/);

    updaterStore.checkStatus = "checking";
    flushSync();
    updaterStore.checkStatus = "up-to-date";
    flushSync();

    expect(container.querySelector(".anim-check-pop")).toBeNull();
  });

  it("renders the audio output settings card with devices", async () => {
    platform.isWindows = true;
    const { findByText } = render(SettingsSystem);

    expect(await findByText("Audio Output")).toBeInTheDocument();
    expect(await findByText("Speakers (Realtek Audio) (Default Output Device)")).toBeInTheDocument();
  });

  it("handles switching output backend and device", async () => {
    platform.isWindows = true;
    const { findByLabelText } = render(SettingsSystem);

    const backendSelect = await findByLabelText("Output Mode");
    expect(backendSelect).toBeInTheDocument();

    await fireEvent.change(backendSelect, { target: { value: "asio" } });
    expect(invoke).toHaveBeenCalledWith("set_audio_output_settings", {
      settings: { backend: "asio", device_id: "asio-1" },
    });
  });

  it("keeps the chosen device when switching from shared to exclusive mode", async () => {
    platform.isWindows = true;
    const { findByLabelText, findByRole } = render(SettingsSystem);
    await findByRole("option", { name: "USB DAC" });
    prefs.audioOutputConfig = { backend: "cpal", device_id: "usb-1" };

    const backendSelect = await findByLabelText("Output Mode");
    await fireEvent.change(backendSelect, { target: { value: "wasapi_exclusive" } });

    expect(invoke).toHaveBeenCalledWith("set_audio_output_settings", {
      settings: { backend: "wasapi_exclusive", device_id: "usb-1" },
    });
  });

  it("offers the system default device in exclusive mode", async () => {
    platform.isWindows = true;
    const { findByLabelText, findByRole } = render(SettingsSystem);
    await findByRole("option", { name: "USB DAC" });
    prefs.audioOutputConfig = { backend: "wasapi_exclusive", device_id: null };
    flushSync();

    const deviceSelect = (await findByLabelText("Output Device")) as HTMLSelectElement;
    const labels = Array.from(deviceSelect.options).map((o) => o.textContent?.trim());
    expect(labels).toEqual([
      "Default Output Device",
      "Speakers (Realtek Audio) (Default Output Device)",
      "USB DAC",
    ]);
  });

  it("opens the ASIO control panel when requested", async () => {
    platform.isWindows = true;
    prefs.audioOutputConfig = { backend: "asio", device_id: "asio-1" };
    prefs.audioOutputDevices = [
      { id: "asio-1", name: "FlexASIO", backend: "asio", is_default: false },
    ];
    const { findByRole } = render(SettingsSystem);

    const btn = await findByRole("button", { name: "Open Driver Control Panel" });
    await fireEvent.click(btn);

    expect(invoke).toHaveBeenCalledWith("open_asio_control_panel", { deviceId: "asio-1" });
  });
});
