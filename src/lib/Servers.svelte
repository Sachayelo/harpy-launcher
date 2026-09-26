<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import {
    getServersStatus,
    serverDeploy,
    serverPower,
    serverRollback,
    type ServerInfo,
    type ServerName,
  } from './api'

  type Action = 'deploy' | 'start' | 'stop' | 'restart' | 'rollback'

  const SERVERS: { key: ServerName; name: string; role: string; address: string }[] = [
    { key: 'prod', name: 'Harpy Express', role: 'Les joueurs', address: '195.88.87.173' },
    { key: 'dev', name: 'Harpy Dev', role: 'Tes tests', address: '195.88.87.173:25566' },
  ]

  const STATES: Record<ServerInfo['state'], string> = {
    running: 'En ligne',
    offline: 'Arrêté',
    starting: 'Démarrage…',
    stopping: 'Arrêt…',
    unknown: 'Inconnu',
  }

  const DONE: Record<Action, string> = {
    deploy: 'mods à jour',
    start: 'serveur démarré',
    stop: 'serveur arrêté',
    restart: 'serveur redémarré',
    rollback: "mods d'avant restaurés",
  }

  let status = $state<Record<ServerName, ServerInfo> | null>(null)
  let statusError = $state('')
  let running = $state('')
  let armed = $state('')
  let armTimer: ReturnType<typeof setTimeout> | undefined
  let log = $state<string[]>([])
  let result = $state<{ ok: boolean; text: string } | null>(null)

  async function load() {
    if (running) return
    try {
      status = await getServersStatus()
      statusError = ''
    } catch (error) {
      statusError = String(error)
    }
  }

  onMount(() => {
    load()
    const timer = setInterval(load, 15_000)
    let stop: (() => void) | undefined
    listen<string>('workshop-log', (event) => (log = [...log.slice(-300), event.payload]))
      .then((unlisten) => (stop = unlisten))
      .catch(() => {})
    return () => {
      clearInterval(timer)
      stop?.()
    }
  })

  const nameOf = (server: ServerName) => SERVERS.find((entry) => entry.key === server)?.name ?? server

  // Anything that would kick connected players asks for a second click.
  async function trigger(server: ServerName, action: Action) {
    const id = `${server}:${action}`
    const force = action !== 'start' && !!status?.[server]?.online
    if (force && armed !== id) {
      armed = id
      clearTimeout(armTimer)
      armTimer = setTimeout(() => (armed = ''), 4000)
      return
    }
    armed = ''
    running = id
    log = []
    result = null
    try {
      if (action === 'deploy') await serverDeploy(server, force)
      else if (action === 'rollback') await serverRollback(server, force)
      else await serverPower(server, action, force)
      result = { ok: true, text: `${nameOf(server)} : ${DONE[action]}.` }
    } catch (error) {
      result = { ok: false, text: `${nameOf(server)} : ${error}` }
    } finally {
      running = ''
      load()
    }
  }

  function memory(info: ServerInfo | undefined) {
    if (!info?.memoryMb || !info.memoryLimitMb) return '—'
    const gigabytes = (mb: number) => (mb / 1024).toLocaleString('fr-FR', { maximumFractionDigits: 1 })
    return `${gigabytes(info.memoryMb)} / ${gigabytes(info.memoryLimitMb)} Go`
  }

  function upToDate(info: ServerInfo | undefined) {
    return !!info?.version && info.version === info.packVersion
  }

  function confirmText(info: ServerInfo | undefined) {
    const count = info?.online ?? 0
    return `Confirmer : ${count} joueur${count > 1 ? 's' : ''} déconnecté${count > 1 ? 's' : ''}`
  }
</script>

