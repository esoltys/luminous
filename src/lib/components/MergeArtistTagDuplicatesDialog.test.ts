import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import MergeArtistTagDuplicatesDialog from "./MergeArtistTagDuplicatesDialog.svelte";
import type { TagDuplicateGroup } from "../types";

const groups: TagDuplicateGroup[] = [
  {
    suggested: "Award-Winning",
    spellings: [
      { name: "Award-Winning", artist_count: 5 },
      { name: "Award Winning", artist_count: 2 },
    ],
  },
  {
    suggested: "Folk",
    spellings: [
      { name: "Folk", artist_count: 1 },
      { name: "folk", artist_count: 1 },
    ],
  },
];

describe("MergeArtistTagDuplicatesDialog", () => {
  it("lists every spelling with its artist count and one merge button per group", () => {
    render(MergeArtistTagDuplicatesDialog, {
      props: { groups, onMerge: vi.fn(), onClose: vi.fn() },
    });

    expect(screen.getByText("Award-Winning")).toBeTruthy();
    expect(screen.getByText("Award Winning")).toBeTruthy();
    expect(screen.getByText("5 artists")).toBeTruthy();
    expect(screen.getByText('Merge into "Award-Winning"')).toBeTruthy();
    expect(screen.getByText('Merge into "Folk"')).toBeTruthy();
    expect(screen.getAllByTestId("tag-duplicate-group")).toHaveLength(2);
  });

  it("merges into the suggested spelling by default", async () => {
    const onMerge = vi.fn();
    render(MergeArtistTagDuplicatesDialog, {
      props: { groups, onMerge, onClose: vi.fn() },
    });

    await fireEvent.click(screen.getByText('Merge into "Award-Winning"'));

    await waitFor(() => expect(onMerge).toHaveBeenCalledWith("Award-Winning", ["Award Winning"]));
  });

  it("merges into the spelling the user picks instead", async () => {
    const onMerge = vi.fn();
    render(MergeArtistTagDuplicatesDialog, {
      props: { groups, onMerge, onClose: vi.fn() },
    });

    await fireEvent.click(screen.getByLabelText(/Award Winning/));

    await fireEvent.click(await screen.findByText('Merge into "Award Winning"'));
    await waitFor(() => expect(onMerge).toHaveBeenCalledWith("Award Winning", ["Award-Winning"]));
  });

  it("closes itself once no groups are left to review", async () => {
    const onClose = vi.fn();
    const { rerender } = render(MergeArtistTagDuplicatesDialog, {
      props: { groups, onMerge: vi.fn(), onClose },
    });
    expect(onClose).not.toHaveBeenCalled();

    await rerender({ groups: [], onMerge: vi.fn(), onClose });

    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });
});
