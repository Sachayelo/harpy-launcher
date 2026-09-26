<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { megabytes, play, syncPack, type PackStatus, type SyncProgress } from './api'

  let {
    status,
    statusError,
    lunarFound,
    developer,
    devSelected,
    onselect,
    onfinished,
  }: {
    status: PackStatus | null
    statusError: string
    lunarFound: boolean | null
    developer: boolean
    devSelected: boolean
    onselect: (dev: boolean) => void
    onfinished: () => void
  } = $props()

  let phase = $state<'idle' | 'syncing' | 'launching'>('idle')
  let progress = $state<SyncProgress | null>(null)
  let step = $state('')
  let message = $state('')
  let error = $state('')

  const percent = $derived(
    progress && progress.bytesTotal ? Math.round((progress.bytesDone / progress.bytesTotal) * 100) : 0,
  )
  const fileName = $derived(progress?.file.split('/').pop() ?? '')
  const busy = $derived(phase !== 'idle')

  onMount(() => {
    const stops: Array<() => void> = []
    const subscribe = <T,>(event: string, handler: (payload: T) => void) =>
      listen<T>(event, (e) => handler(e.payload))
        .then((stop) => stops.push(stop))
        .catch(() => {})
    subscribe<SyncProgress>('sync-progress', (payload) => (progress = payload))
    subscribe<string>('launch-step', (payload) => (step = payload))
    return () => stops.forEach((stop) => stop())
  })

  async function run(action: 'sync' | 'play') {
    phase = action === 'sync' ? 'syncing' : 'launching'
    progress = null
    step = ''
    message = ''
    error = ''
    try {
      if (action === 'sync') {
        await syncPack()
        message = 'Le pack est prêt.'
      } else {
        const outcome = await play()
        message =
          outcome === 'launched'
            ? 'Bon voyage !'
            : outcome === 'alreadyRunning'
              ? 'Le jeu est déjà lancé.'
              : `Lunar est ouvert : lance ${status?.target.profileName}.`
      }
    } catch (e) {
      error = String(e)
    } finally {
      phase = 'idle'
      progress = null
      onfinished()
    }
  }
</script>

<section class="card play">
  {#if developer}
    <div class="switch" role="group" aria-label="Profil">
      <button class:active={!devSelected} disabled={busy} onclick={() => onselect(false)}>Harpy Express</button>
      <button class:active={devSelected} disabled={busy} onclick={() => onselect(true)}>Harpy Dev</button>
    </div>
  {/if}

  {#if busy && progress}
    <div class="progress">
      <div class="line"><b>{fileName}</b><span>{percent} %</span></div>
      <div class="bar"><span style:width="{percent}%"></span></div>
    </div>
  {:else}
    <div class="status">
      {#if status?.state === 'ready'}
        <span class="chip ok">À jour</span>
      {:else if status?.state === 'update'}
        <span class="chip">Mise à jour · {megabytes(status.downloadBytes)}</span>
      {:else if status?.state === 'install'}
        <span class="chip">Non installé</span>
      {:else if status?.state === 'unpublished'}
        <span class="muted">Aucune version publiée</span>
      {:else if statusError}
        <span class="muted warn">{statusError}</span>
      {:else}
        <span class="muted">Recherche de la dernière version…</span>
      {/if}
      {#if status?.version}
        <span class="version">
          {status.version}{status.target.channel === 'prod' ? '' : ` · ${status.target.channel}`}
        </span>
      {/if}
    </div>
  {/if}

  {#if phase === 'syncing'}
    <button class="primary" disabled>Mise à jour…</button>
  {:else if phase === 'launching'}
    <button class="primary" disabled>Lancement…</button>
  {:else if status?.state === 'install'}
    <button class="primary" disabled={!lunarFound} onclick={() => run('sync')}>Installer</button>
  {:else if status?.state === 'update'}
    <button class="primary" disabled={!lunarFound} onclick={() => run('sync')}>Mettre à jour</button>
  {:else}
    <button class="primary" disabled={!lunarFound || status?.state !== 'ready'} onclick={() => run('play')}>
      Jouer
    </button>
  {/if}

  <p class="hint" class:warn={error || lunarFound === false}>
    {#if error}
      {error}
    {:else if lunarFound === false}
      Installe Lunar Client pour jouer
    {:else if phase === 'syncing' || (phase === 'launching' && progress)}
      {progress
        ? `${progress.index} / ${progress.total} fichiers · ${megabytes(progress.bytesDone)} / ${megabytes(progress.bytesTotal)}`
        : 'Vérification des fichiers…'}
    {:else if phase === 'launching'}
      {step || 'Vérification des fichiers…'}
    {:else if message}
      {message}
    {:else if status?.state === 'install'}
      {status.downloadBytes > 0 ? `${megabytes(status.downloadBytes)} · ` : ''}le profil {status.target.profileName}
      sera ajouté à Lunar
    {:else if status?.state === 'update'}
      {status.downloadFiles} fichier{status.downloadFiles > 1 ? 's' : ''} à télécharger pour {status.target.profileName}
    {:else if status?.state === 'ready'}
      Lance {status.target.profileName} et rejoint le serveur
    {/if}
  </p>
</section>

<style>
  .play {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 12px;
  }

  .switch {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: #0a0e16;
  }

  .switch button {
    padding: 5px 0;
    border: 0;
    border-radius: 7px;
    background: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    cursor: pointer;
  }

  .switch button.active {
    background: var(--panel-2);
    color: var(--brass);
    box-shadow: inset 0 0 0 1px rgba(233, 168, 98, 0.25);
  }

  .switch button:disabled {
    cursor: default;
  }

  .status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 24px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    background: rgba(233, 168, 98, 0.12);
    color: var(--brass);
  }

  .chip.ok {
    background: rgba(127, 212, 154, 0.1);
    color: var(--ok);
  }

  .chip::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .version {
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .muted {
    font-size: 13px;
    color: var(--muted);
  }

  .progress {
    display: grid;
    gap: 6px;
  }

  .progress .line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 12px;
    color: var(--muted);
  }

  .progress .line b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    color: #c9c3b8;
  }

  .bar {
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: #0a0e16;
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--brass-deep), var(--brass));
    transition: width 0.15s linear;
  }

  .primary {
    height: 54px;
    border: 0;
    border-radius: 10px;
    cursor: pointer;
    font: 700 22px var(--serif);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #24140a;
    background: linear-gradient(180deg, #f4c283, var(--brass) 45%, var(--brass-deep));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.35),
      0 8px 24px rgba(233, 168, 98, 0.18);
    transition:
      transform 0.12s ease,
      filter 0.12s ease;
  }

  .primary:hover:not(:disabled) {
    filter: brightness(1.06);
  }

  .primary:active:not(:disabled) {
    transform: translateY(1px);
  }

  .primary:disabled {
    cursor: default;
    filter: saturate(0.4) brightness(0.7);
  }

  .hint {
    text-align: center;
    font-size: 12px;
    color: var(--muted);
  }

  .warn {
    color: var(--warn);
  }
</style>
