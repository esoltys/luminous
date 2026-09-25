//! Grammar and parser for advanced-search / smart-playlist query strings,
//! consumed by `collection.rs`'s song search.
//!
//! A query is whitespace-separated tokens (quote with `"`/`'` to include a
//! space in a term). Each token is either a bare full-text term, or a
//! `field:value` filter where `field` is a known song attribute (see the
//! match in `parse_field_filter` for the full alias list, e.g.
//! `plays`/`play_count` both mean `playcount`). `value` may be prefixed with
//! a comparator (`>`, `>=`, `<`, `<=`, `!=`, `=`); if omitted, the default is
//! `=` for numeric fields and a substring `LIKE` match for text fields.
//! Ranges aren't a single-token construct — `bpm:>=120 bpm:<=130` is two
//! filters ANDed together, same as any two field filters. Unrecognized
//! fields are silently dropped (not an error) rather than falling back to a
//! bare-term match.

use rusqlite::types::{ToSqlOutput, Value as SqlValue};
use rusqlite::ToSql;

/// SQL comparison operator for a field filter. `Contains` always renders as
/// `LIKE` — the caller is responsible for wrapping the value in `%...%`
/// (see `parse_field_filter`), this only emits the SQL keyword.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
}

impl Op {
    pub fn to_sql(&self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Neq => "!=",
            Op::Gt => ">",
            Op::Gte => ">=",
            Op::Lt => "<",
            Op::Lte => "<=",
            Op::Contains => "LIKE",
        }
    }
}

/// A parsed filter's right-hand-side value, already coerced to the SQL type
/// its column expects (see `ToSql` impl below) — `Text` values destined for
/// a `Contains` filter already carry the `%...%` wrapping.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Text(String),
    Int(i64),
    Float(f64),
}

impl ToSql for FilterValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            FilterValue::Text(s) => Ok(ToSqlOutput::Owned(SqlValue::Text(s.clone()))),
            FilterValue::Int(i) => Ok(ToSqlOutput::Owned(SqlValue::Integer(*i))),
            FilterValue::Float(f) => Ok(ToSqlOutput::Owned(SqlValue::Real(*f))),
        }
    }
}

/// One `field:value` filter, ready to splice into a SQL `WHERE` clause as
/// `{sql_column} {op.to_sql()} ?` with `value` bound as the parameter.
/// `field` is the user-facing alias as typed (kept for round-tripping in
/// smart-playlist rule UIs); `sql_column` is the resolved real column name.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldFilter {
    pub field: String,
    pub sql_column: &'static str,
    pub is_numeric: bool,
    pub op: Op,
    pub value: FilterValue,
}

impl FieldFilter {
    pub fn to_sql_clause(&self, param_idx: usize) -> String {
        if self.sql_column == "artist_tag" {
            format!(
                "COALESCE(NULLIF(album_artist, ''), artist) IN (SELECT artist_key FROM artist_profiles, json_each(artist_profiles.tags) WHERE json_each.value {} ?{})",
                self.op.to_sql(),
                param_idx
            )
        } else if self.sql_column == "path" {
            match self.op {
                Op::Eq => format!(
                    "(REPLACE(path, '\\', '/') = ?{param_idx} OR REPLACE(path, '\\', '/') LIKE ?{param_idx} || '/%')"
                ),
                Op::Neq => format!(
                    "NOT (REPLACE(path, '\\', '/') = ?{param_idx} OR REPLACE(path, '\\', '/') LIKE ?{param_idx} || '/%')"
                ),
                Op::Contains => format!("REPLACE(path, '\\', '/') LIKE ?{param_idx}"),
                _ => format!("REPLACE(path, '\\', '/') {} ?{param_idx}", self.op.to_sql()),
            }
        } else {
            format!("{} {} ?{}", self.sql_column, self.op.to_sql(), param_idx)
        }
    }
}

/// A query split into its two independent match modes: `bare_terms` (ANDed
/// full-text substring matches against title/artist/album) and
/// `field_filters` (ANDed structured comparisons). Both lists apply
/// together — there's no OR between terms or filters.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedQuery {
    pub bare_terms: Vec<String>,
    pub field_filters: Vec<FieldFilter>,
}

