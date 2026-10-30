<script lang="ts">
  export let items: any[] = [];
  export let start = 0;
  export let end = 0;
  $: start = 0;
  $: end = items.length;
  export function scrollToIndex(_index: number, _options?: unknown) {}
</script>

<div data-testid="virtual-list-mock">
  {#each items as item}
    <slot {item} />
  {/each}
</div>
