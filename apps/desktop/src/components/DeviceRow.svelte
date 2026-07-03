<script lang="ts">
  import { MODES, type Device } from "../lib/api";
  let {
    device,
    selected,
    onselect,
  }: { device: Device; selected: boolean; onselect: () => void } = $props();
  const meta = $derived(MODES[device.mode] ?? MODES.other);
  const sub = $derived(device.ecid || device.serial.slice(0, 20));
</script>

<button class="row" class:selected onclick={onselect}>
  <span class="badge" data-mode={device.mode}>
    <span class="signal"></span>{meta.label}
  </span>
  <span class="info">
    <span class="name">{device.name}</span>
    <span class="sub mono">{sub}</span>
  </span>
</button>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 10px;
    padding: 11px 12px;
    color: var(--ink);
    transition:
      background 0.12s,
      border-color 0.12s;
  }
  .row:hover {
    background: var(--panel-2);
  }
  .row.selected {
    background: var(--signal-soft);
    border-color: var(--signal-line);
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 4px 8px;
    border-radius: 6px;
    flex: none;
    width: 84px;
    justify-content: flex-start;
    color: var(--mode);
    background: color-mix(in srgb, var(--mode) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--mode) 34%, transparent);
  }
  .badge[data-mode="dfu"] {
    --mode: var(--mode-dfu);
  }
  .badge[data-mode="recovery"] {
    --mode: var(--mode-recovery);
  }
  .badge[data-mode="restore"] {
    --mode: var(--mode-restore);
  }
  .badge[data-mode="wtf"] {
    --mode: var(--mode-wtf);
  }
  .badge[data-mode="other"] {
    --mode: var(--mode-other);
  }
  .signal {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--mode);
    box-shadow: 0 0 7px var(--mode);
  }
  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
    gap: 1px;
  }
  .name {
    font-size: 13.5px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 11px;
    color: var(--faint);
  }
</style>
