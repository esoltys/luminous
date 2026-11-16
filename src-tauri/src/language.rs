//! One canonical code per language (#2132). A library tags the same language
//! many ways — `eng`, `english`, `en-US` — and every language reader (the
//! Languages cards, their auto-playlists, `get_songs_by_language`) must agree
//! on which values are the same language, or a card's count stops matching its
//! playlist. They all key on [`canonical_language`]; nothing else defines
//! language identity. Tags in the user's files are never rewritten.

use std::{collections::HashMap, sync::LazyLock};

/// `(ISO 639-1, ISO 639-2/T, ISO 639-2/B, English names separated by '|')`.
/// The `/T` and `/B` codes are equal for most languages; both are listed so
/// the table reads like the standard. Languages without a 639-1 code are not
/// listed and pass through [`canonical_language`] unchanged. Names are limited
/// to ones that denote a single language: dialect or ambiguous names (Cantonese,
/// bare "Gaelic") are left out rather than folded into the wrong code.
const LANGUAGES: &[(&str, &str, &str, &str)] = &[
    ("af", "afr", "afr", "afrikaans"),
    ("sq", "sqi", "alb", "albanian"),
    ("am", "amh", "amh", "amharic"),
    ("ar", "ara", "ara", "arabic"),
    ("hy", "hye", "arm", "armenian"),
    ("az", "aze", "aze", "azerbaijani|azeri"),
    ("eu", "eus", "baq", "basque"),
    ("be", "bel", "bel", "belarusian"),
    ("bn", "ben", "ben", "bengali|bangla"),
    ("bs", "bos", "bos", "bosnian"),
    ("bg", "bul", "bul", "bulgarian"),
    ("my", "mya", "bur", "burmese"),
    ("ca", "cat", "cat", "catalan"),
    ("zh", "zho", "chi", "chinese|mandarin"),
    ("hr", "hrv", "hrv", "croatian"),
    ("cs", "ces", "cze", "czech"),
    ("da", "dan", "dan", "danish"),
    ("nl", "nld", "dut", "dutch|flemish"),
    ("en", "eng", "eng", "english"),
    ("eo", "epo", "epo", "esperanto"),
    ("et", "est", "est", "estonian"),
    ("fi", "fin", "fin", "finnish"),
    ("fr", "fra", "fre", "french"),
    ("gl", "glg", "glg", "galician"),
    ("ka", "kat", "geo", "georgian"),
    ("de", "deu", "ger", "german"),
    ("el", "ell", "gre", "greek"),
    ("gu", "guj", "guj", "gujarati"),
    ("ht", "hat", "hat", "haitian|haitian creole"),
    ("he", "heb", "heb", "hebrew"),
    ("hi", "hin", "hin", "hindi"),
    ("hu", "hun", "hun", "hungarian"),
    ("is", "isl", "ice", "icelandic"),
    ("id", "ind", "ind", "indonesian"),
    ("ga", "gle", "gle", "irish"),
    ("it", "ita", "ita", "italian"),
    ("ja", "jpn", "jpn", "japanese"),
    ("jv", "jav", "jav", "javanese"),
    ("kn", "kan", "kan", "kannada"),
    ("kk", "kaz", "kaz", "kazakh"),
    ("km", "khm", "khm", "khmer"),
    ("ko", "kor", "kor", "korean"),
    ("ku", "kur", "kur", "kurdish"),
    ("ky", "kir", "kir", "kyrgyz|kirghiz"),
    ("lo", "lao", "lao", "lao"),
    ("la", "lat", "lat", "latin"),
    ("lv", "lav", "lav", "latvian"),
    ("lt", "lit", "lit", "lithuanian"),
    ("lb", "ltz", "ltz", "luxembourgish|letzeburgesch"),
    ("mk", "mkd", "mac", "macedonian"),
    ("mg", "mlg", "mlg", "malagasy"),
    ("ms", "msa", "may", "malay"),
    ("ml", "mal", "mal", "malayalam"),
    ("mt", "mlt", "mlt", "maltese"),
    ("mi", "mri", "mao", "maori"),
    ("mr", "mar", "mar", "marathi"),
    ("mn", "mon", "mon", "mongolian"),
    ("ne", "nep", "nep", "nepali"),
    ("no", "nor", "nor", "norwegian"),
    ("nb", "nob", "nob", "norwegian bokmal|bokmal"),
    ("nn", "nno", "nno", "norwegian nynorsk|nynorsk"),
    ("oc", "oci", "oci", "occitan"),
    ("or", "ori", "ori", "oriya|odia"),
    ("ps", "pus", "pus", "pashto|pushto"),
    ("fa", "fas", "per", "persian|farsi"),
    ("pl", "pol", "pol", "polish"),
    ("pt", "por", "por", "portuguese"),
    ("pa", "pan", "pan", "punjabi|panjabi"),
    ("ro", "ron", "rum", "romanian|moldavian"),
    ("rm", "roh", "roh", "romansh"),
    ("ru", "rus", "rus", "russian"),
    ("sm", "smo", "smo", "samoan"),
    ("sa", "san", "san", "sanskrit"),
    ("gd", "gla", "gla", "scottish gaelic"),
    ("sr", "srp", "srp", "serbian"),
    ("sn", "sna", "sna", "shona"),
    ("sd", "snd", "snd", "sindhi"),
    ("si", "sin", "sin", "sinhala|sinhalese"),
    ("sk", "slk", "slo", "slovak"),
    ("sl", "slv", "slv", "slovenian|slovene"),
    ("so", "som", "som", "somali"),
    ("es", "spa", "spa", "spanish|castilian"),
    ("su", "sun", "sun", "sundanese"),
    ("sw", "swa", "swa", "swahili"),
    ("sv", "swe", "swe", "swedish"),
    ("tl", "tgl", "tgl", "tagalog|filipino"),
    ("tg", "tgk", "tgk", "tajik"),
    ("ta", "tam", "tam", "tamil"),
    ("tt", "tat", "tat", "tatar"),
    ("te", "tel", "tel", "telugu"),
    ("th", "tha", "tha", "thai"),
    ("bo", "bod", "tib", "tibetan"),
    ("tr", "tur", "tur", "turkish"),
    ("tk", "tuk", "tuk", "turkmen"),
    ("uk", "ukr", "ukr", "ukrainian"),
    ("ur", "urd", "urd", "urdu"),
    ("ug", "uig", "uig", "uyghur|uighur"),
    ("uz", "uzb", "uzb", "uzbek"),
    ("vi", "vie", "vie", "vietnamese"),
    ("cy", "cym", "wel", "welsh"),
    ("fy", "fry", "fry", "western frisian|frisian"),
    ("xh", "xho", "xho", "xhosa"),
    ("yi", "yid", "yid", "yiddish"),
    ("yo", "yor", "yor", "yoruba"),
    ("zu", "zul", "zul", "zulu"),
];

