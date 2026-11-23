<script lang="ts">
  import { Connection, streamUrl } from './lib/connection.svelte';
  import Spectrum from './lib/Spectrum.svelte';
  import Tracks from './lib/Tracks.svelte';
  import Waterfall from './lib/Waterfall.svelte';

  const conn = new Connection(streamUrl(location));

  $effect(() => {
    conn.start();
    return () => conn.stop();
  });

  const tracks = $derived(conn.frame?.tracks ?? []);
  const alert = $derived(tracks.find((t) => t.kind === 'fhss_control' && t.active));

  $effect(() => {
    document.title = alert ? '⚠ Drone detected · rf-watch' : 'rf-watch';
  });
</script>

<header>
  <div class="brand">
    <span class="logo" aria-hidden="true"></span>
    <strong>rf-watch</strong>
    {#if conn.device}<span class="muted">{conn.device} · v{conn.version}</span>{/if}
  </div>
  <div class="conn {conn.state}">
    <span class="dot"></span>
    {conn.state === 'open' ? 'live' : conn.state === 'connecting' ? 'connecting…' : 'offline – retrying'}
  </div>
</header>

<div class="alert" class:on={!!alert} role="status" aria-live="assertive">
  {#if alert}
    <strong>DRONE CONTROL LINK DETECTED</strong>
    <span>hopping {alert.bandwidth_mhz.toFixed(0)} MHz around {alert.center_mhz.toFixed(0)} MHz · {Math.round(alert.confidence * 100)} %</span>
  {:else}
    <span>No drone control link · monitoring {conn.band ? `${conn.band.start_mhz}–${conn.band.stop_mhz} MHz` : ''}</span>
  {/if}
</div>

<main>
  <section class="panel spectrum">
    <h2>Spectrum</h2>
    {#if conn.band}
      <Spectrum band={conn.band} frame={conn.frame} />
    {/if}
  </section>
  <section class="panel waterfall">
    <h2>Waterfall <span class="muted">· newest on top</span></h2>
    <Waterfall frame={conn.frame} />
  </section>
  <section class="panel tracks">
    <h2>Emitters</h2>
    <Tracks {tracks} now={conn.frame?.t_ms ?? 0} />
  </section>
</main>

<footer class="muted">
  {conn.band ? `${conn.band.start_mhz}–${conn.band.stop_mhz} MHz sweeps · ` : ''}{conn.frame ? `sweep #${conn.frame.seq} · floor ${conn.frame.noise_floor_dbm.toFixed(1)} dBm` : 'waiting for data'}
</footer>

<style>
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .logo {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--teal);
    box-shadow: 0 0 0 4px rgba(110, 240, 210, 0.15);
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 6px;
    font: 12px var(--mono);
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }
  .conn.open .dot {
    background: var(--teal);
    box-shadow: 0 0 8px var(--teal);
  }
  .conn.closed .dot {
    background: var(--red);
  }
  .alert {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    align-items: baseline;
    padding: 10px 16px;
    background: var(--panel);
    color: var(--muted);
    border-bottom: 1px solid var(--line);
    transition: background 0.3s;
  }
  .alert.on {
    background: var(--red-bg);
    color: #ffd9d9;
    animation: pulse 1.4s ease-in-out infinite;
  }
  .alert strong {
    color: #fff;
    letter-spacing: 0.06em;
  }
  @keyframes pulse {
    50% {
      background: #5a1717;
    }
  }
  main {
    display: grid;
    gap: 12px;
    padding: 12px 16px;
    grid-template-columns: minmax(0, 1fr);
  }
  @media (min-width: 960px) {
    main {
      grid-template-columns: minmax(0, 2fr) minmax(300px, 1fr);
      grid-template-rows: 300px 300px;
    }
    .tracks {
      grid-column: 2;
      grid-row: 1 / 3;
    }
  }
  .panel {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 12px 12px;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .spectrum,
  .waterfall {
    height: 260px;
  }
  @media (min-width: 960px) {
    .spectrum,
    .waterfall {
      height: auto;
    }
  }
  .tracks {
    overflow: auto;
  }
  h2 {
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
    margin: 0 0 8px;
  }
  h2 .muted {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
  }
  .panel > :global(:last-child) {
    flex: 1;
    min-height: 0;
  }
  footer {
    padding: 4px 16px 16px;
    font: 11px var(--mono);
  }
  .muted {
    color: var(--muted);
  }
</style>
