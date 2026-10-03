//! Equalizer APO parametric profiles (#1336) — the `ParametricEQ.txt` files
//! AutoEq publishes for thousands of headphones:
//!
//! ```text
//! Preamp: -6.2 dB
//! Filter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0.70
//! Filter 2: ON PK Fc 180 Hz Gain -3.1 dB Q 0.53
//! ```
//!
//! Parsing is all-or-nothing: a profile is returned only if every line maps
//! exactly onto the engine's filters and every value is inside its
//! `EQ_*_RANGE`. Anything else — an unsupported filter type, an unknown
//! directive, an out-of-range gain — rejects the whole file, because applying
//! the rest would not sound like the profile. Nothing is clamped.

use crate::equalizer::{
    ParametricBand, ParametricKind, EQ_FREQ_RANGE, EQ_GAIN_RANGE, EQ_PREAMP_RANGE, EQ_Q_RANGE,
    PARAMETRIC_MAX_BANDS,
};
use crate::models::SettingRange;
use serde::Serialize;
use std::path::Path;

/// Profile files are a few hundred bytes; the cap keeps a mis-picked file
/// (an album, a disk image) from being read into memory.
pub const MAX_PROFILE_FILE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedProfile {
    pub preamp: f32,
    pub bands: Vec<ParametricBand>,
}

/// Why a profile was rejected. Serialises as `{ "code": ..., ...details }`,
/// which the UI maps to a translated message. `line` is 1-based.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ImportError {
    /// No `Filter` lines.
    Empty,
    TooManyFilters {
        count: usize,
        max: usize,
    },
    /// A filter type the engine has no equivalent for (`LP`, `HP`, `NO`, …).
    UnsupportedFilter {
        line: usize,
        kind: String,
    },
    /// A directive other than `Preamp` / `Filter` (`Channel:`, `Include:`, …).
    UnsupportedLine {
        line: usize,
    },
    Malformed {
        line: usize,
    },
    OutOfRange {
        line: usize,
        field: RangeField,
        value: f32,
        min: f32,
        max: f32,
    },
    PreampOutOfRange {
        value: f32,
        min: f32,
        max: f32,
    },
    FileTooLarge {
        max_bytes: u64,
    },
    ReadFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeField {
    Freq,
    Gain,
    Q,
}

impl ImportError {
    /// The JSON string that crosses IPC as the command's error.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"code":"malformed","line":0}"#.into())
    }
}

/// Read a profile file as text. Handles UTF-8 (with or without BOM) and the
/// UTF-16 that Windows editors sometimes save as.
pub fn read_profile_file(path: &Path) -> Result<String, ImportError> {
    let len = std::fs::metadata(path)
        .map_err(|_| ImportError::ReadFailed)?
        .len();
    if len > MAX_PROFILE_FILE_BYTES {
        return Err(ImportError::FileTooLarge {
            max_bytes: MAX_PROFILE_FILE_BYTES,
        });
    }
    let bytes = std::fs::read(path).map_err(|_| ImportError::ReadFailed)?;
    Ok(decode_text(&bytes))
}

fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |rest: &[u8], le: bool| {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if le {
                    u16::from_le_bytes(c)
                } else {
                    u16::from_be_bytes(c)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Parse an Equalizer APO parametric profile. See the module docs for the
/// all-or-nothing contract.
///
/// Accepted: blank lines, `#` comments, at most one `Preamp: <x> dB`
/// (default 0), and `Filter[ N]: ON|OFF PK|PEQ|LSC|HSC Fc <f> Hz Gain <g> dB
/// Q <q>`. Keywords are case-insensitive; `OFF` imports a disabled band.
pub fn parse_parametric_profile(text: &str) -> Result<ImportedProfile, ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut preamp: Option<f32> = None;
    let mut bands = Vec::new();

    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((directive, rest)) = line.split_once(':') else {
            return Err(ImportError::Malformed { line: line_no });
        };
        let directive = directive.trim().to_ascii_lowercase();
        if directive == "preamp" {
            if preamp.is_some() {
                return Err(ImportError::Malformed { line: line_no });
            }
            preamp = Some(parse_preamp(rest).ok_or(ImportError::Malformed { line: line_no })?);
        } else if is_filter_directive(&directive) {
            bands.push(parse_filter(rest, line_no)?);
        } else {
            return Err(ImportError::UnsupportedLine { line: line_no });
        }
    }

    if bands.is_empty() {
        return Err(ImportError::Empty);
    }
    if bands.len() > PARAMETRIC_MAX_BANDS {
        return Err(ImportError::TooManyFilters {
            count: bands.len(),
            max: PARAMETRIC_MAX_BANDS,
        });
    }
    let preamp = preamp.unwrap_or(0.0);
    if !in_range(EQ_PREAMP_RANGE, preamp) {
        return Err(ImportError::PreampOutOfRange {
            value: preamp,
            min: EQ_PREAMP_RANGE.min,
            max: EQ_PREAMP_RANGE.max,
        });
    }
    Ok(ImportedProfile { preamp, bands })
}

