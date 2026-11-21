/**
 * Whether a window-level playback shortcut (Space, arrows) should leave this
 * keydown alone: a repeat, a key typed into an editable field, or a key a
 * focused control already handled — it claims the key with `preventDefault()`
 * (e.g. the arrow keys that nudge a parametric EQ node).
 */
export function shouldSkipGlobalShortcut(event: KeyboardEvent): boolean {
  if (event.repeat || event.defaultPrevented) return true;
  const target = event.target;
  if (!(target instanceof HTMLElement)) return false;
  return target.closest("input, textarea, select, [contenteditable]") !== null;
}

/**
 * The visualizer step a keydown asks for: `[` is the previous preset (-1), `]` the next (1),
 * anything else 0 (#2245). Matched by character, so it follows the keyboard layout. AltGr
 * (Ctrl+Alt on Windows) types the brackets on some layouts and counts; a plain Ctrl+[ /
 * Ctrl+] is Back / Forward and doesn't.
 */
export function visualizerStepForKey(event: KeyboardEvent): -1 | 0 | 1 {
  if (event.metaKey || (event.ctrlKey && !event.altKey)) return 0;
  if (event.key === "[") return -1;
  if (event.key === "]") return 1;
  return 0;
}
