<script lang="ts">
  import type { Device, Firmware } from "../lib/api";
  let {
    device,
    firmware,
    onConfirm,
    onCancel,
  }: {
    device: Device;
    firmware: Firmware;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let typed = $state("");
  const ready = $derived(typed.trim().toUpperCase() === "ERASE");
</script>

<div
  class="backdrop"
  role="button"
  tabindex="-1"
  onclick={onCancel}
  onkeydown={(e) => e.key === "Escape" && onCancel()}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="0"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="mark">ERASE</div>
    <h2>Erase and restore this Mac?</h2>
    <p>
      This permanently erases <b>everything</b> on {device.name}
      <span class="mono">({device.ecid})</span> and installs macOS
      {firmware.version} <span class="mono">({firmware.build})</span>.
    </p>
    <label for="confirm-input">Type <b>ERASE</b> to continue</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input
      id="confirm-input"
      class="mono"
      bind:value={typed}
      autocomplete="off"
      autocorrect="off"
      autocapitalize="characters"
      spellcheck="false"
      autofocus
      onkeydown={(e) => e.key === "Enter" && ready && onConfirm()}
    />
    <div class="actions">
      <button class="btn ghost" onclick={onCancel}>Cancel</button>
      <button class="btn danger" disabled={!ready} onclick={onConfirm}>Erase &amp; restore</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(4, 6, 9, 0.72);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .modal {
    width: 420px;
    max-width: calc(100vw - 40px);
    background: var(--panel-2);
    border: 1px solid var(--line-2);
    border-radius: 16px;
    padding: 24px;
    box-shadow: 0 40px 90px -30px rgba(0, 0, 0, 0.8);
  }
  .mark {
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.2em;
    color: var(--danger);
    background: rgba(239, 106, 106, 0.12);
    border: 1px solid rgba(239, 106, 106, 0.4);
    border-radius: 6px;
    padding: 4px 9px;
    display: inline-block;
  }
  h2 {
    font-size: 19px;
    letter-spacing: -0.02em;
    margin: 14px 0 8px;
  }
  p {
    color: var(--muted);
    font-size: 14px;
    margin: 0 0 18px;
  }
  p b {
    color: var(--ink);
  }
  label {
    display: block;
    font-size: 12px;
    color: var(--faint);
    margin-bottom: 7px;
  }
  input {
    width: 100%;
    background: var(--bg);
    border: 1px solid var(--line-2);
    border-radius: 8px;
    padding: 10px 12px;
    color: var(--ink);
    font-size: 14px;
    letter-spacing: 0.1em;
  }
  input:focus {
    border-color: var(--danger);
    outline: none;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
  }
</style>
