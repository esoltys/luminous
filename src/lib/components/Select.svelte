<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    value: string;
    onchange: (e: Event & { currentTarget: HTMLSelectElement }) => void;
    id?: string;
    /** Color/spacing classes — the caller owns these; appearance reset and the chevron are always applied here. */
    class?: string;
    /** Right-inset of the chevron icon, matched to the select's own right padding. Defaults to the standard 0.625rem/pr-8 pairing. */
    chevronPosition?: string;
    children: Snippet;
  }

  let { value, onchange, id, class: className = "", chevronPosition = "0.625rem", children }: Props = $props();
</script>

<!--
  Every native <select> needs appearance-none (or the browser paints its own
  widget over our theme colors instead of respecting them — see #147) plus a
  manually-drawn chevron to replace the native arrow that appearance-none
  removes. Owning both here means no call site can forget either one.
-->
<select
  {id}
  {value}
  {onchange}
  class="appearance-none -webkit-appearance-none cursor-pointer {className}"
  style="background-image: url(&quot;data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 20 20' fill='none'%3E%3Cpath stroke='%239ca3af' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='m6 8 4 4 4-4'/%3E%3C/svg%3E&quot;); background-position: right {chevronPosition} center; background-repeat: no-repeat; background-size: 1.25em;"
>
  {@render children()}
</select>
