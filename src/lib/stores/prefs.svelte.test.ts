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
