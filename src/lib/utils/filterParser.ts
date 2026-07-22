export interface Rule {
  field: string;
  op: string;
  value: string;
}

export function parseSearchRules(query: string): Rule[] {
  const rules: Rule[] = [];
  if (!query.trim()) return rules;

  const tokens: string[] = [];
  let current = "";
  let inQuotes = false;
  let quoteChar = "";

  for (const ch of query) {
    if (ch === '"' || ch === "'") {
      if (inQuotes && ch === quoteChar) {
        inQuotes = false;
      } else if (!inQuotes) {
        inQuotes = true;
        quoteChar = ch;
      }
      current += ch;
    } else if (/\s/.test(ch) && !inQuotes) {
      if (current.trim()) {
        tokens.push(current.trim());
        current = "";
      }
    } else {
      current += ch;
    }
  }
  if (current.trim()) {
    tokens.push(current.trim());
  }

  for (const token of tokens) {
    const colonIdx = token.indexOf(":");
    if (colonIdx > 0) {
      const field = token.slice(0, colonIdx).trim().toLowerCase();
      let rawVal = token.slice(colonIdx + 1).replace(/^['"]|['"]$/g, "").trim();
      let op = "=";

      if (rawVal.startsWith(">=")) {
        op = ">=";
        rawVal = rawVal.slice(2);
      } else if (rawVal.startsWith("<=")) {
        op = "<=";
        rawVal = rawVal.slice(2);
      } else if (rawVal.startsWith("!=")) {
        op = "!=";
        rawVal = rawVal.slice(2);
      } else if (rawVal.startsWith(">")) {
        op = ">";
        rawVal = rawVal.slice(1);
      } else if (rawVal.startsWith("<")) {
        op = "<";
        rawVal = rawVal.slice(1);
      } else if (rawVal.startsWith("=")) {
        op = "=";
        rawVal = rawVal.slice(1);
      } else if (["artist", "album", "title", "genre", "composer"].includes(field)) {
        op = "contains";
      }

      if (field && rawVal) {
        rules.push({ field, op, value: rawVal });
      }
    }
  }

  return rules;
}

export function hasAdvancedSearchTerms(query: string): boolean {
  return parseSearchRules(query).length > 0;
}
