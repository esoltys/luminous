import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import WalkthroughOverlay from "./WalkthroughOverlay.svelte";
import { walkthroughStore } from "../stores/walkthrough.svelte";
import { i18n } from "../stores/i18n.svelte";

/** Every step's real anchor lives in a different app component — stub one
 * target element per step id directly on document.body so the overlay can
 * resolve `[data-walkthrough-target="<id>"]` without mounting the whole app. */
function mountAllTargets() {
  for (const step of walkthroughStore.steps) {
    const el = document.createElement("div");
    el.setAttribute("data-walkthrough-target", step.id);
    document.body.appendChild(el);
  }
}

describe("WalkthroughOverlay.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en";
    walkthroughStore.isActive = false;
    walkthroughStore.currentStepIndex = 0;
    walkthroughStore.hasCompleted = false;
    mountAllTargets();
  });

  afterEach(() => {
    document.querySelectorAll("[data-walkthrough-target]").forEach((el) => el.remove());
    walkthroughStore.isActive = false;
  });

  it("renders nothing when the tour isn't active", () => {
    render(WalkthroughOverlay);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows the first step's title and description once started", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();

    await waitFor(() => {
      expect(screen.getByText("Navigate your library")).toBeInTheDocument();
    });
    expect(screen.getByText(/Jump between Home, Collection/)).toBeInTheDocument();
  });

  it("Next advances the store to the next step", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());

    await fireEvent.click(screen.getByText("Next"));

    expect(walkthroughStore.currentStepIndex).toBe(1);
  });

  it("Back is hidden on the first step and appears after advancing", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());

    expect(screen.queryByText("Back")).not.toBeInTheDocument();

    await fireEvent.click(screen.getByText("Next"));
    await waitFor(() => expect(screen.getByText("Back")).toBeInTheDocument());
  });

  it("the skip button deactivates the tour and marks it completed", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());

    await fireEvent.click(screen.getByLabelText("Skip"));

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.hasCompleted).toBe(true);
  });

  it("Escape skips the tour", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(walkthroughStore.isActive).toBe(false);
  });

  it("the last step's button reads Finish and ends the tour", async () => {
    render(WalkthroughOverlay);
    walkthroughStore.start();
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());

    for (let i = 0; i < walkthroughStore.totalSteps; i++) {
      await fireEvent.click(screen.getByText(i === walkthroughStore.totalSteps - 1 ? "Finish" : "Next"));
    }

    expect(walkthroughStore.isActive).toBe(false);
    expect(walkthroughStore.hasCompleted).toBe(true);
  });
});
