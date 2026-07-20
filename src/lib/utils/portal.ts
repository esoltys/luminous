// Moves a node to document.body so fixed-position overlays (modals) escape
// ancestors that establish a CSS containing block via `transform` — notably
// the +layout.svelte flip-card, whose `transform: rotateY(0deg)` on
// .flip-face otherwise traps `position: fixed` descendants inside it, both
// shrinking their viewport coverage and pinning their stacking order below
// the floating PlayerBar dock (z-40) that sits outside that subtree.
export function portal(node: HTMLElement) {
  document.body.appendChild(node);

  return {
    destroy() {
      node.parentNode?.removeChild(node);
    },
  };
}
