<script lang="ts">
  import { CheckIcon as Check } from "phosphor-svelte";
  import type { ColorChoice } from "../badgeChoices";

  interface Props {
    choices: ColorChoice[];
    value: string | null;
    onChange: (value: string | null) => void;
    /** "md" (default) is the 28px swatch-with-checkmark used in form
     * sections (folder/WebDAV badge colour). "sm" is a compact 20px
     * border-highlight variant sized for tight spaces like a context-menu
     * popover (genre card colour). */
    size?: "sm" | "md";
    /** Wraps at a fixed column count via CSS grid instead of flex-wrap —
     * use when the row count must stay fixed regardless of the container's
     * actual width (e.g. a context-menu popover meant to show exactly N
     * columns), since flex-wrap's wrap point depends on guessing the
     * container width from swatch size + gap, which is easy to get wrong. */
    columns?: number;
  }

  let { choices, value, onChange, size = "md", columns }: Props = $props();
</script>

<div
  class="items-center {columns ? 'grid' : 'flex flex-wrap'} {size === 'sm' ? 'gap-1.5' : 'gap-2.5'}"
  style={columns ? `grid-template-columns: repeat(${columns}, min-content);` : ""}
>
  {#each choices as choice}
    {@const isSelected = value === choice.value}
    {@const swatchColor = choice.swatchColor ?? choice.value}
    <button
      type="button"
      onclick={() => onChange(choice.value)}
      class="relative rounded-full border-2 transition-transform duration-150 flex items-center justify-center
        {size === 'sm' ? 'w-5 h-5' : 'w-7 h-7'}
        {isSelected
          ? (size === 'sm' ? 'border-brand-text-primary' : 'scale-110 ring-2 ring-brand-accent')
          : (size === 'sm' ? 'border-transparent hover:border-brand-border' : 'hover:scale-105')}
        {choice.value === null ? 'bg-brand-sidebar border-brand-border' : (size === 'sm' ? '' : 'border-white/20')}"
      style={swatchColor ? `background-color: ${swatchColor};` : ''}
      title={choice.label}
      aria-label={choice.label}
    >
      {#if isSelected && size !== 'sm'}
        <Check class="w-3.5 h-3.5 {choice.value === null ? 'text-brand-accent-text' : 'text-white'} drop-shadow-sm" />
      {/if}
    </button>
  {/each}
</div>
