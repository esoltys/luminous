import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createFeedToggle } from "./feedToggle";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

describe("createFeedToggle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("counts each feed separately", () => {
    const spectrum = createFeedToggle("set_spectrum_enabled");
    const scope = createFeedToggle("set_scope_enabled");
    spectrum.acquire();
    scope.acquire();
    scope.release();
    vi.advanceTimersByTime(1000);

    expect(invoke).toHaveBeenCalledWith("set_spectrum_enabled", { enabled: true });
    expect(invoke).toHaveBeenCalledWith("set_scope_enabled", { enabled: true });
    expect(invoke).toHaveBeenCalledWith("set_scope_enabled", { enabled: false });
    expect(invoke).not.toHaveBeenCalledWith("set_spectrum_enabled", { enabled: false });
  });
});
