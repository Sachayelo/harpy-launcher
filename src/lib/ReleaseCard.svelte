<script lang="ts">
  import { CHANGE_KINDS, longDate, type Release } from './api'

  // One version's patch note, exactly as players read it (also the workshop's preview).
  let {
    release,
    compact = false,
    current = false,
  }: { release: Release; compact?: boolean; current?: boolean } = $props()

  const sections = $derived(
    CHANGE_KINDS.map((kind) => ({
      ...kind,
      items: release.changes.filter((change) => change.kind === kind.kind && change.text.trim()),
    })).filter((section) => section.items.length),
  )
</script>

<article class="release" class:compact>
  {#if release.image && !compact}
    <img class="cover" src={release.image} alt="" />
  {/if}
  <header>
    {#if release.title}
      <h3>{release.title}</h3>
    {/if}
    <p class="meta">
      {#if current}<span class="now">Version actuelle</span>{/if}
      {release.version} · {longDate(release.published)}
    </p>
  </header>
  {#if sections.length}
    {#each sections as section (section.kind)}
      <section class="kind {section.kind}">
        <h4>{section.title}</h4>
        <ul>
          {#each section.items as change, i (i)}
            <li>{change.text}</li>
          {/each}
        </ul>
      </section>
    {/each}
  {:else}
    <p class="empty">Corrections et améliorations.</p>
  {/if}
</article>

<style>
  .release {
    display: grid;
    gap: 10px;
  }

  .cover {
    width: 100%;
    max-height: 170px;
    object-fit: cover;
    border-radius: 8px;
    border: 1px solid var(--line);
  }

  h3 {
    font: 700 21px var(--serif);
    letter-spacing: 0.02em;
    color: var(--text);
  }

  .compact h3 {
    font-size: 17px;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .now {
    padding: 1px 7px;
    border-radius: 999px;
    background: rgba(233, 168, 98, 0.12);
    font-size: 11px;
    color: var(--brass);
  }

  .kind {
    --tint: var(--brass);
  }

  .kind.balance {
    --tint: #8cb8ef;
  }

  .kind.fix {
    --tint: var(--ok);
  }

  h4 {
    margin-bottom: 5px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--tint);
  }

  ul {
    display: grid;
    gap: 5px;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    gap: 10px;
    color: #c9c3b8;
    user-select: text;
  }

  .compact li {
    font-size: 13px;
  }

  li::before {
    content: '';
    flex: none;
    width: 5px;
    height: 5px;
    margin-top: 8px;
    border-radius: 50%;
    background: var(--tint);
  }

  .empty {
    color: var(--muted);
  }
</style>
