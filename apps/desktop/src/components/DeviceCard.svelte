<script lang="ts">
  import type { Device } from "../lib/api";
  let { device }: { device: Device } = $props();
</script>

<div class="card">
  <div class="head"><span class="bead"></span> {device.name}</div>
  <dl>
    {#if device.identifier}
      <dt>identifier</dt>
      <dd class="mono">{device.identifier}</dd>
    {/if}
    <dt>chip</dt>
    <dd class="mono">{device.chip} · {device.board}</dd>
    <dt>ECID</dt>
    <dd class="mono">{device.ecid}</dd>
    {#if device.srtg}
      <dt>iBoot</dt>
      <dd class="mono">{device.srtg}</dd>
    {/if}
    {#if device.port}
      <dt>port</dt>
      <dd>
        {device.port.location ?? "unknown"}
        {#if device.port.dfu}<span class="tag ok">DFU port</span>{:else}<span class="tag">not DFU</span>{/if}
      </dd>
    {/if}
  </dl>
</div>

<style>
  .card {
    background: linear-gradient(180deg, var(--panel-2), var(--panel));
    border: 1px solid var(--line-2);
    border-radius: 14px;
    padding: 20px 22px;
    width: 100%;
    max-width: 420px;
  }
  .head {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.02em;
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
  }
  .bead {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--signal);
    box-shadow: 0 0 12px var(--signal);
  }
  dl {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 8px 14px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--faint);
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.06em;
    align-self: center;
  }
  dd {
    margin: 0;
    color: var(--ink);
    font-size: 13px;
  }
  .tag {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--line-2);
    color: var(--faint);
    margin-left: 6px;
  }
  .tag.ok {
    background: color-mix(in srgb, var(--signal) 20%, transparent);
    color: var(--signal);
  }
</style>
