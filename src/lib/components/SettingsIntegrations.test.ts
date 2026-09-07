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
      });
    }
    if (cmd === "get_scrobble_cache_status") {
      return Promise.resolve({
        pending_count: 0,
        last_error: null,
        last_attempt: null,
      });
    }
    if (cmd === "has_acoustid_env_key") {
      return Promise.resolve(false);
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

  it("renders all three integration cards: ListenBrainz, Picard, and AcoustID", async () => {
    const { findByText } = render(SettingsIntegrations);

    expect(await findByText("ListenBrainz Scrobbler")).toBeInTheDocument();
    expect(await findByText("MusicBrainz Picard Integration")).toBeInTheDocument();
    expect(await findByText("AcoustID Integration")).toBeInTheDocument();
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
});
