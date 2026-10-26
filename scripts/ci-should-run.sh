#!/usr/bin/env bash
# Determines whether a CI scope (<frontend|backend|lean|actions>) needs to run
# for the current commit/PR.
#
# Contract:
# - Prints "true" or "false" to stdout (and writes `run=<true|false>` to
#   `$GITHUB_OUTPUT` when set).
# - Always fails open ("true") outside `pull_request` events (e.g. `push` to
#   `main`, `schedule`, `workflow_dispatch`, or local runs) or if the changed
#   file list cannot be determined.

set -euo pipefail

SCOPE="${1:-}"
if [[ -z "$SCOPE" ]]; then
  echo "Usage: $0 <frontend|backend|lean|actions>" >&2
  exit 2
fi

emit() {
  local val="$1"
  echo "$val"
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    echo "run=${val}" >> "$GITHUB_OUTPUT"
  fi
  exit 0
}

# Only filter on pull_request events; push to main and scheduled runs always
# execute every job unconditionally.
if [[ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]]; then
  emit "true"
fi

if [[ -n "${CI_CHANGED_FILES:-}" ]]; then
  CHANGED_FILES="$CI_CHANGED_FILES"
else
  # On a GitHub Actions pull_request checkout with fetch-depth >= 2, HEAD is the
  # synthetic merge commit and HEAD^1 is the base branch tip.
  if ! CHANGED_FILES=$(git diff --name-only HEAD^1 HEAD 2>/dev/null); then
    if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
      git fetch --no-tags --depth=1 origin "${GITHUB_BASE_REF}" 2>/dev/null || true
      CHANGED_FILES=$(git diff --name-only FETCH_HEAD HEAD 2>/dev/null || true)
    else
      CHANGED_FILES=""
    fi
  fi
fi

# Fail open if we could not resolve a non-empty diff.
if [[ -z "$CHANGED_FILES" ]]; then
  emit "true"
fi

matches_scope() {
  local scope="$1"
  local file="$2"
  case "$scope" in
    frontend)
      case "$file" in
        src/*|scripts/*|e2e/*|ast-grep/*|docs/user-guide/*|static/*|.github/workflows/*)
          return 0 ;;
        package.json|bun.lock|tsconfig.json|vite.config.js|vite.perf.config.js|vitest.setup.ts|knip.json|sgconfig.yml)
          return 0 ;;
        src-tauri/Cargo.toml|Cargo.lock|src-tauri/tauri.conf.json|src-tauri/AppxManifest.xml|src-tauri/capabilities/*)
          return 0 ;;
        *)
          return 1 ;;
      esac
      ;;
    backend)
      case "$file" in
        src-tauri/*|crates/*|features/*|Cargo.toml|Cargo.lock|deny.toml)
          return 0 ;;
        .github/workflows/ci.yml|.github/workflows/audit.yml|.github/workflows/codeql.yml|scripts/ci-should-run.sh)
          return 0 ;;
        *)
          return 1 ;;
      esac
      ;;
    lean)
      case "$file" in
        verification/lean/*|.github/workflows/ci.yml|scripts/ci-should-run.sh)
          return 0 ;;
        *)
          return 1 ;;
      esac
      ;;
    actions)
      case "$file" in
        .github/workflows/*|.github/actions/*|scripts/ci-should-run.sh)
          return 0 ;;
        *)
          return 1 ;;
      esac
      ;;
    *)
      return 0 ;;
  esac
}

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if matches_scope "$SCOPE" "$file"; then
    emit "true"
  fi
done <<< "$CHANGED_FILES"

emit "false"
