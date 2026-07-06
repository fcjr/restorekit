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
    type Mode,
  } from "./lib/api";
  import { checkForUpdates } from "./lib/updater";

  type Phase = "idle" | "resolving" | "downloading" | "restoring" | "done" | "error";

  const isWindows =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

  // ---- device / host state ----
  let devices = $state<Device[]>([]);
  let selectedSerial = $state<string | null>(null);
  let canTrigger = $state(false);
  let manual = $state("");
  let cache = $state<CacheInfo | null>(null);

  // ---- per-action state ----
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
  let openedSettings = $state(false); // "waiting…" hint after opening Login Items
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

  const hostLabel = $derived(
    isWindows ? "Windows host" : canTrigger ? "DFU-capable host" : "Detect-only host",
  );

  const showBanner = $derived(
    canTrigger && helperState !== "" && helperState !== "enabled" && phase === "idle",
  );

  // Which action panel the selected device gets.
  const mMode = $derived.by<"restore" | "usb" | "dfu" | "manual">(() => {
    const d = selected;
    if (!d) return "restore";
    if (d.restorable) return d.driver_ready ? "restore" : "usb";
    return canTrigger ? "dfu" : "manual";
  });

  const progress = $derived.by(() => {
    if (phase === "resolving")
      return {
        title: "Resolving firmware",
        label: "Contacting Apple",
        pct: 6,
        sub: "Matching the latest signed build for this model.",
      };
    if (phase === "downloading") {
      return {
        title: dl.verifying
          ? "Verifying download"
          : dl.cached
            ? "Using cached firmware"
            : "Downloading firmware",
        label: dl.verifying ? "Verifying checksum" : "Downloading",
        pct: dl.verifying ? 100 : dlPercent,
        sub: dl.total > 0 ? `${gib(dl.received)} / ${gib(dl.total)}` : "",
      };
    }
    if (phase === "restoring")
      return {
        title: rs.name,
        label: "Restoring",
        pct: rs.percent,
        sub: "Do not disconnect the target.",
      };
    return { title: "", label: "", pct: 0, sub: "" };
  });

  const MODE_TAG: Record<Mode, string> = {
    dfu: "DFU",
    recovery: "RCVRY",
    restore: "RSTR",
    wtf: "WTF",
    booted: "BOOT",
    other: "CONN",
  };
  const MODE_COLOR: Record<Mode, string> = {
    dfu: "var(--acc)",
    recovery: "var(--alt)",
    restore: "var(--ok)",
    wtf: "var(--danger)",
    booted: "var(--acc)",
    other: "var(--fnt)",
  };

  function firmwareLine(): string {
    if (ipswPath) return ipswPath.split("/").pop() ?? "local IPSW";
    if (firmware) return `macOS ${firmware.version} · ${gib(firmware.size)}`;
    return "resolved automatically";
  }

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
    openedSettings = false;
    needsApproval = true;
  }

  // Open the approval screen proactively (from the setup banner), with no
  // trigger queued behind it.
  function setupHelper() {
    pendingTrigger = null;
    approvalNote = "";
    approved = false;
    openedSettings = false;
    needsApproval = true;
  }

  // While the approval screen is open, watch for the helper flipping to enabled
  // (the user toggled it in System Settings) and celebrate without them having
  // to click anything.
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
      openedSettings = false;
      if (which === "dfu") await enterDfu();
      else if (which === "reboot") await rebootTarget();
    }, 1500);
  }

  async function openHelperSettings() {
    approvalNote = "";
    openedSettings = true;
    try {
      await api.approveHelper();
    } catch (e) {
      approvalNote = String(e);
    }
  }

  function closeApproval() {
    needsApproval = false;
    pendingTrigger = null;
    openedSettings = false;
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
  <!-- titlebar -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="brand">
      <svg viewBox="0 0 32 32" width="15" height="15" aria-hidden="true">
        <rect x="7" y="7" width="18" height="18" rx="3" fill="none" stroke="var(--ink)" stroke-width="1.8" />
        <path d="M4 16 H10 L12.2 11 L16 21 L19 16 H28" fill="none" stroke="var(--acc)" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <span class="name">restorekit</span>
    </div>
    <div class="grow"></div>
    <span class="host">
      <span class="hostdot" style="background:{canTrigger ? 'var(--acc)' : 'var(--fnt)'}"></span>
      {hostLabel}
    </span>
  </div>

  <!-- setup banner -->
  {#if showBanner}
    <div class="banner">
      <span class="bandot"></span>
      <div class="banmsg"><b>One-time setup.</b> Approve the DFU helper so triggering works without a password.</div>
      <button class="btn primary sm" onclick={setupHelper}>Set up helper</button>
    </div>
  {/if}

  <div class="body">
    <!-- roster -->
    <aside class="roster">
      <div class="roster-head"><span>Targets</span><span class="count">{devices.length}</span></div>
      {#if devices.length}
        {#each devices as d (d.serial)}
          <button
            class="row"
            class:sel={d.serial === selectedSerial && !running}
            onclick={() => select(d.serial)}
          >
            <span class="mode" style="color:{MODE_COLOR[d.mode]}">{MODE_TAG[d.mode]}</span>
            <span class="rowmeta">
              <span class="rowname">{d.name}</span>
              <span class="rowecid">{d.ecid || "—"}</span>
            </span>
          </button>
        {/each}
      {:else}
        <div class="roster-empty"><span class="pulse"></span><span>No devices</span></div>
      {/if}
    </aside>

    <!-- detail -->
    <section class="detail">
      {#if phase === "resolving" || phase === "downloading" || phase === "restoring"}
        <div class="pane">
          <div class="eyebrow">{active?.name ?? selected?.name ?? ""}</div>
          <h2>{progress.title}</h2>
          <div class="prow">
            <span class="plabel">{progress.label}</span>
            <span class="ppct">{Math.round(progress.pct)}%</span>
          </div>
          <div class="pbar"><div class="pfill" style="width:{Math.max(2, Math.min(100, progress.pct))}%"></div></div>
          {#if progress.sub}<div class="psub">{progress.sub}</div>{/if}
        </div>
      {:else if phase === "done"}
        <div class="hero">
          <div class="eyebrow ok">{doneKind === "restore" ? "Restored" : "Downloaded"}</div>
          <div class="badge ok">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12.5 L10 17.5 L19 6.5" /></svg>
          </div>
          <h1>{doneKind === "restore" ? "Restored." : "Firmware ready."}</h1>
          <p class="lede">
            {doneKind === "restore"
              ? "The target is booting to Setup Assistant."
              : "The matching firmware is cached and ready to restore."}
          </p>
          <button class="btn" onclick={resetAction}>Back to devices</button>
        </div>
      {:else if phase === "error"}
        <div class="pane">
          <div class="eyebrow danger">Failed</div>
          <pre class="log danger">{error}</pre>
          <button class="btn" onclick={resetAction}>Back</button>
        </div>
      {:else if devices.length === 0}
        <div class="hero">
          <div class="badge"><span class="pulse"></span></div>
          <div class="empty-title">No Apple devices connected</div>
          <p class="lede">Cable a Mac to this host's DFU port. RestoreKit detects it the moment it enumerates.</p>
        </div>
      {:else if selected}
        <div class="pane">
          <div class="eyebrow" style="color:{MODE_COLOR[selected.mode]}">
            {MODES[selected.mode].label} · {MODES[selected.mode].hint}
          </div>
          <h1 class="dtitle">
            {selected.name}
            {#if selected.identifier}<span class="dparen">({selected.identifier})</span>{/if}
          </h1>

          <div class="spec">
            <div class="k">Identifier</div><div class="v">{selected.identifier ?? "—"}</div>
            <div class="k">Chip · board</div><div class="v">{selected.chip || "—"} · {selected.board || "—"}</div>
            <div class="k">ECID</div><div class="v">{selected.ecid || "—"}</div>
            <div class="k">iBoot</div><div class="v">{selected.srtg ?? "—"}</div>
            {#if selected.port}
              <div class="k">Port</div>
              <div class="v">
                {selected.port.location ?? "unknown"}
                {#if selected.port.dfu}<span class="tag ok">DFU port</span>{:else}<span class="tag">not DFU</span>{/if}
              </div>
            {/if}
          </div>

          {#if error}<p class="err">{error}</p>{/if}
          {#if busy}<p class="busy">{busy}</p>{/if}

          {#if mMode === "restore"}
            <div class="opts">
              <div class="opt">
                <span class="ok-label">macOS ver.</span>
                <input class="field" placeholder="latest signed" bind:value={osVersion} />
              </div>
              <div class="opt">
                <span class="ok-label">Local IPSW</span>
                <span class="field pick">
                  <button class="pickbtn" class:set={ipswPath} onclick={chooseIpsw}>
                    {ipswPath ? ipswPath.split("/").pop() : "— none —"}
                  </button>
                  {#if ipswPath}
                    <button class="browse" title="Use downloaded firmware instead" onclick={() => (ipswPath = null)}>clear ×</button>
                  {:else}
                    <button class="browse" onclick={chooseIpsw}>browse</button>
                  {/if}
                </span>
              </div>
              <div class="opt">
                <span class="ok-label">Mode</span>
                <span class="seg">
                  <button class="segbtn" class:on={!revive} onclick={() => (revive = false)}>Erase &amp; restore</button>
                  <button class="segbtn" class:on={revive} onclick={() => (revive = true)}>Revive</button>
                </span>
              </div>
            </div>

            <div class="actions">
              <button class="btn primary" onclick={beginRestore}>{revive ? "Revive" : "Erase & restore"}</button>
              <button class="btn" onclick={downloadOnly} disabled={!selected.identifier || !!ipswPath}>Download only</button>
              <button class="btn ghost" onclick={rebootTarget} disabled={!!busy}>Reboot</button>
            </div>
          {:else if mMode === "usb"}
            <div class="notice">
              <div class="notice-body">
                One-time setup: RestoreKit needs USB access to this Mac before it can restore. Binds the WinUSB driver behind a single Windows prompt.
              </div>
              <button class="btn primary" onclick={openDriverSetup}>Set up USB access</button>
            </div>
          {:else if mMode === "dfu"}
            <div class="block">
              <p class="lede">Restore needs DFU mode. Put this Mac into DFU to continue — the trigger asks for permission once; only that step runs as root.</p>
              <div class="actions">
                <button class="btn primary" onclick={enterDfu} disabled={!!busy}>{busy ? "Authorizing…" : "Enter DFU mode"}</button>
                <button class="btn" onclick={rebootTarget} disabled={!!busy}>Reboot</button>
              </div>
            </div>
          {:else}
            <div class="block">
              <p class="lede">This host can't trigger DFU. Put the target into DFU by hand:</p>
              <pre class="log">{manual}</pre>
            </div>
          {/if}
        </div>
      {/if}
    </section>
  </div>

  <!-- footer -->
  <footer class="footer">
    {#if cache}
      <span>
        cache · {cache.count} firmware · {gib(cache.bytes)}
        <span class="faint">{cache.path}</span>
      </span>
      <button class="linkbtn" onclick={() => (confirmingClear = true)} disabled={cache.count === 0}>Clear</button>
    {/if}
  </footer>

  <!-- ===== MODALS ===== -->
  {#if confirming && active}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:{revive ? 'var(--acc)' : 'var(--danger)'}">{revive ? "Revive" : "Erase & restore"}</div>
        <h3>{revive ? "Revive this Mac?" : "Erase & restore this Mac?"}</h3>
        <div class="spec tight">
          <div class="k">Target</div><div class="v">{active.name}</div>
          <div class="k">Firmware</div><div class="v">{firmwareLine()}</div>
        </div>
        <p class="modal-body">
          {revive
            ? "Revive reinstalls firmware and keeps existing data — no erase. The target reboots when done."
            : "This erases all data on the target and installs a fresh copy of macOS. It cannot be undone."}
        </p>
        <div class="modal-actions">
          <button class="btn" onclick={() => { confirming = false; active = null; }}>Cancel</button>
          <button class="btn {revive ? 'primary' : 'danger'}" onclick={runRestore}>{revive ? "Revive target" : "Erase & restore"}</button>
        </div>
      </div>
    </div>
  {/if}

  {#if needsApproval}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:var(--acc)">DFU Helper</div>
        {#if approved}
          <div class="modal-success">
            <div class="badge ok"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12.5 L10 17.5 L19 6.5" /></svg></div>
            <h3>Helper enabled</h3>
            <div class="modal-note">Triggering now works without a password.</div>
          </div>
        {:else}
          <h3>Approve the DFU helper</h3>
          <p class="modal-body">
            RestoreKit installs a small privileged helper so it can trigger DFU over USB-PD. Enable
            <b>restorekit</b> under Login Items → Allow in the Background, then come back — this window detects it automatically.
          </p>
          {#if approvalNote}<p class="err">{approvalNote}</p>{/if}
          <div class="modal-actions start">
            <button class="btn primary" onclick={openHelperSettings}>Open Login Items</button>
            <button class="btn ghost" onclick={closeApproval}>Not now</button>
            <div class="grow"></div>
            {#if openedSettings || approvalChecking}
              <span class="waiting"><span class="spin"></span>waiting…</span>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if settingUpDriver}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:var(--acc)">USB Access</div>
        {#if driverDone}
          <div class="modal-success">
            <div class="badge ok"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12.5 L10 17.5 L19 6.5" /></svg></div>
            <h3>Driver bound</h3>
            <div class="modal-note">Every Mac cabled to this PC will just work.</div>
          </div>
        {:else}
          <h3>Set up USB access</h3>
          <p class="modal-body">This binds the WinUSB driver to the target behind a single Windows prompt — one time per PC.</p>
          {#if driverError}<p class="err">{driverError}</p>{/if}
          <div class="modal-actions start">
            <button class="btn primary" onclick={runDriverSetup} disabled={driverBusy}>{driverBusy ? "Binding…" : "Bind WinUSB driver"}</button>
            <button class="btn ghost" onclick={() => (settingUpDriver = false)}>Cancel</button>
            <div class="grow"></div>
            {#if driverBusy}<span class="waiting"><span class="spin"></span>binding…</span>{/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if confirmingClear && cache}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:var(--danger)">Cache</div>
        <h3>Clear firmware cache?</h3>
        <p class="modal-body">This deletes {cache.count} cached firmware ({gib(cache.bytes)}). You'll re-download it next time.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (confirmingClear = false)}>Cancel</button>
          <button class="btn danger" onclick={clearCache}>Clear cache</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--ink2);
  }
  .grow {
    flex: 1;
  }

  /* titlebar */
  .titlebar {
    display: flex;
    align-items: center;
    gap: 13px;
    height: 42px;
    padding: 0 15px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .host {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 10.5px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--mut);
  }
  .hostdot {
    width: 7px;
    height: 7px;
    display: block;
  }

  /* banner */
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    background: var(--accsoft);
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  .bandot {
    width: 7px;
    height: 7px;
    background: var(--acc);
    display: block;
    flex: none;
  }
  .banmsg {
    flex: 1;
    font-size: 12px;
    color: var(--ink2);
  }
  .banmsg b {
    color: var(--ink);
    font-weight: 600;
  }

  /* body */
  .body {
    flex: 1;
    display: grid;
    grid-template-columns: 250px 1fr;
    min-height: 0;
  }

  /* roster */
  .roster {
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    background: var(--bar);
    overflow-y: auto;
  }
  .roster-head {
    display: flex;
    justify-content: space-between;
    padding: 15px 16px 12px;
    font-size: 10px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--fnt);
  }
  .count {
    color: var(--dim);
  }
  .row {
    display: flex;
    gap: 12px;
    padding: 13px 16px 13px 14px;
    border-top: 1px solid var(--line);
    border-left: 2px solid transparent;
    background: transparent;
    text-align: left;
    color: inherit;
    font: inherit;
  }
  .row:last-child {
    border-bottom: 1px solid var(--line);
  }
  .row:hover {
    background: color-mix(in srgb, var(--acc) 4%, transparent);
  }
  .row.sel {
    border-left-color: var(--acc);
    background: var(--accsoft);
  }
  .row .mode {
    flex: none;
    width: 50px;
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding-top: 1px;
  }
  .rowmeta {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .rowname {
    font-size: 12.5px;
    color: var(--ink);
  }
  .rowecid {
    font-size: 10.5px;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .roster-empty {
    border-top: 1px solid var(--line);
    padding: 34px 18px;
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    font-size: 11.5px;
    color: var(--mut);
  }
  .pulse {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--fnt);
    animation: rk-live 1.8s ease-in-out infinite;
  }

  /* detail */
  .detail {
    padding: 24px 28px 22px;
    overflow-y: auto;
    min-height: 0;
  }
  .pane {
    max-width: 520px;
  }
  .eyebrow {
    font-size: 10.5px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--mut);
    margin-bottom: 12px;
  }
  .eyebrow.ok {
    color: var(--ok);
  }
  .eyebrow.danger {
    color: var(--danger);
  }
  h1 {
    margin: 0 0 20px;
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .dtitle {
    display: flex;
    align-items: baseline;
    gap: 9px;
    flex-wrap: wrap;
  }
  .dparen {
    color: var(--mut);
    font-weight: 400;
    font-size: 15px;
  }
  h2 {
    margin: 0 0 24px;
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  h3 {
    margin: 0 0 12px;
    font-size: 17px;
    font-weight: 600;
    color: var(--ink);
  }

  /* spec table */
  .spec {
    max-width: 520px;
    border: 1px solid var(--line);
    display: grid;
    grid-template-columns: 128px 1fr;
  }
  .spec.tight {
    grid-template-columns: 96px 1fr;
    margin-bottom: 14px;
  }
  .spec .k,
  .spec .v {
    padding: 10px 14px;
    border-top: 1px solid var(--line);
  }
  .spec .k:first-child,
  .spec .v:nth-child(2) {
    border-top: 0;
  }
  .spec .k {
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--fnt);
    border-right: 1px solid var(--line);
  }
  .spec .v {
    font-size: 12.5px;
    color: var(--ink2);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tag {
    font-size: 9.5px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 1px 6px;
    background: var(--line2);
    color: var(--fnt);
  }
  .tag.ok {
    background: var(--accsoft);
    color: var(--acc);
  }

  /* options */
  .opts {
    max-width: 520px;
    margin-top: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .ok-label {
    width: 96px;
    flex: none;
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--fnt);
  }
  .field {
    flex: 1;
    border: 1px solid var(--line);
    background: transparent;
    padding: 8px 11px;
    font-family: inherit;
    font-size: 12px;
    color: var(--ink2);
  }
  input.field::placeholder {
    color: var(--dim);
  }
  input.field:focus {
    outline: none;
    border-color: var(--line2);
  }
  .field.pick {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0;
  }
  .pickbtn {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    padding: 8px 11px;
    font: inherit;
    font-size: 12px;
    color: var(--dim);
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pickbtn.set {
    color: var(--ink2);
  }
  .browse {
    flex: none;
    color: var(--ink2);
    border: 1px solid var(--line2);
    background: transparent;
    padding: 2px 9px;
    margin-right: 6px;
    font: inherit;
    font-size: 10.5px;
  }
  .browse:hover {
    border-color: var(--fnt);
  }

  /* segmented */
  .seg {
    display: inline-flex;
  }
  .segbtn {
    font-size: 11px;
    padding: 6px 13px;
    font-weight: 600;
    border: 1px solid var(--line2);
    background: transparent;
    color: var(--mut);
    font-family: inherit;
  }
  .segbtn + .segbtn {
    border-left: 0;
  }
  .segbtn.on {
    background: var(--acc);
    color: var(--accink);
    border-color: var(--acc);
  }

  .actions {
    display: flex;
    gap: 10px;
    margin-top: 22px;
    align-items: center;
    flex-wrap: wrap;
  }
  .block {
    max-width: 520px;
    margin-top: 20px;
  }

  .notice {
    max-width: 520px;
    margin-top: 20px;
    border: 1px solid var(--line);
    padding: 16px 18px;
  }
  .notice-body {
    font-size: 12.5px;
    color: var(--ink2);
    line-height: 1.6;
    margin-bottom: 14px;
  }

  .lede {
    font-size: 12.5px;
    color: var(--mut);
    line-height: 1.6;
    margin: 0 0 16px;
    max-width: 46ch;
  }
  .err {
    color: var(--danger);
    font-size: 12px;
    margin: 14px 0 0;
  }
  .busy {
    color: var(--acc);
    font-size: 12px;
    margin: 12px 0 0;
  }

  /* progress */
  .prow {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 9px;
  }
  .plabel {
    font-size: 12.5px;
    color: var(--ink2);
  }
  .ppct {
    font-size: 12px;
    color: var(--mut);
  }
  .pbar {
    height: 6px;
    background: var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  .pfill {
    height: 100%;
    background: var(--acc);
    border-radius: 4px;
    transition: width 0.3s ease;
  }
  .psub {
    margin-top: 10px;
    font-size: 11.5px;
    color: var(--fnt);
  }

  /* hero (empty / done) */
  .hero {
    height: 100%;
    min-height: 380px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 14px;
  }
  .badge {
    width: 46px;
    height: 46px;
    border: 1px solid var(--line2);
    border-radius: 9px;
    display: grid;
    place-items: center;
  }
  .badge.ok {
    border-color: var(--ok);
    border-radius: 50%;
    color: var(--ok);
  }
  .empty-title {
    font-size: 15px;
    color: var(--ink);
  }
  .hero .lede {
    max-width: 36ch;
    margin: 0;
  }

  /* log / pre */
  .log {
    margin: 0 0 16px;
    border: 1px solid var(--line);
    background: var(--bar);
    padding: 14px 16px;
    font-family: inherit;
    font-size: 12px;
    color: var(--ink2);
    line-height: 1.9;
    white-space: pre-wrap;
    max-height: 260px;
    overflow-y: auto;
  }
  .log.danger {
    border-color: var(--danger);
    background: var(--dangersoft);
    color: var(--danger);
    line-height: 1.7;
  }

  /* footer */
  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding: 0 15px;
    background: var(--bar);
    border-top: 1px solid var(--line);
    font-size: 10.5px;
    color: var(--fnt);
    flex: none;
  }
  .footer .faint {
    color: var(--dim);
    margin-left: 8px;
  }
  .linkbtn {
    border: 0;
    background: transparent;
    color: var(--mut);
    font: inherit;
    font-size: 10.5px;
  }
  .linkbtn:hover {
    color: var(--ink2);
  }
  .linkbtn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  /* modals */
  .scrim {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 60;
    padding: 20px;
  }
  .modal {
    width: 430px;
    max-width: 100%;
    background: var(--raise);
    border: 1px solid var(--line2);
    border-radius: 9px;
    padding: 22px 24px;
  }
  .modal .eyebrow {
    margin-bottom: 12px;
  }
  .modal-body {
    font-size: 12px;
    color: var(--mut);
    line-height: 1.7;
    margin: 0 0 18px;
  }
  .modal-body b {
    color: var(--ink2);
  }
  .modal-actions {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    align-items: center;
  }
  .modal-actions.start {
    justify-content: flex-start;
  }
  .modal-actions .grow {
    flex: 1;
  }
  .modal-success {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    padding: 14px 0;
  }
  .modal-note {
    font-size: 12px;
    color: var(--mut);
  }
  .waiting {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
    color: var(--mut);
  }
  .spin {
    width: 11px;
    height: 11px;
    border: 1.6px solid var(--line2);
    border-top-color: var(--acc);
    border-radius: 50%;
    animation: rk-spin 0.7s linear infinite;
    display: block;
  }
</style>
