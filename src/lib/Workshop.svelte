<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import {
    commitAndPush,
    getLauncherRelease,
    getPackPreview,
    getRepositories,
    megabytes,
    promotePack,
    publishLauncher,
    publishPack,
    type LauncherRelease,
    type PackPreview,
    type Repository,
  } from './api'

  let { onpublished }: { onpublished: () => void } = $props()

  let preview = $state<PackPreview | null>(null)
  let previewError = $state('')
  let notes = $state('')
  let repositories = $state<Repository[]>([])
  let messages = $state<Record<string, string>>({})
  let release = $state<LauncherRelease | null>(null)
  let releaseError = $state('')
  let releaseArmed = $state(false)
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let running = $state('')
  let logOwner = $state('')
  let log = $state<string[]>([])
  let result = $state<{ ok: boolean; text: string } | null>(null)

  const groups = $derived(
    preview && !preview.nothing
      ? [
          { title: 'Ajoutés', files: preview.added, kind: 'added' },
          { title: 'Modifiés', files: preview.updated, kind: 'updated' },
          { title: 'Retirés', files: preview.removed, kind: 'removed' },
        ].filter((group) => group.files.length)
      : [],
  )

  const canPromote = $derived(
    !!preview?.devVersion && preview.devVersion !== preview.prodVersion && !running,
  )

  async function loadPreview() {
    preview = null
    previewError = ''
    try {
      preview = await getPackPreview()
    } catch (error) {
      previewError = String(error)
    }
  }

  const launcherUpToDate = $derived(!!release?.online && release.unreleased === 0)

  const releaseBlocker = $derived.by(() => {
    if (!release) return ''
    if (!release.keyFound) return 'Clé de signature introuvable sur ce PC.'
    if (repositories.find((repository) => repository.name === 'harpy-launcher')?.changeCount)
      return "Commit & push le code du launcher d'abord."
    return ''
  })

  async function loadRelease() {
    releaseError = ''
    try {
      release = await getLauncherRelease()
    } catch (error) {
      releaseError = String(error)
    }
  }

  async function loadRepositories() {
    try {
      repositories = await getRepositories()
    } catch (error) {
      result = { ok: false, text: String(error) }
    }
  }

  onMount(() => {
    loadPreview()
    loadRepositories()
    loadRelease()
    let stop: (() => void) | undefined
    listen<string>('workshop-log', (event) => (log = [...log.slice(-300), event.payload]))
      .then((unlisten) => (stop = unlisten))
      .catch(() => {})
    return () => stop?.()
  })

  async function act(label: string, action: () => Promise<string>) {
    running = label
    logOwner = label
    log = []
    result = null
    try {
      result = { ok: true, text: await action() }
    } catch (error) {
      result = { ok: false, text: String(error) }
    } finally {
      running = ''
      loadRepositories()
    }
  }

  const publish = () =>
    act('publish', async () => {
      const lines = notes
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean)
      await publishPack(lines)
      notes = ''
      onpublished()
      await loadPreview()
      return `Version ${preview?.devVersion ?? ''} publiée sur dev.`
    })

  const promote = () =>
    act('promote', async () => {
      const version = preview?.devVersion
      await promotePack()
      onpublished()
      await loadPreview()
      return `Version ${version} envoyée en prod : les joueurs l'auront à leur prochain lancement.`
    })

  // Every player gets it within minutes: the first click only arms the button.
  function releaseLauncher() {
    const version = release?.next
    if (!version) return
    if (!releaseArmed) {
      releaseArmed = true
      clearTimeout(armTimer)
      armTimer = setTimeout(() => (releaseArmed = false), 4000)
      return
    }
    releaseArmed = false
    clearTimeout(armTimer)
    act('launcher', async () => {
      await publishLauncher(version)
      await loadRelease()
      return `Launcher ${version} en ligne : les joueurs l'auront à leur prochain lancement.`
    })
  }

  const save = (repository: Repository) =>
    act(repository.name, async () => {
      const text = await commitAndPush(repository.name, messages[repository.name] ?? '')
      messages[repository.name] = ''
      return text
    })

  function describe(repository: Repository) {
    if (!repository.tracked) return 'pas sauvegardé dans Git'
    const parts = []
    if (repository.changeCount) parts.push(`${repository.changeCount} modification${repository.changeCount > 1 ? 's' : ''}`)
    if (repository.unpushed) parts.push(`${repository.unpushed} commit${repository.unpushed > 1 ? 's' : ''} à envoyer`)
    return parts.length ? parts.join(' · ') : 'à jour'
  }
