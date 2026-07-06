<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    onProgress,
    pickIpsw,
    gib,
    MODES,
    APPROVAL_REQUIRED,
    type Device,
    type Firmware,
    type ProgressEvent,
    type CacheInfo,
  } from "./lib/api";
  import { checkForUpdates } from "./lib/updater";
  import DeviceCard from "./components/DeviceCard.svelte";
  import DeviceRow from "./components/DeviceRow.svelte";
  import Progress from "./components/Progress.svelte";
  import ConfirmErase from "./components/ConfirmErase.svelte";
  import Confirm from "./components/Confirm.svelte";
  import ApproveHelper from "./components/ApproveHelper.svelte";
  import SetupUsb from "./components/SetupUsb.svelte";
  import Logo from "./components/Logo.svelte";

  type Phase = "idle" | "resolving" | "downloading" | "restoring" | "done" | "error";

  let devices = $state<Device[]>([]);
  let selectedSerial = $state<string | null>(null);
  let canTrigger = $state(false);
  let manual = $state("");
  let cache = $state<CacheInfo | null>(null);

  // Per-action state.
  let phase = $state<Phase>("idle");
  let active = $state<Device | null>(null); // frozen device an action runs against
  let firmware = $state<Firmware | null>(null);
  let osVersion = $state("");
  let ipswPath = $state<string | null>(null);
  let revive = $state(false);
  let busy = $state(""); // short-lived status for trigger/reboot
  let error = $state("");
  let confirming = $state(false);
  let confirmingClear = $state(false);
  let doneKind = $state<"restore" | "download">("restore");
  let needsApproval = $state(false);
  let approvalNote = $state("");
  let approvalChecking = $state(false);
  let pendingTrigger = $state<"dfu" | "reboot" | null>(null);
  let helperState = $state(""); // "" until known; then enabled | requiresApproval | …
  let approved = $state(false); // brief success state inside the approval screen
  // Windows WinUSB driver setup (mirrors the helper approval flow).
  let settingUpDriver = $state(false);
  let driverBusy = $state(false);
  let driverDone = $state(false);
  let driverError = $state("");
  let dl = $state({ received: 0, total: 0, cached: false, verifying: false });
  let rs = $state({ name: "starting", percent: 0 });

  const selected = $derived(devices.find((d) => d.serial === selectedSerial) ?? null);
  const dlPercent = $derived(dl.total > 0 ? (dl.received / dl.total) * 100 : 0);
  const running = $derived(phase !== "idle");

  function handleProgress(e: ProgressEvent) {
    switch (e.event) {
      case "cache_hit":
        dl.cached = true;
        break;
      case "download_resumed":
      case "download_progress":
        dl.received = (e as any).received ?? dl.received;
        if ("total" in e) dl.total = (e as any).total;
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

  async function refresh() {
    try {
      devices = await api.listDevices();
      if (!selectedSerial && devices.length) selectedSerial = devices[0].serial;
    } catch {
      /* enumeration hiccups are fine */
    }
  }

  // Only DFU-capable hosts use the privileged helper; elsewhere the trigger
  // isn't offered, so its approval state is irrelevant.
  async function refreshHelper() {
    if (!canTrigger) return;
    try {
      helperState = await api.helperStatus();
    } catch {
      /* leave the last known state */
    }
  }

  onMount(() => {
    api.hostCanTrigger().then((v) => {
      canTrigger = v;
      refreshHelper();
    });
    api.manualInstructions().then((v) => (manual = v));
    api.cacheInfo().then((v) => (cache = v)).catch(() => {});
    checkForUpdates();
    refresh();
    const poll = setInterval(() => {
      if (phase === "idle") {
        refresh();
        refreshHelper();
      }
    }, 2000);
    const unlisten = onProgress(handleProgress);
    return () => {
      clearInterval(poll);
      unlisten.then((u) => u());
    };
  });

  function select(serial: string) {
    if (running) return;
    selectedSerial = serial;
    resetAction();
  }
  function resetAction() {
    phase = "idle";
    active = null;
    firmware = null;
    error = "";
    confirming = false;
  }

  // Windows: bind WinUSB so restorekit can open the cabled Mac. Mirrors the
  // macOS helper-approval flow — a one-time setup behind a single prompt.
  function openDriverSetup() {
    driverError = "";
    driverDone = false;
    settingUpDriver = true;
  }
  async function runDriverSetup() {
    driverBusy = true;
    driverError = "";
    try {
      await api.setupDriver();
      driverDone = true;
      await refresh(); // driver_ready should now flip to true
      setTimeout(() => {
        settingUpDriver = false;
        driverDone = false;
      }, 1400);
    } catch (e) {
      driverError = String(e);
    } finally {
      driverBusy = false;
    }
  }

  async function enterDfu() {
    error = "";
    busy = "Triggering DFU…";
    try {
      const dfuDev = await api.triggerDfu();
      await refresh();
      if (dfuDev) selectedSerial = dfuDev.serial;
    } catch (e) {
      if (String(e).includes(APPROVAL_REQUIRED)) requestApproval("dfu");
      else error = String(e);
    } finally {
      busy = "";
    }
  }

  async function rebootTarget() {
    error = "";
    busy = "Rebooting…";
    try {
      await api.rebootTarget();
      await refresh();
    } catch (e) {
      if (String(e).includes(APPROVAL_REQUIRED)) requestApproval("reboot");
      else error = String(e);
    } finally {
      busy = "";
    }
  }

  function requestApproval(which: "dfu" | "reboot") {
    pendingTrigger = which;
    approvalNote = "";
    needsApproval = true;
  }

  // Open the approval screen proactively (from the setup banner), with no
  // trigger queued behind it.
  function setupHelper() {
    pendingTrigger = null;
    approvalNote = "";
    approved = false;
    needsApproval = true;
  }

  // While the approval screen is open, watch for the helper flipping to enabled
  // (the user toggled it in System Settings) and celebrate without them having
  // to click Try again.
  $effect(() => {
    if (!needsApproval || approved) return;
    const iv = setInterval(async () => {
      let s = "";
      try {
        s = await api.helperStatus();
      } catch {
        return;
      }
      helperState = s;
      if (s === "enabled") {
        clearInterval(iv);
        onApproved();
      }
    }, 1200);
    return () => clearInterval(iv);
  });

  async function onApproved() {
    if (approved) return;
    approved = true;
    approvalNote = "";
    try {
      await api.focusApp(); // surface over System Settings
    } catch {
      /* focus is best-effort */
    }
    const which = pendingTrigger;
    pendingTrigger = null;
    // Let the success state read for a beat, then continue any queued trigger.
    setTimeout(async () => {
      needsApproval = false;
      approved = false;
      if (which === "dfu") await enterDfu();
      else if (which === "reboot") await rebootTarget();
    }, 1500);
  }

  async function openHelperSettings() {
    approvalNote = "";
    try {
      await api.approveHelper();
    } catch (e) {
      approvalNote = String(e);
    }
  }

  async function retryApproval() {
    approvalChecking = true;
    approvalNote = "";
    try {
      const status = await api.helperStatus();
      helperState = status;
      if (status !== "enabled") {
        approvalNote = "Not enabled yet — turn RestoreKit on under Login Items, then try again.";
        return;
      }
      needsApproval = false;
      const which = pendingTrigger;
      pendingTrigger = null;
      if (which === "dfu") await enterDfu();
      else if (which === "reboot") await rebootTarget();
    } finally {
      approvalChecking = false;
    }
  }

  async function beginRestore() {
    if (!selected) return;
    error = "";
    active = selected;
    if (ipswPath) {
      confirming = true;
      return;
    }
    if (!selected.identifier) {
      error = "This Mac model isn't recognized, so firmware can't be resolved. Pick a local IPSW.";
      phase = "error";
      return;
    }
    phase = "resolving";
    try {
      firmware = await api.resolveFirmware(selected.identifier, osVersion);
      phase = "idle";
      confirming = true;
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  async function runRestore() {
    confirming = false;
    if (!active) return;
    try {
      let ipsw = ipswPath;
      if (!ipsw && firmware) {
        dl = { received: 0, total: firmware.size, cached: false, verifying: false };
        phase = "downloading";
        ipsw = await api.downloadFirmware(firmware);
      }
      if (!ipsw) throw new Error("no firmware to restore");
      rs = { name: "starting", percent: 0 };
      phase = "restoring";
      await api.restore(ipsw, active.serial, revive);
      doneKind = "restore";
      phase = "done";
      api.cacheInfo().then((v) => (cache = v)).catch(() => {});
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  async function downloadOnly() {
    if (!selected?.identifier) return;
    error = "";
    active = selected;
    phase = "resolving";
    try {
      firmware = await api.resolveFirmware(selected.identifier, osVersion);
      dl = { received: 0, total: firmware.size, cached: false, verifying: false };
      phase = "downloading";
      await api.downloadFirmware(firmware);
      doneKind = "download";
      phase = "done";
      api.cacheInfo().then((v) => (cache = v)).catch(() => {});
    } catch (e) {
      error = String(e);
      phase = "error";
    }
  }

  async function chooseIpsw() {
    const p = await pickIpsw();
    if (p) ipswPath = p;
  }

  async function clearCache() {
    confirmingClear = false;
    await api.clearCache();
    cache = await api.cacheInfo();
  }
</script>

<div class="app">
  <header>
    <Logo />
    <div class="host mono">{canTrigger ? "DFU-capable host" : "detect-only host"}</div>
  </header>

  {#if canTrigger && helperState && helperState !== "enabled"}
    <div class="setup-banner">
      <span class="dot"></span>
      <div class="msg">
        <b>One-time setup:</b> approve the DFU helper so triggering works without a
        password.
      </div>
      <button class="btn primary sm" onclick={setupHelper}>Set up helper</button>
    </div>
  {/if}

  <div class="body">
    <aside class="sidebar">
      <div class="list-head">
        <span>Devices</span><span class="count">{devices.length}</span>
      </div>
      <div class="list">
        {#if devices.length === 0}
          <div class="empty">
            <div class="pulse"></div>
            <p>No Apple devices connected.</p>
            <span>Cable a Mac to this host's DFU port.</span>
          </div>
        {:else}
          {#each devices as d (d.serial)}
            <DeviceRow
              device={d}
              selected={d.serial === selectedSerial && !running}
              onselect={() => select(d.serial)}
            />
          {/each}
        {/if}
      </div>
    </aside>

    <section class="detail">
      {#if phase === "downloading"}
        <div class="stage">
          <span class="eyebrow">{active?.name}</span>
          <h2>{dl.verifying ? "Verifying download" : dl.cached ? "Using cached firmware" : "Downloading firmware"}</h2>
          <Progress
            label={dl.verifying ? "Verifying checksum" : "Downloading"}
            percent={dl.verifying ? 100 : dlPercent}
            sub={dl.total > 0 ? `${gib(dl.received)} / ${gib(dl.total)}` : ""}
          />
        </div>
      {:else if phase === "restoring"}
        <div class="stage">
          <span class="eyebrow">{active?.name}</span>
          <h2>{rs.name}</h2>
          <Progress label="Restoring" percent={rs.percent} sub="Do not disconnect the target." />
        </div>
      {:else if phase === "done"}
        <div class="stage center">
          <span class="eyebrow alive">{doneKind === "restore" ? "Restored" : "Downloaded"}</span>
          <h1>{doneKind === "restore" ? "Restored." : "Firmware ready."}</h1>
          <p class="lede">
            {doneKind === "restore"
              ? "The target is booting to Setup Assistant."
              : "The matching firmware is cached and ready to restore."}
          </p>
          <button class="btn" onclick={resetAction}>Back to devices</button>
        </div>
      {:else if phase === "error"}
        <div class="stage center">
          <span class="eyebrow danger">Failed</span>
          <pre class="log mono">{error}</pre>
          <button class="btn" onclick={resetAction}>Back</button>
        </div>
      {:else if !selected}
        <div class="stage center muted">
          <span class="eyebrow">No selection</span>
          <p class="lede">Select a device to see its details and actions.</p>
        </div>
      {:else}
        <div class="stage">
          <div class="detail-head">
            <div>
              <span class="eyebrow" style="color: var(--mode-{selected.mode})">
                {MODES[selected.mode].label} · {MODES[selected.mode].hint}
              </span>
            </div>
          </div>
          <DeviceCard device={selected} />

          {#if error}<p class="err">{error}</p>{/if}
          {#if busy}<p class="busy mono">{busy}</p>{/if}

          {#if selected.restorable}
            {#if !selected.driver_ready}
              <div class="usb-needed">
                <p class="lede">
                  One-time setup: RestoreKit needs USB access to this Mac before it
                  can restore.
                </p>
                <div class="actions">
                  <button class="btn primary" onclick={openDriverSetup}>
                    Set up USB access
                  </button>
                </div>
                <p class="hint">
                  Binds the WinUSB driver behind a single Windows prompt — one time
                  per PC, then every Mac just works.
                </p>
              </div>
            {:else}
            <div class="options">
              <div class="opt-head">
                Options <span class="faint">— defaults are fine; override only if you need to</span>
              </div>
              <div class="opt">
                <label for="osv">macOS version <span class="faint">optional</span></label>
                <input id="osv" class="mono" placeholder="latest signed" bind:value={osVersion} />
              </div>
              <div class="opt">
                <label for="ipsw">Local IPSW <span class="faint">optional</span></label>
                <div class="picker-wrap">
                  <button id="ipsw" class="picker mono" class:set={ipswPath} onclick={chooseIpsw}>
                    {ipswPath ? ipswPath.split("/").pop() : "Choose a .ipsw file…"}
                  </button>
                  {#if ipswPath}
                    <button
                      class="clearx"
                      title="Use downloaded firmware instead"
                      onclick={() => (ipswPath = null)}>×</button
                    >
                  {/if}
                </div>
              </div>
              <p class="opt-hint">
                {ipswPath
                  ? "Restoring from your chosen file — nothing will be downloaded."
                  : "Leave empty to download the matching firmware automatically."}
              </p>
              <label class="toggle">
                <input type="checkbox" bind:checked={revive} />
                Revive <span class="faint">— reinstall firmware, keep data (no erase)</span>
              </label>
            </div>

            <div class="actions">
              <button class="btn primary" onclick={beginRestore}>
                {revive ? "Revive" : "Erase & restore"}
              </button>
              <button class="btn" onclick={downloadOnly} disabled={!selected.identifier || !!ipswPath}>
                Download only
              </button>
              <button class="btn ghost" onclick={rebootTarget} disabled={!!busy}>Reboot out of DFU</button>
            </div>
            {/if}
          {:else if canTrigger}
            <p class="lede">Restore needs DFU mode. Put this Mac into DFU to continue.</p>
            <div class="actions">
              <button class="btn primary" onclick={enterDfu} disabled={!!busy}>
                {busy ? "Authorizing…" : "Enter DFU mode"}
              </button>
              <button class="btn ghost" onclick={rebootTarget} disabled={!!busy}>Reboot</button>
            </div>
            <p class="hint">The trigger asks for permission (Touch ID) — only that step runs as root.</p>
          {:else}
            <p class="lede">This host can't trigger DFU. Put the target into DFU by hand:</p>
            <pre class="log mono manual">{manual}</pre>
          {/if}
        </div>
      {/if}
    </section>
  </div>

  <footer>
    {#if cache}
      <span class="mono">
        Cache · {cache.count} firmware · {gib(cache.bytes)}
        <span class="faint">{cache.path}</span>
      </span>
      <button
        class="btn ghost sm"
        onclick={() => (confirmingClear = true)}
        disabled={cache.count === 0}>Clear</button
      >
    {/if}
  </footer>

  {#if confirming && active}
    <ConfirmErase
      device={active}
      {firmware}
      localIpsw={ipswPath}
      {revive}
      onConfirm={runRestore}
      onCancel={() => {
        confirming = false;
        active = null;
      }}
    />
  {/if}

  {#if confirmingClear && cache}
    <Confirm
      title="Clear firmware cache?"
      body={`This deletes ${cache.count} cached firmware (${gib(cache.bytes)}). You'll re-download next time.`}
      confirmLabel="Clear cache"
      danger
      onConfirm={clearCache}
      onCancel={() => (confirmingClear = false)}
    />
  {/if}

  {#if needsApproval}
    <ApproveHelper
      {approved}
      note={approvalNote}
      checking={approvalChecking}
      onOpenSettings={openHelperSettings}
      onRetry={retryApproval}
      onClose={() => {
        needsApproval = false;
        pendingTrigger = null;
      }}
    />
  {/if}

  {#if settingUpDriver}
    <SetupUsb
      busy={driverBusy}
      done={driverDone}
      error={driverError}
      onSetup={runDriverSetup}
      onClose={() => (settingUpDriver = false)}
    />
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
    padding: 13px 20px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    -webkit-app-region: drag;
  }
  .host {
    font-size: 11px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .setup-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 20px;
    background: var(--signal-soft);
    border-bottom: 1px solid var(--signal-line);
    font-size: 13px;
    color: var(--ink);
  }
  .setup-banner .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--signal);
  }
  .setup-banner .msg {
    flex: 1;
  }
  .setup-banner b {
    font-weight: 600;
  }
  .btn.sm {
    padding: 6px 12px;
    font-size: 12.5px;
  }
  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 268px 1fr;
    min-height: 0;
  }
  .sidebar {
    background: var(--panel);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .list-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 16px 8px;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--faint);
  }
  .count {
    color: var(--muted);
  }
  .list {
    padding: 4px 8px 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .empty {
    text-align: center;
    color: var(--faint);
    padding: 40px 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .empty p {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: 13px;
  }
  .empty span {
    font-size: 12px;
  }
  .pulse {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--line-2);
    animation: pulse 1.8s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      background: var(--muted);
      transform: scale(1.25);
    }
  }
  .detail {
    padding: 30px 34px;
    overflow-y: auto;
    display: grid;
    place-items: start start;
  }
  .stage {
    width: 100%;
    max-width: 440px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .stage.center {
    place-self: center;
    align-items: center;
    text-align: center;
  }
  .stage.muted {
    color: var(--muted);
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
    max-width: 40ch;
  }
  .hint {
    color: var(--faint);
    font-size: 12px;
    margin: 0;
  }
  .options {
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 16px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .opt label {
    width: 108px;
    font-size: 12px;
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    flex: none;
  }
  .opt input,
  .picker {
    flex: 1;
    background: var(--bg);
    border: 1px solid var(--line-2);
    border-radius: 8px;
    padding: 8px 11px;
    color: var(--ink);
    font-size: 13px;
    text-align: left;
  }
  .opt input:focus,
  .picker:hover {
    border-color: var(--signal-line);
    outline: none;
  }
  .clearx {
    background: transparent;
    border: 0;
    color: var(--faint);
    font-size: 18px;
    line-height: 1;
    padding: 0 4px;
  }
  .clearx:hover {
    color: var(--danger);
  }
  .opt-head {
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .opt-head .faint {
    color: var(--faint);
  }
  .opt label .faint {
    text-transform: none;
    letter-spacing: 0;
    font-size: 10px;
    margin-left: 5px;
    color: var(--faint);
  }
  .picker-wrap {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .picker.set {
    color: var(--ink);
  }
  .picker:not(.set) {
    color: var(--faint);
  }
  .opt-hint {
    margin: -4px 0 0 118px;
    font-size: 11px;
    color: var(--faint);
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 13px;
    color: var(--muted);
  }
  .toggle .faint {
    color: var(--faint);
  }
  .actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
    margin: 0;
  }
  .busy {
    color: var(--signal);
    font-size: 13px;
    margin: 0;
  }
  .eyebrow.alive {
    color: var(--alive);
  }
  .eyebrow.danger {
    color: var(--danger);
  }
  .log {
    white-space: pre-wrap;
    text-align: left;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 14px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--muted);
    width: 100%;
    max-height: 240px;
    overflow-y: auto;
  }
  .log.mono.manual {
    color: var(--muted);
  }
  pre.log {
    color: var(--danger);
    border-color: rgba(239, 106, 106, 0.35);
  }
  pre.log.manual {
    color: var(--muted);
    border-color: var(--line);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 20px;
    background: var(--panel);
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
  footer .faint {
    color: var(--faint);
    margin-left: 8px;
  }
  .btn.sm {
    padding: 6px 12px;
    font-size: 12px;
  }
</style>
