<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import {
    dismissCrash,
    megabytes,
    openGameFolder,
    repair,
    shareLog,
    type Crash,
    type SyncProgress,
  } from './api'

  // With a crash: what happened and what to try. Without: the same tools, for
  // a player who has a problem the launcher didn't spot.
  let { crash, onclose }: { crash: Crash | null; onclose: (repaired: boolean) => void } = $props()

  let busy = $state('')
  let link = $state('')
  let copied = $state(false)
  let message = $state('')
  let error = $state('')
  let disableExtra = $state(false)
  let progress = $state<SyncProgress | null>(null)

  const titles = {
    game: 'Le jeu a planté',
    java: 'Java s\'est arrêté net',
    loader: 'Le jeu n\'a pas pu démarrer',
  }

  const when = $derived.by(() => {
    if (!crash) return ''
    const minutes = Math.round((Date.now() - crash.at) / 60_000)
    if (minutes < 1) return "à l'instant"
    if (minutes < 60) return `il y a ${minutes} min`
    const hours = Math.round(minutes / 60)
    if (hours < 24) return `il y a ${hours} h`
    return new Date(crash.at).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long' })
  })

  onMount(() => {
    let stop: (() => void) | undefined
    listen<SyncProgress>('sync-progress', (event) => {
      if (busy === 'repair') progress = event.payload
    })
      .then((unlisten) => (stop = unlisten))
      .catch(() => {})
    return () => stop?.()
  })

  async function run(label: string, action: () => Promise<void>) {
    busy = label
    error = ''
    message = ''
    try {
      await action()
    } catch (e) {
      error = String(e)
    } finally {
      busy = ''
      progress = null
    }
  }

  const share = (path: string | null) =>
    run('share', async () => {
      link = await shareLog(path)
      copied = false
      try {
        await navigator.clipboard.writeText(link)
        copied = true
      } catch {
        // The link stays on screen to copy by hand.
      }
    })

  const fix = () =>
    run('repair', async () => {
      const result = await repair(disableExtra)
      message =
        result.moved > 0
          ? `Pack réparé. ${result.moved} mod${result.moved > 1 ? 's' : ''} perso rangé${result.moved > 1 ? 's' : ''} dans mods-desactives.`
          : 'Pack réparé : chaque fichier est revenu à sa version d\'origine.'
    })

  async function close(repaired = false) {
    if (crash) await dismissCrash().catch(() => {})
    onclose(repaired)
  }

  function key(event: KeyboardEvent) {
    if (event.key === 'Escape' && !busy) close(!!message)
  }
</script>

<svelte:window onkeydown={key} />

