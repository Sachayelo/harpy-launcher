<script lang="ts">
  import ReleaseCard from './ReleaseCard.svelte'
  import type { PackStatus } from './api'

  let { status, onhistory }: { status: PackStatus | null; onhistory: () => void } = $props()

  const dev = $derived(status?.target.channel === 'dev')
</script>

<section class="card news">
  <header>
    <h2>Nouveautés</h2>
    {#if dev}
      <span class="badge" title="Ce que tu lis là, seuls les testeurs le voient">Journal dev</span>
    {/if}
    {#if status?.history.length}
      <button class="link" onclick={onhistory}>Historique</button>
    {/if}
  </header>
  <div class="scroll">
    {#if status?.release}
      <ReleaseCard release={status.release} compact />
    {:else}
      <p class="empty">Les nouveautés de chaque version s'afficheront ici.</p>
    {/if}
  </div>
</section>

<style>
  .news {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 8px;
  }

  header h2 {
    margin: 0;
  }

  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    background: rgba(140, 184, 239, 0.12);
    font-size: 11px;
    font-weight: 600;
    color: #8cb8ef;
  }

  .link {
    margin-left: auto;
    border: 0;
    background: none;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .link:hover {
    color: var(--brass);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding-right: 4px;
  }

  .empty {
    color: var(--muted);
  }
</style>
