/** The pattern Organize uses until the user picks another. Mirrors `organizer.rs`. */
export const DEFAULT_ORGANIZE_TEMPLATE = "%albumartist/{%year - }{%album/}{%disc-}{%track }%title";

/**
 * One level of the path a preset produces, described in words for people who
 * never read pattern syntax. `optional` marks a level that only some songs get,
 * such as a disc folder that exists only for multi-disc albums.
 */
interface OrganizeOutlineRow {
  kind: "folder" | "file";
  labelKey: string;
  optional?: boolean;
}

export interface OrganizePreset {
  id: "default" | "alternative";
  labelKey: string;
  template: string;
  /** Folder rows from outermost in, then exactly one file row. Kept in step with `template` by organizePresets.test.ts. */
  outline: readonly OrganizeOutlineRow[];
}

/** The built-in Organize presets, in the order their cards appear. */
export const ORGANIZE_PRESETS: readonly OrganizePreset[] = [
  {
    id: "default",
    labelKey: "organizer.presetDefault",
    template: DEFAULT_ORGANIZE_TEMPLATE,
    outline: [
      { kind: "folder", labelKey: "organizer.outlineAlbumArtist" },
      { kind: "folder", labelKey: "organizer.outlineYearAlbum" },
      { kind: "file", labelKey: "organizer.outlineTrackTitle" },
    ],
  },
  {
    id: "alternative",
    labelKey: "organizer.presetAlternative",
    template: "%artist/%album (%year)/{CD %disc/}{%track-}%artist-%title",
    outline: [
      { kind: "folder", labelKey: "organizer.outlineArtist" },
      { kind: "folder", labelKey: "organizer.outlineAlbumYear" },
      { kind: "folder", labelKey: "organizer.outlineDiscFolder", optional: true },
      { kind: "file", labelKey: "organizer.outlineTrackArtistTitle" },
    ],
  },
];
