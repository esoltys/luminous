//! Filter Parser module — parses search expressions with field-specific comparators.

use rusqlite::types::{ToSqlOutput, Value as SqlValue};
use rusqlite::ToSql;

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

#[derive(Debug, Clone, PartialEq)]
pub struct FieldFilter {
    pub field: String,
    pub sql_column: &'static str,
    pub is_numeric: bool,
    pub op: Op,
    pub value: FilterValue,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ParsedQuery {
    pub bare_terms: Vec<String>,
    pub field_filters: Vec<FieldFilter>,
}

/// Parse a raw search query string into bare full-text search terms and field-specific filter comparators.
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

    for ch in input.chars() {
        if ch == '"' || ch == '\'' {
            if in_quotes && ch == quote_char {
                in_quotes = false;
            } else if !in_quotes {
                in_quotes = true;
                quote_char = ch;
            }
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.trim().is_empty() {
                tokens.push(current.trim().to_string());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }

    tokens
}

fn parse_field_filter(token: &str) -> Option<FieldFilter> {
    let colon_idx = token.find(':')?;
    let (field_part, val_part) = token.split_at(colon_idx);
    let val_str = val_part[1..].trim_matches('"').trim_matches('\'');

    let field_clean = field_part.trim().to_lowercase();
    if field_clean.is_empty() || val_str.is_empty() {
        return None;
    }

    let (sql_column, is_numeric) = match field_clean.as_str() {
        "artist" | "album_artist" => ("COALESCE(NULLIF(album_artist, ''), artist)", false),
        "album" => ("album", false),
        "title" => ("title", false),
        "genre" => ("genre", false),
        "composer" => ("composer", false),
        "year" => ("year", true),
        "bitrate" => ("bitrate", true),
        "track" | "track_number" => ("track", true),
        "disc" | "disc_number" => ("disc", true),
        "rating" | "stars" => ("rating", true),
        "playcount" | "plays" | "play_count" => ("playcount", true),
        "skipcount" | "skips" | "skip_count" => ("skipcount", true),
        "lastplayed" | "last_played" => ("lastplayed", true),
        "duration" | "length" => ("length_nanosec", true),
        _ => return None,
    };

    let (op, raw_val) = parse_op_and_value(val_str, is_numeric);

    let value = if sql_column == "length_nanosec" {
        FilterValue::Int(parse_duration_ns(raw_val)?)
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
        FilterValue::Text(raw_val.to_string())
    };

    Some(FieldFilter {
        field: field_clean,
        sql_column,
        is_numeric,
        op,
        value,
    })
}

fn parse_op_and_value<'a>(val_str: &'a str, is_numeric: bool) -> (Op, &'a str) {
    if val_str.starts_with(">=") {
        (Op::Gte, &val_str[2..])
    } else if val_str.starts_with("<=") {
        (Op::Lte, &val_str[2..])
    } else if val_str.starts_with("!=") {
        (Op::Neq, &val_str[2..])
    } else if val_str.starts_with('>') {
        (Op::Gt, &val_str[1..])
    } else if val_str.starts_with('<') {
        (Op::Lt, &val_str[1..])
    } else if val_str.starts_with('=') {
        (Op::Eq, &val_str[1..])
    } else if is_numeric {
        (Op::Eq, val_str)
    } else {
        (Op::Contains, val_str)
    }
}

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
    fn test_parse_duration() {
        assert_eq!(parse_duration_ns("3:45"), Some(225_000_000_000));
        assert_eq!(parse_duration_ns("1:02:03"), Some(3723_000_000_000));
        assert_eq!(parse_duration_ns("180"), Some(180_000_000_000));
    }
}
