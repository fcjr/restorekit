<script lang="ts">
  import type { Device, Firmware } from "../lib/api";
  let {
    device,
    firmware,
    localIpsw,
    revive = false,
    onConfirm,
    onCancel,
  }: {
    device: Device;
    firmware?: Firmware | null;
    localIpsw?: string | null;
    revive?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  let typed = $state("");
  const ready = $derived(revive || typed.trim().toUpperCase() === "ERASE");
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
    <div class="mark" class:revive>{revive ? "REVIVE" : "ERASE"}</div>
    <h2>{revive ? "Revive this Mac?" : "Erase and restore this Mac?"}</h2>
    <p>
      {#if revive}
        This reinstalls firmware on {device.name}
        <span class="mono">({device.ecid})</span> without erasing user data.
      {:else}
        This permanently erases <b>everything</b> on {device.name}
        <span class="mono">({device.ecid})</span> and installs
        {#if firmware}macOS {firmware.version} <span class="mono">({firmware.build})</span>{:else}the
          selected IPSW{/if}.
      {/if}
    </p>
    {#if !revive}
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
    {/if}
    <div class="actions">
      <button class="btn ghost" onclick={onCancel}>Cancel</button>
      <button
        class="btn {revive ? 'primary' : 'danger'}"
        disabled={!ready}
        onclick={onConfirm}
      >
        {revive ? "Revive" : "Erase & restore"}
      </button>
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
  .mark.revive {
    color: var(--signal);
    background: var(--signal-soft);
    border-color: var(--signal-line);
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
