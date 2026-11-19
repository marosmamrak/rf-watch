<script lang="ts">
  import { dbm, duration, mhz, percent } from './format';
  import { KIND_LABEL, type Track } from './protocol';

  let { tracks, now }: { tracks: Track[]; now: number } = $props();
</script>

{#if tracks.length === 0}
  <p class="empty">No emitters above threshold.</p>
{:else}
  <ul>
    {#each tracks as t (t.id)}
      <li class:fhss={t.kind === 'fhss_control'} class:lost={!t.active}>
        <div class="head">
          <span class="kind">{KIND_LABEL[t.kind]}</span>
          <span class="badge">{t.active ? 'ACTIVE' : `lost ${duration(now - t.last_seen_ms)} ago`}</span>
        </div>
        <dl>
          <div><dt>Centre</dt><dd>{mhz(t.center_mhz)}</dd></div>
          <div><dt>{t.kind === 'fhss_control' ? 'Hop span' : 'Bandwidth'}</dt><dd>{mhz(t.bandwidth_mhz)}</dd></div>
          <div><dt>Peak</dt><dd>{dbm(t.peak_dbm)}</dd></div>
          <div><dt>Seen for</dt><dd>{duration(t.last_seen_ms - t.first_seen_ms)}</dd></div>
        </dl>
        <div class="conf" title="confidence {percent(t.confidence)}">
          <div style:width={percent(t.confidence).replace(' ', '')}></div>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
  }
  li {
    border: 1px solid var(--line);
    border-left: 3px solid var(--blue);
    border-radius: 8px;
    padding: 10px 12px;
    background: var(--panel-2);
    transition: opacity 0.3s;
  }
  li.fhss {
    border-left-color: var(--red);
  }
  li.lost {
    opacity: 0.55;
  }
  .head {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    align-items: baseline;
  }
  .kind {
    font-weight: 600;
  }
  .badge {
    font: 600 11px var(--mono);
    letter-spacing: 0.05em;
    color: var(--muted);
    white-space: nowrap;
  }
  li.fhss:not(.lost) .badge {
    color: var(--red);
  }
  dl {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: 4px 12px;
    margin: 8px 0;
  }
  dt {
    font-size: 11px;
    color: var(--muted);
  }
  dd {
    margin: 0;
    font: 13px var(--mono);
  }
  .conf {
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .conf div {
    height: 100%;
    background: var(--blue);
    transition: width 0.3s;
  }
  li.fhss .conf div {
    background: var(--red);
  }
  .empty {
    color: var(--muted);
    margin: 0;
  }
</style>