/// `Filter`, or `Filter <number>`.
fn is_filter_directive(directive: &str) -> bool {
    match directive.strip_prefix("filter") {
        Some(n) => {
            let n = n.trim();
            n.is_empty() || n.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn in_range(range: SettingRange, value: f32) -> bool {
    value.is_finite() && value >= range.min && value <= range.max
}

fn parse_number(token: Option<&str>) -> Option<f32> {
    token?.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// `<x> dB`, the unit optional.
fn parse_preamp(rest: &str) -> Option<f32> {
    let mut tokens = rest.split_whitespace();
    let value = parse_number(tokens.next())?;
    match (tokens.next(), tokens.next()) {
        (None, _) => Some(value),
        (Some(unit), None) if unit.eq_ignore_ascii_case("db") => Some(value),
        _ => None,
    }
}

fn parse_filter(rest: &str, line: usize) -> Result<ParametricBand, ImportError> {
    let malformed = ImportError::Malformed { line };
    let mut tokens = rest.split_whitespace().peekable();

    let enabled = match tokens.next().map(str::to_ascii_uppercase).as_deref() {
        Some("ON") => true,
        Some("OFF") => false,
        _ => return Err(malformed),
    };
    let kind_token = tokens.next().ok_or(malformed.clone())?;
    let kind = match kind_token.to_ascii_uppercase().as_str() {
        "PK" | "PEQ" => ParametricKind::Peak,
        "LSC" => ParametricKind::LowShelf,
        "HSC" => ParametricKind::HighShelf,
        _ => {
            return Err(ImportError::UnsupportedFilter {
                line,
                kind: kind_token.to_string(),
            })
        }
    };

    let (mut freq, mut gain_db, mut q) = (None, None, None);
    while let Some(key) = tokens.next() {
        let (slot, unit) = match key.to_ascii_lowercase().as_str() {
            "fc" => (&mut freq, Some("hz")),
            "gain" => (&mut gain_db, Some("db")),
            "q" => (&mut q, None),
            // `BW Oct`, slope variants (`LSC 12dB`), … — shapes the engine
            // can't reproduce exactly.
            _ => return Err(malformed),
        };
        if slot.is_some() {
            return Err(malformed);
        }
        *slot = Some(parse_number(tokens.next()).ok_or(malformed.clone())?);
        if let Some(unit) = unit {
            if tokens.peek().is_some_and(|t| t.eq_ignore_ascii_case(unit)) {
                tokens.next();
            }
        }
    }
    let (Some(freq), Some(gain_db), Some(q)) = (freq, gain_db, q) else {
        return Err(malformed);
    };

    for (field, range, value) in [
        (RangeField::Freq, EQ_FREQ_RANGE, freq),
        (RangeField::Gain, EQ_GAIN_RANGE, gain_db),
        (RangeField::Q, EQ_Q_RANGE, q),
    ] {
        if !in_range(range, value) {
            return Err(ImportError::OutOfRange {
                line,
                field,
                value,
                min: range.min,
                max: range.max,
            });
        }
    }

    Ok(ParametricBand {
        kind,
        freq,
        gain_db,
        q,
        enabled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::equalizer::{BiquadFilter, Equalizer, EqualizerConfig};
    use ParametricKind::{HighShelf, LowShelf, Peak};

    fn fixture(name: &str) -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/autoeq")
            .join(name);
        read_profile_file(&path).unwrap()
    }

    fn band(kind: ParametricKind, freq: f32, gain_db: f32, q: f32) -> ParametricBand {
        ParametricBand {
            kind,
            freq,
            gain_db,
            q,
            enabled: true,
        }
    }

    fn anker_q20_bands() -> Vec<ParametricBand> {
        vec![
            band(LowShelf, 105.0, -5.8, 0.70),
            band(Peak, 36.8, 1.7, 1.35),
            band(Peak, 91.2, -4.1, 0.91),
            band(Peak, 459.4, 3.9, 0.83),
            band(Peak, 1087.0, -2.4, 3.65),
            band(Peak, 1776.8, 3.2, 2.66),
            band(Peak, 2926.6, -3.3, 2.51),
            band(Peak, 4813.9, 3.8, 3.82),
            band(Peak, 8377.4, -3.5, 2.49),
            band(HighShelf, 10000.0, 0.5, 0.70),
        ]
    }

    #[test]
    fn parses_anker_soundcore_life_q20_fixture_exactly() {
        let profile =
            parse_parametric_profile(&fixture("Anker Soundcore Life Q20 ParametricEq.txt"))
                .unwrap();
        assert_eq!(profile.preamp, -3.78);
        assert_eq!(profile.bands, anker_q20_bands());
    }

    #[test]
    fn parses_sennheiser_hd_600_fixture_exactly() {
        let profile =
            parse_parametric_profile(&fixture("Sennheiser HD 600 ParametricEQ.txt")).unwrap();
        assert_eq!(profile.preamp, -6.3);
        assert_eq!(
            profile.bands,
            vec![
                band(LowShelf, 105.0, 6.5, 0.70),
                band(Peak, 125.0, -2.7, 0.55),
                band(Peak, 8445.0, 3.3, 1.61),
                band(Peak, 522.0, 0.7, 1.02),
                band(Peak, 1298.0, -1.2, 2.14),
                band(HighShelf, 10000.0, -3.1, 0.70),
                band(Peak, 3158.0, -1.8, 3.67),
                band(Peak, 2166.0, 0.9, 3.32),
                band(Peak, 6639.0, 2.2, 5.82),
                band(Peak, 5433.0, -1.2, 5.70),
            ]
        );
    }

    /// autoeq.app's bass-boost targets push the preamp past the old −12 dB
    /// floor; the widened range takes them as-is.
    #[test]
    fn accepts_bass_boosted_profile_beyond_old_twelve_db_limits() {
        let text = "Preamp: -16.4 dB
                    Filter 1: ON LSC Fc 105 Hz Gain 16.2 dB Q 0.70
                    Filter 2: ON PK Fc 3000 Hz Gain -14.5 dB Q 2.0
";
        let profile = parse_parametric_profile(text).unwrap();
        assert_eq!(profile.preamp, -16.4);
        assert_eq!(
            profile.bands,
            vec![
                band(LowShelf, 105.0, 16.2, 0.70),
                band(Peak, 3000.0, -14.5, 2.0)
            ]
        );
    }

    /// The issue's acceptance check: an imported profile sounds like the
    /// file. The engine's cascade response must equal the sum of each
    /// filter's RBJ response built straight from the file's numbers.
    #[test]
    fn imported_profile_response_matches_per_filter_reference() {
        let text = fixture("Anker Soundcore Life Q20 ParametricEq.txt");
        let profile = parse_parametric_profile(&text).unwrap();
        let mut eq = Equalizer::new();
        eq.update_format(48_000, 2);
        eq.load_user_preset(1, &profile.bands, profile.preamp);

        let freqs: Vec<f32> = (0..200)
            .map(|i| 20.0 * 1000f32.powf(i as f32 / 199.0))
            .collect();
        let got = eq.parametric_response_db(&freqs);
        for (f, got) in freqs.iter().zip(got) {
            let expected: f32 = anker_q20_bands()
                .iter()
                .map(|b| {
                    let mut filter = BiquadFilter::new();
                    filter.calculate_for_kind(b.kind, b.freq, 48_000.0, b.gain_db, b.q);
                    filter.magnitude_db(*f, 48_000.0)
                })
                .sum();
            assert!((got - expected).abs() < 0.01, "{f} Hz: {got} vs {expected}");
        }
        assert_eq!(eq.preamp, -3.78);
    }

    /// A rejected file never reaches the engine: the caller only loads a
    /// profile it got back, so a non-flat config stays exactly as it was.
    #[test]
    fn rejected_profile_leaves_running_config_unchanged() {
        let mut eq = Equalizer::new();
        eq.update_format(48_000, 2);
        eq.load_user_preset(7, &[band(Peak, 1000.0, 4.0, 1.0)], -2.5);
        let before = EqualizerConfig::snapshot(&eq);

        let text = "Preamp: -3 dB
Filter 1: ON PK Fc 100 Hz Gain 2 dB Q 1
Filter 2: ON LP Fc 15000 Hz";
        if let Ok(profile) = parse_parametric_profile(text) {
            eq.load_user_preset(8, &profile.bands, profile.preamp);
        }
        assert_eq!(EqualizerConfig::snapshot(&eq), before);
    }

    #[test]
    fn off_filter_imports_as_disabled_band() {
        let profile =
            parse_parametric_profile("Filter 1: OFF PK Fc 1000 Hz Gain 3 dB Q 1").unwrap();
        assert_eq!(
            profile.bands,
            vec![ParametricBand {
                enabled: false,
                ..band(Peak, 1000.0, 3.0, 1.0)
            }]
        );
        assert_eq!(profile.preamp, 0.0);
    }

    #[test]
    fn tolerates_crlf_bom_comments_unnumbered_filters_and_case() {
        let text = "\u{feff}# AutoEq\r\n\r\npreamp: -2 db\r\nfilter: on peq fc 50 hz gain 2 db q 0.5\r\nFILTER 2:ON HSC Fc 8000 Hz Gain -1.5 dB Q 0.7\r\n";
        let profile = parse_parametric_profile(text).unwrap();
        assert_eq!(profile.preamp, -2.0);
        assert_eq!(
            profile.bands,
            vec![
                band(Peak, 50.0, 2.0, 0.5),
                band(HighShelf, 8000.0, -1.5, 0.7)
            ]
        );
    }

    #[test]
    fn decodes_utf16_le_files() {
        let text = "Preamp: -1 dB\nFilter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\n";
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_text(&bytes), text);
    }

    #[test]
    fn rejects_unsupported_filter_type() {
        let text =
            "Preamp: -1 dB\nFilter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\nFilter 2: ON LP Fc 15000 Hz";
        assert_eq!(
            parse_parametric_profile(text),
            Err(ImportError::UnsupportedFilter {
                line: 3,
                kind: "LP".into()
            })
        );
    }

    #[test]
    fn rejects_unknown_directive() {
        let text = "Channel: L\nFilter 1: ON PK Fc 100 Hz Gain 1 dB Q 1";
        assert_eq!(
            parse_parametric_profile(text),
            Err(ImportError::UnsupportedLine { line: 1 })
        );
    }

    #[test]
    fn rejects_malformed_lines() {
        for (text, line) in [
            ("Filter 1: ON PK Fc abc Hz Gain 1 dB Q 1", 1),
            ("Filter 1: ON PK Fc 100 Hz Gain 1 dB", 1),
            ("Filter 1: ON PK Fc 100 Hz BW Oct 1 Gain 1 dB", 1),
            ("Filter 1: MAYBE PK Fc 100 Hz Gain 1 dB Q 1", 1),
            ("Filter 1: ON PK Fc 100 Hz Fc 200 Hz Gain 1 dB Q 1", 1),
            ("Filter 1: ON PK Fc NaN Hz Gain 1 dB Q 1", 1),
            (
                "Preamp: -1 dB\nPreamp: -2 dB\nFilter: ON PK Fc 1 Hz Gain 1 dB Q 1",
                2,
            ),
            (
                "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\nnot a directive",
                2,
            ),
        ] {
            assert_eq!(
                parse_parametric_profile(text),
                Err(ImportError::Malformed { line }),
                "{text}"
            );
        }
    }

    #[test]
    fn rejects_more_filters_than_the_engine_holds() {
        let text: String = (1..=21)
            .map(|i| format!("Filter {i}: ON PK Fc {} Hz Gain 1 dB Q 1\n", 100 * i))
            .collect();
        assert_eq!(
            parse_parametric_profile(&text),
            Err(ImportError::TooManyFilters { count: 21, max: 20 })
        );
    }

    #[test]
    fn rejects_out_of_range_values_without_clamping() {
        assert_eq!(
            parse_parametric_profile("Filter 1: ON PK Fc 100 Hz Gain 20.5 dB Q 1"),
            Err(ImportError::OutOfRange {
                line: 1,
                field: RangeField::Gain,
                value: 20.5,
                min: EQ_GAIN_RANGE.min,
                max: EQ_GAIN_RANGE.max,
            })
        );
        assert_eq!(
            parse_parametric_profile("Filter 1: ON PK Fc 10 Hz Gain 1 dB Q 1"),
            Err(ImportError::OutOfRange {
                line: 1,
                field: RangeField::Freq,
                value: 10.0,
                min: EQ_FREQ_RANGE.min,
                max: EQ_FREQ_RANGE.max,
            })
        );
        assert_eq!(
            parse_parametric_profile("Preamp: -30 dB\nFilter 1: ON PK Fc 100 Hz Gain 1 dB Q 1"),
            Err(ImportError::PreampOutOfRange {
                value: -30.0,
                min: EQ_PREAMP_RANGE.min,
                max: EQ_PREAMP_RANGE.max,
            })
        );
    }

    #[test]
    fn rejects_a_file_without_filters() {
        assert_eq!(
            parse_parametric_profile("Preamp: -3 dB\n# nothing else"),
            Err(ImportError::Empty)
        );
        assert_eq!(parse_parametric_profile(""), Err(ImportError::Empty));
    }

    #[test]
    fn rejects_oversized_files_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.txt");
        std::fs::write(&path, vec![b'#'; MAX_PROFILE_FILE_BYTES as usize + 1]).unwrap();
        assert_eq!(
            read_profile_file(&path),
            Err(ImportError::FileTooLarge {
                max_bytes: MAX_PROFILE_FILE_BYTES
            })
        );
        assert_eq!(
            read_profile_file(&dir.path().join("missing.txt")),
            Err(ImportError::ReadFailed)
        );
    }

    #[test]
    fn errors_serialise_as_code_plus_details() {
        let json = ImportError::OutOfRange {
            line: 4,
            field: RangeField::Q,
            value: 12.0,
            min: 0.1,
            max: 10.0,
        }
        .to_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["code"], "out_of_range");
        assert_eq!(v["line"], 4);
        assert_eq!(v["field"], "q");
        assert_eq!(ImportError::Empty.to_json(), r#"{"code":"empty"}"#);
    }
}
