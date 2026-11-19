import { i18n } from "../stores/i18n.svelte";

export interface IntegrityIssueCopy {
  label: string;
  description: string;
}

interface IntegrityRule {
  prefixes: string[];
  labelKey: string;
  descKey: string;
  warningOnly?: boolean;
  corruptOnly?: boolean;
}

// Keep prefixes aligned with `crates/luminous-audio/src/source.rs` and
// `crates/luminous-analysis/src/integrity.rs` diagnostics.
const INTEGRITY_RULES: IntegrityRule[] = [
  {
    prefixes: ["FLAC MD5 checksum mismatch"],
    labelKey: "curation.integrityMd5Label",
    descKey: "curation.integrityMd5Desc",
  },
  {
    prefixes: ["Truncated "],
    labelKey: "curation.integrityMinorTruncationLabel",
    descKey: "curation.integrityMinorTruncationDesc",
    warningOnly: true,
  },
  {
    prefixes: ["Truncated "],
    labelKey: "curation.integrityIncompleteFileLabel",
    descKey: "curation.integrityIncompleteFileDesc",
    corruptOnly: true,
  },
  {
    prefixes: ["Empty audio file"],
    labelKey: "curation.integrityEmptyFileLabel",
    descKey: "curation.integrityEmptyFileDesc",
  },
  {
    prefixes: ["Container or format probe failed"],
    labelKey: "curation.integrityUnrecognizedFormatLabel",
    descKey: "curation.integrityUnrecognizedFormatDesc",
  },
  {
    prefixes: ["Zero audio frames decoded"],
    labelKey: "curation.integrityNoAudioLabel",
    descKey: "curation.integrityNoAudioDesc",
  },
  {
    prefixes: ["Corrupt FLAC audio frame", "Corrupt audio frame", "Corrupt trailing audio frame"],
    labelKey: "curation.integrityDamagedAudioLabel",
    descKey: "curation.integrityDamagedAudioDesc",
  },
  {
    prefixes: [
      "FLAC frame decode error",
      "Audio decode error",
      "FLAC stream packet error",
      "Audio stream packet error",
    ],
    labelKey: "curation.integrityUnreadableStreamLabel",
    descKey: "curation.integrityUnreadableStreamDesc",
  },
];

/**
 * Maps a raw backend `song_integrity.error_detail` diagnostic string into a
 * localized, non-technical label and inline plain-language description.
 *
 * Unrecognized strings fall back to displaying the raw detail as the label
 * with no extra description so future backend diagnostics still surface.
 */
export function describeIntegrityIssue(
  errorDetail: string,
  isWarning: boolean,
): IntegrityIssueCopy {
  const rule = INTEGRITY_RULES.find(
    (r) =>
      (!r.warningOnly || isWarning) &&
      (!r.corruptOnly || !isWarning) &&
      r.prefixes.some((prefix) => errorDetail.startsWith(prefix)),
  );
  if (!rule) {
    return { label: errorDetail, description: "" };
  }
  return {
    label: i18n.t(rule.labelKey),
    description: i18n.t(rule.descKey),
  };
}
