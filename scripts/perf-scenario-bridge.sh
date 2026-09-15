#!/usr/bin/env bash
# Scripted load scenarios for the Tokio scheduler diagnostics in
# docs/PERFORMANCE.md (#1002) — drives the app via its local bridge server
# (src-tauri/src/bridge.rs) instead of GUI automation, so the same scenario
# can be repeated identically across a before/after build for comparison in
# tokio-console or against spawn_scheduler_latency_monitor's log output.
#
# `skip-tracks` queues the checked-in test fixtures
# (src-tauri/tests/fixtures/audio/, see scripts/generate_test_fixtures.sh)
# rather than anything from your personal library, and replaces whatever is
# currently queued/playing — it does not attempt to preserve or restore prior
# queue/playback state. Using the fixtures (not a random or library-dependent
# pick) keeps the workload identical across machines, branches, and repeat
# runs, which a before/after comparison depends on; it also avoids WebDAV
# tracks, whose network-fetch latency would swamp the scheduler/lock timing
# this scenario is meant to isolate.
#
# One-time setup: add src-tauri/tests/fixtures/audio as a watched library
# folder in the running app (Settings > Library) and let it scan once, so the
# fixtures have song IDs to queue by.
#
# Requires the app already running with the bridge server reachable
# (bun run tauri dev, or a built binary).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/../src-tauri/tests/fixtures/audio"
FIXTURE_BASENAMES=(song_alpha.mp3 song_beta.wav song_delta.ogg song_epsilon.m4a song_gamma.flac song_short.mp3)

PORT="${LUMINOUS_BRIDGE_PORT:-21849}"
BASE="http://127.0.0.1:${PORT}"

usage() {
  echo "Usage: $0 <skip-tracks|bridge-flood> [count]" >&2
  echo "  skip-tracks [N=50]   queue the test fixtures (repeated to cover N+buffer), then POST next N times in a row" >&2
  echo "  bridge-flood [N=200] fire N concurrent GET /health requests" >&2
  echo "" >&2
  echo "Env: LUMINOUS_DB_PATH overrides the auto-detected luminous.db path (skip-tracks only)." >&2
  exit 1
}

find_db_path() {
  if [ -n "${LUMINOUS_DB_PATH:-}" ]; then
    echo "$LUMINOUS_DB_PATH"
    return 0
  fi
  local candidates=(
    "$HOME/.local/share/org.luminous.music/luminous.db"
    "$HOME/Library/Application Support/org.luminous.music/luminous.db"
    "${APPDATA:-}/39231EricJamesSoltys.LuminousMusicPlayer/luminous.db"
  )
  for p in "${candidates[@]}"; do
    if [ -n "$p" ] && [ -f "$p" ]; then
      echo "$p"
      return 0
    fi
  done
  return 1
}

queue_fixture_tracks() {
  local n="$1"
  if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "error: sqlite3 CLI not found on PATH" >&2
    exit 1
  fi
  local db
  db="$(find_db_path)" || {
    echo "error: couldn't locate luminous.db — set LUMINOUS_DB_PATH=/path/to/luminous.db" >&2
    exit 1
  }

  local where=""
  for f in "${FIXTURE_BASENAMES[@]}"; do
    [ -n "$where" ] && where="${where} OR "
    where="${where}path LIKE '%${f}'"
  done

  local fixture_ids
  fixture_ids=$(sqlite3 "file:${db}?mode=ro" \
    "SELECT id FROM songs WHERE (${where}) AND unavailable=0 ORDER BY path;")

  if [ -z "$fixture_ids" ]; then
    cat >&2 <<EOF
error: none of the checked-in test fixtures are in your library yet.

  One-time setup: add this folder as a watched library folder in Luminous
  and let it scan once, then re-run this script:
    ${FIXTURES_DIR}

Using these fixtures (rather than your personal library) keeps the scenario
identical across machines, branches, and repeat runs.
EOF
    exit 1
  fi

  local fixture_list=()
  while IFS= read -r id; do
    fixture_list+=("$id")
  done <<< "$fixture_ids"
  echo "Found ${#fixture_list[@]}/${#FIXTURE_BASENAMES[@]} fixture tracks in the library."

  # Cycle the fixture IDs to build a queue of length n — next_track() doesn't
  # care about track content, just that there's a fixed, reproducible order.
  local queue_ids=()
  while [ "${#queue_ids[@]}" -lt "$n" ]; do
    queue_ids+=("${fixture_list[@]}")
  done
  queue_ids=("${queue_ids[@]:0:$n}")

  local ids
  ids=$(IFS=,; echo "${queue_ids[*]}")
  echo "Queuing ${#queue_ids[@]} tracks (fixtures repeated) ..."
  curl -sf -X POST "${BASE}/playback/play" \
    -H 'Content-Type: application/json' \
    -d "{\"track_ids\":[${ids}],\"start_index\":0}" >/dev/null
}

[ $# -ge 1 ] || usage
scenario="$1"
count="${2:-}"

if ! curl -sf "${BASE}/health" >/dev/null 2>&1; then
  echo "error: bridge server not reachable at ${BASE} — is the app running?" >&2
  exit 1
fi

case "$scenario" in
  skip-tracks)
    count="${count:-50}"
    queue_fixture_tracks "$((count + 10))"
    echo "Skipping ${count} tracks via ${BASE}/playback/control ..."
    start=$(date +%s.%N)
    for _ in $(seq 1 "$count"); do
      curl -sf -X POST "${BASE}/playback/control" \
        -H 'Content-Type: application/json' \
        -d '{"action":"next"}' >/dev/null
    done
    end=$(date +%s.%N)
    awk -v s="$start" -v e="$end" 'BEGIN { printf "Done in %.3fs\n", e - s }'
    ;;
  bridge-flood)
    count="${count:-200}"
    echo "Firing ${count} concurrent GET ${BASE}/health requests ..."
    start=$(date +%s.%N)
    for _ in $(seq 1 "$count"); do
      curl -sf "${BASE}/health" >/dev/null &
    done
    wait
    end=$(date +%s.%N)
    awk -v s="$start" -v e="$end" 'BEGIN { printf "Done in %.3fs\n", e - s }'
    ;;
  *)
    usage
    ;;
esac
