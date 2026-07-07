<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    onProgress,
    onRestoreJobUpdate,
    onRestoreJobLog,
    pickIpsw,
    exportHistoryCsv,
    exportDevicesCsv,
    gib,
    MODES,
    APPROVAL_REQUIRED,
    type Device,
    type Firmware,
    type ProgressEvent,
    type CacheInfo,
    type HistoryEntry,
    type JobView,
    type Mode,
  } from "./lib/api";
  import { checkForUpdates } from "./lib/updater";

  type Phase = "idle" | "resolving" | "downloading" | "restoring" | "done" | "error";

  const isWindows =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
  const isMac =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");

  // ---- device / host state ----
  let devices = $state<Device[]>([]);
  let selectedSerial = $state<string | null>(null);
  let canTrigger = $state(false);
  let manual = $state("");
  let cache = $state<CacheInfo | null>(null);

  // ---- views: serial capture / list / history / restores ----
  let tab = $state<"restore" | "list" | "history" | "restores">("restore");
  let historyEnabled = $state(true); // false when the app is built without the feature
  let history = $state<HistoryEntry[]>([]);
  let confirmingClearHistory = $state(false);
  let qrSerial = $state<string | null>(null);
  let qrLabel = $state("Hardware serial");
  let qrSvg = $state("");
  let copied = $state(false);
  // Serials already logged this session (`serial|mode`), so a device sitting in
  // recovery across polls is recorded once, not every 2s.
  const recorded = new Set<string>();

  // ---- settings: auto-DFU + Apple Configurator ----
  let autoDfu = $state(false);
  let configuratorErr = $state("");
  let autoTriggering = false; // in-flight guard for auto-DFU
  const autoTriggered = new Set<string>(); // serials auto-triggered this session

  // ---- parallel restore jobs ----
  let jobs = $state<JobView[]>([]);
  let jobLogs = $state<Record<number, string[]>>({});
  let openLogFor = $state<number | null>(null);
  const recordedJobs = new Set<number>(); // jobs already written to history

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

  async function loadHistory() {
    try {
      history = await api.historyList();
    } catch {
      /* history read hiccups are fine */
    }
  }

  // Log a device to the persistent history, de-bounced per session so a Mac
  // idling in recovery isn't recorded on every poll.
  async function recordDevice(d: Device, status: string) {
    if (!historyEnabled) return;
    try {
      await api.recordCapture({
        serial_number: d.serial_number,
        ecid: d.ecid,
        model_identifier: d.identifier,
        name: d.name,
        mode: d.mode,
        status,
        timestamp_rfc3339: new Date().toISOString(),
      });
      await loadHistory();
    } catch {
      /* recording is best-effort */
    }
  }

  // Passively capture any Mac that shows up in recovery/booted with a real
  // hardware serial — the history log fills itself.
  $effect(() => {
    if (!historyEnabled) return;
    for (const d of devices) {
      if (!d.serial_number) continue;
      if (d.mode !== "recovery" && d.mode !== "booted") continue;
      const key = `${d.serial_number}|${d.mode}`;
      if (recorded.has(key)) continue;
      recorded.add(key);
      void recordDevice(d, "captured");
    }
  });

  // The best value to encode for a device: its hardware serial, else ECID, else
  // the raw USB serial string. `null` when the device exposes nothing.
  function qrOf(d: Device): { value: string; label: string } | null {
    if (d.serial_number) return { value: d.serial_number, label: "Hardware serial" };
    if (d.ecid) return { value: d.ecid, label: "ECID" };
    if (d.serial) return { value: d.serial, label: "Identifier" };
    return null;
  }

  async function showQr(value: string, label = "Hardware serial") {
    qrSerial = value;
    qrLabel = label;
    qrSvg = "";
    copied = false;
    try {
      qrSvg = await api.serialQrSvg(value);
    } catch {
      qrSvg = "";
    }
  }
  function closeQr() {
    qrSerial = null;
    qrSvg = "";
  }
  async function copySerial() {
    if (!qrSerial) return;
    try {
      await navigator.clipboard.writeText(qrSerial);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch {
      /* clipboard may be unavailable */
    }
  }

  async function clearHistory() {
    confirmingClearHistory = false;
    await api.historyClear();
    await loadHistory();
  }

  async function doExportCsv() {
    try {
      await exportHistoryCsv();
    } catch {
      /* export cancelled or failed */
    }
  }

  async function doExportDevices() {
    try {
      await exportDevicesCsv();
    } catch {
      /* export cancelled or failed */
    }
  }

  // ---- restore jobs ----
  async function loadJobs() {
    try {
      jobs = await api.listRestoreJobs();
    } catch {
      /* jobs read hiccups are fine */
    }
  }
  function upsertJob(j: JobView) {
    const i = jobs.findIndex((x) => x.id === j.id);
    if (i >= 0) jobs[i] = j;
    else jobs = [...jobs, j];
    // Log a finished restore to history once (best-effort; enrich from devices).
    if (j.status === "done" && historyEnabled && !recordedJobs.has(j.id)) {
      recordedJobs.add(j.id);
      const d = devices.find((x) => x.ecid === j.ecid);
      void api
        .recordCapture({
          serial_number: d?.serial_number ?? null,
          ecid: j.ecid,
          model_identifier: d?.identifier ?? null,
          name: j.name,
          mode: d?.mode ?? "restore",
          status: "restored",
          timestamp_rfc3339: new Date().toISOString(),
        })
        .then(loadHistory)
        .catch(() => {});
    }
  }
  async function cancelJob(id: number) {
    try {
      await api.cancelRestore(id);
    } catch {
      /* ignore */
    }
  }
  async function restartJob(id: number) {
    try {
      await api.restartRestore(id);
    } catch {
      /* ignore */
    }
  }
  async function toggleAutoDfu() {
    autoDfu = !autoDfu;
    try {
      await api.setAutoDfu(autoDfu);
    } catch {
      /* persistence best-effort */
    }
  }
  async function openConfigurator() {
    configuratorErr = "";
    try {
      await api.openAppleConfigurator();
    } catch (e) {
      configuratorErr = String(e);
    }
  }
  async function autoEnterDfu() {
    if (autoTriggering) return;
    autoTriggering = true;
    try {
      await api.triggerDfu();
      await refresh();
    } catch {
      /* best-effort; user can trigger manually */
    } finally {
      autoTriggering = false;
    }
  }
  // Auto-DFU: when enabled and the helper is ready, put a freshly detected
  // booted/recovery Mac into DFU without a click (de-bounced per device).
  $effect(() => {
    if (!autoDfu || !canTrigger || helperState !== "enabled" || running || autoTriggering) return;
    const target = devices.find(
      (d) => (d.mode === "booted" || d.mode === "recovery") && !autoTriggered.has(d.serial),
    );
    if (!target) return;
    autoTriggered.add(target.serial);
    void autoEnterDfu();
  });

  const JOB_COLOR: Record<string, string> = {
    queued: "var(--mut)",
    running: "var(--acc)",
    done: "var(--ok)",
    failed: "var(--danger)",
    canceled: "var(--fnt)",
  };

  // Click-to-copy for table cells and whole rows, with a brief toast.
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function copy(text: string) {
    const t = (text ?? "").trim();
    if (!t || t === "—") return;
    navigator.clipboard
      ?.writeText(t)
      .then(() => {
        toast = `Copied ${t}`;
        clearTimeout(toastTimer);
        toastTimer = setTimeout(() => (toast = ""), 1100);
      })
      .catch(() => {});
  }
  function deviceLine(d: Device): string {
    const id = d.identifier ? ` (${d.identifier})` : "";
    return [d.serial_number ?? "—", `${d.name}${id}`, d.ecid || "—", d.mode, d.port?.location ?? "—"].join("\t");
  }
  function historyLine(h: HistoryEntry): string {
    return [fmtTime(h.timestamp_rfc3339), h.serial_number ?? "—", h.model_identifier ?? h.name, h.ecid || "—", h.mode, h.status].join("\t");
  }

  function fmtTime(rfc: string): string {
    const d = new Date(rfc);
    return isNaN(d.getTime()) ? rfc : d.toLocaleString();
  }

  onMount(() => {
    api.hostCanTrigger().then((v) => {
      canTrigger = v;
      refreshHelper();
    });
    api.manualInstructions().then((v) => (manual = v));
    api.cacheInfo().then((v) => (cache = v)).catch(() => {});
    api.historyEnabled().then((v) => {
      historyEnabled = v;
      if (v) loadHistory();
      else if (tab !== "restore") tab = "restore";
    }).catch(() => {});
    api.getSettings().then((s) => (autoDfu = s.auto_dfu)).catch(() => {});
    checkForUpdates();
    refresh();
    loadJobs();
    const poll = setInterval(() => {
      if (phase === "idle") {
        refresh();
        refreshHelper();
      }
    }, 2000);
    const unlisten = onProgress(handleProgress);
    const unlistenJob = onRestoreJobUpdate(upsertJob);
    const unlistenJobLog = onRestoreJobLog((l) => {
      const cur = jobLogs[l.id] ?? [];
      jobLogs[l.id] = [...cur.slice(-800), l.line];
    });
    return () => {
      clearInterval(poll);
      unlisten.then((u) => u());
      unlistenJob.then((u) => u());
      unlistenJobLog.then((u) => u());
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
      // Hand the restore to the parallel job queue (its own process), then jump
      // to the Restores view so several can run at once.
      await api.enqueueRestore(ipsw, active.ecid, active.name, revive);
      api.cacheInfo().then((v) => (cache = v)).catch(() => {});
      resetAction();
      tab = "restores";
      await loadJobs();
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
  {#snippet copyicon()}<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="11" height="11" rx="1.5" /><path d="M5 15V5.5A1.5 1.5 0 0 1 6.5 4H16" /></svg>{/snippet}
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
    <span class="seg tabs">
      <button class="segbtn" class:on={tab === "restore"} onclick={() => (tab = "restore")}>Restore</button>
      <button class="segbtn" class:on={tab === "list"} onclick={() => (tab = "list")}>Devices</button>
      <button class="segbtn" class:on={tab === "restores"} onclick={() => { tab = "restores"; loadJobs(); }}>Restores</button>
      {#if historyEnabled}
        <button class="segbtn" class:on={tab === "history"} onclick={() => { tab = "history"; loadHistory(); }}>History</button>
      {/if}
    </span>
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

  {#if tab === "restore"}
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
  {:else if tab === "list"}
    <section class="tabview">
      <div class="tabhead">
        <span class="eyebrow">Connected devices · {devices.length}</span>
        <div class="grow"></div>
        <button class="btn sm" onclick={doExportDevices} disabled={!devices.length}>Export CSV</button>
      </div>
      {#if devices.length}
        <table class="tbl">
          <thead>
            <tr><th>Serial</th><th>Model</th><th>ECID</th><th>Mode</th><th>Port</th><th></th></tr>
          </thead>
          <tbody>
            {#each devices as d (d.serial)}
              <tr>
                <td><button class="cellcopy" onclick={() => copy(d.serial_number ?? "")}>{d.serial_number ?? "—"}</button></td>
                <td>
                  <button class="cellcopy" onclick={() => copy(d.name)}>{d.name}</button>
                  {#if d.identifier}<button class="cellcopy cellsub" onclick={() => copy(d.identifier!)}>{d.identifier}</button>{/if}
                </td>
                <td><button class="cellcopy" onclick={() => copy(d.ecid)}>{d.ecid || "—"}</button></td>
                <td><button class="cellcopy" onclick={() => copy(d.mode)}><span class="mtag" style="color:{MODE_COLOR[d.mode]}">{MODE_TAG[d.mode]}</span></button></td>
                <td><button class="cellcopy" onclick={() => copy(d.port?.location ?? "")}>{d.port?.location ?? "—"}</button></td>
                <td class="right nowrap">
                  {#if historyEnabled}
                    {@const q = qrOf(d)}
                    {#if q}<button class="iconbtn" title="Show QR" aria-label="Show QR" onclick={() => showQr(q.value, q.label)}>QR</button>{/if}
                  {/if}
                  <button class="iconbtn" title="Copy row" aria-label="Copy row" onclick={() => copy(deviceLine(d))}>{@render copyicon()}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="tabempty">No devices connected.</div>
      {/if}
      <p class="tabnote">Serial capture works in recovery mode and for booted Macs — DFU mode usually doesn't expose the hardware serial.</p>
    </section>
  {:else if tab === "restores"}
    <section class="tabview">
      <div class="tabhead">
        <span class="eyebrow">Restores · {jobs.length}</span>
      </div>
      {#if jobs.length}
        <table class="tbl">
          <thead>
            <tr><th>Device</th><th>ECID</th><th>Status</th><th>Progress</th><th></th></tr>
          </thead>
          <tbody>
            {#each jobs as j (j.id)}
              <tr>
                <td>{j.name}</td>
                <td>{j.ecid}</td>
                <td>
                  <span class="mtag" style="color:{JOB_COLOR[j.status] ?? 'var(--mut)'}">{j.status}</span>
                  <div class="jobstep">{j.status === "failed" && j.message ? j.message : j.step}</div>
                </td>
                <td class="jobprog">
                  <span class="pbar"><span class="pfill" style="width:{Math.max(2, Math.min(100, j.progress))}%; background:{JOB_COLOR[j.status] ?? 'var(--acc)'}"></span></span>
                  <span class="jobpct">{Math.round(j.progress)}%</span>
                </td>
                <td class="right nowrap">
                  <button class="iconbtn" onclick={() => (openLogFor = j.id)}>Log</button>
                  {#if j.status === "queued" || j.status === "running"}
                    <button class="iconbtn" onclick={() => cancelJob(j.id)}>Cancel</button>
                  {:else}
                    <button class="iconbtn" onclick={() => restartJob(j.id)}>Restart</button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="tabempty">No restores yet. Start one from the Restore tab — it runs here, and several run in parallel, each in its own process.</div>
      {/if}
    </section>
  {:else}
    <section class="tabview">
      <div class="tabhead">
        <span class="eyebrow">History · {history.length}</span>
        <div class="grow"></div>
        <button class="btn sm" onclick={doExportCsv} disabled={!history.length}>Export CSV</button>
        <button class="btn ghost sm" onclick={() => (confirmingClearHistory = true)} disabled={!history.length}>Clear</button>
      </div>
      {#if history.length}
        <table class="tbl">
          <thead>
            <tr><th>When</th><th>Serial</th><th>Model</th><th>ECID</th><th>Mode</th><th>Status</th><th></th></tr>
          </thead>
          <tbody>
            {#each history as h, i (i)}
              <tr>
                <td><button class="cellcopy" onclick={() => copy(fmtTime(h.timestamp_rfc3339))}>{fmtTime(h.timestamp_rfc3339)}</button></td>
                <td><button class="cellcopy" onclick={() => copy(h.serial_number ?? "")}>{h.serial_number ?? "—"}</button></td>
                <td><button class="cellcopy" onclick={() => copy(h.model_identifier ?? h.name)}>{h.model_identifier ?? h.name}</button></td>
                <td><button class="cellcopy" onclick={() => copy(h.ecid)}>{h.ecid || "—"}</button></td>
                <td><button class="cellcopy" onclick={() => copy(h.mode)}><span class="mtag">{h.mode}</span></button></td>
                <td><button class="cellcopy" onclick={() => copy(h.status)}>{h.status}</button></td>
                <td class="right"><button class="iconbtn" title="Copy row" aria-label="Copy row" onclick={() => copy(historyLine(h))}>{@render copyicon()}</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <div class="tabempty">No captures yet. Connect a Mac in recovery mode and its serial is logged here automatically.</div>
      {/if}
    </section>
  {/if}

  <!-- footer -->
  <footer class="footer">
    <span class="footset">
      {#if canTrigger}
        <button class="linkbtn" onclick={toggleAutoDfu} title="Automatically enter DFU on a detected booted/recovery Mac">
          Auto-DFU: <b class:onval={autoDfu}>{autoDfu ? "on" : "off"}</b>
        </button>
      {/if}
      {#if isMac}
        <button class="linkbtn" onclick={openConfigurator}>Apple Configurator</button>
      {/if}
      {#if configuratorErr}<span class="faint">{configuratorErr}</span>{/if}
    </span>
    {#if cache}
      <span class="footcache">
        cache · {cache.count} firmware · {gib(cache.bytes)}
        <span class="faint">{cache.path}</span>
        <button class="linkbtn" onclick={() => (confirmingClear = true)} disabled={cache.count === 0}>Clear</button>
      </span>
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

  {#if qrSerial}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:var(--acc)">Serial capture</div>
        <h3>{qrLabel}</h3>
        <div class="qrwrap">
          {#if qrSvg}{@html qrSvg}{:else}<span class="qrpending">…</span>{/if}
        </div>
        <div class="qrserial">{qrSerial}</div>
        <div class="modal-actions">
          <button class="btn" onclick={copySerial}>{copied ? "Copied" : "Copy"}</button>
          <button class="btn ghost" onclick={closeQr}>Close</button>
        </div>
      </div>
    </div>
  {/if}

  {#if confirmingClearHistory}
    <div class="scrim">
      <div class="modal">
        <div class="eyebrow" style="color:var(--danger)">History</div>
        <h3>Clear device history?</h3>
        <p class="modal-body">This permanently deletes all {history.length} logged {history.length === 1 ? "capture" : "captures"}.</p>
        <div class="modal-actions">
          <button class="btn" onclick={() => (confirmingClearHistory = false)}>Cancel</button>
          <button class="btn danger" onclick={clearHistory}>Clear history</button>
        </div>
      </div>
    </div>
  {/if}

  {#if openLogFor !== null}
    <div class="scrim">
      <div class="modal wide">
        <div class="eyebrow" style="color:var(--acc)">Restore log</div>
        <h3>{jobs.find((j) => j.id === openLogFor)?.name ?? "Device"}</h3>
        <pre class="log joblog">{(jobLogs[openLogFor] ?? []).join("\n") || "No log output yet."}</pre>
        <div class="modal-actions">
          <button class="btn ghost" onclick={() => (openLogFor = null)}>Close</button>
        </div>
      </div>
    </div>
  {/if}

  {#if toast}<div class="toast">{toast}</div>{/if}
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

  .tabs {
    flex: none;
  }

  /* list / history table views */
  .tabview {
    padding: 20px 24px 24px;
    overflow-y: auto;
    min-height: 0;
  }
  .tabhead {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .tabhead .eyebrow {
    margin: 0;
  }
  .tabhead .grow {
    flex: 1;
  }
  .tbl {
    width: 100%;
    border-collapse: collapse;
    border: 1px solid var(--line);
    font-size: 12px;
  }
  .tbl th {
    text-align: left;
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--fnt);
    font-weight: 600;
    padding: 9px 12px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
  }
  .tbl td {
    padding: 9px 12px;
    border-bottom: 1px solid var(--line);
    color: var(--ink2);
    white-space: nowrap;
  }
  .tbl tbody tr:last-child td {
    border-bottom: 0;
  }
  .tbl tbody tr:hover td {
    background: color-mix(in srgb, var(--acc) 4%, transparent);
  }
  .tbl .right {
    text-align: right;
  }
  .nowrap {
    white-space: nowrap;
  }
  .cellcopy {
    display: block;
    max-width: 100%;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cellcopy:hover {
    color: var(--acc);
  }
  .cellsub {
    color: var(--dim);
    font-size: 11px;
    margin-top: 2px;
  }
  .mtag {
    font-size: 10px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--mut);
  }
  .iconbtn {
    border: 1px solid var(--line2);
    background: transparent;
    color: var(--mut);
    font: inherit;
    font-size: 10px;
    letter-spacing: 0.08em;
    padding: 3px 9px;
    vertical-align: middle;
  }
  .iconbtn + .iconbtn {
    margin-left: 6px;
  }
  .iconbtn:hover {
    border-color: var(--acc);
    color: var(--acc);
  }
  .toast {
    position: fixed;
    bottom: 46px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--raise);
    border: 1px solid var(--line2);
    color: var(--ink);
    font-size: 11px;
    padding: 7px 13px;
    z-index: 80;
    max-width: 80vw;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tabempty {
    border: 1px solid var(--line);
    padding: 44px 20px;
    text-align: center;
    color: var(--mut);
    font-size: 12.5px;
  }
  .jobstep {
    margin-top: 3px;
    font-size: 11px;
    color: var(--dim);
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .jobprog {
    min-width: 160px;
  }
  .jobprog .pbar {
    display: inline-block;
    width: 110px;
    height: 6px;
    vertical-align: middle;
    margin-right: 8px;
  }
  .jobprog .pfill {
    display: block;
    height: 100%;
  }
  .jobpct {
    font-size: 11px;
    color: var(--mut);
  }
  .modal.wide {
    width: 640px;
  }
  .joblog {
    max-height: 360px;
    margin: 4px 0 16px;
    font-size: 11.5px;
    line-height: 1.5;
  }
  .tabnote {
    margin: 14px 0 0;
    font-size: 11px;
    color: var(--fnt);
  }

  /* QR modal */
  .qrwrap {
    display: flex;
    align-items: center;
    justify-content: center;
    background: #fff;
    border: 1px solid var(--line2);
    padding: 16px;
    margin: 4px 0 14px;
    min-height: 180px;
  }
  .qrwrap :global(svg) {
    width: 200px;
    height: 200px;
    display: block;
  }
  .qrpending {
    color: #888;
    font-size: 20px;
  }
  .qrserial {
    text-align: center;
    font-size: 13px;
    color: var(--ink);
    letter-spacing: 0.02em;
    word-break: break-all;
    margin-bottom: 16px;
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
  .footset {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .footcache {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .footset .onval {
    color: var(--acc);
    font-weight: 600;
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
