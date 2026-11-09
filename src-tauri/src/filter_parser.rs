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
//! A numeric or date value may also be a range, `bpm:120..130`, either end
//! optional (`added:-30d..`); it expands to the two ANDed filters
//! `bpm:>=120 bpm:<=130`, so an inverted range simply matches nothing.
//! `bitrate` and `samplerate` take unit suffixes: `bitrate:>=320k` (the
//! column is kbps, and a bare value of 10000 or more is read as bps),
//! `samplerate:96k` (96000 Hz). `format:flac` matches a container family
//! (see [`FileType::from_token`]) and `lossless:true|false` its lossless-ness.
//! Unrecognized
//! fields are silently dropped (not an error) rather than falling back to a
//! bare-term match.
//!
//! `added` and `lastplayed` also take calendar dates: an absolute day
//! (`2026-01-31`) or a relative offset back from today (`-30d`; units `d`
//! days, `w` weeks, `m` months, `y` years). Both name a *day* in the local
//! time zone, and the comparators read as a person would say them: `>=` from
//! the start of that day, `>` after the end of it, `<` before it, `<=` up to
//! the end of it, `=` anywhere within it. Relative offsets resolve against
//! the clock when the query is parsed, so `added:>=-30d` always means "the
//! last 30 days". A song with no `lastplayed` counts as older than any date.
//! A bare integer is still an epoch-seconds timestamp, compared as-is.

use chrono::{DateTime, Duration, Local, Months, NaiveDate, TimeZone};
use rusqlite::types::{ToSqlOutput, Value as SqlValue};
use rusqlite::ToSql;

use crate::models::FileType;

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
    /// Epoch seconds `[start, end)` covering one local calendar day, for
    /// `=` / `!=` on a date column. Binds as two parameters, not one — go
    /// through [`FieldFilter::push_clause`] rather than binding the value.
    DayRange {
        start: i64,
        end: i64,
    },
    /// A set of integers for `IN (...)`, e.g. the `filetype` codes of a
    /// format family. Binds one parameter per member; go through
    /// [`FieldFilter::push_clause`].
    IntSet(Vec<i64>),
}

impl ToSql for FilterValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            FilterValue::Text(s) => Ok(ToSqlOutput::Owned(SqlValue::Text(s.clone()))),
            FilterValue::Int(i) => Ok(ToSqlOutput::Owned(SqlValue::Integer(*i))),
            FilterValue::Float(f) => Ok(ToSqlOutput::Owned(SqlValue::Real(*f))),
            FilterValue::DayRange { .. } | FilterValue::IntSet(_) => {
                Err(rusqlite::Error::ToSqlConversionFailure(
                    "a multi-parameter value binds through FieldFilter::push_clause".into(),
                ))
            }
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

/// Columns holding epoch-second dates, which take calendar-date values.
fn is_date_column(sql_column: &str) -> bool {
    matches!(sql_column, "lastplayed" | "added")
}

impl FieldFilter {
    /// Appends this filter's value(s) to `params` and returns the `WHERE`
    /// clause referring to them. The one way callers should splice a filter
    /// into a query: how many parameters a filter needs is its own business.
    pub fn push_clause(self, params: &mut Vec<Box<dyn ToSql>>) -> String {
        let clause = self.to_sql_clause(params.len() + 1);
        match self.value {
            FilterValue::DayRange { start, end } => {
                params.push(Box::new(start));
                params.push(Box::new(end));
            }
            FilterValue::IntSet(members) => {
                for member in members {
                    params.push(Box::new(member));
                }
            }
            value => params.push(Box::new(value)),
        }
        clause
    }

