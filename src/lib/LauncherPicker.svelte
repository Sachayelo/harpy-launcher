<script lang="ts">
  import { onMount } from 'svelte'
  import { getLaunchers, type LauncherInfo, type LauncherKind } from './api'

  let {
    current,
    onpick,
    onclose,
  }: { current: LauncherKind | null; onpick: (kind: LauncherKind) => void; onclose: () => void } = $props()

  let launchers = $state<LauncherInfo[]>([])

  const details: Record<LauncherKind, { mark: string; line: string }> = {
    lunar: { mark: 'L', line: 'Recommandé · le pack se met à jour tout seul' },
    official: { mark: 'M', line: 'Le launcher de Mojang · un profil Harpy Express est créé' },
    prism: { mark: 'P', line: 'Aussi MultiMC · le pack se met à jour tout seul' },
    modrinth: { mark: 'Mr', line: 'Un profil à importer en un clic' },
    curseforge: { mark: 'CF', line: 'Un profil à importer depuis CurseForge' },
    custom: { mark: '…', line: 'ATLauncher, GDLauncher… : choisis le dossier du jeu' },
  }

  onMount(() => {
    getLaunchers()
      .then((list) => (launchers = list))
      .catch(() => {})
  })

  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') onclose()
  }
</script>

<svelte:window onkeydown={key} />

<div class="overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <div class="panel" role="dialog" aria-modal="true" aria-label="Choisir son launcher">
    <header>
      <h2>Avec quoi tu joues ?</h2>
      <button class="close" aria-label="Fermer" onclick={onclose}>
        <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
      </button>
    </header>
    <p class="intro">Le launcher installe le pack dans le launcher que tu utilises déjà.</p>
    <div class="grid">
      {#each launchers as launcher (launcher.kind)}
        <button class="choice" class:active={launcher.kind === current} onclick={() => onpick(launcher.kind)}>
          <span class="mark {launcher.kind}">{details[launcher.kind].mark}</span>
          <span class="text">
            <b>
              {launcher.name}
              {#if launcher.installed && launcher.kind !== 'custom'}<i>installé</i>{/if}
            </b>
            <small>{details[launcher.kind].line}</small>
          </span>
        </button>
      {/each}
    </div>
    <p class="note">
      Badlion Client n'accepte pas les mods Fabric : prends Lunar Client, avec le même compte Minecraft.
    </p>
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
    width: min(640px, 100%);
    padding: 20px 22px;
    border: 1px solid rgba(233, 168, 98, 0.22);
    border-radius: 14px;
    background: var(--panel);
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  h2 {
    font: 700 24px var(--serif);
    letter-spacing: 0.02em;
    color: var(--brass);
  }

  .close {
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
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

  .intro,
  .note {
    font-size: 13px;
    color: var(--muted);
  }

  .note {
    font-size: 12px;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel-2);
    text-align: left;
    cursor: pointer;
    transition:
      border-color 0.12s ease,
      background 0.12s ease;
  }

  .choice:hover {
    border-color: rgba(233, 168, 98, 0.35);
  }

  .choice.active {
    border-color: rgba(233, 168, 98, 0.6);
    background: rgba(233, 168, 98, 0.08);
  }

  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    border-radius: 9px;
    background: #222a3a;
    font: 700 15px var(--serif);
    color: var(--brass);
  }

  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  b {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
  }

  i {
    padding: 0 7px;
    border-radius: 999px;
    background: rgba(127, 212, 154, 0.1);
    font-size: 11px;
    font-style: normal;
    font-weight: 500;
    color: var(--ok);
  }

  small {
    font-size: 12px;
    color: var(--muted);
  }
</style>
