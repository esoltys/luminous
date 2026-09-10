#!/bin/bash
set -uo pipefail

file=$(jq -r '.tool_input.file_path // .tool_response.filePath // empty')

case "$file" in
  *.rs)
    (cd "$CLAUDE_PROJECT_DIR/src-tauri" && cargo clippy --all-targets -- -D warnings) 2>&1 | tail -30
    ;;
  *.ts|*.svelte)
    (cd "$CLAUDE_PROJECT_DIR" && bun run check) 2>&1 | tail -20
    ;;
esac

exit 0
