<script lang="ts">
  import { HEAT_LUT, unit } from './colormap';
  import type { Frame } from './protocol';

  let { frame }: { frame: Frame | null } = $props();

  // History is kept at native resolution (one pixel per bin and sweep) and scaled by CSS.
  const ROWS = 200;
  let canvas: HTMLCanvasElement;
  let image: ImageData | null = null;

  $effect(() => {
    if (!frame) return;
    const bins = frame.power_dbm.length;
    const ctx = canvas.getContext('2d')!;
    if (!image || image.width !== bins) {
      canvas.width = bins;
      canvas.height = ROWS;
      image = ctx.createImageData(bins, ROWS);
      image.data.fill(255);
      for (let i = 0; i < image.data.length; i += 4) image.data.set([5, 7, 12], i);
    }
    // scroll down one row, newest sweep on top
    const d = image.data;
    d.copyWithin(bins * 4, 0, d.length - bins * 4);
    for (let i = 0; i < bins; i++) {
      const c = Math.round(unit(frame.power_dbm[i]) * 255) * 3;
      d[i * 4] = HEAT_LUT[c];
      d[i * 4 + 1] = HEAT_LUT[c + 1];
      d[i * 4 + 2] = HEAT_LUT[c + 2];
      d[i * 4 + 3] = 255;
    }
    ctx.putImageData(image, 0, 0);
  });
</script>

<canvas bind:this={canvas}></canvas>

<style>
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 160px;
    image-rendering: pixelated;
  }
</style>
