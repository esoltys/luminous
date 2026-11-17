import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsAbout from "./SettingsAbout.svelte";
import { i18n } from "../stores/i18n.svelte";
import { openExternalUrl } from "../utils/openExternalUrl";

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
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
});
