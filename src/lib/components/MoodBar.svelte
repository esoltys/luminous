<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { playerStore } from "../stores/player.svelte";

  let canvas = $state<HTMLCanvasElement | null>(null);
  let moodbarData = $state<number[]>([]); // 150 points * 3 (RGB) = 450 bytes

  async function loadMoodbar(songId: number | undefined) {
    if (songId === undefined) {
      moodbarData = [];
      return;
    }
    try {
      const data = await invoke<number[] | null>("get_moodbar_data", { songId });
      if (data) {
        moodbarData = data;
      } else {
        moodbarData = [];
      }
    } catch (e) {
      console.error("Failed to load moodbar:", e);
      moodbarData = [];
    }
  }

  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;
    ctx.clearRect(0, 0, width, height);

    if (moodbarData.length === 0) {
      return;
    }

    const points = moodbarData.length / 3;
    const step = width / points;

    for (let i = 0; i < points; i++) {
      const r = moodbarData[i * 3];
      const g = moodbarData[i * 3 + 1];
      const b = moodbarData[i * 3 + 2];
      ctx.fillStyle = `rgb(${r}, ${g}, ${b})`;
      ctx.fillRect(i * step, 0, step + 1, height);
    }
  }

  $effect(() => {
    loadMoodbar(playerStore.currentSong?.id);
  });

  $effect(() => {
    // Redraw when moodbar data changes or is loaded
    const _data = moodbarData;
    draw();
  });
</script>

<div class="w-full h-[3px] rounded bg-gray-800 overflow-hidden relative opacity-60 hover:opacity-100 transition-opacity duration-200">
  <canvas bind:this={canvas} width="450" height="4" class="w-full h-full object-cover"></canvas>
</div>
