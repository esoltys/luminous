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
