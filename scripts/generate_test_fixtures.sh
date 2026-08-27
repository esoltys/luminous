#!/usr/bin/env bash
# Generates the synthetic audio fixtures under src-tauri/tests/fixtures/audio/
# used by the Cucumber BDD suites (library_scan_bdd, cover_art_bdd, etc.) so
# those scenarios exercise the real scanner/tag-reading/cover-art code paths
# against real files instead of hand-seeded DB rows (issue #578).
#
# The generated files are checked into the repo (they're tiny — short mono
# clips) so tests don't depend on ffmpeg being present in CI. Re-run this
# script and commit the results whenever the fixture set needs to change.
#
# Requires ffmpeg on PATH.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="$SCRIPT_DIR/../src-tauri/tests/fixtures/audio"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "error: ffmpeg not found on PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

# Small solid-color JPEG used as embedded cover art.
COVER="$WORK_DIR/cover.jpg"
ffmpeg -y -loglevel error -f lavfi -i "color=c=blue:s=32x32:d=1" -frames:v 1 -update 1 "$COVER"

# song_alpha.mp3 — sine wave, ID3v2.3, embedded cover art.
ffmpeg -y -loglevel error \
  -f lavfi -i "sine=frequency=440:duration=1.0" \
  -i "$COVER" -map 0:a -map 1:v \
  -metadata artist="Artist One" -metadata album="Album Gold" -metadata title="Song Alpha" \
  -id3v2_version 3 -codec:a libmp3lame -q:a 4 -codec:v copy -disposition:v attached_pic \
  "$OUT_DIR/song_alpha.mp3"

# song_beta.wav — triangle wave, RIFF INFO tags, no embedded art.
ffmpeg -y -loglevel error \
  -f lavfi -i "aevalsrc='2*abs(2*(440*t-floor(440*t+0.5)))-1':s=44100:d=1.2" \
  -metadata artist="Artist Two" -metadata album="Album Silver" -metadata title="Song Beta" \
  -codec:a pcm_s16le \
  "$OUT_DIR/song_beta.wav"

# song_gamma.flac — sawtooth wave, Vorbis comments, deliberately missing the
# album tag (edge case: partial tag data).
ffmpeg -y -loglevel error \
  -f lavfi -i "aevalsrc='2*(440*t-floor(440*t+0.5))':s=44100:d=0.8" \
  -metadata artist="Artist Three" -metadata title="Song Gamma" \
  -codec:a flac \
  "$OUT_DIR/song_gamma.flac"

# song_delta.ogg — sine wave, Vorbis comments with non-ASCII text (edge case:
# unusual characters).
ffmpeg -y -loglevel error \
  -f lavfi -i "sine=frequency=440:duration=0.5" \
  -metadata artist="Artist Four" -metadata album="Café Ünïcode Album" -metadata title="Söng Delta 日本語" \
  -codec:a libvorbis -q:a 3 \
  "$OUT_DIR/song_delta.ogg"

# song_epsilon.m4a — silence, MP4 atoms, embedded cover art.
ffmpeg -y -loglevel error \
  -f lavfi -i "anullsrc=r=44100:cl=mono:d=2.0" \
  -i "$COVER" -map 0:a -map 1:v \
  -metadata artist="Artist Five" -metadata album="Album Five" -metadata title="Silent Track" \
  -codec:a aac -b:a 64k -codec:v mjpeg -disposition:v attached_pic \
  "$OUT_DIR/song_epsilon.m4a"

# song_short.mp3 — very short clip, ID3v2.4, no tags at all (edge case:
# missing tags + minimal length, keeps scan fixtures fast in CI).
ffmpeg -y -loglevel error \
  -f lavfi -i "sine=frequency=440:duration=0.2" \
  -id3v2_version 4 -codec:a libmp3lame -q:a 4 \
  "$OUT_DIR/song_short.mp3"

echo "Generated fixtures in $OUT_DIR:"
ls -la "$OUT_DIR"
