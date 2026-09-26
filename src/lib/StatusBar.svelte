<script lang="ts">
  import { onMount } from 'svelte'
  import { getServerStatus, type LunarState, type ServerStatus } from './api'

  let { lunar }: { lunar: LunarState | null } = $props()

  let server = $state<ServerStatus | null>(null)
  let checked = $state(false)

  onMount(() => {
    const refresh = async () => {
      server = await getServerStatus()
      checked = true
    }
    refresh()
    const timer = setInterval(refresh, 30_000)
    return () => clearInterval(timer)
  })

  const lunarLabels: Record<LunarState, string> = {
    ready: 'Lunar Client détecté',
    neverOpened: 'Lunar Client jamais ouvert',
    missing: 'Lunar Client introuvable',
  }
  const lunarLabel = $derived(lunar ? lunarLabels[lunar] : 'Recherche de Lunar…')

  const serverLabel = $derived(
    !checked
      ? 'Connexion au serveur…'
      : server
        ? `Serveur en ligne · ${server.online} / ${server.max} joueurs`
        : 'Serveur hors ligne',
  )
</script>

<footer class="bar">
  <span class="item" class:ok={lunar === 'ready'} class:off={lunar === 'missing' || lunar === 'neverOpened'}>
    {lunarLabel}
  </span>
  <span
    class="item"
    class:ok={server}
    class:off={checked && !server}
    title={server ? `${server.motd} · ${server.latencyMs} ms` : undefined}
  >
    {serverLabel}
  </span>
</footer>

<style>
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 18px;
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .item::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #3a414e;
  }

  .ok::before {
    background: var(--ok);
    box-shadow: 0 0 8px rgba(127, 212, 154, 0.5);
  }

  .off::before {
    background: var(--warn);
  }
</style>
