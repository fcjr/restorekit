<script lang="ts">
  let {
    label,
    percent,
    sub = "",
    alive = false,
  }: { label: string; percent: number; sub?: string; alive?: boolean } = $props();
</script>

<div class="wrap">
  <div class="row">
    <span class="label">{label}</span>
    <span class="pct mono">{Math.round(percent)}%</span>
  </div>
  <div class="track">
    <div class="fill" class:alive style="width:{Math.max(0, Math.min(100, percent))}%"></div>
  </div>
  {#if sub}<div class="sub mono">{sub}</div>{/if}
</div>

<style>
  .wrap {
    width: 100%;
    max-width: 420px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 8px;
  }
  .label {
    font-size: 14px;
    color: var(--ink);
  }
  .pct {
    font-size: 13px;
    color: var(--muted);
  }
  .track {
    height: 8px;
    border-radius: 6px;
    background: var(--line);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 6px;
    background: linear-gradient(90deg, var(--signal), #ffd089);
    transition: width 0.35s ease;
  }
  .fill.alive {
    background: linear-gradient(90deg, var(--alive), #7fe0b8);
  }
  .sub {
    margin-top: 8px;
    font-size: 12px;
    color: var(--faint);
  }
</style>
