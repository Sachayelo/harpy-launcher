<script lang="ts">
  import type { PackStatus } from './api'

  let { status }: { status: PackStatus | null } = $props()

  const date = $derived(
    status?.published
      ? new Date(status.published).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long' })
      : '',
  )
</script>

<section class="card">
  <h2>Nouveautés</h2>
  {#if status?.notes.length}
    <ul>
      {#each status.notes as note, i (i)}
        <li>{note}</li>
      {/each}
    </ul>
  {:else}
    <p class="empty">Les nouveautés de chaque version s'afficheront ici.</p>
  {/if}
  {#if status?.version}
    <p class="date">Version {status.version} · {date}</p>
  {/if}
</section>

<style>
  ul {
    list-style: none;
    padding: 0;
    display: grid;
    gap: 6px;
  }

  li {
    display: flex;
    gap: 10px;
    color: #c9c3b8;
  }

  li::before {
    content: '';
    flex: none;
    width: 5px;
    height: 5px;
    margin-top: 8px;
    border-radius: 50%;
    background: var(--brass-deep);
  }

  .empty {
    color: var(--muted);
  }

  .date {
    margin-top: 10px;
    font-size: 12px;
    color: var(--muted);
  }
</style>
