<script lang="ts">
  let { items = [], children }: { items: any[]; children?: any } = $props();
</script>

<div data-testid="virtual-list-mock">
  {#each items as item}
    {#if children}
      {@render children({ item })}
    {/if}
  {/each}
</div>
