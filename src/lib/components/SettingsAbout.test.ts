import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { flushSync } from "svelte";
import SettingsAbout from "./SettingsAbout.svelte";
import { i18n } from "../stores/i18n.svelte";
import { updaterStore } from "../stores/updater.svelte";
import { openExternalUrl } from "../utils/openExternalUrl";

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_commit_hash") {
      return Promise.resolve("048f421");
    }
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.75.0"),
}));

describe("SettingsAbout.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
  });

  it("renders the localized Ko-fi support callout and opens Ko-fi on click", async () => {
    const { getByRole, getByText } = render(SettingsAbout);

    expect(
      getByText(
        "Luminous is free and open source. If it's become part of your daily listening, consider supporting development.",
      ),
    ).toBeInTheDocument();

    const kofiButton = getByRole("button", { name: "Support me on Ko-fi" });
    expect(kofiButton).toBeInTheDocument();

    await fireEvent.click(kofiButton);
    expect(openExternalUrl).toHaveBeenCalledWith("https://ko-fi.com/K3K711LEE");
  });

  it("localizes the Ko-fi button and support text", () => {
    i18n.currentLocale = "fr-CA";
    const { getByRole, getByText } = render(SettingsAbout);

    expect(
      getByText(
        "Luminous est gratuit et à code source ouvert. S'il fait désormais partie de votre écoute quotidienne, pensez à soutenir son développement.",
      ),
    ).toBeInTheDocument();
    expect(getByRole("button", { name: "Me soutenir sur Ko-fi" })).toBeInTheDocument();
  });

  it("labels the bug link plainly and opens the bug report form", async () => {
    const { getByRole } = render(SettingsAbout);
    await fireEvent.click(getByRole("button", { name: /Report a bug/ }));
    expect(openExternalUrl).toHaveBeenCalledWith(
      "https://github.com/esoltys/luminous/issues/new?template=bug_report.yml",
    );
  });

  it("renders the updated personal home tagline", () => {
    const { getByText } = render(SettingsAbout);
    expect(getByText("Your personal home for the music you already own.")).toBeInTheDocument();
  });

  it("renders the Application & Updates card above the external links grid", async () => {
    const { findByRole, getByRole } = render(SettingsAbout);
    const checkUpdatesBtn = await findByRole("button", { name: "Check for Updates" });
    const homepageBtn = getByRole("button", { name: /Luminous Homepage/ });

    expect(
      checkUpdatesBtn.compareDocumentPosition(homepageBtn) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("renders the version number and build commit hash", async () => {
    const { findByText } = render(SettingsAbout);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
    expect(await findByText(/build 048f421/)).toBeInTheDocument();
  });

  it("pops the up-to-date check after a manual Check for Updates (#1239)", async () => {
    vi.spyOn(updaterStore, "checkForUpdates").mockImplementation(async () => {
      updaterStore.checkStatus = "up-to-date";
    });
    updaterStore.checkStatus = "idle";
    const { findByRole, container } = render(SettingsAbout);

    await fireEvent.click(await findByRole("button", { name: "Check for Updates" }));

    await vi.waitFor(() => expect(container.querySelector(".anim-check-pop")).not.toBeNull());
  });

  it("does not pop the check when a background check comes back up-to-date (#1239)", async () => {
    updaterStore.checkStatus = "idle";
    const { findByText, container } = render(SettingsAbout);
    await findByText(/v0\.75\.0/);

    updaterStore.checkStatus = "checking";
    flushSync();
    updaterStore.checkStatus = "up-to-date";
    flushSync();

    expect(container.querySelector(".anim-check-pop")).toBeNull();
  });
});
