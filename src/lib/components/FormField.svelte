<script lang="ts">
  import type { Snippet } from "svelte";
  import HelpTip from "./HelpTip.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    label: string;
    for: string;
    /** Spans both columns of the enclosing 2-column form grid. */
    span2?: boolean;
    /** Brief explanation shown via an info icon next to the label (hover or keyboard focus), and read as the field's description. */
    tooltip?: string;
    children: Snippet;
  }

  let { label, for: htmlFor, span2 = false, tooltip, children }: Props = $props();
</script>

<div class="flex flex-col gap-1 {span2 ? 'col-span-2' : ''}">
  <div class="flex items-center gap-1">
    <label for={htmlFor} class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{label}</label>
    {#if tooltip}
      <HelpTip text={tooltip} label={i18n.t("common.aboutField", { field: label })} describes={htmlFor} />
    {/if}
  </div>
  {@render children()}
</div>
