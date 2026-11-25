import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AddStationModal from "./AddStationModal.svelte";
import { radioStore } from "../../stores/radio.svelte";
import type { RadioDirectoryStation } from "../../types";

function makeStation(overrides: Partial<RadioDirectoryStation> = {}): RadioDirectoryStation {
  return {
    stationuuid: "custom-123",
    name: "SomaFM: Indie Pop Rocks!",
    urlResolved: "https://ice1.somafm.com/indiepop-128-mp3",
    homepage: "",
    favicon: "",
    tags: "indie, pop",
    country: "",
    countrycode: "",
    language: "",
    codec: "MP3",
    bitrate: 128,
    hls: false,
    votes: 0,
    clickcount: 0,
    lastcheckok: true,
    changeuuid: "",
    updatedAt: 0,
    playable: true,
    reducedQuality: false,
    ...overrides,
  };
}

describe("AddStationModal.svelte", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("edits a custom station, invokes onSaved, and closes", async () => {
    const station = makeStation();
    const updateSpy = vi
      .spyOn(radioStore, "updateCustomStation")
      .mockResolvedValue({ ...station, name: "Updated Station" });
    const onSaved = vi.fn();
    const onClose = vi.fn();

    const { getByTestId } = render(AddStationModal, {
      station,
      onSaved,
      onClose,
    });

    const nameInput = getByTestId("add-station-name") as HTMLInputElement;
    expect(nameInput.disabled).toBe(false);
    await fireEvent.input(nameInput, { target: { value: "Updated Station" } });

    await fireEvent.click(getByTestId("add-station-submit"));

    await waitFor(() => {
      expect(updateSpy).toHaveBeenCalledWith(
        "custom-123",
        "Updated Station",
        "https://ice1.somafm.com/indiepop-128-mp3",
        "indie, pop",
      );
      expect(onSaved).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("renders directory stations as disabled without a save button", () => {
    const station = makeStation({
      stationuuid: "dir-uuid-456",
      country: "United States",
    });

    const { getByTestId, queryByTestId } = render(AddStationModal, {
      station,
      onClose: vi.fn(),
    });

    expect(getByTestId("station-readonly-note")).toBeInTheDocument();
    expect((getByTestId("add-station-name") as HTMLInputElement).disabled).toBe(true);
    expect((getByTestId("add-station-url") as HTMLInputElement).disabled).toBe(true);
    expect((getByTestId("add-station-tags") as HTMLInputElement).disabled).toBe(true);
    expect(queryByTestId("add-station-submit")).toBeNull();
    expect(getByTestId("test-station-connection")).not.toBeDisabled();
  });

  it("tests the station URI and displays connection success and failure", async () => {
    const station = makeStation();
    const testSpy = vi
      .spyOn(radioStore, "testStationUrl")
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        resolvedUrl: "https://ice1.somafm.com/indiepop-128-mp3",
        contentType: "audio/mpeg",
        bitrate: 128,
        stationName: "Indie Pop Rocks!",
        error: null,
      })
      .mockResolvedValueOnce({
        ok: false,
        status: 404,
        resolvedUrl: "https://ice1.somafm.com/indiepop-128-mp3",
        contentType: null,
        bitrate: null,
        stationName: null,
        error: "Server returned HTTP 404",
      });

    const { getByTestId } = render(AddStationModal, {
      station,
      onClose: vi.fn(),
    });

    await fireEvent.click(getByTestId("test-station-connection"));
    await waitFor(() => {
      expect(testSpy).toHaveBeenCalledWith("https://ice1.somafm.com/indiepop-128-mp3");
      expect(getByTestId("station-connection-result")).toHaveTextContent(
        "Connected · HTTP 200 · audio/mpeg · 128 kbps",
      );
    });

    await fireEvent.click(getByTestId("test-station-connection"));
    await waitFor(() => {
      expect(getByTestId("station-connection-result")).toHaveTextContent(
        "Server returned HTTP 404",
      );
    });
  });
});
