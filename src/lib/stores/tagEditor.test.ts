import { beforeEach, describe, expect, it, vi } from "vitest";
import { radioStore } from "./radio.svelte";
import { tagEditorStore } from "./tagEditor.svelte";

describe("tagEditorStore", () => {
  beforeEach(() => {
    tagEditorStore.close();
    radioStore.closeStationEditor();
    vi.restoreAllMocks();
  });

  it("opens a library song target and tracks open state", () => {
    expect(tagEditorStore.isOpen).toBe(false);
    expect(tagEditorStore.target).toBeNull();

    const onSaved = vi.fn();
    tagEditorStore.open(42, { onSaved });

    expect(tagEditorStore.isOpen).toBe(true);
    expect(tagEditorStore.target).toEqual({ kind: "song", songId: 42 });
    expect(tagEditorStore.onSaved).toBe(onSaved);
  });

  it("opens an unindexed file target with folder metadata", () => {
    const onSaved = vi.fn();
    tagEditorStore.open(
      {
        filePath: "/music/inbox/track.flac",
        fileName: "track.flac",
        folderPath: ["inbox"],
        folderName: "inbox",
      },
      { onSaved },
    );

    expect(tagEditorStore.isOpen).toBe(true);
    expect(tagEditorStore.target).toEqual({
      kind: "file",
      filePath: "/music/inbox/track.flac",
      fileName: "track.flac",
      folderPath: ["inbox"],
      folderName: "inbox",
    });
    expect(tagEditorStore.onSaved).toBe(onSaved);
  });

  it("replaces the active target and increments openSeq when re-opened", () => {
    tagEditorStore.open(10);
    const firstSeq = tagEditorStore.openSeq;

    tagEditorStore.open(10);
    expect(tagEditorStore.openSeq).toBe(firstSeq + 1);
    expect(tagEditorStore.target).toEqual({ kind: "song", songId: 10 });

    tagEditorStore.open({ filePath: "/other.mp3" });
    expect(tagEditorStore.openSeq).toBe(firstSeq + 2);
    expect(tagEditorStore.target).toEqual({ kind: "file", filePath: "/other.mp3" });
    expect(tagEditorStore.onSaved).toBeUndefined();
  });

  it("clears target and onSaved on close", () => {
    tagEditorStore.open(7, { onSaved: vi.fn() });
    tagEditorStore.close();

    expect(tagEditorStore.isOpen).toBe(false);
    expect(tagEditorStore.target).toBeNull();
    expect(tagEditorStore.onSaved).toBeUndefined();
  });

  it("opens local Song objects in TagEditor without triggering radioStore", () => {
    const spy = vi.spyOn(radioStore, "openStationEditorByUuid");
    const onSaved = vi.fn();

    tagEditorStore.open({ id: 55, path: "/music/track.flac" }, { onSaved });

    expect(spy).not.toHaveBeenCalled();
    expect(tagEditorStore.isOpen).toBe(true);
    expect(tagEditorStore.target).toEqual({ kind: "song", songId: 55 });
    expect(tagEditorStore.onSaved).toBe(onSaved);
  });

  it("routes radio:// Song objects to radioStore instead of opening TagEditor", () => {
    const spy = vi.spyOn(radioStore, "openStationEditorByUuid").mockResolvedValue();
    const onSaved = vi.fn();

    tagEditorStore.open(
      { id: 99, path: "radio://custom-beb67bbf-aa63-40aa-8921-38ec6837255d" },
      { onSaved },
    );

    expect(tagEditorStore.isOpen).toBe(false);
    expect(tagEditorStore.target).toBeNull();
    expect(spy).toHaveBeenCalledWith("custom-beb67bbf-aa63-40aa-8921-38ec6837255d", onSaved);
  });
});
