<script lang="ts">
  import "./storageTones.css";
  import type { StorageTone } from "./DeviceStorageBar.svelte";

  /** Names every slice of a storage bar with its value, so nothing is shown only on hover. */
  let { items }: { items: { tone: StorageTone; label: string }[] } = $props();
</script>

<ul class="flex flex-wrap gap-x-5 gap-y-1 text-xs text-brand-text-secondary">
  {#each items as item (item.tone)}
    <!-- The reserve is drawn at the bar's right end, so its label sits there too. -->
    <li class="flex items-center gap-1.5" class:ml-auto={item.tone === "reserve"}>
      <span class="w-2.5 h-2.5 rounded-[2px] shrink-0 storage-tone-{item.tone}" aria-hidden="true"></span>
      {item.label}
    </li>
  {/each}
</ul>
