import {
  FileIcon,
  FilePdfIcon,
  FileTextIcon,
  FolderIcon,
  ImageIcon,
  ListNumbersIcon,
  MusicNoteIcon,
  PlaylistIcon,
  QuotesIcon,
  ScrollIcon,
} from "phosphor-svelte";
import type { FileEntry, FileKind } from "../../types";

const KIND_ICONS: Record<FileKind, typeof FileIcon> = {
  audio: MusicNoteIcon,
  image: ImageIcon,
  lyrics: QuotesIcon,
  notes: FileTextIcon,
  pdf: FilePdfIcon,
  cueSheet: ListNumbersIcon,
  log: ScrollIcon,
  playlist: PlaylistIcon,
};

/** The icon for a row of the Files browser: what the entry is, by its kind. */
export function fileIcon(entry: FileEntry) {
  if (entry.isDir) return FolderIcon;
  return entry.kind ? KIND_ICONS[entry.kind] : FileIcon;
}
