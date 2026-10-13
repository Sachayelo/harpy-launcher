<script lang="ts">
  import { openLink, stored, type News } from './api'

  let { news }: { news: News | null } = $props()

  const DISMISSED = 'harpy.dismissedAnnouncements'

  function remembered(): string[] {
    try {
      const list = JSON.parse(stored.get(DISMISSED) ?? '[]')
      return Array.isArray(list) ? list : []
    } catch {
      return []
    }
  }

  let dismissed = $state<string[]>(remembered())

  // Maintenance first: it is the one that keeps players from playing.
  const order = { maintenance: 0, event: 1, info: 2 }
  // Two banners at most, so the logo stays visible.
  const live = $derived(news?.live.slice(0, 1) ?? [])
  const visible = $derived(
    (news?.announcements ?? [])
      .filter((item) => !dismissed.includes(item.id))
      .sort((a, b) => order[a.kind] - order[b.kind])
      .slice(0, 2 - live.length),
  )

  function dismiss(id: string) {
    dismissed = [...dismissed, id].slice(-50)
    stored.set(DISMISSED, JSON.stringify(dismissed))
  }

  const open = (url: string) => openLink(url).catch(() => {})
</script>

{#if live.length || visible.length}
  <div class="banners">
    {#each live as stream (stream.channel)}
      <button class="banner live" onclick={() => open(stream.url)}>
        <img src={stream.preview} alt="" />
        <span class="text">
          <b><i class="dot"></i>{stream.channel} est en live</b>
          <small>Regarder sur Twitch</small>
        </span>
      </button>
    {/each}
    {#each visible as item (item.id)}
      <div class="banner {item.kind}">
        {#if item.image}<img src={item.image} alt="" />{/if}
        <span class="text">
          <b>{item.title}</b>
          {#if item.text}<small>{item.text}</small>{/if}
        </span>
        {#if item.url}
          <button class="action" onclick={() => open(item.url!)}>Voir</button>
        {/if}
        {#if item.kind !== 'maintenance'}
          <button class="close" aria-label="Masquer" onclick={() => dismiss(item.id)}>
            <svg viewBox="0 0 10 10"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7" /></svg>
          </button>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .banners {
    position: absolute;
    top: 12px;
    left: 18px;
    right: 18px;
    z-index: 3;
    display: grid;
    justify-items: start;
    gap: 6px;
    pointer-events: none;
  }

  .banner {
    --tint: var(--brass);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 560px;
    padding: 6px 8px 6px 6px;
    border: 1px solid color-mix(in srgb, var(--tint) 40%, transparent);
    border-radius: 10px;
    background: rgba(13, 17, 26, 0.88);
    backdrop-filter: blur(6px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    text-align: left;
    pointer-events: auto;
  }

  .banner.info {
    --tint: #8cb8ef;
  }

  .banner.maintenance {
    --tint: var(--warn);
  }

  .banner.live {
    --tint: #a970ff;
    cursor: pointer;
  }

  .banner.live:hover {
    background: rgba(28, 20, 44, 0.92);
  }

  img {
    flex: none;
    width: 64px;
    height: 36px;
    object-fit: cover;
    border-radius: 6px;
  }

  .banner:not(:has(img)) {
    padding-left: 12px;
    border-left: 3px solid var(--tint);
  }

  .text {
    display: grid;
    min-width: 0;
  }

  b {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }

  small {
    overflow: hidden;
    font-size: 12px;
    color: #b9b3a8;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #ff4d4d;
    box-shadow: 0 0 8px #ff4d4d;
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  .action {
    flex: none;
    padding: 4px 10px;
    border: 1px solid color-mix(in srgb, var(--tint) 45%, transparent);
    border-radius: 7px;
    background: none;
    font-size: 12px;
    font-weight: 600;
    color: var(--tint);
    cursor: pointer;
  }

  .close {
    flex: none;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
    color: var(--text);
  }

  .close svg {
    width: 9px;
    height: 9px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.4;
  }
</style>