/// Every recognized spelling (639-1 code, both 639-2 codes, each English name)
/// mapped to its 639-1 code. Built once; the table is small.
static SPELLINGS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for &(code, t, b, names) in LANGUAGES {
        map.insert(code, code);
        map.insert(t, code);
        map.insert(b, code);
        for name in names.split('|') {
            map.insert(name, code);
        }
    }
    map
});

/// The canonical key for one language tag value: its ISO 639-1 code when the
/// value is a recognized code, English name or regional variant (`en-US`,
/// `pt_BR`), otherwise the trimmed, lowercased value itself so an unrecognized
/// tag stays its own entry rather than being dropped or merged.
///
/// The result is only a grouping key; the UI turns it into a display name.
pub fn canonical_language(raw: &str) -> String {
    let lowered = raw.trim().to_lowercase();
    if let Some(code) = SPELLINGS.get(lowered.as_str()) {
        return (*code).to_string();
    }
    // Regional variant: "en-US", "pt_BR" fold to the base language when that
    // base is recognized.
    if let Some((base, _region)) = lowered.split_once(['-', '_']) {
        if let Some(code) = SPELLINGS.get(base) {
            return (*code).to_string();
        }
    }
    lowered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_codes_names_and_case_to_iso_639_1() {
        for raw in ["en", "eng", "ENG", "English", "  english "] {
            assert_eq!(canonical_language(raw), "en", "{raw}");
        }
        for raw in ["fr", "fra", "fre", "french"] {
            assert_eq!(canonical_language(raw), "fr", "{raw}");
        }
        assert_eq!(canonical_language("deu"), "de");
        assert_eq!(canonical_language("ger"), "de");
        assert_eq!(canonical_language("jpn"), "ja");
    }

    #[test]
    fn folds_regional_variants_to_the_base_language() {
        assert_eq!(canonical_language("en-US"), "en");
        assert_eq!(canonical_language("en_GB"), "en");
        assert_eq!(canonical_language("pt-BR"), "pt");
    }

    #[test]
    fn unrecognized_values_stay_distinct() {
        assert_eq!(canonical_language("Klingon"), "klingon");
        assert_eq!(canonical_language("xx-YY"), "xx-yy");
        assert_eq!(canonical_language(""), "");
    }

    #[test]
    fn table_has_no_conflicting_spellings() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for &(code, t, b, names) in LANGUAGES {
            let spellings = [code, t, b].into_iter().chain(names.split('|'));
            for s in spellings {
                if let Some(prev) = seen.insert(s, code) {
                    assert_eq!(prev, code, "{s} maps to both {prev} and {code}");
                }
            }
        }
    }
}
