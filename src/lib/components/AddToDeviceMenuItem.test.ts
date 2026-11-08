import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AddToDeviceMenuItem from "./AddToDeviceMenuItem.svelte";
import { filesStore } from "../stores/files.svelte";
import { devicesStore } from "../stores/devices.svelte";
import type { ConnectedDevice } from "../types";

const walkman: ConnectedDevice = {
  id: "E:\\",
  kind: "massStorage",
  model: null,
  defaultName: "Walkman",
  storages: [{ id: "E:\\", label: null, mountPath: "E:\\", filesystem: "FAT32", totalBytes: 64_000_000_000, freeBytes: 20_000_000_000 }],
  profile: { nickname: null } as ConnectedDevice["profile"],
  lastSyncedAt: null,
};

const ipod: ConnectedDevice = {
  id: "F:\\",
  kind: "massStorage",
  model: null,
  defaultName: "iPod Classic",
  storages: [{ id: "F:\\", label: null, mountPath: "F:\\", filesystem: "FAT32", totalBytes: 160_000_000_000, freeBytes: 80_000_000_000 }],
  profile: { nickname: null } as ConnectedDevice["profile"],
  lastSyncedAt: null,
};

describe("AddToDeviceMenuItem.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders nothing when no devices are connected", () => {
    devicesStore.devices = [];
    const onClose = vi.fn();
    const { container } = render(AddToDeviceMenuItem, {
      props: {
        source: { kind: "album", album: "Damnation", artist: "Opeth" },
        sourceName: "Damnation",
        onClose,
      },
    });

    expect(container.firstElementChild).toBeNull();
  });

  it("renders direct 1-click item when 1 device is connected", async () => {
    devicesStore.devices = [walkman];
    const addSpy = vi.spyOn(devicesStore, "addToDeviceSync").mockResolvedValue(true);
    const onClose = vi.fn();

    const { getByText } = render(AddToDeviceMenuItem, {
      props: {
        source: { kind: "album", album: "Damnation", artist: "Opeth" },
        sourceName: "Damnation",
        onClose,
      },
    });

    const item = getByText("Add to Walkman");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(addSpy).toHaveBeenCalledWith(walkman, { kind: "album", album: "Damnation", artist: "Opeth" }, "Damnation");
    expect(onClose).toHaveBeenCalled();
  });

  it("renders submenu when multiple devices are connected", async () => {
    devicesStore.devices = [walkman, ipod];
    const addSpy = vi.spyOn(devicesStore, "addToDeviceSync").mockResolvedValue(true);
    const onClose = vi.fn();

    const { getByText } = render(AddToDeviceMenuItem, {
      props: {
        source: { kind: "artist", name: "Opeth" },
        sourceName: "Opeth",
        onClose,
      },
    });

    const trigger = getByText("Add to portable player");
    expect(trigger).toBeInTheDocument();

    // Click trigger to open submenu
    await fireEvent.click(trigger);
    const walkmanOption = getByText("Walkman");
    expect(walkmanOption).toBeInTheDocument();
    expect(getByText("iPod Classic")).toBeInTheDocument();

    await fireEvent.click(walkmanOption);
    expect(addSpy).toHaveBeenCalledWith(walkman, { kind: "artist", name: "Opeth" }, "Opeth");
    expect(onClose).toHaveBeenCalled();
  });

  it("handles songIds directly when adding multiple songs", async () => {
    devicesStore.devices = [walkman];
    const addSongsSpy = vi.spyOn(devicesStore, "addSongsToSync").mockResolvedValue(true);
    const onClose = vi.fn();

    const { getByText } = render(AddToDeviceMenuItem, {
      props: {
        songIds: [1, 2, 3],
        onClose,
      },
    });

    const item = getByText("Add to Walkman");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(addSongsSpy).toHaveBeenCalledWith(walkman, [1, 2, 3]);
    expect(onClose).toHaveBeenCalled();
  });
});
