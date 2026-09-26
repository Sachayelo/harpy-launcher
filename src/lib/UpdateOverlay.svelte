<script lang="ts">
  import icon from '../assets/icon.png'
  import { megabytes, type UpdateProgress } from './api'

  let { version, progress }: { version: string; progress: UpdateProgress | null } = $props()

  const percent = $derived(progress && progress.total ? Math.round((progress.done / progress.total) * 100) : 0)
  const installing = $derived(!!progress?.total && progress.done >= progress.total)
</script>

<div class="overlay">
  <div class="panel">
    <img src={icon} alt="" />
    <h2>Mise à jour du launcher</h2>
    <p class="version">Version {version}</p>
    <div class="bar"><span class:pending={!progress} style:width="{installing ? 100 : percent}%"></span></div>
    <p class="hint">
      {#if installing}
        Installation… le launcher va se rouvrir tout seul.
      {:else if progress?.total}
        {megabytes(progress.done)} / {megabytes(progress.total)}
      {:else}
        Téléchargement…
      {/if}
    </p>
  </div>
</div>

<style>
  /* Covers everything under the title bar, which stays usable. */
  .overlay {
    position: absolute;
    inset: 38px 0 0;
    z-index: 5;
    display: grid;
    place-items: center;
    background:
      radial-gradient(circle at 50% 40%, rgba(233, 168, 98, 0.08), transparent 55%),
      var(--night);
  }

  .panel {
    display: grid;
    justify-items: center;
    width: 320px;
    text-align: center;
  }

  img {
    width: 56px;
    height: 56px;
    margin-bottom: 18px;
    border-radius: 12px;
    image-rendering: pixelated;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.45);
  }

  h2 {
    font: 700 26px var(--serif);
    letter-spacing: 0.02em;
    color: var(--brass);
  }

  .version {
    margin-bottom: 20px;
    font-size: 13px;
    color: var(--muted);
  }

  .bar {
    width: 100%;
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: #141a26;
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--brass-deep), var(--brass));
    transition: width 0.15s linear;
  }

  .bar span.pending {
    width: 30% !important;
    animation: slide 1.2s ease-in-out infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  .hint {
    margin-top: 10px;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