<div class="overlay">
  <div class="panel" role="dialog" aria-modal="true" aria-label="Aide">
    <header>
      <div>
        <h2>{crash ? titles[crash.kind] : 'Un souci avec le jeu ?'}</h2>
        {#if crash}<p class="when">{crash.file} · {when}</p>{/if}
      </div>
      <button class="close" aria-label="Fermer" disabled={!!busy} onclick={() => close(!!message)}>
        <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
      </button>
    </header>

    {#if crash}
      <p class="summary">{crash.summary}</p>
      {#if crash.suspects.length}
        <p class="suspects">Mods en cause : {crash.suspects.join(', ')}</p>
      {/if}
      <ul class="hints">
        {#each crash.hints as hint, i (i)}
          <li>{hint}</li>
        {/each}
      </ul>
    {:else}
      <p class="summary plain">
        Réparer vérifie chaque fichier du pack et remet ceux qui manquent ou ont été abîmés. Si ça ne suffit pas, envoie
        ton log : tu obtiens un lien à coller sur le Discord.
      </p>
    {/if}

    {#if crash?.extraMods.length}
      <label class="extra">
        <input type="checkbox" bind:checked={disableExtra} />
        Ranger aussi mes mods perso ({crash.extraMods.length}) dans mods-desactives
        <span title={crash.extraMods.join('\n')}>?</span>
      </label>
    {/if}

    {#if busy === 'repair' && progress}
      <div class="bar"><span style:width="{Math.round((progress.bytesDone / Math.max(1, progress.bytesTotal)) * 100)}%"></span></div>
      <p class="status">{progress.index} / {progress.total} · {megabytes(progress.bytesDone)}</p>
    {:else if link}
      <p class="status ok">
        {copied ? 'Lien copié : colle-le sur le Discord.' : 'Copie ce lien sur le Discord :'}
        <code>{link}</code>
      </p>
    {:else if message}
      <p class="status ok">{message}</p>
    {/if}
    {#if error}<p class="status warn">{error}</p>{/if}

    <div class="actions">
      <button class="primary" disabled={!!busy} onclick={fix}>
        {busy === 'repair' ? 'Réparation…' : 'Réparer le pack'}
      </button>
      <button disabled={!!busy} onclick={() => share(crash?.path ?? null)}>
        {busy === 'share' ? 'Envoi…' : crash ? 'Envoyer le rapport' : 'Envoyer mon log'}
      </button>
      {#if crash && crash.kind === 'game'}
        <button disabled={!!busy} onclick={() => share(null)}>Envoyer le log</button>
      {/if}
      <button disabled={!!busy} onclick={() => openGameFolder().catch((e) => (error = String(e)))}>Dossier du jeu</button>
    </div>
    <p class="fine">Les rapports sont publiés sur mclo.gs, qui masque les adresses IP.</p>
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
    display: grid;
    gap: 12px;
    width: min(600px, 100%);
    padding: 20px 22px;
    border: 1px solid rgba(233, 138, 110, 0.3);
    border-radius: 14px;
    background: var(--panel);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    font: 700 24px var(--serif);
    letter-spacing: 0.02em;
    color: var(--warn);
  }

  .when {
    font-size: 12px;
    color: var(--muted);
  }

  .close {
    flex: none;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover:not(:disabled) {
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

  .summary {
    max-height: 92px;
    overflow-y: auto;
    padding: 9px 11px;
    border-radius: 8px;
    background: #070a10;
    font: 12px/1.5 Consolas, monospace;
    color: #c9c3b8;
    user-select: text;
    word-break: break-word;
  }

  .summary.plain {
    font: 13px/1.5 var(--sans);
    background: none;
    padding: 0;
    color: #c9c3b8;
  }

  .suspects {
    font-size: 13px;
    color: var(--brass);
  }

  .hints {
    display: grid;
    gap: 5px;
    padding: 0;
    list-style: none;
  }

  .hints li {
    display: flex;
    gap: 9px;
    font-size: 13px;
    color: #c9c3b8;
  }

  .hints li::before {
    content: '→';
    color: var(--brass);
  }

  .extra {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: #c9c3b8;
    cursor: pointer;
  }

  .extra span {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--panel-2);
    font-size: 11px;
    color: var(--muted);
    cursor: help;
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
  }

  .status {
    font-size: 12px;
    color: var(--muted);
  }

  .status.ok {
    color: var(--ok);
  }

  .status.warn {
    color: var(--warn);
  }

  code {
    display: block;
    margin-top: 4px;
    font: 12px Consolas, monospace;
    color: var(--text);
    user-select: text;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .actions button {
    height: 36px;
    padding: 0 14px;
    border: 1px solid rgba(233, 168, 98, 0.45);
    border-radius: 9px;
    background: none;
    font-size: 13px;
    font-weight: 600;
    color: var(--brass);
    cursor: pointer;
  }

  .actions .primary {
    border: 0;
    color: #24140a;
    background: linear-gradient(180deg, #f4c283, var(--brass) 45%, var(--brass-deep));
  }

  .actions button:disabled {
    cursor: default;
    filter: saturate(0.3) brightness(0.65);
  }

  .fine {
    font-size: 11px;
    color: var(--muted);
  }
</style>
