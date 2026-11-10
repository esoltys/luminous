<script lang="ts">
  import GenreAliasesDialog from "./GenreAliasesDialog.svelte";
  import { tagsStore } from "../stores/tags.svelte";
</script>

<!-- The single mount point for the alias dialog: every view that calls
     tagsStore.openAliasDialog() renders this host. -->
{#if tagsStore.aliasDialogPrefill !== null}
  <GenreAliasesDialog
    initialAlias={tagsStore.aliasDialogPrefill}
    onClose={() => tagsStore.closeAliasDialog()}
  />
{/if}