/// Parse a raw search-box or smart-playlist-rule string per the grammar
/// documented at the top of this module. Never fails — tokens that don't
/// parse as a recognized `field:value` filter are treated as bare terms
/// instead (quotes stripped), so malformed input degrades to a plain-text
/// search rather than being rejected.
pub fn parse_query(raw_query: &str) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let tokens = tokenize(raw_query);

    for token in tokens {
        if let Some(filter) = parse_field_filter(&token) {
            parsed.field_filters.push(filter);
        } else {
            let clean = token.trim_matches('"').trim_matches('\'').trim();
            if !clean.is_empty() {
                parsed.bare_terms.push(clean.to_string());
            }
        }
    }

    parsed
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';
    let mut quote_count = 0;
    let mut escaped = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && !escaped {
            escaped = true;
            current.push(ch);
            i += 1;
            continue;
        }

        if (ch == '"' || ch == '\'') && !escaped {
            if in_quotes && ch == quote_char {
                let mut count = 0;
                while i + count < chars.len() && chars[i + count] == quote_char {
                    count += 1;
                }
                if count >= quote_count {
                    for _ in 0..count {
                        current.push(quote_char);
                    }
                    i += count;
                    in_quotes = false;
                    quote_char = ' ';
                    quote_count = 0;
                    escaped = false;
                    continue;
                } else {
                    current.push(ch);
                    i += 1;
                    escaped = false;
                    continue;
                }
            } else if !in_quotes {
                let mut count = 0;
                while i + count < chars.len() && chars[i + count] == ch {
                    count += 1;
                }
                let next_char = if i + count < chars.len() {
                    Some(chars[i + count])
                } else {
                    None
                };
                if count == 2 && next_char.is_none_or(|nc| nc.is_whitespace() || nc == ';') {
                    current.push(ch);
                    current.push(ch);
                    i += 2;
                    escaped = false;
                    continue;
                }
                in_quotes = true;
                quote_char = ch;
                quote_count = count;
                for _ in 0..count {
                    current.push(ch);
                }
                i += count;
                escaped = false;
                continue;
            }
        }

        if ch.is_whitespace() && !in_quotes {
            if !current.trim().is_empty() {
                tokens.push(current.trim().to_string());
                current.clear();
            }
        } else {
            current.push(ch);
        }
        escaped = false;
        i += 1;
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }

    tokens
}

fn strip_enclosing_quotes(mut s: &str) -> &str {
    loop {
        let trimmed = s.trim();
        if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
        {
            s = &trimmed[1..trimmed.len() - 1];
        } else {
            return trimmed;
        }
    }
}

fn parse_op_and_raw_value(raw: &str, is_numeric: bool) -> (Op, String) {
    let raw_trimmed = strip_enclosing_quotes(raw);
    let (op, after_op) = if let Some(rest) = raw_trimmed.strip_prefix(">=") {
        (Op::Gte, rest)
    } else if let Some(rest) = raw_trimmed.strip_prefix("<=") {
        (Op::Lte, rest)
    } else if let Some(rest) = raw_trimmed.strip_prefix("!=") {
        (Op::Neq, rest)
    } else if let Some(rest) = raw_trimmed.strip_prefix('>') {
        (Op::Gt, rest)
    } else if let Some(rest) = raw_trimmed.strip_prefix('<') {
        (Op::Lt, rest)
    } else if let Some(rest) = raw_trimmed.strip_prefix('=') {
        (Op::Eq, rest)
    } else if is_numeric {
        (Op::Eq, raw_trimmed)
    } else {
        (Op::Contains, raw_trimmed)
    };

    let inner = strip_enclosing_quotes(after_op);
    let unescaped = inner
        .replace("\\\"", "\"")
        .replace("\\'", "'")
        .replace("\\\\", "\\");
    let clean = strip_enclosing_quotes(&unescaped);
    (op, clean.to_string())
}

