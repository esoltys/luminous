import { describe, it, expect } from "vitest";
import {
  getParentFolder,
  getFolderName,
  isDiscFolder,
  getAlbumFolderPath,
} from "./pathUtils";

describe("pathUtils", () => {
  describe("isDiscFolder", () => {
    it("identifies disc and CD subfolders correctly", () => {
      expect(isDiscFolder("Disc 1")).toBe(true);
      expect(isDiscFolder("disc 02")).toBe(true);
      expect(isDiscFolder("CD1")).toBe(true);
      expect(isDiscFolder("CD 2")).toBe(true);
      expect(isDiscFolder("Disk 1")).toBe(true);
      expect(isDiscFolder("CD A")).toBe(true);
      expect(isDiscFolder("disc b")).toBe(true);
    });

    it("rejects non-disc folders", () => {
      expect(isDiscFolder("")).toBe(false);
      expect(isDiscFolder("Albatross")).toBe(false);
      expect(isDiscFolder("2012 - Albatross")).toBe(false);
      expect(isDiscFolder("Discography")).toBe(false);
      expect(isDiscFolder("CDs")).toBe(false);
    });
  });

  describe("getParentFolder", () => {
    it("handles Windows backslash paths", () => {
      expect(
        getParentFolder(
          "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-01 Head Together.flac"
        )
      ).toBe("C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross");
    });

    it("handles Windows drive root path", () => {
      expect(getParentFolder("C:\\song.mp3")).toBe("C:\\");
    });

    it("handles Unix paths", () => {
      expect(getParentFolder("/home/user/music/Big Wreck/2012 - Albatross/01.flac")).toBe(
        "/home/user/music/Big Wreck/2012 - Albatross"
      );
    });

    it("handles Unix root path", () => {
      expect(getParentFolder("/song.mp3")).toBe("/");
    });

    it("handles WebDAV URLs", () => {
      expect(
        getParentFolder(
          "https://example.com/remote.php/webdav/Music/Big%20Wreck/2012%20-%20Albatross/01.flac"
        )
      ).toBe("https://example.com/remote.php/webdav/Music/Big%20Wreck/2012%20-%20Albatross");
    });

    it("handles subsonic:// paths without falling back to a drive root", () => {
      expect(getParentFolder("subsonic://3/tr-abc123")).toBe("subsonic://3/");
    });

    it("handles empty or relative path without directory", () => {
      expect(getParentFolder("")).toBe("");
      expect(getParentFolder("song.mp3")).toBe("");
    });
  });

  describe("getFolderName", () => {
    it("extracts folder name from paths", () => {
      expect(
        getFolderName(
          "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross"
        )
      ).toBe("2012 - Albatross");
      expect(getFolderName("/music/Big Wreck/Disc 1")).toBe("Disc 1");
      expect(getFolderName("Disc 2")).toBe("Disc 2");
    });
  });

  describe("getAlbumFolderPath", () => {
    it("resolves the album directory for single or multiple tracks in the same folder", () => {
      const paths = [
        "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-01 Head Together.flac",
        "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross\\1-02 All Is Fair.flac",
      ];
      expect(getAlbumFolderPath(paths)).toBe(
        "C:\\Users\\ericj\\OneDrive\\Music\\Big Wreck\\2012 - Albatross"
      );
    });

    it("steps up from disc subfolders (e.g. Disc 1, Disc 2) to the album directory", () => {
      const paths = [
        "C:\\Music\\Pink Floyd\\The Wall\\Disc 1\\01 In the Flesh.flac",
        "C:\\Music\\Pink Floyd\\The Wall\\Disc 2\\01 Hey You.flac",
      ];
      expect(getAlbumFolderPath(paths)).toBe("C:\\Music\\Pink Floyd\\The Wall");
    });

    it("steps up from a single disc subfolder (e.g. CD1)", () => {
      const paths = [
        "C:\\Music\\Artist\\Album\\CD1\\01 Track.flac",
        "C:\\Music\\Artist\\Album\\CD1\\02 Track.flac",
      ];
      expect(getAlbumFolderPath(paths)).toBe("C:\\Music\\Artist\\Album");
    });

    it("handles Unix paths with disc subfolders", () => {
      const paths = [
        "/home/user/Music/Artist/Album/CD 1/01.mp3",
        "/home/user/Music/Artist/Album/CD 2/01.mp3",
      ];
      expect(getAlbumFolderPath(paths)).toBe("/home/user/Music/Artist/Album");
    });

    it("handles WebDAV URLs", () => {
      const paths = [
        "https://example.com/webdav/Music/Album/CD1/01.flac",
        "https://example.com/webdav/Music/Album/CD2/01.flac",
      ];
      expect(getAlbumFolderPath(paths)).toBe("https://example.com/webdav/Music/Album");
    });

    it("returns empty string if no paths are provided", () => {
      expect(getAlbumFolderPath([])).toBe("");
      expect(getAlbumFolderPath(["", "   "])).toBe("");
    });
  });
});
