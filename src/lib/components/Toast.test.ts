import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import Toast from "./Toast.svelte";
import { toastStore } from "../stores/toast.svelte";
import { i18n } from "../stores/i18n.svelte";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

describe("Toast.svelte", () => {
  let writeTextMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    i18n.currentLocale = "en";
    writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, {
      clipboard: {
        writeText: writeTextMock,
      },
    });
    for (const m of [...toastStore.messages]) {
      toastStore.dismiss(m.id);
    }
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders an error toast with a copy button and copies text to clipboard on click", async () => {
    toastStore.show("Failed to save tags: write error", "error");

    render(Toast);

    expect(screen.getByText("Failed to save tags: write error")).toBeInTheDocument();

    const copyBtn = screen.queryByLabelText("Copy error to clipboard");
    expect(copyBtn).toBeInTheDocument();
    expect(copyBtn).toHaveAttribute("title", "Copy error to clipboard");

    await fireEvent.click(copyBtn!);

    expect(writeTextMock).toHaveBeenCalledWith("Failed to save tags: write error");

    expect(screen.queryByLabelText("Copied to clipboard")).toBeInTheDocument();

    await vi.advanceTimersByTimeAsync(1500);

    expect(screen.queryByLabelText("Copy error to clipboard")).toBeInTheDocument();
    expect(screen.queryByLabelText("Copied to clipboard")).not.toBeInTheDocument();
  });

  it("does not render a copy button for non-error toasts", () => {
    toastStore.show("Playlist saved", "success");
    toastStore.show("Scanned 50 songs", "info");
    toastStore.show("Playback warning", "warning");
    toastStore.show("Milestone reached!", "milestone");

    render(Toast);

    expect(screen.queryByLabelText("Copy error to clipboard")).not.toBeInTheDocument();
    expect(screen.queryByLabelText(/copy/i)).not.toBeInTheDocument();
  });

  it("dismisses the toast when clicking the dismiss button", async () => {
    const id = toastStore.show("Temporary error", "error");

    render(Toast);

    const dismissBtn = screen.getByLabelText("Dismiss notification");
    expect(dismissBtn).toBeInTheDocument();

    await fireEvent.click(dismissBtn);

    expect(toastStore.messages.find((m) => m.id === id)).toBeUndefined();
  });

  it("renders localized labels in French", async () => {
    i18n.currentLocale = "fr";
    toastStore.show("Échec de l'enregistrement", "error");

    render(Toast);

    const copyBtn = screen.queryByLabelText("Copier l'erreur dans le presse-papiers");
    expect(copyBtn).toBeInTheDocument();
    expect(copyBtn).toHaveAttribute("title", "Copier l'erreur dans le presse-papiers");

    const dismissBtn = screen.queryByLabelText("Fermer la notification");
    expect(dismissBtn).toBeInTheDocument();

    await fireEvent.click(copyBtn!);

    expect(screen.queryByLabelText("Copié dans le presse-papiers")).toBeInTheDocument();
  });

  it("renders a toast action button and invokes its callback on click without dismissing the toast", async () => {
    const onClick = vi.fn();
    const id = toastStore.show("Update downloaded — restart to finish installing.", "success", undefined, undefined, {
      label: "Restart to Update",
      onClick,
    });

    render(Toast);

    const actionBtn = screen.getByText("Restart to Update");
    expect(actionBtn).toBeInTheDocument();

    await fireEvent.click(actionBtn);

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(toastStore.messages.find((m) => m.id === id)).toBeDefined();
  });
});
