#!/bin/bash
set -uo pipefail

file=$(jq -r '.tool_input.file_path // .tool_response.filePath // empty')

# One check at a time: a rapid run of edits must not stack cargo/bun builds.
exec 9>"${TMPDIR:-/tmp}/luminous-post-edit-check.lock"
flock -n 9 || exit 0

case "$file" in
  *.rs)
    (cd "$CLAUDE_PROJECT_DIR/src-tauri" && timeout 180 cargo clippy --all-targets -- -D warnings) 2>&1 | tail -30
    ;;
  *.ts|*.svelte)
    (cd "$CLAUDE_PROJECT_DIR" && timeout 180 bun run check) 2>&1 | tail -20
    ;;
esac

exit 0