fn parse_field_filter(token: &str) -> Option<FieldFilter> {
    let colon_idx = token.find(':')?;
    let (field_part, val_part) = token.split_at(colon_idx);
    let field_clean = field_part.trim().to_lowercase();
    if field_clean.is_empty() {
        return None;
    }

    let (sql_column, is_numeric) = match field_clean.as_str() {
        "artist" => ("COALESCE(NULLIF(album_artist, ''), artist)", false),
        "album_artist" => ("album_artist", false),
        "album" => ("album", false),
        "title" => ("title", false),
        "genre" => ("genre", false),
        "composer" => ("composer", false),
        "year" => ("year", true),
        "originalyear" | "original_year" => ("originalyear", true),
        "bitrate" => ("bitrate", true),
        "track" | "track_number" => ("track", true),
        "disc" | "disc_number" => ("disc", true),
        "rating" | "stars" => ("rating", true),
        "playcount" | "plays" | "play_count" => ("playcount", true),
        "skipcount" | "skips" | "skip_count" => ("skipcount", true),
        "lastplayed" | "last_played" => ("lastplayed", true),
        "added" => ("added", true),
        "duration" | "length" => ("length_nanosec", true),
        "bpm" => ("bpm", true),
        "key" | "initial_key" => ("initial_key", false),
        "samplerate" | "sample_rate" => ("samplerate", true),
        "bitdepth" | "bit_depth" => ("bitdepth", true),
        "channels" => ("channels", true),
        "compilation" => ("compilation", true),
        "artist_tag" | "artist-tag" | "artisttag" | "artist_tags" | "tag" | "tags" => {
            ("artist_tag", false)
        }
        "lyrics" | "lyric" => ("lyrics", false),
        "folder" | "subfolder" | "directory" | "path" => ("path", false),
        _ => return None,
    };

    let (op, raw_val) = parse_op_and_raw_value(val_part[1..].trim(), is_numeric);
    if raw_val.is_empty() {
        return None;
    }

    let value = if sql_column == "path" {
        let clean_path = strip_enclosing_quotes(&raw_val);
        let mut normalized_val = clean_path.replace('\\', "/");
        if normalized_val.len() > 1 && normalized_val.ends_with('/') {
            normalized_val.pop();
        }
        if op == Op::Contains {
            FilterValue::Text(format!("%{normalized_val}%"))
        } else {
            FilterValue::Text(normalized_val)
        }
    } else if sql_column == "length_nanosec" {
        FilterValue::Int(parse_duration_ns(&raw_val)?)
    } else if is_numeric {
        if let Ok(i) = raw_val.parse::<i64>() {
            FilterValue::Int(i)
        } else if let Ok(f) = raw_val.parse::<f64>() {
            FilterValue::Float(f)
        } else {
            return None;
        }
    } else if op == Op::Contains {
        FilterValue::Text(format!("%{raw_val}%"))
    } else {
        FilterValue::Text(raw_val)
    };

    Some(FieldFilter {
        field: field_clean,
        sql_column,
        is_numeric,
        op,
        value,
    })
}

