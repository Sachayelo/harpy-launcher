<script lang="ts">
  import { onMount } from 'svelte'
  import TitleBar from './lib/TitleBar.svelte'
  import Hero from './lib/Hero.svelte'
  import News from './lib/News.svelte'
  import PlayPanel from './lib/PlayPanel.svelte'
  import StatusBar from './lib/StatusBar.svelte'
  import Workshop from './lib/Workshop.svelte'
  import UpdateOverlay from './lib/UpdateOverlay.svelte'
  import { listen } from '@tauri-apps/api/event'
  import {
    checkUpdate,
    findLunar,
    getPackStatus,
    getSettings,
    installUpdate,
    setDevChannel,
    setDeveloper,
    type LauncherSettings,
    type PackStatus,
    type UpdateProgress,
  } from './lib/api'

  let status = $state<PackStatus | null>(null)
  let statusError = $state('')
  let lunarFound = $state<boolean | null>(null)
  let settings = $state<LauncherSettings | null>(null)
  let view = $state<'play' | 'workshop'>('play')
  let toast = $state('')
  let toastTimer: ReturnType<typeof setTimeout> | undefined
  let request = 0

  // Launcher updates are mandatory and automatic; they only wait for the
  // player to be done installing or launching, and for the workshop to close.
  let busy = $state(false)
  let pendingUpdate = $state<string | null>(null)
  let update = $state<{ version: string; progress: UpdateProgress | null } | null>(null)

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
    findLunar().then((path) => (lunarFound = path !== null))
    getSettings()
      .then((next) => (settings = next))
      .catch(() => {})
    refresh()

    lookForUpdate()
    const timer = setInterval(lookForUpdate, 30 * 60_000)
    let stop: (() => void) | undefined
    listen<UpdateProgress>('update-progress', (event) => {
      if (update) update.progress = event.payload
    })
      .then((unlisten) => (stop = unlisten))
      .catch(() => {})
    return () => {
      clearInterval(timer)
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
  {#if view === 'workshop'}
    <div class="workshop-area">
      <Workshop onpublished={refresh} />
    </div>
  {:else}
    <Hero onsecret={toggleDeveloper} />
    <main class="deck">
      <News {status} />
      <PlayPanel
        {status}
        {statusError}
        {lunarFound}
        developer={settings?.developer ?? false}
        devSelected={settings?.target.channel === 'dev'}
        onselect={selectDev}
        onfinished={refresh}
        onbusy={(next) => (busy = next)}
      />
    </main>
  {/if}
  <StatusBar {lunarFound} />
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

  .workshop-area {
    grid-row: 2 / 4;
    display: grid;
    min-height: 0;
  }

  .deck {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 14px;
    padding: 0 18px 18px;
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
