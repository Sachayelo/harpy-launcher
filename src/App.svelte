<script lang="ts">
  import { onMount } from 'svelte'
  import TitleBar from './lib/TitleBar.svelte'
  import Hero from './lib/Hero.svelte'
  import News from './lib/News.svelte'
  import PlayPanel from './lib/PlayPanel.svelte'
  import StatusBar from './lib/StatusBar.svelte'
  import Workshop from './lib/Workshop.svelte'
  import Servers from './lib/Servers.svelte'
  import UpdateOverlay from './lib/UpdateOverlay.svelte'
  import Announcements from './lib/Announcements.svelte'
  import PatchNotes from './lib/PatchNotes.svelte'
  import LauncherPicker from './lib/LauncherPicker.svelte'
  import CrashHelp from './lib/CrashHelp.svelte'
  import { listen } from '@tauri-apps/api/event'
  import {
    checkUpdate,
    getCrash,
    getLauncherState,
    getNews,
    getPackStatus,
    getSettings,
    installUpdate,
    setDevChannel,
    setDeveloper,
    setLauncher,
    stored,
    type Crash,
    type LauncherKind,
    type LauncherSettings,
    type LauncherState,
    type News as NewsData,
    type PackStatus,
    type UpdateProgress,
  } from './lib/api'

  let status = $state<PackStatus | null>(null)
  let statusError = $state('')
  let launcher = $state<LauncherState | null>(null)
  let settings = $state<LauncherSettings | null>(null)
  let news = $state<NewsData | null>(null)
  let view = $state<'play' | 'workshop' | 'servers'>('play')
  let toast = $state('')
  let toastTimer: ReturnType<typeof setTimeout> | undefined
  let request = 0

  // At most one panel over the launcher at a time.
  let panel = $state<
    | { kind: 'picker' }
    | { kind: 'notes'; mode: 'whatsNew' | 'history' }
    | { kind: 'help'; crash: Crash | null }
    | null
  >(null)

  // Launcher updates are mandatory and automatic; they only wait for the
  // player to be done installing or launching, and for the workshop to close.
  let busy = $state(false)
  let pendingUpdate = $state<string | null>(null)
  let update = $state<{ version: string; progress: UpdateProgress | null } | null>(null)

  const releases = $derived(status?.release ? [status.release, ...status.history] : [])

  async function lookForUpdate() {
    if (update) return
    try {
      const { available } = await checkUpdate()
      if (available) pendingUpdate = available
    } catch {
      // Offline or GitHub unreachable: try again at the next check.
    }
  }

  async function applyUpdate(version: string) {
    update = { version, progress: null }
    try {
      await installUpdate()
    } catch (error) {
      update = null
      notify(`Mise à jour du launcher impossible : ${error}`)
    }
  }

  $effect(() => {
    if (pendingUpdate && !update && !busy && view === 'play') {
      const version = pendingUpdate
      pendingUpdate = null
      applyUpdate(version)
    }
  })

  async function refresh() {
    const current = ++request
    try {
      const next = await getPackStatus()
      if (current === request) {
        status = next
        statusError = ''
      }
    } catch (error) {
      if (current === request) statusError = String(error)
    }
  }

  // Returning players get the new version's patch note once; first-timers
  // just start remembering.
  $effect(() => {
    const release = status?.release
    if (!release || !status) return
    const key = `harpy.seenVersion.${status.target.channel}`
    const seen = stored.get(key)
    if (seen === release.version) return
    stored.set(key, release.version)
    if (seen && !panel && (release.changes.length || release.title)) {
      panel = { kind: 'notes', mode: 'whatsNew' }
    }
  })

  // While the launcher isn't set up, keep checking: the player is installing
  // or opening it and should not have to restart Harpy Launcher afterwards.
  async function checkLauncher() {
    try {
      const next = await getLauncherState()
      const becameReady = launcher !== null && launcher.setup !== 'ready' && next.setup === 'ready'
      launcher = next
      if (becameReady) refresh()
    } catch {
      // Checked again below.
    }
  }

  $effect(() => {
    if (launcher?.setup === 'ready') return
    const timer = setInterval(checkLauncher, 3000)
    return () => clearInterval(timer)
  })

  function launcherChanged(next: LauncherState) {
    launcher = next
    status = null
    refresh()
  }

  async function pickLauncher(kind: LauncherKind) {
    panel = null
    if (kind === launcher?.kind) return
    try {
      launcherChanged(await setLauncher(kind))
    } catch (error) {
      notify(String(error))
    }
  }

  async function loadNews() {
    try {
      news = await getNews()
    } catch {
      // Announcements are a bonus: keep the last ones.
    }
  }

  async function lookForCrash() {
    if (panel || busy || view !== 'play') return
    try {
      const crash = await getCrash()
      if (crash && !panel) panel = { kind: 'help', crash }
    } catch {
      // Nothing to show.
    }
  }

  function notify(text: string) {
    toast = text
    clearTimeout(toastTimer)
    toastTimer = setTimeout(() => (toast = ''), 2500)
  }

  function reload(next: LauncherSettings) {
    settings = next
    if (!next.admin) view = 'play'
    status = null
    statusError = ''
    refresh()
    checkLauncher()
    loadNews()
  }

  async function toggleDeveloper() {
    try {
      const next = await setDeveloper(!settings?.developer)
      notify(next.developer ? 'Mode développeur activé' : 'Mode développeur désactivé')
      reload(next)
    } catch (error) {
      notify(String(error))
    }
  }

  async function selectDev(dev: boolean) {
    if ((settings?.target.channel === 'dev') === dev) return
    try {
      reload(await setDevChannel(dev))
    } catch (error) {
      notify(String(error))
    }
  }

  onMount(() => {
    checkLauncher()
    getSettings()
      .then((next) => (settings = next))
      .catch(() => {})
    refresh()
    loadNews()
    lookForCrash()

    lookForUpdate()
    const timers = [
      setInterval(lookForUpdate, 30 * 60_000),
      setInterval(loadNews, 3 * 60_000),
      setInterval(lookForCrash, 60_000),
    ]
    // Back from the game: the first thing to know is whether it crashed.
    const focus = () => lookForCrash()
    window.addEventListener('focus', focus)
    let stop: (() => void) | undefined
    listen<UpdateProgress>('update-progress', (event) => {
      if (update) update.progress = event.payload
    })
      .then((unlisten) => (stop = unlisten))
      .catch(() => {})
    return () => {
      timers.forEach(clearInterval)
      window.removeEventListener('focus', focus)
      stop?.()
    }
  })
