<!--
  An inline warning or error line: a warning icon plus the message in the theme's
  primary text colour. The icon and wording carry the meaning, never a palette
  colour (docs/UI_CONVENTIONS.md "State is never colour-only", #1878). The icon
  is centred on the first line of text however large the text size is.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { WarningIcon } from "phosphor-svelte";

  interface Props {
    children: Snippet;
    /** `compact` is 11px (dense rows); the icon scales with the text. */
    size?: "default" | "compact";
    /** Pass "alert" for errors that appear in response to a user action. */
    role?: "alert";
    id?: string;
    class?: string;
  }

  let { children, size = "default", role, id, class: className = "" }: Props = $props();
</script>

<div
  {id}
  {role}
  class="flex items-start gap-1.5 text-brand-text-primary {size === 'compact'
    ? 'text-[11px]'
    : 'text-xs'} {className}"
>
  <WarningIcon class="w-[1.1em] h-[1.1em] shrink-0 translate-y-[calc((1lh-1.1em)/2)]" />
  <span class="min-w-0 wrap-anywhere">{@render children()}</span>
</div>
