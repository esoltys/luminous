import { i18n } from "../stores/i18n.svelte";

export interface IntegrityIssueCopy {
  label: string;
  description: string;
}

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
  if (errorDetail === "FLAC MD5 checksum mismatch") {
    return {
      label: i18n.t("curation.integrityMd5Label"),
      description: i18n.t("curation.integrityMd5Desc"),
    };
  }
  if (errorDetail.startsWith("Truncated ")) {
    return isWarning
      ? {
          label: i18n.t("curation.integrityMinorTruncationLabel"),
          description: i18n.t("curation.integrityMinorTruncationDesc"),
        }
      : {
          label: i18n.t("curation.integrityIncompleteFileLabel"),
          description: i18n.t("curation.integrityIncompleteFileDesc"),
        };
  }
  if (errorDetail.startsWith("Empty audio file")) {
    return {
      label: i18n.t("curation.integrityEmptyFileLabel"),
      description: i18n.t("curation.integrityEmptyFileDesc"),
    };
  }
  if (errorDetail.startsWith("Container or format probe failed")) {
    return {
      label: i18n.t("curation.integrityUnrecognizedFormatLabel"),
      description: i18n.t("curation.integrityUnrecognizedFormatDesc"),
    };
  }
  if (errorDetail.startsWith("Zero audio frames decoded")) {
    return {
      label: i18n.t("curation.integrityNoAudioLabel"),
      description: i18n.t("curation.integrityNoAudioDesc"),
    };
  }
  if (
    errorDetail.startsWith("Corrupt FLAC audio frame") ||
    errorDetail.startsWith("Corrupt audio frame") ||
    errorDetail.startsWith("Corrupt trailing audio frame")
  ) {
    return {
      label: i18n.t("curation.integrityDamagedAudioLabel"),
      description: i18n.t("curation.integrityDamagedAudioDesc"),
    };
  }
  if (
    errorDetail.startsWith("FLAC frame decode error") ||
    errorDetail.startsWith("Audio decode error") ||
    errorDetail.startsWith("FLAC stream packet error") ||
    errorDetail.startsWith("Audio stream packet error")
  ) {
    return {
      label: i18n.t("curation.integrityUnreadableStreamLabel"),
      description: i18n.t("curation.integrityUnreadableStreamDesc"),
    };
  }

  return { label: errorDetail, description: "" };
}
