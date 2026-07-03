<script lang="ts">
  let {
    note = "",
    checking = false,
    approved = false,
    onOpenSettings,
    onRetry,
    onClose,
  }: {
    note?: string;
    checking?: boolean;
    approved?: boolean;
    onOpenSettings: () => void;
    onRetry: () => void;
    onClose: () => void;
  } = $props();
</script>

<div
  class="backdrop"
  role="button"
  tabindex="-1"
  onclick={onClose}
  onkeydown={(e) => e.key === "Escape" && onClose()}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    tabindex="0"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    {#if approved}
      <div class="success">
        <div class="check">✓</div>
        <h2>Helper approved</h2>
        <p>You're all set — triggering runs without a password now.</p>
      </div>
    {:else}
      <span class="eyebrow">One-time setup</span>
      <h2>Approve the helper</h2>
      <p>
        Triggering DFU runs a small background helper with elevated privileges.
        Approve it once and every trigger after that is instant — no password.
      </p>

      <ol class="steps">
        <li>
          <span class="n mono">1</span>
          <span class="txt">Click <b>Open System Settings</b> below.</span>
        </li>
        <li>
          <span class="n mono">2</span>
          <span class="txt">Turn <b>RestoreKit</b> on under <b>Allow in the Background</b>.</span>
        </li>
        <li>
          <span class="n mono">3</span>
          <span class="txt">That's it — this screen updates on its own.</span>
        </li>
      </ol>

      {#if note}
        <p class="note">{note}</p>
      {/if}

      <div class="actions">
        <button class="btn ghost" onclick={onClose}>Close</button>
        <button class="btn" onclick={onOpenSettings}>Open System Settings</button>
        <button class="btn primary" onclick={onRetry} disabled={checking}>
          {checking ? "Checking…" : "Try again"}
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
