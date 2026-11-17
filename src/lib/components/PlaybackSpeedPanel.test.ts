import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import PlaybackSpeedPanel from "./PlaybackSpeedPanel.svelte";
import { playerStore } from "../stores/player.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  // The backend answers with the speed it applied.
  invoke: vi.fn(async (_cmd: string, args?: { speed?: number }) => args?.speed ?? null),
}));

describe("PlaybackSpeedPanel.svelte (#2142)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    playerStore.playbackSpeed = 1;
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("reads the speed from the player store, to two decimals", () => {
    playerStore.playbackSpeed = 1.25;
    render(PlaybackSpeedPanel);
    expect(screen.getByText("1.25×", { selector: "[aria-live]" })).toBeInTheDocument();
  });

  it("offers the presets and marks the one in effect", () => {
    playerStore.playbackSpeed = 1.5;
    render(PlaybackSpeedPanel);
    expect(screen.getByRole("button", { name: /^1.5×$/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /^2×$/ })).toHaveAttribute("aria-pressed", "false");
    expect(screen.getByRole("button", { name: /^1×\s+Normal$/ })).toBeInTheDocument();
  });

  it("applies a preset at once", async () => {
    render(PlaybackSpeedPanel);
    await fireEvent.click(screen.getByRole("button", { name: /^2×$/ }));
    expect(invoke).toHaveBeenCalledWith("set_playback_speed", { speed: 2 });
    expect(playerStore.playbackSpeed).toBe(2);
  });

  it("steps by 0.05 with the + and - buttons", async () => {
    playerStore.playbackSpeed = 1;
    render(PlaybackSpeedPanel);
    await fireEvent.click(screen.getByRole("button", { name: "Faster" }));
    expect(invoke).toHaveBeenLastCalledWith("set_playback_speed", { speed: 1.05 });
    await fireEvent.click(screen.getByRole("button", { name: "Slower" }));
    // The store holds what the backend applied (1.05), so one step down is 1.00.
    expect(invoke).toHaveBeenLastCalledWith("set_playback_speed", { speed: 1 });
  });

  it("disables the step buttons at the ends of the range", () => {
    playerStore.playbackSpeed = 3;
    const { unmount } = render(PlaybackSpeedPanel);
    expect(screen.getByRole("button", { name: "Faster" })).toBeDisabled();
    unmount();
    playerStore.playbackSpeed = 0.25;
    render(PlaybackSpeedPanel);
    expect(screen.getByRole("button", { name: "Slower" })).toBeDisabled();
  });

  it("limits the slider to 0.25-3.0 in 0.05 steps", () => {
    render(PlaybackSpeedPanel);
    const slider = screen.getByRole("slider");
    expect(slider).toHaveAttribute("min", "0.25");
    expect(slider).toHaveAttribute("max", "3");
    expect(slider).toHaveAttribute("step", "0.05");
  });

  it("follows a drag in the readout but applies it at most once per interval", async () => {
    vi.useFakeTimers();
    render(PlaybackSpeedPanel);
    const slider = screen.getByRole("slider");
    await fireEvent.input(slider, { target: { value: "1.2" } });
    await fireEvent.input(slider, { target: { value: "1.4" } });
    expect(screen.getByText("1.40×", { selector: "[aria-live]" })).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(150);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("set_playback_speed", { speed: 1.4 });
  });

  it("applies the released value at once and snaps it to the grid", async () => {
    render(PlaybackSpeedPanel);
    await fireEvent.change(screen.getByRole("slider"), { target: { value: "1.12" } });
    expect(invoke).toHaveBeenCalledWith("set_playback_speed", { speed: 1.1 });
  });
});
