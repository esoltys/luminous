import type { FileEntry, FileEntryStatus } from "../../types";
import { shownName } from "./shownName";

const STATUS_ALIASES: Record<string, FileEntryStatus> = {
  library: "inLibrary",
  inlibrary: "inLibrary",
  yes: "inLibrary",
  unscanned: "notScanned",
  notscanned: "notScanned",
  art: "albumArt",
  albumart: "albumArt",
  notaudio: "notAudio",
  excluded: "excluded",
  other: "notFromLuminous",
  notfromluminous: "notFromLuminous",
};

export interface ParsedFileFilter {
  /** Lowercased file extensions without leading dot (e.g. ["flac", "cue"]). */
  extensions: string[];
  /** Target FileEntryStatus values, or "unknown" when an unrecognized status: token was given. */
  statuses: (FileEntryStatus | "unknown")[];
  /** Lowercased free-text substrings that must all appear in the entry's shown name. */
  terms: string[];
  /** Raw free-text portion joined by spaces (for "Search your whole library for …"). */
  freeText: string;
  /** True when at least one extension, status, or free-text term is active. */
  isActive: boolean;
}

/**
 * Parses a Files filter query supporting `ext:<type>`, `status:<state>`, and
 * free-text substring terms (#1886). Incomplete prefix tokens (`ext:`,
 * `status:`) are ignored while typing so the list doesn't blank out mid-token.
 */
export function parseFileFilter(query: string): ParsedFileFilter {
  const extensions: string[] = [];
  const statuses: (FileEntryStatus | "unknown")[] = [];
  const terms: string[] = [];
  const freeParts: string[] = [];

  const tokens = query.trim().split(/\s+/).filter(Boolean);
  for (const token of tokens) {
    const lower = token.toLowerCase();
    if (lower.startsWith("ext:")) {
      const ext = lower.slice(4).replace(/^\.+/, "").trim();
      if (ext) extensions.push(ext);
    } else if (lower.startsWith("status:")) {
      const rawStatus = lower.slice(7).replace(/[-_]/g, "").trim();
      if (rawStatus) {
        statuses.push(STATUS_ALIASES[rawStatus] ?? "unknown");
      }
    } else {
      terms.push(lower);
      freeParts.push(token);
    }
  }

  return {
    extensions,
    statuses,
    terms,
    freeText: freeParts.join(" "),
    isActive: extensions.length > 0 || statuses.length > 0 || terms.length > 0,
  };
}

/**
 * Whether `entry` matches `filter`. Multiple `ext:` or `status:` tokens match
 * any of the listed extensions/statuses (OR within category), while different
 * categories and multiple free-text terms combine with AND.
 */
export function matchesFileFilter(entry: FileEntry, filter: ParsedFileFilter): boolean {
  if (!filter.isActive) return true;

  if (filter.extensions.length > 0) {
    if (entry.isDir) return false;
    const dotIdx = entry.name.lastIndexOf(".");
    const nameExt = dotIdx > 0 ? entry.name.slice(dotIdx + 1).toLowerCase() : "";
    const typeExt = entry.fileType?.toLowerCase() ?? "";
    const matchesExt = filter.extensions.some((ext) => ext === nameExt || ext === typeExt);
    if (!matchesExt) return false;
  }

  if (filter.statuses.length > 0) {
    const matchesStatus = filter.statuses.some((st) => st !== "unknown" && entry.status === st);
    if (!matchesStatus) return false;
  }

  if (filter.terms.length > 0) {
    const display = shownName(entry.name, entry.missingTag).toLowerCase();
    for (const term of filter.terms) {
      if (!display.includes(term)) return false;
    }
  }

  return true;
}

export interface HighlightSegment {
  text: string;
  match: boolean;
}

/**
 * Splits `text` into contiguous matched and unmatched segments for rendering
 * highlighted filter matches without raw HTML injection.
 */
export function highlightSegments(text: string, terms: string[]): HighlightSegment[] {
  const validTerms = terms.filter((t) => t.length > 0);
  if (!text || validTerms.length === 0) {
    return [{ text, match: false }];
  }

  const lower = text.toLowerCase();
  const matchedMask = new Uint8Array(text.length);

  for (const rawTerm of validTerms) {
    const term = rawTerm.toLowerCase();
    let start = 0;
    while (start <= lower.length - term.length) {
      const idx = lower.indexOf(term, start);
      if (idx === -1) break;
      matchedMask.fill(1, idx, idx + term.length);
      start = idx + 1;
    }
  }

  const segments: HighlightSegment[] = [];
  let segStart = 0;
  let currentMatch = matchedMask[0] === 1;

  for (let i = 1; i < text.length; i++) {
    const isMatch = matchedMask[i] === 1;
    if (isMatch !== currentMatch) {
      segments.push({ text: text.slice(segStart, i), match: currentMatch });
      segStart = i;
      currentMatch = isMatch;
    }
  }
  segments.push({ text: text.slice(segStart), match: currentMatch });
  return segments;
}
