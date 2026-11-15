/**
 * Structured search queries understood by the Artists and Albums views, which
 * filter in the browser against loaded artist profiles. The Songs view sends
 * the same `country:` syntax to the backend (`filter_parser.rs`, #2095), so the
 * rules here mirror it: `=`, `:` and `!=` only, and the value must be a whole
 * two-letter code. Anything else is not a structured query (plain text).
 */

export type ArtistQuery =
  { kind: "tag"; tag: string } | { kind: "country"; code: string; negate: boolean };

const TAG_RE = /^(?:artist[-_]?tags?|artisttags?|tags?):(.+)$/i;
const COUNTRY_RE = /^(?:artist[-_]?country|country):\s*(!=|=)?\s*(.+)$/i;

function stripQuotes(value: string): string {
  return value.replace(/^['"]|['"]$/g, "").trim();
}

/** Parse a search string into a structured artist query, or `null` for plain text. */
export function parseArtistQuery(rawQuery: string): ArtistQuery | null {
  const query = rawQuery.trim();

  const tagMatch = query.match(TAG_RE);
  if (tagMatch) {
    return { kind: "tag", tag: stripQuotes(tagMatch[1]).toLowerCase() };
  }

  const countryMatch = query.match(COUNTRY_RE);
  if (countryMatch) {
    const code = stripQuotes(countryMatch[2]);
    if (/^[a-z]{2}$/i.test(code)) {
      return { kind: "country", code: code.toUpperCase(), negate: countryMatch[1] === "!=" };
    }
  }

  return null;
}

/**
 * Whether an artist satisfies a structured query. An artist with no profile or
 * no country is outside every country, so `!=` includes them. An empty tag
 * matches everything, as the plain `tag:` search always has.
 */
export function artistMatchesQuery(
  query: ArtistQuery,
  profile: { tags?: string[]; country?: string | null } | undefined,
): boolean {
  if (query.kind === "tag") {
    if (!query.tag) return true;
    return profile?.tags?.some((t) => t.toLowerCase().includes(query.tag)) ?? false;
  }
  const inCountry = profile?.country?.toUpperCase() === query.code;
  return query.negate ? !inCountry : inCountry;
}
