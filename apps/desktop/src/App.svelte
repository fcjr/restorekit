<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import licensesHtml from "./lib/licenses.html?raw";
  import {
    api,
    onProgress,
    onRestoreJobUpdate,
    onRestoreJobLog,
    pickIpsw,
    exportHistoryCsv,
    exportDevicesCsv,
    exportSeenCsv,
    gib,
    MODES,
    APPROVAL_REQUIRED,
    type Device,
    type Firmware,
    type ProgressEvent,
    type CacheInfo,
    type HistoryEntry,
    type JobView,
    type SeenDevice,
    type Mode,
  } from "./lib/api";
  import { checkForUpdates } from "./lib/updater";

  type Phase = "idle" | "resolving" | "downloading" | "done" | "error";

  const isWindows =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");
  const isMac =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Mac");

  // ---- device / host state ----
  let devices = $state<Device[]>([]);
  let selectedKey = $state<string | null>(null); // roster row key (ECID or serial)
  let canTrigger = $state(false);
  let manual = $state("");
  let cache = $state<CacheInfo | null>(null);

  // ---- views: restore (device-centric) / list / history / about ----
  let tab = $state<"restore" | "list" | "history" | "about">("restore");
  let appVersion = $state("");
  const licenseCount = (licensesHtml.match(/class="lic-name"/g) ?? []).length;
  let devSubtab = $state<"connected" | "history">("connected"); // Devices tab mode
  let seenDevices = $state<SeenDevice[]>([]);
  let restoreView = $state<"detail" | "list">("detail"); // Restore tab layout
  let showQrInList = $state(false); // inline QR codes in the Devices list
  let qrCache = $state<Record<string, string>>({}); // value → QR svg
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

  // Captured serials survive the transition into DFU (which exposes no serial):
  // remember the last serial seen for each ECID and host port.
  const serialByEcid = new Map<string, string>();
  const serialByPort = new Map<string, string>();
  const ecidByPort = new Map<string, string>(); // last known ECID per host port
  function serialFor(d: Device): string | null {
    if (d.serial_number) return d.serial_number;
    const e = ecidFor(d);
    if (e && serialByEcid.has(e)) return serialByEcid.get(e)!;
    const loc = d.port?.location;
    if (loc && serialByPort.has(loc)) return serialByPort.get(loc)!;
    // Fall back to the persisted seen-device history (survives app restarts).
    if (e) {
      const sd = seenDevices.find((s) => s.ecid === e && s.serial_number);
      if (sd?.serial_number) return sd.serial_number;
    }
    return null;
  }
  // A device's ECID, falling back to the last ECID seen on its port — during a
  // restore the device re-enumerates through modes that don't expose the ECID.
  function ecidFor(d: Device): string | null {
    if (d.ecid) return d.ecid;
    const loc = d.port?.location;
    return loc && ecidByPort.has(loc) ? ecidByPort.get(loc)! : null;
  }

  // ---- settings: auto-DFU + Apple Configurator ----
  let autoDfu = $state(false);
  let configuratorErr = $state("");
  let autoTriggering = false; // in-flight guard for auto-DFU
  const autoTriggered = new Set<string>(); // serials auto-triggered this session

  // ---- parallel restore jobs ----
  let jobs = $state<JobView[]>([]);
  let jobLogs = $state<Record<number, string[]>>({});
  let logEl = $state<HTMLElement | null>(null);
  let logFollow = $state(true); // Console.app-style: follow tail unless scrolled up
  let jobStart = $state<Record<number, number>>({});
  let jobEnd = $state<Record<number, number>>({});
  let now = $state(0); // ticks every second so elapsed times update live
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

  // Unified roster: each connected device merged with its restore job (if any),
  // plus jobs whose device isn't currently enumerated (rebooting mid-restore, or
  // finished). Keyed by ECID so a row survives the device re-enumerating.
  interface RosterRow {
    key: string;
    ecid: string | null;
    name: string;
    device: Device | null;
    job: JobView | null;
  }
  function latestJobForEcid(e: string | null): JobView | null {
    if (!e) return null;
    let match: JobView | null = null;
    for (const j of jobs) if (j.ecid === e) match = j;
    return match;
  }
  const roster = $derived.by<RosterRow[]>(() => {
    const rows: RosterRow[] = [];
    const seen = new Set<string>();
    for (const d of devices) {
      const e = ecidFor(d);
      const key = e ?? d.serial;
      rows.push({ key, ecid: e, name: d.name, device: d, job: latestJobForEcid(e) });
      seen.add(key);
      if (e) seen.add(e);
    }
    for (const j of jobs) {
      if (seen.has(j.ecid)) continue;
      rows.push({ key: j.ecid, ecid: j.ecid, name: j.name, device: null, job: j });
      seen.add(j.ecid);
    }
    return rows;
  });
  const selectedRow = $derived(roster.find((r) => r.key === selectedKey) ?? roster[0] ?? null);
  const selected = $derived(selectedRow?.device ?? null);
  const selectedJob = $derived(selectedRow?.job ?? null);
  const restoring = $derived(
    selectedJob != null && (selectedJob.status === "queued" || selectedJob.status === "running"),
  );
  const dlPercent = $derived(dl.total > 0 ? (dl.received / dl.total) * 100 : 0);
  const running = $derived(phase !== "idle");

  const hostLabel = $derived(
    isWindows ? "Windows host" : canTrigger ? "DFU-capable host" : "Detect-only host",
  );

  const showBanner = $derived(
    canTrigger && helperState !== "" && helperState !== "enabled" && phase === "idle",
  );

  // Which action a device gets: full restore config, WinUSB setup, DFU trigger,
  // or manual instructions.
  function deviceMode(d: Device): "restore" | "usb" | "dfu" | "manual" {
    if (d.restorable) return d.driver_ready ? "restore" : "usb";
    return canTrigger ? "dfu" : "manual";
  }
  const mMode = $derived(selected ? deviceMode(selected) : "restore");

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
    }
  }

  async function refresh() {
    try {
      const list = await api.listDevices();
      for (const d of list) {
        if (d.ecid && d.port?.location) ecidByPort.set(d.port.location, d.ecid);
        if (!d.serial_number) continue;
        if (d.ecid) serialByEcid.set(d.ecid, d.serial_number);
        if (d.port?.location) serialByPort.set(d.port.location, d.serial_number);
      }
      devices = list;
      recordSeen(list);
    } catch {
      /* enumeration hiccups are fine */
    }
  }

  // Upsert connected devices (that have an ECID) into the persistent seen-device
  // history — one deduped row per Mac, enriched as it moves through modes.
  function recordSeen(list: Device[]) {
    if (!historyEnabled) return;
    const nowIso = new Date().toISOString();
    const rows: SeenDevice[] = list
      .filter((d) => d.ecid)
      .map((d) => ({
        ecid: d.ecid,
        serial_number: serialFor(d),
        model_identifier: d.identifier,
        name: d.name,
        chip: d.chip || null,
        board: d.board || null,
        mode: d.mode,
        port: d.port?.location ?? null,
        first_seen: nowIso,
        last_seen: nowIso,
      }));
    if (rows.length) api.recordSeenDevices(rows).catch(() => {});
  }
  async function loadSeen() {
    try {
      seenDevices = await api.listSeenDevices();
    } catch {
      /* seen-list hiccups are fine */
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
  // The QR encodes the hardware serial for asset tracking; no serial, no QR
  // (an ECID QR isn't useful to scan into inventory).
  function qrOf(d: Device): { value: string; label: string } | null {
    const s = serialFor(d);
    return s ? { value: s, label: "Hardware serial" } : null;
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
  // Generate (once, cached) the QR SVG for a value shown inline in the list.
  async function ensureQr(value: string) {
    if (!value || qrCache[value] !== undefined) return;
    qrCache[value] = ""; // mark pending so we only fetch once
    try {
      qrCache[value] = await api.serialQrSvg(value);
    } catch {
      qrCache[value] = "";
    }
  }
  $effect(() => {
    if (!showQrInList) return;
    for (const d of devices) {
      const q = qrOf(d);
      if (q) ensureQr(q.value);
    }
  });
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
      if (devSubtab === "history") await exportSeenCsv();
      else await exportDevicesCsv();
    } catch {
      /* export cancelled or failed */
    }
  }
  // Open a connected device in the Restore pane.
  function openInRestore(d: Device) {
    selectedKey = ecidFor(d) ?? d.serial;
    logFollow = true;
    tab = "restore";
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
    // Track elapsed time across the job's lifecycle (reset on re-queue/restart).
    if (j.status === "queued") {
      delete jobStart[j.id];
      delete jobEnd[j.id];
    } else if (j.status === "running" && jobStart[j.id] === undefined) {
      jobStart[j.id] = Date.now();
    } else if (
      (j.status === "done" || j.status === "failed" || j.status === "canceled") &&
      jobEnd[j.id] === undefined
    ) {
      jobEnd[j.id] = Date.now();
    }
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
  function onLogScroll() {
    if (!logEl) return;
    // "At the bottom" (within a small slack) means keep following the tail.
    logFollow = logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 24;
  }
  function jumpToLive() {
    logFollow = true;
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  }
  // Auto-scroll the inline log to the tail as new lines arrive, while following.
  $effect(() => {
    if (!selectedJob || !logEl) return;
    void (jobLogs[selectedJob.id]?.length ?? 0); // re-run when new lines land
    if (logFollow) logEl.scrollTop = logEl.scrollHeight;
  });

  function fmtElapsed(j: JobView): string {
    const start = jobStart[j.id];
    if (start === undefined) return "—";
    const end = jobEnd[j.id] ?? now;
    const secs = Math.max(0, Math.floor((end - start) / 1000));
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  // Remember a serial harvested from a restore's log, keyed by the job's ECID,
  // and persist it — this is the one path that captures a Mac only ever seen in
  // DFU (its serial shows up once restored boots the ramdisk).
  function captureRestoreSerial(jobId: number, serial: string) {
    const job = jobs.find((j) => j.id === jobId);
    if (!job?.ecid || !serial) return;
    serialByEcid.set(job.ecid, serial);
    if (!historyEnabled) return;
    const nowIso = new Date().toISOString();
    void api
      .recordSeenDevices([
        {
          ecid: job.ecid,
          serial_number: serial,
          model_identifier: null,
          name: job.name,
          chip: null,
          board: null,
          mode: "restore",
          port: null,
          first_seen: nowIso,
          last_seen: nowIso,
        },
      ])
      .catch(() => {});
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
  // The status tag for a roster row: the restore job's state takes precedence
  // over the device's USB mode.
  function rowTag(r: RosterRow): { label: string; color: string } {
    const j = r.job;
    if (j && (j.status === "running" || j.status === "queued")) return { label: "RSTR", color: JOB_COLOR.running };
    if (j && j.status === "done") return { label: "DONE", color: JOB_COLOR.done };
    if (j && j.status === "failed") return { label: "FAIL", color: JOB_COLOR.failed };
    if (j && j.status === "canceled") return { label: "CNCL", color: JOB_COLOR.canceled };
    if (r.device) return { label: MODE_TAG[r.device.mode], color: MODE_COLOR[r.device.mode] };
    return { label: "—", color: "var(--mut)" };
  }
  function jobActive(j: JobView | null): boolean {
    return j != null && (j.status === "queued" || j.status === "running");
  }

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
    getVersion().then((v) => (appVersion = v)).catch(() => {});
    checkForUpdates();
    refresh();
    loadJobs();
    if (historyEnabled) loadSeen();
    now = Date.now();
    const clock = setInterval(() => (now = Date.now()), 1000);
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
      // idevicerestore logs the hardware serial mid-restore (from restored);
      // harvest it and remember it by ECID, since DFU never exposes it.
      const m = l.line.match(/device serial number is (\S+)/i);
      if (m) captureRestoreSerial(l.id, m[1]);
    });
    return () => {
      clearInterval(poll);
      clearInterval(clock);
      unlisten.then((u) => u());
      unlistenJob.then((u) => u());
      unlistenJobLog.then((u) => u());
    };
  });

  function select(key: string) {
    if (running) return;
    selectedKey = key;
    logFollow = true;
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
      if (dfuDev) selectedKey = dfuDev.ecid || dfuDev.serial;
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
      // Hand the restore to the parallel job queue (its own process). It shows up
      // live in the roster and detail; several can run at once.
      await api.enqueueRestore(ipsw, active.ecid, active.name, revive);
      api.cacheInfo().then((v) => (cache = v)).catch(() => {});
      selectedKey = active.ecid || active.serial;
      logFollow = true;
      resetAction();
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
      {#if historyEnabled}
        <button class="segbtn" class:on={tab === "history"} onclick={() => { tab = "history"; loadHistory(); }}>History</button>
      {/if}
      <button class="segbtn" class:on={tab === "about"} onclick={() => (tab = "about")}>About</button>
    </span>
    <div class="grow"></div>
    {#if tab === "restore"}
      <span class="seg viewseg">
        <button class="segbtn" class:on={restoreView === "detail"} onclick={() => (restoreView = "detail")}>Detail</button>
        <button class="segbtn" class:on={restoreView === "list"} onclick={() => (restoreView = "list")}>List</button>
      </span>
    {/if}
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

  {#if tab === "restore" && restoreView === "detail"}
  <div class="body">
    <!-- roster: connected devices + restore jobs, unified -->
    <aside class="roster">
      <div class="roster-head"><span>Targets</span><span class="count">{roster.length}</span></div>
      {#if roster.length}
        {#each roster as r (r.key)}
          {@const tag = rowTag(r)}
          <button
            class="row"
            class:sel={r.key === selectedRow?.key && !running}
            onclick={() => select(r.key)}
          >
            <span class="mode" style="color:{tag.color}">{tag.label}</span>
            <span class="rowmeta">
              <span class="rowname">{r.name}</span>
              {#if jobActive(r.job)}
                <span class="minibar"><span class="minifill" style="width:{Math.max(3, Math.min(100, r.job?.progress ?? 0))}%; background:{tag.color}"></span></span>
                <span class="rowsub">{r.job?.step} · {r.job ? fmtElapsed(r.job) : ""}</span>
              {:else if r.job && r.job.status !== "queued"}
                <span class="rowsub" style="color:{tag.color}">{r.job.status}{r.job.status === "failed" && r.job.message ? ` — ${r.job.message}` : ""}</span>
              {:else}
                <span class="rowsub">{r.ecid || (r.device ? r.device.serial : "—")}</span>
              {/if}
            </span>
          </button>
        {/each}
      {:else}
        <div class="roster-empty"><span class="pulse"></span><span>No devices</span></div>
      {/if}
    </aside>

    <!-- detail -->
    <section class="detail">
      {#if phase === "resolving" || phase === "downloading"}
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
          <div class="eyebrow ok">Downloaded</div>
          <div class="badge ok">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12.5 L10 17.5 L19 6.5" /></svg>
          </div>
          <h1>Firmware ready.</h1>
          <p class="lede">The matching firmware is cached and ready to restore.</p>
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
      {:else if selectedRow}
        <div class="pane">
          {#if selectedJob}
            <div class="eyebrow" style="color:{JOB_COLOR[selectedJob.status] ?? 'var(--acc)'}">
              {restoring ? `Restoring · ${selectedJob.step}` : selectedJob.status}
            </div>
          {:else if selected}
            <div class="eyebrow" style="color:{MODE_COLOR[selected.mode]}">
              {MODES[selected.mode].label} · {MODES[selected.mode].hint}
            </div>
          {/if}
          <h1 class="dtitle">
            {selectedRow.name}
            {#if selected?.identifier}<span class="dparen">({selected.identifier})</span>{/if}
          </h1>

          {#if selected}
            <div class="spec">
              <div class="k">Identifier</div><div class="v"><button class="cellcopy" onclick={() => copy(selected.identifier ?? "")}>{selected.identifier ?? "—"}</button></div>
              <div class="k">Serial</div><div class="v"><button class="cellcopy" onclick={() => copy(serialFor(selected) ?? "")}>{serialFor(selected) ?? "—"}</button></div>
              <div class="k">Chip · board</div><div class="v"><button class="cellcopy" onclick={() => copy(`${selected.chip} ${selected.board}`.trim())}>{selected.chip || "—"} · {selected.board || "—"}</button></div>
              <div class="k">ECID</div><div class="v"><button class="cellcopy" onclick={() => copy(selected.ecid ?? "")}>{selected.ecid || "—"}</button></div>
              <div class="k">iBoot</div><div class="v"><button class="cellcopy" onclick={() => copy(selected.srtg ?? "")}>{selected.srtg ?? "—"}</button></div>
              {#if selected.port}
                <div class="k">Port</div>
                <div class="v">
                  <button class="cellcopy" onclick={() => copy(selected.port?.location ?? "")}>{selected.port.location ?? "unknown"}</button>
                  {#if selected.port.dfu}<span class="tag ok">DFU port</span>{:else}<span class="tag">not DFU</span>{/if}
                </div>
              {/if}
            </div>
          {:else if selectedRow.ecid}
            <div class="spec">
              <div class="k">ECID</div><div class="v"><button class="cellcopy" onclick={() => copy(selectedRow.ecid ?? "")}>{selectedRow.ecid}</button></div>
              <div class="k">State</div><div class="v">disconnected (restoring or rebooting)</div>
            </div>
          {/if}

          {#if error}<p class="err">{error}</p>{/if}
          {#if busy}<p class="busy">{busy}</p>{/if}

          {#if selectedJob}
            <div class="block">
              <div class="prow">
                <span class="plabel">{restoring ? selectedJob.step : selectedJob.status}</span>
                <span class="ppct">{Math.round(selectedJob.progress)}% · {fmtElapsed(selectedJob)}</span>
              </div>
              <div class="pbar"><div class="pfill" style="width:{Math.max(2, Math.min(100, selectedJob.progress))}%; background:{JOB_COLOR[selectedJob.status] ?? 'var(--acc)'}"></div></div>
              {#if restoring}
                <p class="psub">Bar is the current step, not the whole restore. Don't disconnect, DFU, or reboot this Mac.</p>
              {:else if selectedJob.status === "failed"}
                <p class="err">{selectedJob.message || "Restore failed."}</p>
              {:else if selectedJob.status === "done"}
                <p class="psub" style="color:var(--ok)">Restored. The target is booting to Setup Assistant.</p>
              {:else if selectedJob.status === "canceled"}
                <p class="psub">Canceled.</p>
              {/if}

              <div class="joblog-wrap">
                <pre class="log joblog" bind:this={logEl} onscroll={onLogScroll}>{(jobLogs[selectedJob.id] ?? []).join("\n") || "Waiting for log output…"}</pre>
                {#if !logFollow}
                  <button class="livebtn" onclick={jumpToLive}>Jump to live ↓</button>
                {/if}
              </div>

              <div class="actions">
                {#if restoring}
                  <button class="btn danger" onclick={() => cancelJob(selectedJob.id)}>Cancel restore</button>
                {:else}
                  <button class="btn primary" onclick={() => restartJob(selectedJob.id)}>Restart</button>
                {/if}
              </div>
            </div>
          {:else if selected}
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
          {/if}
        </div>
      {/if}
    </section>
  </div>
  {:else if tab === "restore"}
    <section class="tabview">
      <div class="tabhead"><span class="eyebrow">Targets · {roster.length}</span></div>
      {#if roster.length}
        <div class="dlist">
          {#each roster as r (r.key)}
            {@const tag = rowTag(r)}
            <div class="dcard">
              <div class="dcard-main">
                <div class="dcard-head">
                  <span class="mtag" style="color:{tag.color}">{tag.label}</span>
                  <span class="dcard-name">{r.name}</span>
                  {#if r.device?.identifier}<span class="dparen">({r.device.identifier})</span>{/if}
                </div>
                <div class="dcard-meta">
                  {#if r.device && serialFor(r.device)}<span>serial {serialFor(r.device)}</span>{/if}
                  <span>ecid {r.ecid ?? "—"}</span>
                  {#if r.device}<span>{r.device.mode}{r.device.port?.location ? ` · ${r.device.port.location}` : ""}</span>{:else}<span>disconnected</span>{/if}
                </div>
                {#if r.job && jobActive(r.job)}
                  <div class="dcard-prog">
                    <span class="pbar"><span class="pfill" style="width:{Math.max(3, Math.min(100, r.job.progress))}%; background:{tag.color}"></span></span>
                    <span class="jobpct">{Math.round(r.job.progress)}% · {r.job.step} · {fmtElapsed(r.job)}</span>
                  </div>
                {:else if r.job && r.job.status !== "queued"}
                  <div class="dcard-meta"><span style="color:{tag.color}">{r.job.status}{r.job.status === "failed" && r.job.message ? ` — ${r.job.message}` : ""}</span></div>
                {/if}
              </div>
              <div class="dcard-actions">
                {#if r.job && jobActive(r.job)}
                  <button class="btn danger sm" onclick={() => cancelJob(r.job!.id)}>Cancel</button>
                {:else if r.job}
                  <button class="btn sm" onclick={() => restartJob(r.job!.id)}>Restart</button>
                {:else if r.device}
                  {@const dm = deviceMode(r.device)}
                  {#if dm === "restore"}
                    <button class="btn primary sm" disabled={!!busy} onclick={() => { selectedKey = r.key; beginRestore(); }}>Erase & restore</button>
                    <button class="btn ghost sm" disabled={!!busy} onclick={() => { selectedKey = r.key; rebootTarget(); }}>Reboot</button>
                  {:else if dm === "dfu"}
                    <button class="btn primary sm" disabled={!!busy} onclick={() => { selectedKey = r.key; enterDfu(); }}>Enter DFU</button>
                  {:else if dm === "usb"}
                    <button class="btn primary sm" onclick={() => { selectedKey = r.key; restoreView = "detail"; }}>Set up USB</button>
                  {/if}
                {/if}
                <button class="btn ghost sm" title="Full config + log" onclick={() => { select(r.key); restoreView = "detail"; }}>Open →</button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="tabempty">No devices connected. Cable a Mac to this host's DFU port.</div>
      {/if}
    </section>
  {:else if tab === "list"}
    <section class="tabview">
      <div class="tabhead">
        <span class="seg">
          <button class="segbtn" class:on={devSubtab === "connected"} onclick={() => (devSubtab = "connected")}>Connected · {devices.length}</button>
          {#if historyEnabled}
            <button class="segbtn" class:on={devSubtab === "history"} onclick={() => { devSubtab = "history"; loadSeen(); }}>All seen · {seenDevices.length}</button>
          {/if}
        </span>
        <div class="grow"></div>
        {#if historyEnabled && devSubtab === "connected"}
          <button class="btn ghost sm" onclick={() => (showQrInList = !showQrInList)}>{showQrInList ? "Hide QR" : "Show QR"}</button>
        {/if}
        <button class="btn sm" onclick={doExportDevices} disabled={devSubtab === "history" ? !seenDevices.length : !devices.length}>Export CSV</button>
      </div>

      {#if devSubtab === "connected"}
        {#if devices.length}
          <table class="tbl">
            <thead>
              <tr><th>Serial</th><th>Model</th><th>ECID</th><th>Mode</th><th>Port</th>{#if showQrInList}<th>QR</th>{/if}<th></th></tr>
            </thead>
            <tbody>
              {#each devices as d (d.serial)}
                {@const s = serialFor(d)}
                {@const e = ecidFor(d)}
                {@const q = qrOf(d)}
                <tr>
                  <td><button class="cellcopy" onclick={() => copy(s ?? "")}>{s ?? "—"}</button></td>
                  <td>
                    <button class="cellcopy" onclick={() => copy(d.name)}>{d.name}</button>
                    {#if d.identifier}<button class="cellcopy cellsub" onclick={() => copy(d.identifier!)}>{d.identifier}</button>{/if}
                  </td>
                  <td><button class="cellcopy" onclick={() => copy(e ?? "")}>{e ?? "—"}</button></td>
                  <td><button class="cellcopy" onclick={() => copy(d.mode)}><span class="mtag" style="color:{MODE_COLOR[d.mode]}">{MODE_TAG[d.mode]}</span></button></td>
                  <td><button class="cellcopy" onclick={() => copy(d.port?.location ?? "")}>{d.port?.location ?? "—"}</button></td>
                  {#if showQrInList}
                    <td class="qrcell">{#if q && qrCache[q.value]}{@html qrCache[q.value]}{/if}</td>
                  {/if}
                  <td class="right nowrap">
                    <button class="iconbtn" title="Open in Restore" onclick={() => openInRestore(d)}>Open →</button>
                    {#if historyEnabled && q}
                      <button class="iconbtn" title="Show QR" aria-label="Show QR" onclick={() => showQr(q.value, q.label)}>QR</button>
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
      {:else if seenDevices.length}
        <table class="tbl">
          <thead>
            <tr><th>Serial</th><th>Model</th><th>ECID</th><th>Last mode</th><th>First seen</th><th>Last seen</th></tr>
          </thead>
          <tbody>
            {#each seenDevices as sd (sd.ecid)}
              <tr>
                <td><button class="cellcopy" onclick={() => copy(sd.serial_number ?? "")}>{sd.serial_number ?? "—"}</button></td>
                <td>
                  <button class="cellcopy" onclick={() => copy(sd.name)}>{sd.name}</button>
                  {#if sd.model_identifier}<button class="cellcopy cellsub" onclick={() => copy(sd.model_identifier!)}>{sd.model_identifier}</button>{/if}
                </td>
                <td><button class="cellcopy" onclick={() => copy(sd.ecid)}>{sd.ecid}</button></td>
                <td><span class="mtag">{sd.mode}</span></td>
                <td class="seentime">{fmtTime(sd.first_seen)}</td>
                <td class="seentime">{fmtTime(sd.last_seen)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="tabnote">Every Mac ever seen, deduped by ECID and enriched as it passes through modes.</p>
      {:else}
        <div class="tabempty">No devices seen yet. Connect a Mac and it's remembered here.</div>
      {/if}
    </section>
  {:else if tab === "history"}
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
  {:else}
    <section class="tabview about">
      <div class="tabhead"><span class="eyebrow">About</span></div>
      <h1 class="dtitle">restorekit{appVersion ? ` · ${appVersion}` : ""}</h1>
      <p class="lede">
        DFU-restore Apple Silicon Macs, from macOS, Linux or Windows. Free and open source under
        Apache-2.0. The DFU trigger is a Rust port of AsahiLinux's macvdmtool.
      </p>
      <div class="aboutmeta">
        <button class="cellcopy" onclick={() => copy("https://github.com/fcjr/restorekit")}>github.com/fcjr/restorekit</button>
      </div>

      <div class="tabhead" style="margin-top:26px">
        <span class="eyebrow">Third-party licenses · {licenseCount}</span>
      </div>
      <p class="tabnote" style="margin:0 0 12px">
        restorekit bundles these open-source components (generated with cargo-about plus the
        vendored C libraries). The restorekit source is Apache-2.0; macOS builds link Apache,
        LGPL and BSD components, while Linux and Windows builds also bundle GPL-3.0 usbmuxd and
        are conveyed as a whole under GPL-3.0.
      </p>
      <div class="licenses">{@html licensesHtml}</div>
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
    position: relative;
    display: flex;
    align-items: center;
    gap: 13px;
    height: 42px;
    padding: 0 15px;
    background: var(--bar);
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  /* Keep the tabs centered against the whole bar, not the space between the
     brand and the (variable-width) right-side controls. */
  .titlebar .tabs {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
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
  .rowsub {
    font-size: 10.5px;
    color: var(--dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .minibar {
    display: block;
    height: 4px;
    background: var(--line);
    overflow: hidden;
    margin: 1px 0 3px;
  }
  .minifill {
    display: block;
    height: 100%;
    transition: width 0.3s ease;
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
  /* Restore list view: full-width multi-line device cards */
  .dlist {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
  }
  .dcard {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 14px 16px;
    border-top: 1px solid var(--line);
  }
  .dcard:first-child {
    border-top: 0;
  }
  .dcard-main {
    flex: 1;
    min-width: 0;
  }
  .dcard-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
  }
  .dcard-name {
    font-size: 14px;
    color: var(--ink);
  }
  .dcard-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    margin-top: 5px;
    font-size: 11.5px;
    color: var(--mut);
  }
  .dcard-prog {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
    max-width: 460px;
  }
  .dcard-prog .pbar {
    flex: 1;
    height: 6px;
  }
  .dcard-prog .pfill {
    display: block;
    height: 100%;
  }
  .dcard-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .qrcell {
    width: 116px;
  }
  .qrcell :global(svg) {
    width: 104px;
    height: 104px;
    display: block;
    background: #fff;
    padding: 5px;
    border: 1px solid var(--line2);
  }
  .seentime {
    font-size: 11.5px;
    color: var(--mut);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
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
  .joblog-wrap {
    position: relative;
    margin: 4px 0 16px;
  }
  .joblog {
    max-height: 360px;
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
    /* Override the app-wide user-select:none so the log can be copied. */
    user-select: text;
    -webkit-user-select: text;
    cursor: auto;
  }
  .livebtn {
    position: absolute;
    right: 12px;
    bottom: 12px;
    border: 1px solid var(--acc);
    background: var(--raise);
    color: var(--acc);
    font: inherit;
    font-size: 10.5px;
    letter-spacing: 0.05em;
    padding: 4px 10px;
    cursor: pointer;
  }
  .livebtn:hover {
    background: var(--accsoft);
  }
  .tabnote {
    margin: 14px 0 0;
    font-size: 11px;
    color: var(--fnt);
  }

  /* about + licenses */
  .about .lede {
    max-width: 58ch;
  }
  .aboutmeta {
    margin-top: 4px;
    font-size: 12px;
    color: var(--mut);
  }
  .licenses {
    border: 1px solid var(--line);
    padding: 2px 16px;
  }
  .licenses :global(.lic) {
    border-top: 1px solid var(--line);
    padding: 14px 0;
  }
  .licenses :global(.lic:first-child) {
    border-top: 0;
  }
  .licenses :global(.lic-name) {
    margin: 0 0 4px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink);
  }
  .licenses :global(.lic-used) {
    margin-bottom: 8px;
    font-size: 11px;
    color: var(--mut);
    word-break: break-word;
  }
  .licenses :global(.lic-text) {
    margin: 0;
    max-height: 200px;
    overflow-y: auto;
    border: 1px solid var(--line);
    background: var(--bar);
    padding: 8px 10px;
    font-size: 10.5px;
    line-height: 1.5;
    color: var(--fnt);
    white-space: pre-wrap;
    user-select: text;
    -webkit-user-select: text;
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
