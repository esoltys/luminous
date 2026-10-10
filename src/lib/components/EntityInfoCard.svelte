<script lang="ts">
  import type { Snippet } from "svelte";
  import { ArrowUpRightIcon as ArrowUpRight } from "phosphor-svelte";
  import type { ContextView } from "../stores/context.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import InfoSections from "./InfoSections.svelte";

  interface Props {
    view: ContextView;
    /** The summary line: the card's name, or whatever stands in for it. */
    title: Snippet;
    /** The title already shows the community rating. */
    hideRating?: boolean;
  }

  let { view, title, hideRating = false }: Props = $props();
</script>

<!-- The detail view's stand-in for the info sidebar, shown only while the sidebar is not. -->
<details
  open
  ontoggle={(e) => windowLayoutStore.setOverviewExpanded(e.currentTarget.open)}
  class="group/overview border border-brand-border rounded-xl bg-brand-sidebar/60 backdrop-blur-md overflow-hidden shadow-xs transition-all @container"
>
  <summary class="flex items-center justify-between px-4 py-2.5 @xl:px-5 @xl:py-3 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
    {@render title()}
    <ArrowUpRight class="w-3.5 h-3.5 text-brand-text-secondary/70" />
  </summary>
  <div class="p-4 @xl:p-5 border-t border-brand-border/60">
    <InfoSections {view} entity {hideRating} />
  </div>
</details>
