import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import MusicBrainzProfilePopover from "./MusicBrainzProfilePopover.svelte";
import { musicbrainzStore } from "../stores/musicbrainz.svelte";
import { scrobblerStore } from "../stores/scrobbler.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { openExternalUrl } from "../utils/openExternalUrl";

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
    if (cmd === "get_scrobble_cache_status") {
      return Promise.resolve({
        pending_count: 0,
        last_error: null,
        last_attempt: null,
      });
    }
    return Promise.resolve(null);
  }),
}));

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
}));

describe("MusicBrainzProfilePopover.svelte", () => {
  const onClose = vi.fn();
  let anchorEl: HTMLButtonElement;

  beforeEach(() => {
    vi.clearAllMocks();
    anchorEl = document.createElement("button");
    document.body.appendChild(anchorEl);

    musicbrainzStore.username = "test_mb_user";
    musicbrainzStore.email = "test@example.com";
    scrobblerStore.enabled = true;
    scrobblerStore.paused = false;
    scrobblerStore.nowPlayingEnabled = true;
    scrobblerStore.username = "test_lb_user";
  });

  it("renders user identity in header and closes on close button click", async () => {
    const { getByText, getByTitle, queryByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    expect(getByText("test_mb_user")).toBeInTheDocument();
    expect(queryByText("test@example.com")).not.toBeInTheDocument();

    const closeBtn = getByTitle("Close");
    await fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalled();
  });

  it("does not render About MusicBrainz or the small ListenBrainz icon", () => {
    const { queryByText, container } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    expect(queryByText("About MusicBrainz")).not.toBeInTheDocument();
    expect(container.querySelector('img[src="/listenbrainz-icon.png"]')).not.toBeInTheDocument();
  });

  it("displays Active badge when scrobbling is enabled and not paused", () => {
    scrobblerStore.enabled = true;
    scrobblerStore.paused = false;

    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    expect(getByText("Scrobbling")).toBeInTheDocument();
    expect(getByText("Active")).toBeInTheDocument();
  });

  it("displays Paused badge when scrobbling is enabled and paused", () => {
    scrobblerStore.enabled = true;
    scrobblerStore.paused = true;

    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    expect(getByText("Paused")).toBeInTheDocument();
  });

  it("displays Inactive badge when scrobbling is not enabled", () => {
    scrobblerStore.enabled = false;

    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    expect(getByText("Inactive")).toBeInTheDocument();
  });

  it("allows toggling Pause all scrobbling and Send Now Playing status in the popup", async () => {
    scrobblerStore.enabled = true;
    scrobblerStore.paused = false;
    scrobblerStore.nowPlayingEnabled = true;

    const setPausedSpy = vi.spyOn(scrobblerStore, "setPaused");
    const setNowPlayingSpy = vi.spyOn(scrobblerStore, "setNowPlayingEnabled");

    const { getByLabelText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    const pauseToggle = getByLabelText("Pause all scrobbling");
    await fireEvent.click(pauseToggle);
    expect(setPausedSpy).toHaveBeenCalledWith(true);

    const nowPlayingToggle = getByLabelText("Send Now Playing status");
    await fireEvent.click(nowPlayingToggle);
    expect(setNowPlayingSpy).toHaveBeenCalledWith(false);
  });

  it("opens ListenBrainz listener profile and MusicBrainz editor profile links", async () => {
    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    const lbProfileBtn = getByText("ListenBrainz listener profile");
    await fireEvent.click(lbProfileBtn);
    expect(openExternalUrl).toHaveBeenCalledWith("https://listenbrainz.org/user/test_lb_user/");

    const mbProfileBtn = getByText("MusicBrainz editor profile");
    await fireEvent.click(mbProfileBtn);
    expect(openExternalUrl).toHaveBeenCalledWith("https://musicbrainz.org/user/test_mb_user");
  });

  it("navigates to integration settings when clicked", async () => {
    const openSettingsSpy = vi.spyOn(navigationStore, "openSettings");

    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    const settingsBtn = getByText("Integration settings");
    await fireEvent.click(settingsBtn);

    expect(openSettingsSpy).toHaveBeenCalledWith("integrations");
    expect(onClose).toHaveBeenCalled();
  });

  it("calls logout and onClose when Log out is clicked", async () => {
    const logoutSpy = vi.spyOn(musicbrainzStore, "logout").mockResolvedValue();

    const { getByText } = render(MusicBrainzProfilePopover, {
      isOpen: true,
      anchorEl,
      onClose,
    });

    const logoutBtn = getByText("Log out");
    await fireEvent.click(logoutBtn);
    await tick();

    expect(logoutSpy).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });
});
