<script lang="ts">
  import ReleaseCard from './ReleaseCard.svelte'
  import type { Release } from './api'

  // "whatsNew" greets a returning player with the version that just came out;
  // "history" lists every version of the channel.
  let {
    mode,
    releases,
    onclose,
  }: { mode: 'whatsNew' | 'history'; releases: Release[]; onclose: () => void } = $props()

  const shown = $derived(mode === 'whatsNew' ? releases.slice(0, 1) : releases)

  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') onclose()
  }
</script>

<svelte:window onkeydown={key} />

<div class="overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <div class="panel" role="dialog" aria-modal="true" aria-label={mode === 'whatsNew' ? 'Quoi de neuf' : 'Historique'}>
    <header>
      <h2>{mode === 'whatsNew' ? 'Quoi de neuf ?' : 'Historique des versions'}</h2>
      <button class="close" aria-label="Fermer" onclick={onclose}>
        <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
      </button>
    </header>
    <div class="list">
      {#each shown as release, i (release.version)}
        <ReleaseCard {release} current={i === 0} />
        {#if i < shown.length - 1}<hr />{/if}
      {/each}
    </div>
    {#if mode === 'whatsNew'}
      <button class="ok" onclick={onclose}>C'est parti</button>
    {/if}
  </div>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 38px 0 0;
    z-index: 6;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(5, 7, 12, 0.78);
    backdrop-filter: blur(3px);
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: min(580px, 100%);
    /* The window minus the title bar and the overlay's padding. */
    max-height: calc(100vh - 38px - 48px);
    min-height: 0;
    padding: 20px 22px;
    border: 1px solid rgba(233, 168, 98, 0.22);
    border-radius: 14px;
    background: var(--panel);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    font: 700 24px var(--serif);
    letter-spacing: 0.02em;
    color: var(--brass);
  }

  .close {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
    background: var(--panel-2);
    color: var(--text);
  }

  .close svg {
    width: 11px;
    height: 11px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }

  .list {
    display: grid;
    gap: 16px;
    min-height: 0;
    overflow-y: auto;
    padding-right: 6px;
  }

  hr {
    border: 0;
    border-top: 1px solid var(--line);
  }

  .ok {
    align-self: flex-end;
    padding: 9px 22px;
    border: 0;
    border-radius: 9px;
    font-weight: 600;
    color: #24140a;
    background: linear-gradient(180deg, #f4c283, var(--brass) 45%, var(--brass-deep));
    cursor: pointer;
  }
</style>
