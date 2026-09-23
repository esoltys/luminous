<script lang="ts">
  import { boxBlurRGBA } from "../utils/boxBlur";

  // A heavily blurred copy of a cover image, for ambient backdrops.
  //
  // CSS `filter: blur()` on a full-size image is re-applied to the whole
  // backdrop on every composited frame, and the translucent backdrop-blur
  // headers/tables scrolling over it resample it each frame too — scrolling
  // a detail view dropped to ~40fps. A blur that heavy leaves no detail, so
  // the same look comes from blurring a tiny copy once, here, and letting
  // the GPU scale the canvas up (bilinear, via object-fit): nothing is
  // re-filtered per frame.
  interface Props {
    src: string;
    class?: string;
    style?: string;
  }

  let { src, class: className = "", style = "" }: Props = $props();

  // Matches the `blur-2xl` (blur(40px)) look it replaces: the blur is sized
  // from how large the canvas is actually drawn, since one canvas pixel
  // covers (drawn size / SIZE) CSS pixels.
  const SIZE = 128;
  const CSS_BLUR_PX = 40;

  /** Box radius whose three passes give a Gaussian of `sigma` (σ² ≈ r(r+1)). */
  function boxRadiusFor(sigma: number): number {
    return Math.max(1, Math.round(Math.sqrt(sigma * sigma + 0.25) - 0.5));
  }

  let canvas = $state<HTMLCanvasElement | null>(null);

  $effect(() => {
    const target = canvas;
    const url = src;
    if (!target || !url) return;

    let cancelled = false;
    const img = new Image();
    // Needed to read the pixels back for the blur (same as theme color
    // extraction); if the read still fails, the unblurred small copy is shown.
    img.crossOrigin = "Anonymous";
    img.onload = () => {
      if (cancelled) return;
      const ctx = target.getContext("2d");
      if (!ctx) return;
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = "high";
      // object-fit: cover — the centered square, like the <img> this replaces
      const side = Math.min(img.naturalWidth, img.naturalHeight);
      ctx.clearRect(0, 0, SIZE, SIZE);
      ctx.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, SIZE, SIZE);
      try {
        // object-fit: cover scales the square canvas by its larger drawn side
        const drawnPx = Math.max(target.clientWidth, target.clientHeight) || 1000;
        const pixels = ctx.getImageData(0, 0, SIZE, SIZE);
        boxBlurRGBA(pixels.data, SIZE, SIZE, boxRadiusFor((CSS_BLUR_PX * SIZE) / drawnPx));
        ctx.putImageData(pixels, 0, 0);
      } catch (e) {
        console.warn("BlurredCover: couldn't read cover pixels, showing it unblurred:", e);
      }
    };
    img.src = url;

    return () => {
      cancelled = true;
    };
  });
</script>

<canvas bind:this={canvas} width={SIZE} height={SIZE} class="object-cover {className}" {style} aria-hidden="true"></canvas>
