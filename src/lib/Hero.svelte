<script lang="ts">
  import banner from '../assets/featured_image.png'

  let { onsecret }: { onsecret: () => void } = $props()

  let taps: number[] = []

  function tap() {
    const now = Date.now()
    taps = [...taps.filter((time) => now - time < 2500), now]
    if (taps.length >= 5) {
      taps = []
      onsecret()
    }
  }

  const flakes = Array.from({ length: 40 }, () => ({
    left: Math.random() * 100,
    size: 1.5 + Math.random() * 2.5,
    opacity: 0.3 + Math.random() * 0.5,
    drift: -30 + Math.random() * 60,
    duration: 6 + Math.random() * 8,
    delay: -Math.random() * 14,
  }))
</script>

<section class="hero">
  <div class="ambient" style:background-image="url({banner})"></div>
  <div class="snow">
    {#each flakes as flake, i (i)}
      <i
        style:left="{flake.left}%"
        style:width="{flake.size}px"
        style:height="{flake.size}px"
        style:opacity={flake.opacity}
        style:--drift="{flake.drift}px"
        style:animation-duration="{flake.duration}s"
        style:animation-delay="{flake.delay}s"
      ></i>
    {/each}
  </div>
  <!-- Hidden on purpose: five quick taps on the logo toggle developer mode. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <img class="logo" src={banner} alt="The Last Voyage of the Harpy Express" onclick={tap} />
</section>

<style>
  .hero {
    position: relative;
    overflow: hidden;
    display: grid;
    place-items: center;
  }

  .hero::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(10, 13, 20, 0.1), transparent 40%, var(--night));
  }

  .ambient {
    position: absolute;
    inset: -40px;
    background: center / cover;
    filter: blur(28px) saturate(1.2);
    opacity: 0.45;
  }

  .logo {
    position: relative;
    z-index: 1;
    width: min(520px, 80%);
    mix-blend-mode: lighten;
    mask-image: radial-gradient(ellipse 72% 78% at center, #000 58%, transparent 100%);
  }

  .snow {
    position: absolute;
    inset: 0;
    z-index: 1;
    pointer-events: none;
  }

  .snow i {
    position: absolute;
    top: -8px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.55);
    animation: fall linear infinite;
  }

  @keyframes fall {
    to {
      transform: translate(var(--drift), 330px);
      opacity: 0;
    }
  }
</style>
