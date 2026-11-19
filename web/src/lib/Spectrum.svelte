<script lang="ts">
  import { DBM_MAX, DBM_MIN, unit } from './colormap';
  import type { Band, Frame } from './protocol';

  let { band, frame }: { band: Band; frame: Frame | null } = $props();

  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);

  const PAD = { l: 44, r: 10, t: 10, b: 24 };

  // Peak hold decays slowly so short hops stay visible for a moment.
  let peak: Float32Array | null = null;

  $effect(() => {
    if (!frame || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const ctx = canvas.getContext('2d')!;
    ctx.scale(dpr, dpr);
    draw(ctx, frame);
  });

  function draw(ctx: CanvasRenderingContext2D, f: Frame) {
    const w = width - PAD.l - PAD.r;
    const h = height - PAD.t - PAD.b;
    const x = (mhz: number) => PAD.l + ((mhz - band.start_mhz) / (band.stop_mhz - band.start_mhz)) * w;
    const xb = (i: number) => PAD.l + ((i + 0.5) / f.power_dbm.length) * w;
    const y = (dbm: number) => PAD.t + (1 - unit(dbm)) * h;

    ctx.clearRect(0, 0, width, height);
    ctx.font = '11px ui-monospace, monospace';

    // grid
    ctx.strokeStyle = 'rgba(140,160,190,0.12)';
    ctx.fillStyle = 'rgba(160,175,200,0.7)';
    ctx.lineWidth = 1;
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    for (let d = DBM_MIN; d <= DBM_MAX; d += 10) {
      ctx.beginPath();
      ctx.moveTo(PAD.l, y(d));
      ctx.lineTo(PAD.l + w, y(d));
      ctx.stroke();
      ctx.fillText(String(d), PAD.l - 6, y(d));
    }
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    const step = w < 360 ? 20 : 10;
    for (let m = Math.ceil(band.start_mhz / step) * step; m <= band.stop_mhz; m += step) {
      ctx.beginPath();
      ctx.moveTo(x(m), PAD.t);
      ctx.lineTo(x(m), PAD.t + h);
      ctx.stroke();
      ctx.fillText(String(m), x(m), PAD.t + h + 6);
    }

    // tracks
    for (const t of f.tracks) {
      const fhss = t.kind === 'fhss_control';
      const a = t.active ? 1 : 0.4;
      ctx.fillStyle = fhss ? `rgba(255,70,70,${0.13 * a})` : `rgba(80,150,255,${0.13 * a})`;
      const x0 = x(t.center_mhz - t.bandwidth_mhz / 2);
      const x1 = x(t.center_mhz + t.bandwidth_mhz / 2);
      ctx.fillRect(x0, PAD.t, Math.max(2, x1 - x0), h);
    }

    // peak hold
    if (!peak || peak.length !== f.power_dbm.length) peak = new Float32Array(f.power_dbm).fill(DBM_MIN);
    for (let i = 0; i < peak.length; i++) peak[i] = Math.max(f.power_dbm[i], peak[i] - 0.6);
    ctx.strokeStyle = 'rgba(255,180,90,0.45)';
    ctx.beginPath();
    peak.forEach((p, i) => (i ? ctx.lineTo(xb(i), y(p)) : ctx.moveTo(xb(i), y(p))));
    ctx.stroke();

    // live trace with fill
    const grad = ctx.createLinearGradient(0, PAD.t, 0, PAD.t + h);
    grad.addColorStop(0, 'rgba(90,230,200,0.45)');
    grad.addColorStop(1, 'rgba(90,230,200,0.02)');
    ctx.beginPath();
    ctx.moveTo(xb(0), PAD.t + h);
    f.power_dbm.forEach((p, i) => ctx.lineTo(xb(i), y(p)));
    ctx.lineTo(xb(f.power_dbm.length - 1), PAD.t + h);
    ctx.closePath();
    ctx.fillStyle = grad;
    ctx.fill();
    ctx.strokeStyle = 'rgb(110,240,210)';
    ctx.lineWidth = 1.3;
    ctx.beginPath();
    f.power_dbm.forEach((p, i) => (i ? ctx.lineTo(xb(i), y(p)) : ctx.moveTo(xb(i), y(p))));
    ctx.stroke();

    // threshold
    ctx.setLineDash([4, 4]);
    ctx.strokeStyle = 'rgba(255,220,120,0.6)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(PAD.l, y(f.threshold_dbm));
    ctx.lineTo(PAD.l + w, y(f.threshold_dbm));
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = 'rgba(255,220,120,0.8)';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'bottom';
    ctx.fillText('threshold', PAD.l + w - 4, y(f.threshold_dbm) - 3);
  }
</script>

<div class="wrap" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
</div>

<style>
  .wrap {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 180px;
  }
  canvas {
    position: absolute;
    inset: 0;
  }
</style>
