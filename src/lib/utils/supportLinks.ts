/**
 * Where users go to report a problem or ask for help. Shared by the Help page
 * and Settings > About so the two entry points never point at different places.
 */

/** Opens GitHub's bug report form directly, rather than the raw issue list. */
export const REPORT_BUG_URL =
  "https://github.com/esoltys/luminous/issues/new?template=bug_report.yml";

/** Questions and usage help; matches the contact link in .github/ISSUE_TEMPLATE/config.yml. */
export const GET_HELP_URL = "https://www.reddit.com/r/LuminousMusicPlayer/";
