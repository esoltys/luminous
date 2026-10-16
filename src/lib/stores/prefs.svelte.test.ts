import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { prefs } from "./prefs.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("prefs.setAutostart", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.autostartEnabled = false;
  });

  it("optimistically sets the new value and persists it via invoke", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await prefs.setAutostart(true);

    expect(prefs.autostartEnabled).toBe(true);
    expect(invoke).toHaveBeenCalledWith("set_autostart_enabled", { enabled: true });
  });

  it("reverts to the previous value when the backend call fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("permission denied"));

    await prefs.setAutostart(true);

    expect(prefs.autostartEnabled).toBe(false);
  });
});

describe("prefs.setOnlineEnabled", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.onlineEnabled = true;
  });

  it("persists through the dedicated command, not the generic setting writer", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await prefs.setOnlineEnabled(false);

    expect(prefs.onlineEnabled).toBe(false);
    expect(invoke).toHaveBeenCalledWith("set_online_enabled", { enabled: false });
  });

  it("reverts when the backend rejects the change", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("db locked"));

    await prefs.setOnlineEnabled(false);

    expect(prefs.onlineEnabled).toBe(true);
  });
});

describe("prefs.audioOutput", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.audioOutputDevices = [];
    prefs.audioOutputConfig = { backend: "cpal", device_id: null };
  });

  it("refreshes audio devices and active config", async () => {
    const mockDevices = [
      { id: "c1", name: "Realtek", backend: "cpal", is_default: true },
      { id: "a1", name: "FlexASIO", backend: "asio", is_default: false },
    ];
    const mockConfig = { backend: "asio", device_id: "a1" };

    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "get_audio_output_devices") return Promise.resolve(mockDevices);
      if (cmd === "get_audio_output_settings") return Promise.resolve(mockConfig);
      return Promise.resolve(undefined);
    });

    await prefs.refreshAudioDevices();

    expect(prefs.audioOutputDevices).toEqual(mockDevices);
    expect(prefs.audioOutputConfig).toEqual(mockConfig);
  });

  it("persists audio output config via invoke", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    const newConfig = { backend: "asio" as const, device_id: "a1" };
    await prefs.setAudioOutput(newConfig);

    expect(prefs.audioOutputConfig).toEqual(newConfig);
    expect(invoke).toHaveBeenCalledWith("set_audio_output_settings", { settings: newConfig });
  });

  it("opens ASIO control panel with target device id", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await prefs.openAsioControlPanel("a1");

    expect(invoke).toHaveBeenCalledWith("open_asio_control_panel", { deviceId: "a1" });
  });
});
