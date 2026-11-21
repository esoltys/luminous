<script lang="ts">
  // The playbar slot: a live mini-view of the selected visualizer, or its still preview
  // while the immersive visualizer is open and owns the live animation.
  import { visualizerStore } from "../stores/visualizer.svelte";
  import VisualizerCanvas from "./VisualizerCanvas.svelte";

  let { preview = false }: { preview?: boolean } = $props();
</script>

<VisualizerCanvas
  preset={visualizerStore.preset}
  size="mini"
  {preview}
  class="w-full h-full opacity-85 group-hover:opacity-100 hover:opacity-100 transition-opacity duration-200"
/>
