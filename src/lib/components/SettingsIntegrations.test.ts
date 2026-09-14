import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import SettingsIntegrations from "./SettingsIntegrations.svelte";
import { scrobblerStore } from "../stores/scrobbler.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_scrobbler_settings") {
      return Promise.resolve({
        listenbrainz_enabled: false,
        listenbrainz_token: "",
        listenbrainz_username: null,
        scrobble_now_playing: true,
        scrobble_ratings: true,
        scrobble_paused: false,
        min_duration_secs: 30,
        discord_enabled: false,
        discord_client_id: "1548913001715990610",
        discord_show_album: true,
        discord_show_time: true,
      });
    }
    if (cmd === "get_discord_status") {
      return Promise.resolve("connected");
    }
    if (cmd === "get_scrobble_cache_status") {
      return Promise.resolve({
        pending_count: 0,
        last_error: null,
        last_attempt: null,
      });
    }
    if (cmd === "get_all_app_settings") {
      return Promise.resolve({});
    }
    if (cmd === "get_picard_path") {
      return Promise.resolve(null);
    }
    return Promise.resolve(null);
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("SettingsIntegrations.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders all integration cards: Online Data Sources, ListenBrainz, Discord, and Picard", async () => {
    const { findByText, findByRole } = render(SettingsIntegrations);

    expect(await findByText("Online Data Sources")).toBeInTheDocument();
    expect(await findByText("ListenBrainz Scrobbler")).toBeInTheDocument();
    expect(await findByRole("heading", { name: "Discord Rich Presence" })).toBeInTheDocument();
    expect(await findByRole("heading", { name: "MusicBrainz Picard" })).toBeInTheDocument();
  });

  it("toggles Discord Rich Presence and shows sub-options when enabled", async () => {
    scrobblerStore.discordEnabled = false;

    const { getByLabelText, findByText } = render(SettingsIntegrations);

    const toggle = getByLabelText("Enable Discord Rich Presence");
    expect(toggle).toBeInTheDocument();
    expect(toggle).not.toBeChecked();

    await fireEvent.click(toggle);
    await tick();

    expect(scrobblerStore.discordEnabled).toBe(true);
    expect(await findByText("Show album name")).toBeInTheDocument();
    expect(await findByText("Show elapsed and remaining time")).toBeInTheDocument();
  });

  it("hides Enable toggle until user token is validated, then enables scrobbling", async () => {
    scrobblerStore.username = null;
    scrobblerStore.enabled = false;

    const { findByText, queryByLabelText, getByLabelText } = render(SettingsIntegrations);

    await findByText("ListenBrainz Scrobbler");
    expect(queryByLabelText("Enable ListenBrainz scrobbling")).not.toBeInTheDocument();

    // Simulate successful token validation
    scrobblerStore.username = "test_user";
    await tick();
    const toggle = getByLabelText("Enable ListenBrainz scrobbling");
    expect(toggle).toBeInTheDocument();

    await fireEvent.click(toggle);
    await tick();
    expect(scrobblerStore.enabled).toBe(true);
    expect(await findByText("Send Now Playing status")).toBeInTheDocument();
    expect(await findByText("Synchronize track ratings")).toBeInTheDocument();
  });

  it("renders Picard and ListenBrainz logos", async () => {
    const { findByAltText } = render(SettingsIntegrations);

    const picardImg = await findByAltText("Picard");
    expect(picardImg).toHaveAttribute("src", "/picard-icon.png");

    const lbImg = await findByAltText("ListenBrainz");
    expect(lbImg).toHaveAttribute("src", "/listenbrainz-icon.png");
  });

  it("toggles the Missing MusicBrainz ID auto-playlist setting", async () => {
    const { getByLabelText } = render(SettingsIntegrations);

    const toggle = getByLabelText(/Missing MusicBrainz ID/i);
    expect(toggle).toBeInTheDocument();
    expect(toggle).toBeChecked();

    await fireEvent.click(toggle);
    await tick();
    expect(toggle).not.toBeChecked();
  });
});
