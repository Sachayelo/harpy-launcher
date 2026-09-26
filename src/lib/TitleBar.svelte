<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import icon from '../assets/icon.png'

  type View = 'play' | 'workshop' | 'servers'

  let {
    admin,
    version,
    view,
    onview,
  }: { admin: boolean; version: string; view: View; onview: (view: View) => void } = $props()
</script>

<header class="titlebar" data-tauri-drag-region>
  <span class="brand" data-tauri-drag-region>
    <img src={icon} alt="" />
    Harpy Launcher
    {#if version}<small data-tauri-drag-region>{version}</small>{/if}
  </span>

  {#if admin}
    <nav class="tabs">
      <button class:active={view === 'play'} onclick={() => onview('play')}>Jouer</button>
      <button class:active={view === 'workshop'} onclick={() => onview('workshop')}>Atelier</button>
      <button class:active={view === 'servers'} onclick={() => onview('servers')}>Serveurs</button>
    </nav>
  {/if}

  <div class="controls">
    <button aria-label="Réduire" onclick={() => getCurrentWindow().minimize()}>
      <svg viewBox="0 0 10 10"><path d="M1 5h8" /></svg>
    </button>
    <button aria-label="Fermer" class="close" onclick={() => getCurrentWindow().close()}>
      <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 5px 0 14px;
    border-bottom: 1px solid var(--line);
    background: rgba(10, 13, 20, 0.9);
    font-size: 12px;
    color: var(--muted);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    letter-spacing: 0.02em;
  }

  .brand small {
    font-size: 11px;
    color: #5d6574;
    font-variant-numeric: tabular-nums;
  }

  .brand img {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    image-rendering: pixelated;
    pointer-events: none;
  }

  .tabs {
    position: absolute;
    left: 50%;
    display: flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #0a0e16;
    transform: translateX(-50%);
  }

  .tabs button {
    padding: 3px 14px;
    border: 0;
    border-radius: 6px;
    background: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    cursor: pointer;
  }

  .tabs button.active {
    background: var(--panel-2);
    color: var(--brass);
  }

  .controls {
    display: flex;
    gap: 2px;
  }

  .controls button {
    width: 40px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: none;
    color: #6b7382;
    cursor: pointer;
  }

  .controls button:hover {
    background: #1a2130;
    color: var(--text);
  }

  .controls .close:hover {
    background: #8c2a2a;
    color: #fff;
  }

  svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
  }
</style>
