<script lang="ts">
  export let items: any[] = [];
</script>

<div data-testid="virtual-list-mock">
  {#each items as item}
    <slot {item} />
  {/each}
</div>