    /// The clause for this filter with its first parameter at `?{param_idx}`
    /// (a [`FilterValue::DayRange`] takes `param_idx` and `param_idx + 1`).
    pub fn to_sql_clause(&self, param_idx: usize) -> String {
        if let FilterValue::IntSet(members) = &self.value {
            let placeholders: Vec<String> = (0..members.len())
                .map(|i| format!("?{}", param_idx + i))
                .collect();
            let not = if self.op == Op::Neq { "NOT " } else { "" };
            return format!("{} {not}IN ({})", self.sql_column, placeholders.join(", "));
        }
        if let FilterValue::DayRange { .. } = self.value {
            let col = self.sql_column;
            let inside = format!("{col} >= ?{param_idx} AND {col} < ?{}", param_idx + 1);
            return if self.op == Op::Neq {
                // A never-set date is outside every day. NOT NULL is NULL,
                // so default that case to "matches".
                format!("COALESCE(NOT ({inside}), 1)")
            } else {
                format!("({inside})")
            };
        }
        if is_date_column(self.sql_column) && matches!(self.op, Op::Lt | Op::Lte | Op::Neq) {
            // A never-played song is older than any date.
            return format!(
                "({col} IS NULL OR {col} {op} ?{param_idx})",
                col = self.sql_column,
                op = self.op.to_sql()
            );
        }
        if self.sql_column == "artist_tag" {
            format!(
                "COALESCE(NULLIF(album_artist, ''), artist) IN (SELECT artist_key FROM artist_profiles, json_each(artist_profiles.tags) WHERE json_each.value {} ?{})",
                self.op.to_sql(),
                param_idx
            )
        } else if self.sql_column == "language" {
            match self.op {
                Op::Eq => format!(
                    "(language = ?{param_idx} OR language LIKE ?{param_idx} || '; %' OR language LIKE '%; ' || ?{param_idx} || '; %' OR language LIKE '%; ' || ?{param_idx})"
                ),
                Op::Neq => format!(
                    "NOT (language = ?{param_idx} OR language LIKE ?{param_idx} || '; %' OR language LIKE '%; ' || ?{param_idx} || '; %' OR language LIKE '%; ' || ?{param_idx})"
                ),
                Op::Contains => format!("language LIKE ?{param_idx}"),
                _ => format!("language {} ?{param_idx}", self.op.to_sql()),
            }
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
    parse_query_at(raw_query, Local::now())
}

/// [`parse_query`] with the clock injected, which relative date values
/// (`-30d`) resolve against.
pub fn parse_query_at(raw_query: &str, now: DateTime<Local>) -> ParsedQuery {
    let mut parsed = ParsedQuery::default();
    let tokens = tokenize(raw_query);

    for token in tokens {
        if let Some(bounds) = expand_range(&token) {
            let filters: Vec<FieldFilter> = bounds
                .iter()
                .filter_map(|bound| parse_field_filter(bound, now))
                .collect();
            if filters.len() == bounds.len() {
                parsed.field_filters.extend(filters);
                continue;
            }
        }
        if let Some(filter) = parse_field_filter(&token, now) {
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

/// Map a user-facing field alias to its SQL column and whether it compares
/// numerically; `None` for anything that isn't a filterable field.
fn resolve_field(field: &str) -> Option<(&'static str, bool)> {
    Some(match field {
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
        "language" | "lang" => ("language", false),
        "lyrics" | "lyric" => ("lyrics", false),
        "folder" | "subfolder" | "directory" | "path" => ("path", false),
        "format" | "codec" => ("filetype", false),
        "lossless" => ("filetype", false),
        _ => return None,
    })
}

/// Split a `field:a..b` range token on a numeric or date field into
/// `field:>=a` and `field:<=b` (an empty end is dropped). `None` if the
/// token isn't a range.
fn expand_range(token: &str) -> Option<Vec<String>> {
    let (field_part, val_part) = token.split_at(token.find(':')?);
    let field = field_part.trim().to_lowercase();
    let (column, is_numeric) = resolve_field(&field)?;
    if !is_numeric && !is_date_column(column) {
        return None;
    }
    let val = strip_enclosing_quotes(&val_part[1..]);
    if val.starts_with(['>', '<', '=', '!']) {
        return None;
    }
    let (lo, hi) = val.split_once("..")?;
    let (lo, hi) = (lo.trim(), hi.trim());
    let mut bounds = Vec::new();
    if !lo.is_empty() {
        bounds.push(format!("{field}:>={lo}"));
    }
    if !hi.is_empty() {
        bounds.push(format!("{field}:<={hi}"));
    }
    (!bounds.is_empty()).then_some(bounds)
}

/// Read a `bitrate`/`samplerate` value with an optional unit. `bitrate` is
/// stored in kbps (`320`, `320k`, `320 kbps`; a bare value of 10000 or more
/// is taken as bps), `samplerate` in Hz (`96000`, `96k`, `44.1khz`, `48000hz`).
fn parse_quantity(raw: &str, sql_column: &str) -> Option<i64> {
    let lower = raw.trim().to_lowercase();
    let (number, kilo) = if let Some(n) = lower.strip_suffix("kbps").or(lower.strip_suffix("khz")) {
        (n, true)
    } else if let Some(n) = lower.strip_suffix("hz") {
        (n, false)
    } else if let Some(n) = lower.strip_suffix('k') {
        (n, true)
    } else {
        (lower.as_str(), false)
    };
    let value: f64 = number.trim().parse().ok()?;
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let scaled = match (sql_column, kilo) {
        ("samplerate", true) => value * 1000.0,
        ("bitrate", false) if value >= 10_000.0 => value / 1000.0,
        _ => value,
    };
    Some(scaled.round() as i64)
}

/// Resolve a `format:` / `lossless:` value to an `IN` / `NOT IN` over the
/// `filetype` codes it selects (`lossless:false` is the lossless set negated).
fn parse_filetype_filter(field: &str, raw: &str, op: &Op) -> Option<(Op, FilterValue)> {
    if !matches!(op, Op::Eq | Op::Neq | Op::Contains) {
        return None;
    }
    let (types, mut negated) = if field == "lossless" {
        let wanted = match raw.to_lowercase().as_str() {
            "true" | "yes" | "1" => true,
            "false" | "no" | "0" => false,
            _ => return None,
        };
        (FileType::lossless_types(), !wanted)
    } else {
        (FileType::from_token(raw), false)
    };
    if types.is_empty() {
        return None;
    }
    if *op == Op::Neq {
        negated = !negated;
    }
    let codes = types.into_iter().map(|t| t as i64).collect();
    let op = if negated { Op::Neq } else { Op::Eq };
    Some((op, FilterValue::IntSet(codes)))
}

fn parse_field_filter(token: &str, now: DateTime<Local>) -> Option<FieldFilter> {
    let colon_idx = token.find(':')?;
    let (field_part, val_part) = token.split_at(colon_idx);
    let field_clean = field_part.trim().to_lowercase();
    if field_clean.is_empty() {
        return None;
    }

    let (sql_column, is_numeric) = resolve_field(&field_clean)?;

    let (op, raw_val) = parse_op_and_raw_value(val_part[1..].trim(), is_numeric);
    if raw_val.is_empty() {
        return None;
    }

    if sql_column == "filetype" {
        let (op, value) = parse_filetype_filter(&field_clean, &raw_val, &op)?;
        return Some(FieldFilter {
            field: field_clean,
            sql_column,
            is_numeric,
            op,
            value,
        });
    }

    if is_date_column(sql_column) {
        if let Some((op, value)) =
            parse_day(&raw_val, now.date_naive()).and_then(|day| day_comparison(&op, day))
        {
            return Some(FieldFilter {
                field: field_clean,
                sql_column,
                is_numeric,
                op,
                value,
            });
        }
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
    } else if matches!(sql_column, "bitrate" | "samplerate") {
        FilterValue::Int(parse_quantity(&raw_val, sql_column)?)
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

/// Midnight starting `day` in the local time zone, as epoch seconds. A zone
/// whose clocks skip midnight (a DST gap) starts the day at its first
/// existing hour.
fn local_day_start(day: NaiveDate) -> Option<i64> {
    (0..4).find_map(|hour| {
        let naive = day.and_hms_opt(hour, 0, 0)?;
        Local
            .from_local_datetime(&naive)
            .earliest()
            .map(|t| t.timestamp())
    })
}

/// Read a date value: `YYYY-MM-DD`, or `-<n><unit>` counted back from
/// `today` (`d` days, `w` weeks, `m` months, `y` years). `None` for anything
/// else, including bare integers, which stay epoch timestamps.
fn parse_day(raw: &str, today: NaiveDate) -> Option<NaiveDate> {
    if let Ok(day) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Some(day);
    }
    let rest = raw.strip_prefix('-')?;
    let unit = rest.chars().last()?;
    let count: u32 = rest[..rest.len() - unit.len_utf8()].parse().ok()?;
    match unit.to_ascii_lowercase() {
        'd' => today.checked_sub_signed(Duration::days(i64::from(count))),
        'w' => today.checked_sub_signed(Duration::days(i64::from(count) * 7)),
        'm' => today.checked_sub_months(Months::new(count)),
        'y' => today.checked_sub_months(Months::new(count.checked_mul(12)?)),
        _ => None,
    }
}

/// Translate "`op` this calendar day" into a comparison against epoch
/// seconds: `>=` / `<` pin to the day's start, `>` / `<=` to the start of
/// the next day, `=` / `!=` to the whole day.
fn day_comparison(op: &Op, day: NaiveDate) -> Option<(Op, FilterValue)> {
    let start = local_day_start(day)?;
    let end = local_day_start(day.succ_opt()?)?;
    Some(match op {
        Op::Gte => (Op::Gte, FilterValue::Int(start)),
        Op::Lt => (Op::Lt, FilterValue::Int(start)),
        Op::Gt => (Op::Gte, FilterValue::Int(end)),
        Op::Lte => (Op::Lt, FilterValue::Int(end)),
        Op::Neq => (Op::Neq, FilterValue::DayRange { start, end }),
        Op::Eq | Op::Contains => (Op::Eq, FilterValue::DayRange { start, end }),
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
        let q3 = parse_query(
            "folder:\"/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\"",
        );
        assert_eq!(q3.field_filters.len(), 1);
        assert_eq!(
            q3.field_filters[0].value,
            FilterValue::Text(
                "%/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC%"
                    .to_string()
            )
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
            FilterValue::Text(
                "/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC"
                    .to_string()
            )
        );

        // Windows path with equals operator and internal quotes
        let q8 = parse_query(
            "path:=\"C:\\Music\\Shortwave\\CKLZ-FM 104.7 \\\"The Lizard\\\" Kelowna, BC\"",
        );
        assert_eq!(q8.field_filters.len(), 1);
        assert_eq!(q8.field_filters[0].op, Op::Eq);
        assert_eq!(
            q8.field_filters[0].value,
            FilterValue::Text(
                "C:/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC".to_string()
            )
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
            FilterValue::Text(
                "/home/esoltys/Music/Shortwave/CKLZ-FM 104.7 \"The Lizard\" Kelowna, BC"
                    .to_string()
            )
        );
    }

    // --- date values for `added` / `lastplayed` ---

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, 30, 0)
            .earliest()
            .unwrap()
    }

    fn day_start(y: i32, m: u32, d: u32) -> i64 {
        local_day_start(NaiveDate::from_ymd_opt(y, m, d).unwrap()).unwrap()
    }

    fn only_filter(query: &str, now: DateTime<Local>) -> FieldFilter {
        let mut parsed = parse_query_at(query, now);
        assert!(
            parsed.bare_terms.is_empty(),
            "{query} fell back to bare terms"
        );
        assert_eq!(parsed.field_filters.len(), 1);
        parsed.field_filters.remove(0)
    }

    #[test]
    fn absolute_day_comparators_read_as_a_person_would() {
        let now = at(2026, 6, 15, 12);
        let (start, end) = (day_start(2026, 1, 31), day_start(2026, 2, 1));

        let f = only_filter("added:>=2026-01-31", now);
        assert_eq!((f.op, f.value), (Op::Gte, FilterValue::Int(start)));
        let f = only_filter("added:>2026-01-31", now);
        assert_eq!((f.op, f.value), (Op::Gte, FilterValue::Int(end)));
        let f = only_filter("added:<2026-01-31", now);
        assert_eq!((f.op, f.value), (Op::Lt, FilterValue::Int(start)));
        let f = only_filter("added:<=2026-01-31", now);
        assert_eq!((f.op, f.value), (Op::Lt, FilterValue::Int(end)));
        // Bare date means "on that day".
        let f = only_filter("lastplayed:2026-01-31", now);
        assert_eq!(
            (f.op, f.value),
            (Op::Eq, FilterValue::DayRange { start, end })
        );
        let f = only_filter("lastplayed:!=2026-01-31", now);
        assert_eq!(
            (f.op, f.value),
            (Op::Neq, FilterValue::DayRange { start, end })
        );
    }

    #[test]
    fn relative_offsets_resolve_against_the_clock_to_a_day() {
        let now = at(2026, 3, 31, 23);
        let cases = [
            ("-30d", (2026, 3, 1)),
            ("-2w", (2026, 3, 17)),
            ("-1m", (2026, 2, 28)), // month-end clamps
            ("-1y", (2025, 3, 31)),
            ("-0d", (2026, 3, 31)),
        ];
        for (offset, (y, m, d)) in cases {
            let f = only_filter(&format!("added:>={offset}"), now);
            assert_eq!(f.value, FilterValue::Int(day_start(y, m, d)), "{offset}");
        }
        // The hour of the day doesn't move the window.
        let early = only_filter("added:>=-7d", at(2026, 3, 31, 0));
        let late = only_filter("added:>=-7d", at(2026, 3, 31, 23));
        assert_eq!(early.value, late.value);
    }

    #[test]
    fn integers_and_junk_are_not_dates() {
        let now = at(2026, 6, 15, 12);
        let f = only_filter("added:>=1700000000", now);
        assert_eq!((f.op, f.value), (Op::Gte, FilterValue::Int(1_700_000_000)));
        let f = only_filter("added:>=-5", now);
        assert_eq!(f.value, FilterValue::Int(-5));
        // Not a real date / unknown unit: the token degrades to a bare term.
        for bad in ["added:2026-02-30", "added:-3x", "added:-d"] {
            let parsed = parse_query_at(bad, now);
            assert!(parsed.field_filters.is_empty(), "{bad}");
        }
        // Other numeric fields never read dates.
        assert!(parse_query_at("year:2026-01-01", now)
            .field_filters
            .is_empty());
    }

    #[test]
    fn date_filters_match_songs_through_sql() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE songs (id INTEGER, lastplayed INTEGER)", [])
            .unwrap();
        let now = at(2026, 6, 15, 12);
        let day = |y, m, d| day_start(y, m, d);
        // 1: never played, 2: long ago, 3: on 6/10 at noon, 4: today.
        for (id, lp) in [
            (1, None),
            (2, Some(day(2025, 1, 1) + 60)),
            (3, Some(day(2026, 6, 10) + 43_200)),
            (4, Some(day(2026, 6, 15) + 60)),
        ] {
            conn.execute(
                "INSERT INTO songs VALUES (?1, ?2)",
                rusqlite::params![id, lp],
            )
            .unwrap();
        }
        let ids = |query: &str| -> Vec<i64> {
            let mut params: Vec<Box<dyn ToSql>> = Vec::new();
            let clauses: Vec<String> = parse_query_at(query, now)
                .field_filters
                .into_iter()
                .map(|f| f.push_clause(&mut params))
                .collect();
            let sql = format!(
                "SELECT id FROM songs WHERE {} ORDER BY id",
                clauses.join(" AND ")
            );
            let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql).unwrap();
            let rows = stmt.query_map(refs.as_slice(), |r| r.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(ids("lastplayed:>=-7d"), vec![3, 4]); // the last 7 days
        assert_eq!(ids("lastplayed:<-7d"), vec![1, 2]); // not in the last 7 days, never-played included
        assert_eq!(ids("lastplayed:2026-06-10"), vec![3]);
        assert_eq!(ids("lastplayed:!=2026-06-10"), vec![1, 2, 4]);
        assert_eq!(ids("lastplayed:>2026-06-10"), vec![4]);
        assert_eq!(
            ids("lastplayed:>=2026-06-01 lastplayed:<=2026-06-10"),
            vec![3]
        );
    }

    fn technical_ids(query: &str) -> Vec<i64> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE songs (id INTEGER, filetype INTEGER, bitrate INTEGER, samplerate INTEGER, year INTEGER, added INTEGER)",
            [],
        )
        .unwrap();
        let now = at(2026, 6, 15, 12);
        let day = |y, m, d| day_start(y, m, d) + 60;
        // 1: MP3 320k, 2: FLAC 96k, 3: ALAC 44.1k, 4: Opus 128k, 5: WAV
        for (id, ft, br, sr, yr, added) in [
            (1, 1, 320, 44100, 1975, day(2026, 6, 10)),
            (2, 2, 2400, 96000, 1985, day(2025, 12, 1)),
            (3, 8, 900, 44100, 1979, day(2026, 1, 1)),
            (4, 5, 128, 48000, 2020, day(2026, 6, 14)),
            (5, 10, 1411, 44100, 1970, day(2024, 1, 1)),
        ] {
            conn.execute(
                "INSERT INTO songs VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![id, ft, br, sr, yr, added],
            )
            .unwrap();
        }
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        let clauses: Vec<String> = parse_query_at(query, now)
            .field_filters
            .into_iter()
            .map(|f| f.push_clause(&mut params))
            .collect();
        let sql = format!(
            "SELECT id FROM songs WHERE {} ORDER BY id",
            clauses.join(" AND ")
        );
        let refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows = stmt.query_map(refs.as_slice(), |r| r.get(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    }

    #[test]
    fn format_and_lossless_select_filetypes() {
        assert_eq!(technical_ids("format:flac"), vec![2]);
        assert_eq!(technical_ids("format:FLAC"), vec![2]);
        assert_eq!(technical_ids("format:m4a"), vec![3]);
        assert_eq!(technical_ids("format:!=mp3"), vec![2, 3, 4, 5]);
        assert_eq!(technical_ids("lossless:true"), vec![2, 3, 5]);
        assert_eq!(technical_ids("lossless:false"), vec![1, 4]);
        // An unknown format is not a filter; it degrades to a bare term.
        let q = parse_query("format:nonsense");
        assert!(q.field_filters.is_empty());
        assert_eq!(q.bare_terms, vec!["format:nonsense"]);
        assert!(parse_query("lossless:maybe").field_filters.is_empty());
    }

    #[test]
    fn bitrate_and_samplerate_accept_units() {
        assert_eq!(technical_ids("bitrate:>=320k"), vec![1, 2, 3, 5]);
        assert_eq!(technical_ids("bitrate:\"<=192 kbps\""), vec![4]);
        assert_eq!(technical_ids("bitrate:320000"), vec![1]);
        assert_eq!(technical_ids("bitrate:128"), vec![4]);
        assert_eq!(technical_ids("samplerate:>=96k"), vec![2]);
        assert_eq!(technical_ids("samplerate:44.1k"), vec![1, 3, 5]);
        assert_eq!(technical_ids("samplerate:48000hz"), vec![4]);
        assert!(parse_query("bitrate:fast").field_filters.is_empty());
    }

    #[test]
    fn ranges_expand_to_bounds() {
        assert_eq!(technical_ids("year:1970..1979"), vec![1, 3, 5]);
        assert_eq!(technical_ids("year:1980.."), vec![2, 4]);
        assert_eq!(technical_ids("year:..1975"), vec![1, 5]);
        assert_eq!(technical_ids("bitrate:128k..320k"), vec![1, 4]);
        assert_eq!(technical_ids("added:-30d.."), vec![1, 4]);
        assert_eq!(technical_ids("added:-1y..-6m"), vec![2]);
        assert!(technical_ids("year:1990..1980").is_empty());
        // Text fields never read `..` as a range.
        let q = parse_query("album:Wait..Now");
        assert_eq!(q.field_filters.len(), 1);
        assert_eq!(
            q.field_filters[0].value,
            FilterValue::Text("%Wait..Now%".to_string())
        );
    }
}
