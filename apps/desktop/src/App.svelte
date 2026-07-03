<script lang="ts">
  import { onMount } from "svelte";
  import { api, onProgress, gib, type Device, type Firmware, type ProgressEvent } from "./lib/api";
  import DeviceCard from "./components/DeviceCard.svelte";
  import Progress from "./components/Progress.svelte";
  import ConfirmErase from "./components/ConfirmErase.svelte";

  type Phase =
    | "idle"
    | "detected"
    | "resolving"
    | "confirm"
    | "downloading"
    | "restoring"
    | "done"
    | "error";

  let phase = $state<Phase>("idle");
  let device = $state<Device | null>(null);
  let firmware = $state<Firmware | null>(null);
  let canTrigger = $state(false);
  let manual = $state("");
  let triggering = $state(false);
  let revive = $state(false);
  let error = $state("");

  let dl = $state({ received: 0, total: 0, cached: false, verifying: false });
  let rs = $state({ name: "starting", percent: 0 });

  const dlPercent = $derived(dl.total > 0 ? (dl.received / dl.total) * 100 : 0);
  const settled = $derived(phase === "idle" || phase === "detected");

  function handleProgress(e: ProgressEvent) {
    switch (e.event) {
      case "cache_hit":
        dl.cached = true;
        break;
      case "download_resumed":
        dl.received = (e as any).received;
        break;
      case "download_progress":
        dl.received = (e as any).received;
        dl.total = (e as any).total;
        break;
      case "verifying":
        dl.verifying = true;
        break;
      case "restore_step":
        rs.name = (e as any).name;
        rs.percent = (e as any).progress * 100;
        break;
    }
  }

  onMount(() => {
    api.hostCanTrigger().then((v) => (canTrigger = v));
    api.manualInstructions().then((v) => (manual = v));

    const poll = setInterval(async () => {
      if (!settled) return;
      try {
        const devices = await api.listDevices();
        if (devices.length > 0) {
          device = devices[0];
          if (phase === "idle") phase = "detected";
        } else {
          device = null;
          if (phase === "detected") phase = "idle";
        }
      } catch {
        /* transient enumeration errors are fine */
      }
    }, 2000);

    const unlisten = onProgress(handleProgress);
    return () => {
      clearInterval(poll);
      unlisten.then((u) => u());
    };
  });

  async function enterDfu() {
    error = "";
    triggering = true;
    try {
      device = await api.triggerDfu();
      phase = "detected";
    } catch (e) {
      error = String(e);
    } finally {
      triggering = false;
    }
  }

  async function beginRestore() {
    if (!device?.identifier) {
      error = "This Mac model isn't recognized, so firmware can't be resolved.";
      phase = "error";
      return;
    }
    error = "";
    phase = "resolving";
    try {
      firmware = await api.resolveFirmware(device.identifier);
      phase = "confirm";
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  async function runRestore() {
    if (!device || !firmware) return;
    dl = { received: 0, total: firmware.size, cached: false, verifying: false };
    phase = "downloading";
    try {
      const ipsw = await api.downloadFirmware(firmware);
      rs = { name: "starting", percent: 0 };
      phase = "restoring";
      await api.restore(ipsw, device.serial, revive);
      phase = "done";
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  function reset() {
    error = "";
    firmware = null;
    phase = device ? "detected" : "idle";
  }
</script>

<div class="app">
  <header>
    <div class="brand"><span class="dot"></span> restorekit</div>
    <div class="host mono">{canTrigger ? "Apple Silicon host" : "manual DFU host"}</div>
  </header>

  <main>
    {#if phase === "idle"}
      <div class="stage center">
        <span class="eyebrow">No device</span>
        <h1>Connect a Mac in DFU mode</h1>
        {#if canTrigger}
          <p class="lede">Cable the target to this Mac's DFU port, then trigger DFU below.</p>
          <button class="btn primary" onclick={enterDfu} disabled={triggering}>
            {triggering ? "Waiting for authorization…" : "Enter DFU mode"}
          </button>
          <p class="hint">Requires your admin password — only the trigger runs as root.</p>
        {:else}
          <p class="lede">This host can't trigger DFU electronically. Put the target into DFU by hand:</p>
          <pre class="manual mono">{manual}</pre>
        {/if}
        {#if error}<p class="err">{error}</p>{/if}
      </div>
    {:else if phase === "detected"}
      <div class="stage">
        <span class="eyebrow">Detected in DFU</span>
        {#if device}<DeviceCard {device} />{/if}
        <label class="revive">
          <input type="checkbox" bind:checked={revive} />
          Revive instead of erase <span class="faint">(keep data, fix firmware only)</span>
        </label>
        <button class="btn primary wide" onclick={beginRestore}>
          {revive ? "Revive this Mac" : "Erase & restore this Mac"}
        </button>
      </div>
    {:else if phase === "resolving"}
      <div class="stage center">
        <span class="eyebrow">Firmware</span>
        <h1>Finding the right macOS…</h1>
        <div class="spinner"></div>
      </div>
    {:else if phase === "downloading"}
      <div class="stage">
        <span class="eyebrow">Firmware · macOS {firmware?.version}</span>
        <h2>{dl.verifying ? "Verifying download" : dl.cached ? "Already downloaded" : "Downloading firmware"}</h2>
        <Progress
          label={dl.verifying ? "Verifying checksum" : "Downloading"}
          percent={dl.verifying ? 100 : dlPercent}
          sub={dl.total > 0 ? `${gib(dl.received)} / ${gib(dl.total)}` : ""}
        />
      </div>
    {:else if phase === "restoring"}
      <div class="stage">
        <span class="eyebrow">Restoring</span>
        <h2>{rs.name}</h2>
        <Progress label="Restore" percent={rs.percent} sub="Do not disconnect the target." />
      </div>
    {:else if phase === "done"}
      <div class="stage center">
        <span class="eyebrow alive">Complete</span>
        <h1>Restored.</h1>
        <p class="lede">The target is booting to Setup Assistant.</p>
        <button class="btn" onclick={reset}>Restore another</button>
      </div>
    {:else if phase === "error"}
      <div class="stage center">
        <span class="eyebrow danger">Failed</span>
        <h1>Something went wrong</h1>
        <pre class="manual mono err-log">{error}</pre>
        <button class="btn" onclick={reset}>Back</button>
      </div>
    {/if}
  </main>

  {#if phase === "confirm" && device && firmware}
    <ConfirmErase {device} {firmware} onConfirm={runRestore} onCancel={reset} />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 22px;
    border-bottom: 1px solid var(--line);
    -webkit-app-region: drag;
  }
  .brand {
    font-family: var(--font-mono);
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--signal);
    box-shadow: 0 0 10px var(--signal);
  }
  .host {
    font-size: 11px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  main {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 28px;
    overflow-y: auto;
  }
  .stage {
    width: 100%;
    max-width: 440px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .stage.center {
    align-items: center;
    text-align: center;
  }
  h1 {
    font-size: 26px;
    letter-spacing: -0.03em;
    margin: 4px 0 0;
    text-wrap: balance;
  }
  h2 {
    font-size: 18px;
    letter-spacing: -0.02em;
    margin: 0;
  }
  .lede {
    color: var(--muted);
    margin: 0;
    max-width: 34ch;
  }
  .hint {
    color: var(--faint);
    font-size: 12px;
    margin: 0;
  }
  .wide {
    width: 100%;
  }
  .revive {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 13px;
    color: var(--muted);
  }
  .revive .faint {
    color: var(--faint);
  }
  .manual {
    white-space: pre-wrap;
    text-align: left;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 14px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--muted);
    max-width: 440px;
    max-height: 220px;
    overflow-y: auto;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
    margin: 0;
  }
  .err-log {
    color: var(--danger);
    border-color: rgba(239, 106, 106, 0.35);
  }
  .eyebrow.alive {
    color: var(--alive);
  }
  .eyebrow.danger {
    color: var(--danger);
  }
  .spinner {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid var(--line-2);
    border-top-color: var(--signal);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
