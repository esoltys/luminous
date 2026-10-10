<script lang="ts">
  import { i18n, type Locale } from "../stores/i18n.svelte";
  import { localeLabel, localePickerGroups } from "../locales";
  import Select from "./Select.svelte";

  /** UI language picker: switches and persists the locale; callers own only id and styling. */
  let { id, class: className = "" }: { id?: string; class?: string } = $props();

  const languageGroups = localePickerGroups();
</script>

<Select
  {id}
  value={i18n.currentLocale}
  onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
  class={className}
>
  {#each languageGroups.pinned as tag (tag)}
    <option value={tag} lang={tag}>{localeLabel(tag)}</option>
  {/each}
  {#if languageGroups.rest.length > 0}
    <!-- Non-focusable separator: a disabled option is skipped by keyboard navigation. -->
    <option disabled>──────────</option>
    {#each languageGroups.rest as tag (tag)}
      <option value={tag} lang={tag}>{localeLabel(tag)}</option>
    {/each}
  {/if}
</Select>
