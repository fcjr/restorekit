<script lang="ts">
  let {
    title,
    body,
    confirmLabel = "Confirm",
    danger = false,
    onConfirm,
    onCancel,
  }: {
    title: string;
    body: string;
    confirmLabel?: string;
    danger?: boolean;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
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
    <h2>{title}</h2>
    <p>{body}</p>
    <div class="actions">
      <button class="btn ghost" onclick={onCancel}>Cancel</button>
      <button class="btn {danger ? 'danger' : 'primary'}" onclick={onConfirm}>{confirmLabel}</button>
    </div>
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
    width: 380px;
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
    margin: 0 0 8px;
  }
  p {
    color: var(--muted);
    font-size: 14px;
    margin: 0 0 20px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