/// Parse a duration filter value as nanoseconds, matching `songs.length_nanosec`'s
/// unit. Accepts a bare integer of seconds, or `MM:SS`/`H:MM:SS` — the same
/// shorthand a user would type for a track length. Returns `None` on
/// anything else, which `parse_field_filter` propagates up so the whole
/// token falls back to a bare-term match instead of a duration filter.
pub fn parse_duration_ns(s: &str) -> Option<i64> {
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            let mins: i64 = parts[0].parse().ok()?;
            let secs: i64 = parts[1].parse().ok()?;
            Some((mins * 60 + secs) * 1_000_000_000)
        } else if parts.len() == 3 {
            let hours: i64 = parts[0].parse().ok()?;
            let mins: i64 = parts[1].parse().ok()?;
            let secs: i64 = parts[2].parse().ok()?;
            Some((hours * 3600 + mins * 60 + secs) * 1_000_000_000)
        } else {
            None
        }
    } else {
        let secs: i64 = s.parse().ok()?;
        Some(secs * 1_000_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query() {
        let q = parse_query("rating:>=4 year:<2000 genre:jazz \"miles davis\"");
        assert_eq!(q.bare_terms, vec!["miles davis"]);
        assert_eq!(q.field_filters.len(), 3);

        assert_eq!(q.field_filters[0].field, "rating");
        assert_eq!(q.field_filters[0].op, Op::Gte);
        assert_eq!(q.field_filters[0].value, FilterValue::Int(4));

        assert_eq!(q.field_filters[1].field, "year");
        assert_eq!(q.field_filters[1].op, Op::Lt);
        assert_eq!(q.field_filters[1].value, FilterValue::Int(2000));

        assert_eq!(q.field_filters[2].field, "genre");
        assert_eq!(q.field_filters[2].op, Op::Contains);
        assert_eq!(
            q.field_filters[2].value,
            FilterValue::Text("%jazz%".to_string())
        );
    }

    #[test]
    fn test_parse_extended_tag_fields() {
        // Ranges aren't a single-token construct — the Smart Playlist builder
        // composes them as two separate >=/<= filters on the same field.
        let q = parse_query("bpm:>=120 bpm:<=130 key:Am album_artist:Various compilation:1");
        assert_eq!(q.field_filters.len(), 5);
        assert_eq!(q.field_filters[0].sql_column, "bpm");
        assert_eq!(q.field_filters[0].op, Op::Gte);
        assert_eq!(q.field_filters[0].value, FilterValue::Int(120));
        assert_eq!(q.field_filters[1].sql_column, "bpm");
        assert_eq!(q.field_filters[1].op, Op::Lte);

        assert_eq!(q.field_filters[2].field, "key");
        assert_eq!(q.field_filters[2].sql_column, "initial_key");
        assert_eq!(q.field_filters[2].op, Op::Contains);
        assert_eq!(
            q.field_filters[2].value,
            FilterValue::Text("%Am%".to_string())
        );

        assert_eq!(q.field_filters[3].field, "album_artist");
        assert_eq!(q.field_filters[3].sql_column, "album_artist");

        assert_eq!(q.field_filters[4].sql_column, "compilation");
        assert_eq!(q.field_filters[4].value, FilterValue::Int(1));
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration_ns("3:45"), Some(225_000_000_000));
        assert_eq!(parse_duration_ns("1:02:03"), Some(3_723_000_000_000));
        assert_eq!(parse_duration_ns("180"), Some(180_000_000_000));
    }

    #[test]
    fn test_parse_lyrics_filter() {
        // Both aliases should resolve to the `lyrics` column via LIKE (Contains).
        let q = parse_query("lyrics:hello");
        assert_eq!(q.bare_terms, Vec::<String>::new());
        assert_eq!(q.field_filters.len(), 1);
        assert_eq!(q.field_filters[0].field, "lyrics");
        assert_eq!(q.field_filters[0].sql_column, "lyrics");
        assert_eq!(q.field_filters[0].op, Op::Contains);
        assert_eq!(
            q.field_filters[0].value,
            FilterValue::Text("%hello%".to_string())
        );
        assert_eq!(q.field_filters[0].to_sql_clause(1), "lyrics LIKE ?1");

        // Singular alias
        let q2 = parse_query("lyric:world");
        assert_eq!(q2.field_filters.len(), 1);
        assert_eq!(q2.field_filters[0].sql_column, "lyrics");

        // Lyrics: filter must NOT bleed into bare-term FTS search
        let q3 = parse_query("lyrics:heart blue");
        assert_eq!(q3.bare_terms, vec!["blue"]);
        assert_eq!(q3.field_filters.len(), 1);
        assert_eq!(q3.field_filters[0].sql_column, "lyrics");
    }

    #[test]
    fn test_parse_artist_tag_filter() {
        let q = parse_query("artist-tag:canadian tag:country");
        assert_eq!(q.field_filters.len(), 2);
        assert_eq!(q.field_filters[0].field, "artist-tag");
        assert_eq!(q.field_filters[0].sql_column, "artist_tag");
        assert_eq!(q.field_filters[0].op, Op::Contains);
        assert_eq!(
            q.field_filters[0].value,
            FilterValue::Text("%canadian%".to_string())
        );
        assert_eq!(
            q.field_filters[0].to_sql_clause(1),
            "COALESCE(NULLIF(album_artist, ''), artist) IN (SELECT artist_key FROM artist_profiles, json_each(artist_profiles.tags) WHERE json_each.value LIKE ?1)"
        );

        assert_eq!(q.field_filters[1].field, "tag");
        assert_eq!(q.field_filters[1].sql_column, "artist_tag");
    }

    #[test]
    fn test_parse_folder_filter() {
        let q = parse_query("folder:\"Radio Downloads\"");
        assert_eq!(q.field_filters.len(), 1);
        assert_eq!(q.field_filters[0].field, "folder");
        assert_eq!(q.field_filters[0].sql_column, "path");
        assert_eq!(q.field_filters[0].op, Op::Contains);
        assert_eq!(
            q.field_filters[0].value,
            FilterValue::Text("%Radio Downloads%".to_string())
        );
        assert_eq!(
            q.field_filters[0].to_sql_clause(1),
            "REPLACE(path, '\\', '/') LIKE ?1"
        );

        // Windows path slashes normalized in filter value
        let q2 = parse_query("path:\"Music\\Radio Downloads\"");
        assert_eq!(q2.field_filters.len(), 1);
        assert_eq!(
            q2.field_filters[0].value,
            FilterValue::Text("%Music/Radio Downloads%".to_string())
        );

        // Folder path with spaces and internal quotes (e.g. station name in path)
        let q3 = parse_query("folder:\"/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\"");
        assert_eq!(q3.field_filters.len(), 1);
        assert_eq!(
            q3.field_filters[0].value,
            FilterValue::Text("%/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC%".to_string())
        );

        // Exact folder match with equals operator
        let q4 = parse_query("folder:=\"/home/esoltys/Music/Shortwave/SomaFM PopTron\"");
        assert_eq!(q4.field_filters.len(), 1);
        assert_eq!(q4.field_filters[0].op, Op::Eq);
        assert_eq!(
            q4.field_filters[0].value,
            FilterValue::Text("/home/esoltys/Music/Shortwave/SomaFM PopTron".to_string())
        );
        assert_eq!(
            q4.field_filters[0].to_sql_clause(1),
            "(REPLACE(path, '\\', '/') = ?1 OR REPLACE(path, '\\', '/') LIKE ?1 || '/%')"
        );

        // Double-quoted path from file picker
        let q5 = parse_query("folder:=\"\"/home/esoltys/Music/Shortwave/SomaFM PopTron\"\"");
        assert_eq!(q5.field_filters.len(), 1);
        assert_eq!(q5.field_filters[0].op, Op::Eq);
        assert_eq!(
            q5.field_filters[0].value,
            FilterValue::Text("/home/esoltys/Music/Shortwave/SomaFM PopTron".to_string())
        );

        // Double-quoted path with contains
        let q6 = parse_query("folder:\"\"/home/esoltys/Music/Shortwave/SomaFM PopTron\"\"");
        assert_eq!(q6.field_filters.len(), 1);
        assert_eq!(q6.field_filters[0].op, Op::Contains);
        assert_eq!(
            q6.field_filters[0].value,
            FilterValue::Text("%/home/esoltys/Music/Shortwave/SomaFM PopTron%".to_string())
        );

        // Folder equals with internal quotes in path
        let q7 = parse_query("folder:=\"/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\"");
        assert_eq!(q7.field_filters.len(), 1);
        assert_eq!(q7.field_filters[0].op, Op::Eq);
        assert_eq!(
            q7.field_filters[0].value,
            FilterValue::Text("/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC".to_string())
        );

        // Windows path with equals operator and internal quotes
        let q8 = parse_query("path:=\"C:\\Music\\Shortwave\\CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\"");
        assert_eq!(q8.field_filters.len(), 1);
        assert_eq!(q8.field_filters[0].op, Op::Eq);
        assert_eq!(
            q8.field_filters[0].value,
            FilterValue::Text("C:/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC".to_string())
        );

        // Not equals folder filter
        let q9 = parse_query("folder:!=\"/home/esoltys/Music/Shortwave/SomaFM PopTron\"");
        assert_eq!(q9.field_filters.len(), 1);
        assert_eq!(q9.field_filters[0].op, Op::Neq);
        assert_eq!(
            q9.field_filters[0].to_sql_clause(1),
            "NOT (REPLACE(path, '\\', '/') = ?1 OR REPLACE(path, '\\', '/') LIKE ?1 || '/%')"
        );

        // Accumulated triple-quoted path recovery
        let q10 = parse_query("folder:=\"\\\"\\\"\\\"/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\\\"\\\"\\\"\"");
        assert_eq!(q10.field_filters.len(), 1);
        assert_eq!(q10.field_filters[0].op, Op::Eq);
        assert_eq!(
            q10.field_filters[0].value,
            FilterValue::Text("/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC".to_string())
        );
    }
}
