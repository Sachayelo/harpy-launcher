<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import ChangeEditor from './ChangeEditor.svelte'
  import ReleaseCard from './ReleaseCard.svelte'
  import {
    commitAndPush,
    getLauncherRelease,
    getNewsDraft,
    getPackPreview,
    getPromotePreview,
    getRepositories,
    megabytes,
    mergeChanges,
    promotePack,
    publishLauncher,
    publishNews,
    publishPack,
    stageNewsImage,
    stored,
    type Announcement,
    type Change,
    type LauncherRelease,
    type NewsFile,
    type PackPreview,
    type PromotePreview,
    type Release,
    type Repository,
    type StagedImage,
  } from './api'

  let { onpublished }: { onpublished: () => void } = $props()

  // Dev is for testing: its notes go into a journal that only testers see.
  // Prod is what players get: its patch note starts from that journal.
  let tab = $state<'dev' | 'prod' | 'news'>('dev')

  let preview = $state<PackPreview | null>(null)
  let previewError = $state('')
  let repositories = $state<Repository[]>([])
  let messages = $state<Record<string, string>>({})
  let release = $state<LauncherRelease | null>(null)
  let releaseError = $state('')
  let releaseArmed = $state(false)
  let promoteArmed = $state(false)
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let running = $state('')
  let logOwner = $state('')
  let log = $state<string[]>([])
  let result = $state<{ ok: boolean; text: string } | null>(null)

  const DEV_DRAFT = 'harpy.draft.dev'
  const PROD_DRAFT = 'harpy.draft.prod'
  const blank = (): Change[] => [{ kind: 'new', text: '' }]
  const filled = (changes: Change[]) =>
    changes.map((change) => ({ kind: change.kind, text: change.text.trim() })).filter((change) => change.text)

  function restore<T>(key: string): T | null {
    try {
      return JSON.parse(stored.get(key) ?? 'null')
    } catch {
      return null
    }
  }

  let devNotes = $state<Change[]>(restore<Change[]>(DEV_DRAFT) ?? blank())
  $effect(() => stored.set(DEV_DRAFT, JSON.stringify(devNotes)))

  let promote = $state<PromotePreview | null>(null)
  let promoteError = $state('')
  let prodTitle = $state('')
  let prodImage = $state<StagedImage | null>(null)
  let prodNotes = $state<Change[]>(blank())
  let showPreview = $state(false)
  $effect(() => {
    if (!promote) return
    stored.set(
      PROD_DRAFT,
      JSON.stringify({ devVersion: promote.devVersion, title: prodTitle, image: prodImage, notes: prodNotes }),
    )
  })

  let newsDraft = $state<NewsFile | null>(null)
  let newsError = $state('')
  let twitch = $state('')

  const groups = $derived(
    preview && !preview.nothing
      ? [
          { title: 'Ajoutés', files: preview.added, kind: 'added' },
          { title: 'Modifiés', files: preview.updated, kind: 'updated' },
          { title: 'Retirés', files: preview.removed, kind: 'removed' },
        ].filter((group) => group.files.length)
      : [],
  )

  const pendingVersions = $derived(promote?.pending.map((release) => release.version) ?? [])

  const playerPreview = $derived<Release | null>(
    promote
      ? {
          version: promote.devVersion,
          published: new Date().toISOString(),
          title: prodTitle.trim() || null,
          image: prodImage?.preview ?? null,
          changes: filled(prodNotes),
        }
      : null,
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

  function journalNotes() {
    const merged = mergeChanges(promote?.pending ?? [])
    return merged.length ? merged : blank()
  }

  async function loadPromote() {
    promoteError = ''
    try {
      promote = await getPromotePreview()
    } catch (error) {
      promoteError = String(error)
      return
    }
    // A note being written for this same dev version is kept.
    const draft = restore<{ devVersion: string; title: string; image: StagedImage | null; notes: Change[] }>(PROD_DRAFT)
    if (draft && draft.devVersion === promote.devVersion) {
      prodTitle = draft.title
      prodImage = draft.image
      prodNotes = draft.notes.length ? draft.notes : journalNotes()
    } else {
      prodTitle = ''
      prodImage = null
      prodNotes = journalNotes()
    }
  }

  async function loadNews() {
    newsError = ''
    try {
      newsDraft = await getNewsDraft()
      twitch = newsDraft.twitch.join(', ')
    } catch (error) {
      newsError = String(error)
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

  function refreshPack() {
    loadPreview()
    loadPromote()
  }

  onMount(() => {
    loadPreview()
    loadPromote()
    loadNews()
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
      const version = preview?.version
      await publishPack({ title: '', image: '', changes: filled(devNotes) })
      devNotes = blank()
      onpublished()
      await Promise.all([loadPreview(), loadPromote()])
      return `Version ${version ?? ''} publiée sur dev. Ses notes attendent dans le journal du passage en prod.`
    })

  // Every player gets it at their next launch: the first click only arms the button.
  function arm(which: 'launcher' | 'promote') {
    clearTimeout(armTimer)
    releaseArmed = which === 'launcher'
    promoteArmed = which === 'promote'
    armTimer = setTimeout(() => {
      releaseArmed = false
      promoteArmed = false
    }, 4000)
  }

  function disarm() {
    clearTimeout(armTimer)
    releaseArmed = false
    promoteArmed = false
  }

  function sendToProd() {
    const version = promote?.devVersion
    if (!version) return
    if (!promoteArmed) return arm('promote')
    disarm()
    act('promote', async () => {
      await promotePack({ title: prodTitle.trim(), image: prodImage?.url ?? '', changes: filled(prodNotes) })
      stored.set(PROD_DRAFT, 'null')
      onpublished()
      await Promise.all([loadPreview(), loadPromote()])
      return `Version ${version} envoyée en prod : les joueurs la reçoivent à leur prochain lancement, avec sa note.`
    })
  }

  async function pickImage() {
    try {
      const staged = await stageNewsImage()
      if (staged) prodImage = staged
    } catch (error) {
      result = { ok: false, text: String(error) }
    }
  }

  function addAnnouncement() {
    if (!newsDraft) return
    newsDraft.announcements.push({ id: '', kind: 'event', title: '', text: '', until: null, audience: 'all', url: null })
  }

  function removeAnnouncement(item: Announcement) {
    if (!newsDraft) return
    newsDraft.announcements = newsDraft.announcements.filter((other) => other !== item)
  }

  const sendNews = () =>
    act('news', async () => {
      if (!newsDraft) return ''
      const channels = twitch
        .split(/[\s,;]+/)
        .map((channel) => channel.trim())
        .filter(Boolean)
      await publishNews({
        twitch: channels,
        announcements: newsDraft.announcements.map((item) => ({
          ...item,
          until: item.until || null,
          url: item.url?.trim() || null,
        })),
      })
      await loadNews()
      return 'Annonces publiées : les launchers les affichent dans les minutes qui viennent.'
    })

  function releaseLauncher() {
    const version = release?.next
    if (!version) return
    if (!releaseArmed) return arm('launcher')
    disarm()
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
      <span class="pipeline">
        <span class="stage dev">dev <b>{preview?.devVersion ?? '…'}</b></span>
        <svg viewBox="0 0 16 8"><path d="M0 4h14M11 1l3 3-3 3" /></svg>
        <span class="stage prod">prod <b>{preview?.prodVersion ?? '…'}</b></span>
      </span>
      <button class="link" disabled={!!running} onclick={refreshPack}>Actualiser</button>
    </header>

    <nav class="tabs">
      <button class:active={tab === 'dev'} onclick={() => (tab = 'dev')}>1 · Tester sur dev</button>
      <button class:active={tab === 'prod'} onclick={() => (tab = 'prod')}>
        2 · Sortir en prod
        {#if pendingVersions.length}<i>{pendingVersions.length}</i>{/if}
      </button>
      <button class:active={tab === 'news'} onclick={() => (tab = 'news')}>Annonces</button>
    </nav>

    <div class="scroll">
      {#if tab === 'dev'}
        <p class="explain">
          Seuls les testeurs (mode développeur) reçoivent dev. Tes notes vont dans le journal : tu les reprends au
          passage en prod, les joueurs ne les voient pas avant.
        </p>
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
          <p class="label">Notes de cette version</p>
          <ChangeEditor bind:changes={devNotes} placeholder="Ex. Nouveau bundle Overwatch (Twitch T2)" />
        {/if}
      {:else if tab === 'prod'}
        {#if promoteError}
          <p class="warn">{promoteError}</p>
        {:else if !promote}
          <p class="muted">Lecture du journal…</p>
        {:else if promote.nothing}
          <p class="muted">La prod est déjà en {promote.devVersion}. Publie sur dev d'abord.</p>
        {:else}
          <p class="explain">
            Ce que tous les joueurs liront en ouvrant le launcher. Le journal de
            {pendingVersions.length > 1 ? `${pendingVersions.length} versions dev` : 'la version dev'}
            ({pendingVersions.join(', ')}) est déjà repris : trie, reformule, regroupe.
          </p>
          <div class="fields">
            <input bind:value={prodTitle} placeholder="Titre de la mise à jour (ex. Swag & Flex Update)" />
            {#if prodImage}
              <span class="thumb">
                <img src={prodImage.preview} alt="" />
                <button class="link" onclick={() => (prodImage = null)}>Retirer</button>
              </span>
            {:else}
              <button class="ghost" onclick={pickImage}>+ Image</button>
            {/if}
          </div>
          <div class="label row">
            Notes des joueurs
            <button class="link" onclick={() => (prodNotes = journalNotes())}>Reprendre le journal</button>
            <button class="link" onclick={() => (showPreview = !showPreview)}>
              {showPreview ? 'Modifier' : 'Aperçu joueur'}
            </button>
          </div>
          {#if showPreview && playerPreview}
            <div class="player-view">
              <ReleaseCard release={playerPreview} current />
            </div>
          {:else}
            <ChangeEditor bind:changes={prodNotes} placeholder="Ce que les joueurs vont découvrir" />
          {/if}
        {/if}
      {:else}
        {#if newsError}
          <p class="warn">{newsError}</p>
        {:else if !newsDraft}
          <p class="muted">Lecture des annonces…</p>
        {:else}
          <p class="explain">
            Bannières en haut du launcher : maintenance, événement, fin d'un bundle… Une date de fin les retire toute
            seule. Les chaînes Twitch s'affichent quand elles sont en live.
          </p>
          <input bind:value={twitch} placeholder="Chaînes Twitch, séparées par des virgules" />
          {#each newsDraft.announcements as item, i (i)}
            <div class="announcement">
              <div class="line-fields">
                <select bind:value={item.kind}>
                  <option value="event">Événement</option>
                  <option value="info">Info</option>
                  <option value="maintenance">Maintenance</option>
                </select>
                <input bind:value={item.title} placeholder="Titre" />
                <button class="remove" aria-label="Supprimer l'annonce" onclick={() => removeAnnouncement(item)}>
                  <svg viewBox="0 0 10 10"><path d="M2 2l6 6M8 2l-6 6" /></svg>
                </button>
              </div>
              <input bind:value={item.text} placeholder="Texte (une phrase)" />
              <div class="line-fields">
                <select bind:value={item.audience}>
                  <option value="all">Tout le monde</option>
                  <option value="prod">Joueurs (prod)</option>
                  <option value="dev">Testeurs (dev)</option>
                </select>
                <input type="date" bind:value={item.until} title="Dernier jour d'affichage" />
                <input bind:value={item.url} placeholder="Lien https:// (facultatif)" />
              </div>
            </div>
          {/each}
          <button class="ghost" onclick={addAnnouncement}>+ Annonce</button>
        {/if}
      {/if}

      {#if log.length && logOwner !== 'launcher'}
        <pre class="log">{log.join('\n')}</pre>
      {/if}
    </div>

    <div class="actions">
      {#if tab === 'dev'}
        <button class="primary" disabled={!preview || preview.nothing || !!running} onclick={publish}>
          {running === 'publish' ? 'Publication…' : `Publier ${preview?.version ?? ''} sur dev`}
        </button>
      {:else if tab === 'prod'}
        <button
          class="primary"
          class:armed={promoteArmed}
          disabled={!promote || promote.nothing || !!running}
          onclick={sendToProd}
        >
          {#if running === 'promote'}
            Envoi…
          {:else if promote?.nothing}
            Prod à jour
          {:else if promoteArmed}
            Confirmer : envoyer à tous les joueurs
          {:else}
            Passer {promote?.devVersion ?? ''} en prod
          {/if}
        </button>
      {:else}
        <button class="primary" disabled={!newsDraft || !!running} onclick={sendNews}>
          {running === 'news' ? 'Publication…' : 'Publier les annonces'}
        </button>
      {/if}
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

  input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #0a0e16;
    font: 13px var(--sans);
    color: var(--text);
    user-select: text;
  }

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

  .pipeline {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--muted);
  }

  .pipeline svg {
    width: 16px;
    height: 8px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
  }

  .stage b {
    font-weight: 600;
  }

  .stage.dev b {
    color: #8cb8ef;
  }

  .stage.prod b {
    color: var(--brass);
  }

  .tabs {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: #0a0e16;
  }

  .tabs button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 5px 12px;
    border: 0;
    border-radius: 7px;
    background: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--muted);
    cursor: pointer;
  }

  .tabs button.active {
    background: var(--panel-2);
    color: var(--brass);
    box-shadow: inset 0 0 0 1px rgba(233, 168, 98, 0.25);
  }

  .tabs i {
    min-width: 17px;
    padding: 0 5px;
    border-radius: 999px;
    background: rgba(233, 168, 98, 0.18);
    font-size: 11px;
    font-style: normal;
    font-weight: 600;
    color: var(--brass);
  }

  .explain {
    margin-bottom: 8px;
    font-size: 12px;
    color: var(--muted);
  }

  .label {
    margin: 10px 0 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #c9c3b8;
  }

  .label.row {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .label.row .link {
    margin-left: 0;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
  }

  .label.row .link:first-of-type {
    margin-left: auto;
  }

  .fields {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .thumb {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .thumb img {
    width: 56px;
    height: 34px;
    object-fit: cover;
    border-radius: 5px;
  }

  .thumb .link {
    margin-left: 0;
  }

  .ghost {
    flex: none;
    justify-self: start;
    padding: 6px 12px;
    border: 1px dashed rgba(233, 168, 98, 0.4);
    border-radius: 8px;
    background: none;
    font-size: 12px;
    color: var(--brass);
    cursor: pointer;
  }

  .player-view {
    padding: 12px 14px;
    border: 1px solid rgba(233, 168, 98, 0.2);
    border-radius: 10px;
    background: var(--panel-2);
  }

  .announcement {
    display: grid;
    gap: 6px;
    margin: 10px 0;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: rgba(255, 255, 255, 0.015);
  }

  .line-fields {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .announcement input,
  .announcement select,
  .pack > .scroll > input {
    padding: 6px 9px;
    font-size: 12px;
  }

  select {
    flex: none;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: #0a0e16;
    font: 12px var(--sans);
    color: var(--text);
  }

  input[type='date'] {
    flex: none;
    width: 140px;
    color-scheme: dark;
  }

  .remove {
    flex: none;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .remove:hover {
    color: var(--warn);
  }

  .remove svg {
    width: 9px;
    height: 9px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }

  .primary.armed {
    background: linear-gradient(180deg, #f6a58b, var(--warn) 50%, #c0644a);
  }
</style>
