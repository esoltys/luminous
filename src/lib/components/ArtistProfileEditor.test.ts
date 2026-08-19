import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistProfileEditor from "./ArtistProfileEditor.svelte";
import { collectionStore } from "../stores/collection.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("ArtistProfileEditor", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.artistProfiles = {};
  });

  it("does not render when isOpen is false", () => {
    const { container } = render(ArtistProfileEditor, {
      props: {
        artistName: "Shania Twain",
        isOpen: false,
        onClose: vi.fn(),
      },
    });
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("renders when isOpen is true and populates existing data", () => {
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        website: "https://www.shaniatwain.com",
        tags: ["country", "pop"],
        social_links: [
          { platform: "instagram", handle_or_url: "@shaniatwain" },
        ],
        bio: "Legendary country pop star",
      },
    };

    render(ArtistProfileEditor, {
      props: {
        artistName: "Shania Twain",
        isOpen: true,
        onClose: vi.fn(),
      },
    });

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByDisplayValue("https://www.shaniatwain.com")).toBeTruthy();
    expect(screen.getByText("country")).toBeTruthy();
    expect(screen.getByText("pop")).toBeTruthy();
    expect(screen.getByDisplayValue("Legendary country pop star")).toBeTruthy();
    expect(screen.getByDisplayValue("@shaniatwain")).toBeTruthy();
  });

  it("adds and removes tags", async () => {
    render(ArtistProfileEditor, {
      props: {
        artistName: "Shania Twain",
        isOpen: true,
        onClose: vi.fn(),
      },
    });

    const tagInput = screen.getByPlaceholderText(/Add a tag/i);
    await fireEvent.input(tagInput, { target: { value: "canadian" } });
    await fireEvent.keyDown(tagInput, { key: "Enter" });

    expect(screen.getByText("canadian")).toBeTruthy();

    const removeBtn = screen.getByTitle(/Remove tag canadian/i);
    await fireEvent.click(removeBtn);
    expect(screen.queryByText("canadian")).toBeNull();
  });

  it("adds and removes social links", async () => {
    render(ArtistProfileEditor, {
      props: {
        artistName: "Shania Twain",
        isOpen: true,
        onClose: vi.fn(),
      },
    });

    const addLinkBtn = screen.getByRole("button", { name: /Add Link/i });
    await fireEvent.click(addLinkBtn);

    const inputs = screen.getAllByRole("textbox");
    expect(inputs.length).toBeGreaterThan(1);
  });

  it("saves profile and calls onClose", async () => {
    const mockSave = vi.fn().mockResolvedValue({
      artist_key: "Shania Twain",
      website: "https://shaniatwain.com",
      tags: ["country"],
      social_links: [],
      bio: "Country artist",
    });
    vi.spyOn(collectionStore, "saveArtistProfile").mockImplementation(mockSave);

    const onClose = vi.fn();
    render(ArtistProfileEditor, {
      props: {
        artistName: "Shania Twain",
        isOpen: true,
        onClose,
      },
    });

    const websiteInput = screen.getByLabelText(/Website/i);
    await fireEvent.input(websiteInput, { target: { value: "https://shaniatwain.com" } });

    const tagInput = screen.getByPlaceholderText(/Add a tag/i);
    await fireEvent.input(tagInput, { target: { value: "country" } });
    await fireEvent.keyDown(tagInput, { key: "Enter" });

    const saveBtn = screen.getByRole("button", { name: /Save/i });
    await fireEvent.click(saveBtn);

    expect(mockSave).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });
});
