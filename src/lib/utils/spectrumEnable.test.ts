import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

describe("spectrumEnable", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.resetModules();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("enables once for the first acquire and ignores further acquires while active", async () => {
    const { acquireSpectrum } = await import("./spectrumEnable");
    acquireSpectrum();
    acquireSpectrum();
    acquireSpectrum();

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: true });
  });

  it("debounces the disable so a release doesn't disable immediately", async () => {
    const { acquireSpectrum, releaseSpectrum } = await import("./spectrumEnable");
    acquireSpectrum();
    releaseSpectrum();

    expect(invoke).toHaveBeenCalledTimes(1); // just the initial enable
    expect(invoke).not.toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });
  });

  it("cancels the pending disable when re-acquired within the debounce window (destroy-then-recreate)", async () => {
    const { acquireSpectrum, releaseSpectrum } = await import("./spectrumEnable");
    acquireSpectrum(); // e.g. the old consumer mounts
    releaseSpectrum(); // ...and is destroyed
    vi.advanceTimersByTime(100); // well within the debounce window
    acquireSpectrum(); // a fresh consumer mounts before the disable fires
    vi.advanceTimersByTime(1000); // run out the debounce window entirely

    expect(invoke).not.toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });
  });

  it("disables once the debounce window elapses with no re-acquire", async () => {
    const { acquireSpectrum, releaseSpectrum } = await import("./spectrumEnable");
    acquireSpectrum();
    releaseSpectrum();
    vi.advanceTimersByTime(1000);

    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });
  });

  it("only disables after the last of several consumers releases", async () => {
    const { acquireSpectrum, releaseSpectrum } = await import("./spectrumEnable");
    acquireSpectrum(); // consumer A
    acquireSpectrum(); // consumer B
    releaseSpectrum(); // A goes away
    vi.advanceTimersByTime(1000);
    expect(invoke).not.toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });

    releaseSpectrum(); // B goes away too
    vi.advanceTimersByTime(1000);
    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });
  });
});
