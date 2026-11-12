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
  storages: [
    {
      id: "E:\\",
      label: null,
      mountPath: "E:\\",
      filesystem: "FAT32",
      totalBytes: 64_000_000_000,
      freeBytes: 20_000_000_000,
    },
  ],
  profile: { nickname: null } as ConnectedDevice["profile"],
  lastSyncedAt: null,
};

const ipod: ConnectedDevice = {
  id: "F:\\",
  kind: "massStorage",
  model: null,
  defaultName: "iPod Classic",
  storages: [
    {
      id: "F:\\",
      label: null,
      mountPath: "F:\\",
      filesystem: "FAT32",
      totalBytes: 160_000_000_000,
      freeBytes: 80_000_000_000,
    },
  ],
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

  it("renders direct 1-click item when 1 device is connected and adds when not in sync", async () => {
    devicesStore.devices = [walkman];
    devicesStore.syncStates = {
      [walkman.id]: {
        settings: { storageId: null, sources: [], unselected: "remove" },
        lastSummary: null,
        running: false,
      },
    };
    const toggleSpy = vi.spyOn(devicesStore, "toggleDeviceSync").mockResolvedValue(true);
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
    expect(toggleSpy).toHaveBeenCalledWith(
      walkman,
      { kind: "album", album: "Damnation", artist: "Opeth" },
      "Damnation",
      undefined,
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("renders 'Remove from {device}' when item is already in sync", async () => {
    devicesStore.devices = [walkman];
    devicesStore.syncStates = {
      [walkman.id]: {
        settings: {
          storageId: null,
          sources: [{ kind: "album", album: "Damnation", artist: "Opeth" }],
          unselected: "remove",
        },
        lastSummary: null,
        running: false,
      },
    };
    const toggleSpy = vi.spyOn(devicesStore, "toggleDeviceSync").mockResolvedValue(true);
    const onClose = vi.fn();

    const { getByText } = render(AddToDeviceMenuItem, {
      props: {
        source: { kind: "album", album: "Damnation", artist: "Opeth" },
        sourceName: "Damnation",
        onClose,
      },
    });

    const item = getByText("Remove from Walkman");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(toggleSpy).toHaveBeenCalledWith(
      walkman,
      { kind: "album", album: "Damnation", artist: "Opeth" },
      "Damnation",
      undefined,
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("renders submenu when multiple devices are connected and displays sync status", async () => {
    devicesStore.devices = [walkman, ipod];
    devicesStore.syncStates = {
      [walkman.id]: {
        settings: {
          storageId: null,
          sources: [{ kind: "artist", name: "Opeth" }],
          unselected: "remove",
        },
        lastSummary: null,
        running: false,
      },
      [ipod.id]: {
        settings: { storageId: null, sources: [], unselected: "remove" },
        lastSummary: null,
        running: false,
      },
    };
    const toggleSpy = vi.spyOn(devicesStore, "toggleDeviceSync").mockResolvedValue(true);
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
    expect(getByText("In sync")).toBeInTheDocument();
    expect(getByText("iPod Classic")).toBeInTheDocument();

    await fireEvent.click(walkmanOption);
    expect(toggleSpy).toHaveBeenCalledWith(
      walkman,
      { kind: "artist", name: "Opeth" },
      "Opeth",
      undefined,
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("handles songIds directly and toggles them", async () => {
    devicesStore.devices = [walkman];
    devicesStore.syncStates = {
      [walkman.id]: {
        settings: {
          storageId: null,
          sources: [
            { kind: "song", id: 1 },
            { kind: "song", id: 2 },
          ],
          unselected: "remove",
        },
        lastSummary: null,
        running: false,
      },
    };
    const toggleSpy = vi.spyOn(devicesStore, "toggleDeviceSync").mockResolvedValue(true);
    const onClose = vi.fn();

    const { getByText } = render(AddToDeviceMenuItem, {
      props: {
        songIds: [1, 2],
        onClose,
      },
    });

    // Both songs in sync -> "Remove from Walkman"
    const item = getByText("Remove from Walkman");
    expect(item).toBeInTheDocument();

    await fireEvent.click(item);
    expect(toggleSpy).toHaveBeenCalledWith(walkman, undefined, undefined, [1, 2]);
    expect(onClose).toHaveBeenCalled();
  });
});
