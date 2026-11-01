<script lang="ts">
  /** A small ring filled to `fraction` (0–1): a running sync on the collapsed Files tile. */
  let { fraction, class: className = "" }: { fraction: number; class?: string } = $props();

  const RADIUS = 6;
  const CIRCUMFERENCE = 2 * Math.PI * RADIUS;
</script>

<svg viewBox="0 0 16 16" class={className} role="presentation" data-testid="files-sync-ring">
  <circle cx="8" cy="8" r={RADIUS + 1.5} class="fill-brand-sidebar" />
  <circle cx="8" cy="8" r={RADIUS} fill="none" stroke-width="2.5" class="stroke-brand-border" />
  <circle
    cx="8"
    cy="8"
    r={RADIUS}
    fill="none"
    stroke-width="2.5"
    stroke-linecap="round"
    class="stroke-brand-accent-text"
    stroke-dasharray={CIRCUMFERENCE}
    stroke-dashoffset={CIRCUMFERENCE * (1 - Math.min(1, Math.max(0, fraction)))}
    transform="rotate(-90 8 8)"
  />
</svg>
