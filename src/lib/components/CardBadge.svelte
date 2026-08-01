<script lang="ts">
  import type { ComponentType, SvelteComponent } from "svelte";

  interface Props {
    icon: ComponentType<SvelteComponent<{ class?: string }>>;
    label: string;
    title?: string;
    /** Spins the icon (e.g. a refresh/auto-refill indicator). */
    spin?: boolean;
  }

  let { icon: Icon, label, title, spin = false }: Props = $props();
</script>

<div
  class="absolute top-2 right-2 z-10 flex items-center gap-1 px-2 py-0.5 rounded-full bg-brand-accent text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg select-none"
  {title}
>
  <Icon class="w-2.5 h-2.5 {spin ? 'animate-spin [animation-duration:3s]' : ''}" />
  {label}
</div>
