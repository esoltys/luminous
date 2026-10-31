<script lang="ts" module>
  /** How a slice of a device's storage is drawn. */
  export type StorageTone = "music" | "other" | "removed" | "copied" | "reserve" | "free";

  export interface StorageSlice {
    tone: Exclude<StorageTone, "reserve" | "free">;
    bytes: number;
  }
</script>

<script lang="ts">
  import "./storageTones.css";

  /**
   * One storage drawn to scale (#839): `slices` from the left, the reserve
   * kept free for the device at the right end of the capacity, and anything
   * past the capacity in the warning colour. Values are the backend's; this
   * only lays them out. Never animated: it changes on every checkbox click.
   */
  let {
    slices,
    capacityBytes,
    reserveBytes,
    scaleBytes = undefined,
    height = "h-2.5",
    label,
  }: {
    slices: StorageSlice[];
    capacityBytes: number;
    reserveBytes: number;
    /** Bytes the full width stands for, so bars drawn together share a scale. Defaults to what this bar needs. */
    scaleBytes?: number;
    height?: string;
    /** Read out in place of the bar, which is drawn for sighted users only. */
    label: string;
  } = $props();

  const used = $derived(slices.reduce((sum, s) => sum + s.bytes, 0));
  const scale = $derived(Math.max(1, scaleBytes ?? Math.max(capacityBytes, used)));
  const pct = (bytes: number) => `${(Math.max(0, bytes) / scale) * 100}%`;
  const overBytes = $derived(Math.max(0, used - capacityBytes));
</script>

<div class="relative w-full min-w-0 {height}" role="img" aria-label={label}>
  <div class="absolute inset-y-0 left-0 rounded-[3px] bg-brand-border/25 border border-brand-border/40" style:width={pct(capacityBytes)}></div>
  <div class="absolute inset-0 flex overflow-hidden rounded-[3px]" aria-hidden="true">
    {#each slices as slice, i (i)}
      {#if slice.bytes > 0}
        <span class="h-full shrink-0 storage-tone-{slice.tone}" style:width={pct(slice.bytes)}></span>
      {/if}
    {/each}
  </div>
  {#if reserveBytes > 0 && capacityBytes > 0}
    <span
      class="absolute inset-y-0 storage-tone-reserve"
      style:left={pct(capacityBytes - reserveBytes)}
      style:width={pct(reserveBytes)}
      aria-hidden="true"
    ></span>
  {/if}
  {#if overBytes > 0}
    <span
      class="absolute inset-y-0 rounded-r-[3px] storage-tone-over"
      style:left={pct(capacityBytes)}
      style:width={pct(overBytes)}
      aria-hidden="true"
    ></span>
  {/if}
  {#if scale > capacityBytes || scaleBytes !== undefined}
    <span class="absolute -inset-y-1 w-0.5 bg-brand-text-primary/80" style:left={pct(capacityBytes)} aria-hidden="true"></span>
  {/if}
</div>