<div class="servers">
  {#each SERVERS as server (server.key)}
    {@const info = status?.[server.key]}
    {@const busy = running.startsWith(`${server.key}:`)}
    <section class="card server">
      <header>
        <h2>{server.name}</h2>
        <span class="state {info?.state ?? 'unknown'}">{info ? STATES[info.state] : '…'}</span>
      </header>
      <p class="role">{server.role} · {server.address}</p>

      <dl>
        <div>
          <dt>Joueurs</dt>
          <dd>{info?.online != null ? `${info.online} / ${info.max}` : '—'}</dd>
        </div>
        <div>
          <dt>Mémoire</dt>
          <dd>{memory(info)}</dd>
        </div>
        <div>
          <dt>Mods installés</dt>
          <dd>{info?.version ?? 'jamais mis à jour ici'}</dd>
        </div>
        <div>
          <dt>Pack {server.key}</dt>
          <dd>{info?.packVersion ?? '—'}</dd>
        </div>
      </dl>

      <div class="actions">
        <button
          class="primary"
          disabled={!info || !!running || !info.packVersion || upToDate(info)}
          onclick={() => trigger(server.key, 'deploy')}
        >
          {#if running === `${server.key}:deploy`}
            Mise à jour…
          {:else if armed === `${server.key}:deploy`}
            {confirmText(info)}
          {:else if upToDate(info)}
            Mods à jour
          {:else}
            Installer {info?.packVersion ?? ''}
          {/if}
        </button>
        {#if info?.state === 'offline'}
          <button class="secondary" disabled={!!running} onclick={() => trigger(server.key, 'start')}>
            {running === `${server.key}:start` ? 'Démarrage…' : 'Démarrer'}
          </button>
        {:else}
          <button class="secondary" disabled={!info || !!running} onclick={() => trigger(server.key, 'restart')}>
            {running === `${server.key}:restart`
              ? 'Redémarrage…'
              : armed === `${server.key}:restart`
                ? 'Confirmer'
                : 'Redémarrer'}
          </button>
          <button class="secondary" disabled={!info || !!running} onclick={() => trigger(server.key, 'stop')}>
            {running === `${server.key}:stop` ? 'Arrêt…' : armed === `${server.key}:stop` ? 'Confirmer' : 'Arrêter'}
          </button>
        {/if}
      </div>

      <button
        class="link"
        disabled={!info?.backups || !!running}
        onclick={() => trigger(server.key, 'rollback')}
      >
        {#if busy && running.endsWith(':rollback')}
          Retour en arrière…
        {:else if armed === `${server.key}:rollback`}
          {confirmText(info)}
        {:else if info?.backups}
          Revenir aux mods d'avant ({info.backups} sauvegarde{info.backups > 1 ? 's' : ''})
        {:else}
          Aucune sauvegarde pour revenir en arrière
        {/if}
      </button>
    </section>
  {/each}

  <section class="card console">
    {#if statusError}
      <p class="warn">{statusError}</p>
    {:else if log.length}
      <pre>{log.join('\n')}</pre>
    {:else}
      <p class="muted">
        Les mises à jour installent les mods du pack sur le serveur : ses réglages, son monde et ses mods à lui
        (spark…) ne sont pas touchés. Les mods d'avant sont sauvegardés à chaque fois.
      </p>
    {/if}
    {#if result}
      <p class="result" class:error={!result.ok}>{result.text}</p>
    {/if}
  </section>
</div>

<style>
  .servers {
    display: grid;
    grid-template-columns: 1fr 1fr;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 12px 14px;
    min-height: 0;
    padding: 16px 18px 18px;
  }

  .server {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  header h2 {
    margin: 0;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 500;
    background: rgba(143, 151, 166, 0.12);
    color: var(--muted);
  }

  .state::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }

  .state.running {
    background: rgba(127, 212, 154, 0.1);
    color: var(--ok);
  }

  .state.starting,
  .state.stopping {
    background: rgba(233, 168, 98, 0.12);
    color: var(--brass);
  }

  .role {
    margin-top: -6px;
    font-size: 12px;
    color: var(--muted);
  }

  dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 14px;
    margin: 0;
  }

  dt {
    font-size: 11px;
    color: var(--muted);
  }

  dd {
    margin: 0;
    font-size: 13px;
    color: #c9c3b8;
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .actions button {
    height: 38px;
    border-radius: 9px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .primary {
    flex: 1;
    border: 0;
    color: #24140a;
    background: linear-gradient(180deg, #f4c283, var(--brass) 45%, var(--brass-deep));
  }

  .secondary {
    padding: 0 12px;
    border: 1px solid rgba(233, 168, 98, 0.45);
    background: none;
    color: var(--brass);
  }

  .actions button:disabled {
    cursor: default;
    filter: saturate(0.3) brightness(0.65);
  }

  .link {
    align-self: flex-start;
    border: 0;
    padding: 0;
    background: none;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .link:hover:not(:disabled) {
    color: var(--brass);
  }

  .link:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .console {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  pre {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: #070a10;
    font: 11px/1.45 Consolas, monospace;
    color: #9aa3b2;
    white-space: pre-wrap;
    user-select: text;
  }

  .muted {
    font-size: 12px;
    color: var(--muted);
  }

  .warn {
    font-size: 12px;
    color: var(--warn);
  }

  .result {
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
