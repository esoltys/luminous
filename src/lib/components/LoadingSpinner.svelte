<script lang="ts">
  import { CircleNotchIcon as LoaderCircle } from "phosphor-svelte";

  interface Props {
    label: string;
    size?: "sm" | "md";
  }

  let { label, size = "md" }: Props = $props();

  let sizeClass = $derived(size === "sm" ? "w-6 h-6" : "w-8 h-8");
</script>

<LoaderCircle class="{sizeClass} animate-spin text-brand-accent-text" />
<span class="text-xs text-brand-text-secondary/60 font-medium">{label}</span>
