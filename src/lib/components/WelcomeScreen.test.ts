import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import WelcomeScreen from "./WelcomeScreen.svelte";
import { i18n } from "../stores/i18n.svelte";

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
}));

describe("WelcomeScreen.svelte", () => {
  it("renders the welcome copy and Get Started button", () => {
    i18n.currentLocale = "en-CA";
    render(WelcomeScreen, { onGetStarted: vi.fn() });

    expect(screen.getByText(i18n.t("welcome.title"))).toBeInTheDocument();
    expect(screen.getByText(i18n.t("welcome.subtitle"))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: i18n.t("welcome.getStarted") })).toBeInTheDocument();
  });

  it("calls onGetStarted when the button is clicked", async () => {
    i18n.currentLocale = "en-CA";
    const onGetStarted = vi.fn();
    render(WelcomeScreen, { onGetStarted });

    await fireEvent.click(screen.getByRole("button", { name: i18n.t("welcome.getStarted") }));

    expect(onGetStarted).toHaveBeenCalledOnce();
  });

  it("opens the Terms of Service and Privacy Policy links externally", async () => {
    i18n.currentLocale = "en-CA";
    const { openExternalUrl } = await import("../utils/openExternalUrl");
    render(WelcomeScreen, { onGetStarted: vi.fn() });

    await fireEvent.click(screen.getByRole("button", { name: i18n.t("welcome.termsOfService") }));
    expect(openExternalUrl).toHaveBeenCalledWith("https://github.com/esoltys/luminous/blob/main/TERMS.md#english");

    await fireEvent.click(screen.getByRole("button", { name: i18n.t("welcome.privacyPolicy") }));
    expect(openExternalUrl).toHaveBeenCalledWith("https://github.com/esoltys/luminous/blob/main/PRIVACY.md#english");
  });

  it("switches the card and the legal links to the language picked on it", async () => {
    i18n.currentLocale = "en-CA";
    const { openExternalUrl } = await import("../utils/openExternalUrl");
    render(WelcomeScreen, { onGetStarted: vi.fn() });

    await fireEvent.change(screen.getByRole("combobox", { name: i18n.t("welcome.language") }), {
      target: { value: "de" },
    });

    expect(i18n.currentLocale).toBe("de");
    await fireEvent.click(screen.getByRole("button", { name: i18n.t("welcome.privacyPolicy") }));
    expect(openExternalUrl).toHaveBeenLastCalledWith("https://github.com/esoltys/luminous/blob/main/PRIVACY.md#deutsch");
    i18n.currentLocale = "en-CA";
  });
});
