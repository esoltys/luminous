export interface Rule {
  field: string;
  op: string;
  value: string;
}

export function stripEnclosingQuotes(str: string): string {
  let s = str.trim();
  while (
    (s.startsWith('"') && s.endsWith('"') && s.length >= 2) ||
    (s.startsWith("'") && s.endsWith("'") && s.length >= 2)
  ) {
    s = s.slice(1, -1).trim();
  }
  return s;
}

export function parseSearchRules(query: string): Rule[] {
  const rules: Rule[] = [];
  if (!query.trim()) return rules;

  const tokens: string[] = [];
  let current = "";
  let inQuotes = false;
  let quoteChar = "";
  let quoteCount = 0;
  let escaped = false;
  let i = 0;

  while (i < query.length) {
    const ch = query[i];

    if (ch === "\\" && !escaped) {
      escaped = true;
      current += ch;
      i++;
      continue;
    }

    if ((ch === '"' || ch === "'") && !escaped) {
      if (inQuotes && ch === quoteChar) {
        let count = 0;
        while (i + count < query.length && query[i + count] === quoteChar) {
          count++;
        }
        if (count >= quoteCount) {
          for (let c = 0; c < count; c++) {
            current += quoteChar;
          }
          i += count;
          inQuotes = false;
          quoteChar = "";
          quoteCount = 0;
          escaped = false;
          continue;
        } else {
          current += ch;
          i++;
          escaped = false;
          continue;
        }
      } else if (!inQuotes) {
        let count = 0;
        while (i + count < query.length && query[i + count] === ch) {
          count++;
        }
        const nextChar = i + count < query.length ? query[i + count] : "";
        if (count === 2 && (!nextChar || /\s|;/.test(nextChar))) {
          current += ch + ch;
          i += 2;
          escaped = false;
          continue;
        }
        inQuotes = true;
        quoteChar = ch;
        quoteCount = count;
        for (let c = 0; c < count; c++) {
          current += ch;
        }
        i += count;
        escaped = false;
        continue;
      }
    }

    if (/\s/.test(ch) && !inQuotes) {
      if (current.trim()) {
        tokens.push(current.trim());
        current = "";
      }
    } else {
      current += ch;
    }
    escaped = false;
    i++;
  }
  if (current.trim()) {
    tokens.push(current.trim());
  }

  for (const token of tokens) {
    const colonIdx = token.indexOf(":");
    if (colonIdx > 0) {
      const field = token.slice(0, colonIdx).trim().toLowerCase();
      let rawVal = token.slice(colonIdx + 1).trim();
      rawVal = stripEnclosingQuotes(rawVal);
      let op = "=";
      let hasExplicitOp = false;

      if (rawVal.startsWith(">=")) {
        op = ">=";
        rawVal = rawVal.slice(2);
        hasExplicitOp = true;
      } else if (rawVal.startsWith("<=")) {
        op = "<=";
        rawVal = rawVal.slice(2);
        hasExplicitOp = true;
      } else if (rawVal.startsWith("!=")) {
        op = "!=";
        rawVal = rawVal.slice(2);
        hasExplicitOp = true;
      } else if (rawVal.startsWith(">")) {
        op = ">";
        rawVal = rawVal.slice(1);
        hasExplicitOp = true;
      } else if (rawVal.startsWith("<")) {
        op = "<";
        rawVal = rawVal.slice(1);
        hasExplicitOp = true;
      } else if (rawVal.startsWith("=")) {
        op = "=";
        rawVal = rawVal.slice(1);
        hasExplicitOp = true;
      }

      rawVal = stripEnclosingQuotes(rawVal);
      rawVal = rawVal.replace(/\\"/g, '"').replace(/\\'/g, "'").replace(/\\\\/g, "\\");
      rawVal = stripEnclosingQuotes(rawVal);

      let normalizedField = field;
      if (
        [
          "artist_tag",
          "artist-tag",
          "artisttag",
          "artist_tags",
        ].includes(field)
      ) {
        normalizedField = "artist_tag";
        if (!hasExplicitOp) op = "contains";
      } else if (
        [
          "folder",
          "subfolder",
          "directory",
          "path",
        ].includes(field)
      ) {
        normalizedField = "folder";
        if (!hasExplicitOp) op = "contains";
      } else if (
        [
          "artist",
          "album_artist",
          "album",
          "title",
          "genre",
          "composer",
          "key",
          "initial_key",
          "tag",
          "tags",
        ].includes(field)
      ) {
        if (!hasExplicitOp) op = "contains";
      }

      if (normalizedField && rawVal) {
        rules.push({ field: normalizedField, op, value: rawVal });
      }
    }
  }

  return rules;
}

export function hasAdvancedSearchTerms(query: string): boolean {
  return parseSearchRules(query).length > 0;
}

/**
 * True if a playlist's `dynamic_spec` is a user-authored Smart Playlist rule
 * spec (e.g. "genre:rock", "artist:Miles Davis; rating:>=4"), as opposed to a
 * system genre/decade/BPM/artist-tag/daypart auto-playlist. Smart Playlist specs always contain
 * a "field:value" rule; system genre auto-playlists use a "tag:" prefix
 * (#548), decade auto-playlists use "decade:", BPM auto-playlists use
 * "bpmrange:", artist tag auto-playlists use "artisttag:", and the Moment
 * Mix (formerly Daypart Mix, #223) auto-playlist uses "daypart:". Mirrors
 * the categorization in playlist.rs.
 */
export function isSmartPlaylistSpec(spec: string | null | undefined): boolean {
  return (
    !!spec &&
    spec.includes(":") &&
    !spec.startsWith("decade:") &&
    !spec.startsWith("bpmrange:") &&
    !spec.startsWith("tag:") &&
    !spec.startsWith("artisttag:") &&
    !spec.startsWith("daypart:")
  );
}
