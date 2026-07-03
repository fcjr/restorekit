<script lang="ts">
  // The mark: a silicon die (chip) with a heartbeat pulse tracing through it —
  // the moment a dead SoC shows a sign of life. Pulse draws in once on mount.
  let { size = 26, wordmark = true }: { size?: number; wordmark?: boolean } = $props();
</script>

<span class="logo" style="--s:{size}px">
  <svg viewBox="0 0 32 32" width={size} height={size} aria-hidden="true">
    <!-- die pins -->
    <g class="pins" stroke="var(--ink)" stroke-width="1.6" stroke-linecap="round">
      <line x1="11" y1="2.5" x2="11" y2="6" />
      <line x1="16" y1="2.5" x2="16" y2="6" />
      <line x1="21" y1="2.5" x2="21" y2="6" />
      <line x1="11" y1="26" x2="11" y2="29.5" />
      <line x1="16" y1="26" x2="16" y2="29.5" />
      <line x1="21" y1="26" x2="21" y2="29.5" />
      <line x1="2.5" y1="11" x2="6" y2="11" />
      <line x1="2.5" y1="21" x2="6" y2="21" />
      <line x1="26" y1="11" x2="29.5" y2="11" />
      <line x1="26" y1="21" x2="29.5" y2="21" />
    </g>
    <!-- die package -->
    <rect
      x="6"
      y="6"
      width="20"
      height="20"
      rx="4.5"
      fill="none"
      stroke="var(--ink)"
      stroke-width="1.7"
    />
    <!-- heartbeat pulse -->
    <polyline
      class="pulse"
      points="4,16 11,16 13.5,10 16,22 18.5,16 28,16"
      fill="none"
      stroke="var(--signal)"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
  {#if wordmark}
    <span class="wm">Restore<span class="k">Kit</span></span>
  {/if}
</span>

<style>
  .logo {
    display: inline-flex;
    align-items: center;
    gap: 9px;
  }
  svg {
    display: block;
    flex: none;
  }
  .pulse {
    stroke-dasharray: 44;
    stroke-dashoffset: 44;
    animation: trace 1.1s cubic-bezier(0.4, 0, 0.2, 1) 0.15s forwards;
  }
  @keyframes trace {
    to {
      stroke-dashoffset: 0;
    }
  }
  .wm {
    font-family: var(--font-mono);
    font-weight: 600;
    font-size: 15px;
    letter-spacing: -0.03em;
    color: var(--ink);
  }
  .wm .k {
    color: var(--signal);
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      stroke-dashoffset: 0;
      animation: none;
    }
  }
</style>
