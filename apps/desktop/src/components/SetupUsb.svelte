<script lang="ts">
  let {
    busy = false,
    done = false,
    error = "",
    onSetup,
    onClose,
  }: {
    busy?: boolean;
    done?: boolean;
    error?: string;
    onSetup: () => void;
    onClose: () => void;
  } = $props();
</script>

<div
  class="backdrop"
  role="button"
  tabindex="-1"
  onclick={() => !busy && onClose()}
  onkeydown={(e) => e.key === "Escape" && !busy && onClose()}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="0"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    {#if done}
      <div class="success">
        <div class="check">✓</div>
        <h2>USB access ready</h2>
        <p>RestoreKit can talk to the Mac now — you're set to restore.</p>
      </div>
    {:else}
      <span class="eyebrow">One-time setup</span>
      <h2>Set up USB access</h2>
      <p>
        Windows needs the <b>WinUSB</b> driver bound to a Mac in DFU before RestoreKit
        can reach it. This installs it for you — approve the Windows prompt when it
        appears. You only do this once per PC; every Mac after that just works.
      </p>

      <ol class="steps">
        <li>
          <span class="n mono">1</span>
          <span class="txt">Click <b>Set up USB access</b> below.</span>
        </li>
        <li>
          <span class="n mono">2</span>
          <span class="txt">Approve the <b>User Account Control</b> prompt.</span>
        </li>
        <li>
          <span class="n mono">3</span>
          <span class="txt">That's it — this screen updates on its own.</span>
        </li>
      </ol>

      {#if error}
        <p class="note">{error}</p>
      {/if}

      <div class="actions">
        <button class="btn ghost" onclick={onClose} disabled={busy}>Close</button>
        <button class="btn primary" onclick={onSetup} disabled={busy}>
          {busy ? "Setting up…" : "Set up USB access"}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(18, 23, 34, 0.32);
    backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .modal {
    width: 440px;
    max-width: calc(100vw - 40px);
    background: var(--panel);
    border: 1px solid var(--line-2);
    border-radius: 16px;
    padding: 24px;
    box-shadow: var(--shadow);
  }
  h2 {
    font-size: 18px;
    letter-spacing: -0.02em;
    margin: 4px 0 8px;
  }
  p {
    color: var(--muted);
    font-size: 14px;
    margin: 0 0 16px;
  }
  .steps {
    list-style: none;
    margin: 0 0 18px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .steps li {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 14px;
    color: var(--ink);
  }
  .txt {
    flex: 1;
    line-height: 1.5;
  }
  .n {
    flex: none;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--signal-soft);
    color: var(--signal);
    font-size: 11px;
    font-weight: 600;
  }
  .steps b {
    font-weight: 600;
  }
  .note {
    margin: -4px 0 16px;
    padding: 8px 11px;
    border-radius: 8px;
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .success {
    text-align: center;
    padding: 14px 0 6px;
  }
  .check {
    width: 52px;
    height: 52px;
    margin: 0 auto 16px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--alive-soft);
    color: var(--alive);
    font-size: 26px;
    font-weight: 700;
  }
  .success h2 {
    margin: 0 0 6px;
  }
  .success p {
    margin: 0;
  }
</style>
