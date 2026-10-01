import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import MarkdownEditor from "./MarkdownEditor.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("MarkdownEditor.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("does not render when isOpen is false", () => {
    const { container } = render(MarkdownEditor, {
      props: {
        isOpen: false,
        title: "Edit Artist Bio — Devin Townsend",
        initialValue: "Initial bio text",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("renders when isOpen is true and shows initial draft in edit mode", () => {
    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio — Devin Townsend",
        initialValue: "Initial bio text",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("Edit Artist Bio — Devin Townsend")).toBeInTheDocument();
    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.value).toBe("Initial bio text");
  });

  it("toggles between edit and preview mode", async () => {
    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio",
        initialValue: "## Early Life\n\nBorn in Vancouver.",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });

    const previewBtn = screen.getByRole("button", { name: /preview/i });
    await fireEvent.click(previewBtn);

    // Textarea is hidden in preview mode
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByText(/Born in Vancouver/)).toBeInTheDocument();

    const editBtn = screen.getByRole("button", { name: "Edit" });
    await fireEvent.click(editBtn);
    expect(screen.getByRole("textbox")).toBeInTheDocument();
  });

  it("calls onApply with updated content when Apply button is clicked", async () => {
    const onApply = vi.fn();
    const onClose = vi.fn();

    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio",
        initialValue: "Draft text",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply,
        onClose,
      },
    });

    const textarea = screen.getByRole("textbox");
    await fireEvent.input(textarea, { target: { value: "Updated bio from editor" } });

    const applyBtn = screen.getByRole("button", { name: /apply changes/i });
    await fireEvent.click(applyBtn);

    expect(onApply).toHaveBeenCalledWith("Updated bio from editor");
    expect(onClose).toHaveBeenCalled();
  });

  it("invokes backend open command when Open in external editor is clicked", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue("/path/to/artist.md");

    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio",
        initialValue: "My draft bio",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });

    const openExternalBtn = screen.getByTitle(/open this markdown file in your default system editor/i);
    await fireEvent.click(openExternalBtn);

    expect(invoke).toHaveBeenCalledWith("open_artist_bio_file", {
      artist: "Devin Townsend",
      currentContent: "My draft bio",
    });
  });

  it("silently refreshes clean buffer when window regains focus and disk changed", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue("New content from disk");

    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio",
        initialValue: "Original content",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });

    // Window gets focus
    window.dispatchEvent(new Event("focus"));
    await new Promise((r) => setTimeout(r, 10));

    expect(invoke).toHaveBeenCalledWith("read_artist_bio_file", {
      artist: "Devin Townsend",
    });

    const textarea = screen.getByRole("textbox") as HTMLTextAreaElement;
    expect(textarea.value).toBe("New content from disk");
    // No conflict prompt since buffer was clean
    expect(screen.queryByText(/modified on disk/i)).toBeNull();
  });

  it("shows conflict banner when user has local edits and disk changed, and allows reload", async () => {
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue("External edit on disk");

    render(MarkdownEditor, {
      props: {
        isOpen: true,
        title: "Edit Artist Bio",
        initialValue: "Original content",
        targetType: "artist",
        targetKey: "Devin Townsend",
        onApply: vi.fn(),
        onClose: vi.fn(),
      },
    });

    // User types in editor (dirty buffer)
    const textarea = screen.getByRole("textbox");
    await fireEvent.input(textarea, { target: { value: "Local in-app edits" } });

    // Window gets focus
    window.dispatchEvent(new Event("focus"));
    await new Promise((r) => setTimeout(r, 10));

    // Conflict banner appears
    expect(screen.getByText(/modified on disk by an external editor/i)).toBeInTheDocument();
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Local in-app edits");

    // Click Reload from disk
    const reloadBtn = screen.getByRole("button", { name: /reload from disk/i });
    await fireEvent.click(reloadBtn);

    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("External edit on disk");
    expect(screen.queryByText(/modified on disk by an external editor/i)).toBeNull();
  });
});
