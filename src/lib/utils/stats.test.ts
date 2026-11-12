import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { rateAlbum } from "./stats";

describe("rateAlbum", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("invokes set_album_rating with the album and rating and returns the stored value", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(4);

    await expect(rateAlbum("Fake Nudes", 4)).resolves.toBe(4);
    expect(invoke).toHaveBeenCalledWith("set_album_rating", { album: "Fake Nudes", rating: 4 });
  });

  it("returns the backend's normalized value rather than the requested one", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(-1);

    await expect(rateAlbum("Fake Nudes", 0)).resolves.toBe(-1);
  });

  it("logs and returns null when the command fails", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const failure = new Error("db locked");
    vi.mocked(invoke).mockRejectedValueOnce(failure);

    await expect(rateAlbum("Fake Nudes", 4)).resolves.toBeNull();
    expect(error).toHaveBeenCalledWith("Failed to rate album:", failure);
  });

  it.each([undefined, null, ""])("does not invoke and returns null for album %j", async (name) => {
    await expect(rateAlbum(name, 4)).resolves.toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});