</script>

<div class="workshop">
  <section class="card pack">
    <header>
      <h2>Pack</h2>
      <span class="versions">
        dev <b>{preview?.devVersion ?? '…'}</b> · prod <b>{preview?.prodVersion ?? '…'}</b>
      </span>
      <button class="link" disabled={!!running} onclick={loadPreview}>Actualiser</button>
    </header>

    <div class="scroll">
      {#if previewError}
        <p class="warn">{previewError}</p>
      {:else if !preview}
        <p class="muted">Analyse du dossier pack…</p>
      {:else if preview.nothing}
        <p class="muted">Rien de nouveau dans pack/ depuis la dernière publication.</p>
      {:else}
        {#each groups as group (group.kind)}
          <p class="group {group.kind}">{group.title} ({group.files.length})</p>
          <ul>
            {#each group.files as file (file)}
              <li>{file.replace(/^mods\//, '')}</li>
            {/each}
          </ul>
        {/each}
        {#if preview.blockedChanged}
          <p class="group removed">Mods bloqués chez les joueurs : {preview.blocked.join(', ') || 'aucun'}</p>
        {/if}
        <p class="meta">
          Version {preview.version} · {preview.uploadFiles} fichier{preview.uploadFiles > 1 ? 's' : ''} à envoyer
          ({megabytes(preview.uploadBytes)})
        </p>
        <textarea bind:value={notes} rows="3" placeholder="Nouveautés pour les joueurs, une par ligne"></textarea>
      {/if}

      {#if log.length && logOwner !== 'launcher'}
        <pre class="log">{log.join('\n')}</pre>
      {/if}
    </div>

    <div class="actions">
      <button class="primary" disabled={!preview || preview.nothing || !!running} onclick={publish}>
        {running === 'publish' ? 'Publication…' : 'Publier sur dev'}
      </button>
      <button class="secondary" disabled={!canPromote} onclick={promote}>
        {#if running === 'promote'}
          Envoi…
        {:else if preview?.devVersion && preview.devVersion === preview.prodVersion}
          Prod à jour
        {:else}
          Passer {preview?.devVersion ?? ''} en prod
        {/if}
      </button>
    </div>
  </section>

  <section class="card launcher">
    <header>
      <h2>Launcher</h2>
      <span class="versions">
        {#if !release}
          …
        {:else if release.online}
          en ligne <b>{release.online}</b>
        {:else}
          jamais publié
        {/if}
      </span>
      <button class="link" disabled={!!running} onclick={loadRelease}>Actualiser</button>
    </header>
    {#if releaseError}
      <p class="small warn">{releaseError}</p>
    {:else if running === 'launcher'}
      <p class="small">{log.at(-1) ?? 'Préparation…'}</p>
    {:else if releaseBlocker}
      <p class="small muted">{releaseBlocker}</p>
    {/if}
    <div class="actions">
      <button
        class="secondary"
        class:armed={releaseArmed}
        disabled={!release?.next || launcherUpToDate || !!releaseBlocker || !!running}
        onclick={releaseLauncher}
      >
        {#if running === 'launcher'}
          Publication…
        {:else if launcherUpToDate}
          Launcher à jour
        {:else if releaseArmed}
          Confirmer : publier {release?.next}
        {:else}
          Publier la version {release?.next ?? ''}
        {/if}
      </button>
    </div>
  </section>

  <section class="card code">
    <header>
      <h2>Code</h2>
      <button class="link" disabled={!!running} onclick={loadRepositories}>Actualiser</button>
    </header>

    <ul class="repositories scroll">
      {#each repositories as repository (repository.name)}
        <li>
          <div class="line">
            <b>{repository.name}</b>
            <span class:dirty={repository.changeCount || repository.unpushed} class:untracked={!repository.tracked}>
              {describe(repository)}
            </span>
          </div>
          {#if repository.tracked && (repository.changeCount || repository.unpushed)}
            <div class="commit">
              {#if repository.changeCount}
                <input bind:value={messages[repository.name]} placeholder="Ce que tu as changé" />
              {/if}
              <button disabled={!!running} onclick={() => save(repository)}>
                {running === repository.name ? '…' : repository.owned ? 'Commit & push' : 'Commit'}
              </button>
            </div>
          {/if}
          {#if repository.tracked && !repository.owned}
            <p class="note">Dépôt de l'auteur d'origine : sauvegarde sur ton PC uniquement.</p>
          {/if}
        </li>
      {/each}
    </ul>
  </section>

  {#if result}
    <p class="result" class:error={!result.ok}>{result.text}</p>
  {/if}
</div>

<style>
  .workshop {
    display: grid;
    grid-template-columns: 1.25fr 1fr;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 12px 14px;
    min-height: 0;
    padding: 16px 18px 18px;
  }

  .pack {
    grid-row: 1 / 3;
  }

  .launcher,
  .code {
    grid-column: 2;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  header h2 {
    margin: 0;
  }

  .versions {
    font-size: 12px;
    color: var(--muted);
  }

  .versions b {
    font-weight: 500;
    color: #c9c3b8;
  }

  .link {
    margin-left: auto;
    border: 0;
    background: none;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .link:hover:not(:disabled) {
    color: var(--brass);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .muted {
    color: var(--muted);
  }

  .warn {
    color: var(--warn);
  }

  .group {
    margin-top: 6px;
    font-size: 12px;
    font-weight: 600;
  }

  .group.added {
    color: var(--ok);
  }

  .group.updated {
    color: var(--brass);
  }

  .group.removed {
    color: var(--warn);
  }

  ul {
    padding: 0;
    list-style: none;
  }

  .pack ul li {
    padding-left: 12px;
    font-size: 12px;
    color: #c9c3b8;
  }

  .meta {
    margin: 10px 0 8px;
    font-size: 12px;
    color: var(--muted);
  }

  textarea,
  input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #0a0e16;
    font: 13px var(--sans);
    color: var(--text);
    resize: none;
    user-select: text;
  }

  textarea:focus,
  input:focus {
    outline: none;
    border-color: rgba(233, 168, 98, 0.45);
  }

  .log {
    margin-top: 10px;
    max-height: 110px;
    overflow-y: auto;
    padding: 8px 10px;
    border-radius: 8px;
    background: #070a10;
    font: 11px/1.45 Consolas, monospace;
    color: #9aa3b2;
    white-space: pre-wrap;
    user-select: text;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .actions button {
    flex: 1;
    height: 40px;
    border-radius: 9px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .primary {
    border: 0;
    color: #24140a;
    background: linear-gradient(180deg, #f4c283, var(--brass) 45%, var(--brass-deep));
  }

  .secondary {
    border: 1px solid rgba(233, 168, 98, 0.45);
    background: none;
    color: var(--brass);
  }

  .secondary.armed {
    background: rgba(233, 168, 98, 0.14);
  }

  .small {
    overflow: hidden;
    font-size: 12px;
    color: #c9c3b8;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small.muted {
    color: var(--muted);
  }

  .small.warn {
    color: var(--warn);
    white-space: normal;
  }

  .actions button:disabled {
    cursor: default;
    filter: saturate(0.3) brightness(0.65);
  }

  .repositories {
    display: grid;
    align-content: start;
    gap: 10px;
  }

  .repositories li {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--line);
  }

  .repositories li:last-child {
    border-bottom: 0;
  }

  .line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 13px;
  }

  .line b {
    font-weight: 500;
    color: #c9c3b8;
  }

  .line span {
    font-size: 12px;
    color: var(--ok);
  }

  .line span.dirty {
    color: var(--brass);
  }

  .line span.untracked {
    color: var(--warn);
  }

  .commit {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }

  .commit input {
    padding: 6px 9px;
    font-size: 12px;
  }

  .commit button {
    flex: none;
    padding: 0 12px;
    border: 1px solid rgba(233, 168, 98, 0.45);
    border-radius: 8px;
    background: none;
    font-size: 12px;
    font-weight: 600;
    color: var(--brass);
    cursor: pointer;
  }

  .commit button:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .note {
    margin-top: 4px;
    font-size: 11px;
    color: var(--muted);
  }

  .result {
    grid-column: 1 / -1;
    padding: 8px 12px;
    border: 1px solid rgba(127, 212, 154, 0.3);
    border-radius: 8px;
    background: var(--panel-2);
    font-size: 12px;
    color: var(--ok);
    user-select: text;
  }

  .result.error {
    border-color: rgba(233, 138, 110, 0.35);
    color: var(--warn);
  }
</style>
