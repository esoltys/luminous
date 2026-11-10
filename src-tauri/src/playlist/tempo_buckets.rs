//! The fixed tempo buckets shared by the BPM auto-playlists, their queries and
//! the Smart Playlist tempo filter.
//!
//! Ranges are half-open, `[min, max)`: a track at exactly a boundary BPM
//! belongs to the higher bucket only, so the buckets partition every BPM above
//! zero with no overlap and no gap. A `bpmrange:<min>-<max>` spec is always read
//! this way (see `CollectionScanner::get_songs_by_bpm_range`); the `bpm:`
//! search filter is a separate, inclusive comparison.

/// One tempo bucket: `min` BPM inclusive up to `max` BPM exclusive (`None`
/// means "or higher").
pub struct TempoBucket {
    /// Stable identifier (never localized, never changes once shipped).
    pub token: &'static str,
    /// English playlist name stored on the materialized auto-playlist row.
    pub name: &'static str,
    /// Key under `playlists.` in the locale files for the localized name.
    pub i18n_key: &'static str,
    pub min: f64,
    pub max: Option<f64>,
}

impl TempoBucket {
    /// The `bpmrange:` spec suffix, e.g. `"60-90"` or the open-ended `"150-"`.
    pub fn spec_suffix(&self) -> String {
        match self.max {
            Some(max) => format!("{}-{}", self.min, max),
            None => format!("{}-", self.min),
        }
    }
}

/// Slowest to fastest, which is also the card order in the playlists view.
pub const TEMPO_BUCKETS: [TempoBucket; 6] = [
    TempoBucket {
        token: "slow",
        name: "Very Slow BPM",
        i18n_key: "bpmSlow",
        min: 0.0,
        max: Some(60.0),
    },
    TempoBucket {
        token: "down-tempo",
        name: "Down-Tempo BPM",
        i18n_key: "bpmDownTempo",
        min: 60.0,
        max: Some(90.0),
    },
    TempoBucket {
        token: "mid-tempo",
        name: "Mid-Tempo BPM",
        i18n_key: "bpmMidTempo",
        min: 90.0,
        max: Some(115.0),
    },
    TempoBucket {
        token: "uptempo",
        name: "Uptempo BPM",
        i18n_key: "bpmUptempo",
        min: 115.0,
        max: Some(130.0),
    },
    TempoBucket {
        token: "high-energy",
        name: "High Energy BPM",
        i18n_key: "bpmHighEnergy",
        min: 130.0,
        max: Some(150.0),
    },
    TempoBucket {
        token: "extreme",
        name: "Extreme BPM",
        i18n_key: "bpmExtreme",
        min: 150.0,
        max: None,
    },
];

/// The half-open `[min, max)` range for a bucket token (`None` max means "or
/// higher"), or `None` for an unknown token.
pub fn bpm_range_for_token(token: &str) -> Option<(f64, Option<f64>)> {
    TEMPO_BUCKETS
        .iter()
        .find(|b| b.token.eq_ignore_ascii_case(token))
        .map(|b| (b.min, b.max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_are_contiguous_and_unbounded_above() {
        assert_eq!(TEMPO_BUCKETS[0].min, 0.0);
        for pair in TEMPO_BUCKETS.windows(2) {
            assert_eq!(pair[0].max, Some(pair[1].min), "gap or overlap");
        }
        assert_eq!(TEMPO_BUCKETS.last().unwrap().max, None);
    }

    #[test]
    fn spec_suffixes_match_the_shipped_specs() {
        let specs: Vec<String> = TEMPO_BUCKETS.iter().map(|b| b.spec_suffix()).collect();
        assert_eq!(
            specs,
            ["0-60", "60-90", "90-115", "115-130", "130-150", "150-"]
        );
    }

    #[test]
    fn bpm_range_for_token_is_case_insensitive_and_rejects_unknown() {
        assert_eq!(bpm_range_for_token("slow"), Some((0.0, Some(60.0))));
        assert_eq!(bpm_range_for_token("Extreme"), Some((150.0, None)));
        assert_eq!(bpm_range_for_token("nope"), None);
    }
}