</script>

<div class="app">
  <TitleBar
    admin={settings?.admin ?? false}
    version={settings?.version ?? ''}
    {view}
    onview={(next) => (view = next)}
  />
  {#if view === 'servers'}
    <div class="workshop-area">
      <Servers />
    </div>
  {:else if view === 'workshop'}
    <div class="workshop-area">
      <Workshop onpublished={refresh} />
    </div>
  {:else}
    <div class="stage">
      <Hero onsecret={toggleDeveloper} />
      <Announcements {news} />
    </div>
    <main class="deck" class:tall={settings?.developer}>
      <News {status} onhistory={() => (panel = { kind: 'notes', mode: 'history' })} />
      <PlayPanel
        {status}
        {statusError}
        {launcher}
        developer={settings?.developer ?? false}
        devSelected={settings?.target.channel === 'dev'}
        onselect={selectDev}
        onfinished={refresh}
        onbusy={(next) => (busy = next)}
        onpicker={() => (panel = { kind: 'picker' })}
        onhelp={() => (panel = { kind: 'help', crash: null })}
        onlauncher={launcherChanged}
      />
    </main>
  {/if}
  <StatusBar {launcher} />
  {#if panel?.kind === 'picker'}
    <LauncherPicker current={launcher?.kind ?? null} onpick={pickLauncher} onclose={() => (panel = null)} />
  {:else if panel?.kind === 'notes' && releases.length}
    <PatchNotes mode={panel.mode} {releases} onclose={() => (panel = null)} />
  {:else if panel?.kind === 'help'}
    <CrashHelp
      crash={panel.crash}
      onclose={(repaired) => {
        panel = null
        if (repaired) refresh()
      }}
    />
  {/if}
  {#if update}
    <UpdateOverlay version={update.version} progress={update.progress} />
  {/if}
  {#if toast}
    <div class="toast">{toast}</div>
  {/if}
</div>

<style>
  .app {
    position: relative;
    height: 100%;
    display: grid;
    grid-template-rows: 38px 1fr auto auto;
  }

  .stage {
    position: relative;
    display: grid;
    min-height: 0;
  }

  .workshop-area {
    grid-row: 2 / 4;
    display: grid;
    min-height: 0;
  }

  .deck {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 14px;
    height: 226px;
    padding: 0 18px 18px;
  }

  /* Room for the Harpy Express / Harpy Dev switch. */
  .deck.tall {
    height: 262px;
  }

  .toast {
    position: absolute;
    top: 52px;
    left: 50%;
    z-index: 10;
    transform: translateX(-50%);
    padding: 7px 14px;
    border: 1px solid rgba(233, 168, 98, 0.3);
    border-radius: 999px;
    background: var(--panel-2);
    font-size: 12px;
    color: var(--brass);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
</style>
