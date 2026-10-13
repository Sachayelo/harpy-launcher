<script lang="ts">
  import { onMount } from 'svelte'
  import { getServerStatus, type LauncherState, type ServerStatus } from './api'

  let { launcher }: { launcher: LauncherState | null } = $props()

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

  const launcherLabel = $derived.by(() => {
    if (!launcher) return 'Recherche du launcher…'
    const custom = launcher.kind === 'custom'
    switch (launcher.setup) {
      case 'ready':
        return custom ? 'Dossier du jeu choisi' : `${launcher.name} détecté`
      case 'neverOpened':
        return `${launcher.name} jamais ouvert`
      case 'needsProfile':
        return custom ? 'Aucun dossier choisi' : `Profil à importer dans ${launcher.name}`
      default:
        return `${launcher.name} introuvable`
    }
  })
  const ready = $derived(launcher?.setup === 'ready')

  const serverLabel = $derived(
    !checked
      ? 'Connexion au serveur…'
      : server
        ? `Serveur en ligne · ${server.online} / ${server.max} joueurs`
        : 'Serveur hors ligne',
  )
</script>

<footer class="bar">
  <span class="item" class:ok={ready} class:off={!!launcher && !ready} title={launcher?.gameDir ?? undefined}>
    {launcherLabel}
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
