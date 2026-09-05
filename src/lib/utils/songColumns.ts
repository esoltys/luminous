import type { VisibleColumns } from "../stores/collection.svelte";
import type { Song } from "../types";

/**
 * Every toggleable column in the Songs table, in left-to-right table order.
 * Shared by ColumnSelector (visibility checkboxes) and the Songs view's Sort
 * dropdown (options gated to currently-visible, sortable columns), so the two
 * never drift out of sync. `field` is the `Song` property to sort by — it
 * matches `key` except for "format" (-> filetype) and "duration"
 * (-> length_nanosec). Columns with no `field` (currently just "actions")
 * aren't sortable and are skipped when building sort options.
 */
export const SONG_TABLE_COLUMNS: { key: keyof VisibleColumns; field?: keyof Song; label: string }[] = [
  { key: "track",        field: "track",          label: "collection.columnTrack" },
  { key: "title",        field: "title",          label: "collection.columnTitle" },
  { key: "artist",       field: "artist",         label: "collection.columnArtist" },
  { key: "album",        field: "album",          label: "collection.columnAlbum" },
  { key: "composer",     field: "composer",       label: "collection.columnComposer" },
  { key: "album_artist", field: "album_artist",   label: "collection.columnAlbumArtist" },
  { key: "artist_tag",   field: "artist_tag" as keyof Song, label: "collection.columnArtistTag" },
  { key: "format",       field: "filetype",       label: "collection.columnFormat" },
  { key: "year",         field: "year",           label: "collection.columnYear" },
  { key: "originalyear", field: "originalyear",   label: "collection.columnOriginalYear" },
  { key: "genre",        field: "genre",          label: "collection.columnGenre" },
  { key: "grouping",     field: "grouping",       label: "collection.columnGrouping" },
  { key: "bpm",          field: "bpm",            label: "collection.columnBpm" },
  { key: "initial_key",  field: "initial_key",    label: "collection.columnInitialKey" },
  { key: "bitrate",      field: "bitrate",        label: "collection.columnBitrate" },
  { key: "samplerate",   field: "samplerate",     label: "collection.columnSampleRate" },
  { key: "bitdepth",     field: "bitdepth",       label: "collection.columnBitDepth" },
  { key: "channels",     field: "channels",       label: "collection.columnChannels" },
  { key: "filesize",     field: "filesize",       label: "collection.columnFileSize" },
  { key: "rating",       field: "rating",         label: "collection.columnRating" },
  { key: "playcount",    field: "playcount",      label: "collection.columnPlayCount" },
  { key: "skipcount",    field: "skipcount",      label: "collection.columnSkipCount" },
  { key: "lastplayed",   field: "lastplayed",     label: "collection.columnLastPlayed" },
  { key: "added",        field: "added",          label: "collection.columnAdded" },
  { key: "duration",     field: "length_nanosec", label: "collection.columnDuration" },
  { key: "path",         field: "path",           label: "collection.columnPath" },
  { key: "library",                               label: "collection.columnLibrary" },
  { key: "actions",                               label: "collection.columnActions" },
];
