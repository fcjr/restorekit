import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { downloadDir, join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";

export type Mode = "dfu" | "recovery" | "wtf" | "restore" | "booted" | "other";

/** The host USB-C port a device is cabled to (macOS only). */
export interface Port {
  /** Whether this is the host's DFU-capable port — the one we trigger DFU on. */
  dfu: boolean;
  /** Firmware location label, e.g. "left-back", when available. */
  location: string | null;
}

export interface Device {
  mode: Mode;
  name: string;
  identifier: string | null;
  chip: string;
  board: string;
  ecid: string;
  srtg: string | null;
  serial: string;
  /** Captured hardware serial (recovery/booted); null when unavailable (e.g. DFU). */
  serial_number: string | null;
  restorable: boolean;
  /** Host port and whether it's DFU-capable (macOS); null when undeterminable. */
  port: Port | null;
  /** Windows: false until WinUSB is bound. Always true on macOS/Linux. */
  driver_ready: boolean;
  /** How this device reaches the host, by USB topology. */
  connection: "direct" | "dongle" | "hub";
  /** The dongle id when reached through one; null otherwise. DFU/reboot route
   *  over this dongle instead of the host trigger. */
  via_dongle: string | null;
  /** Whether the host's own USB-PD trigger can put it into DFU (direct + on the
   *  DFU port). False when reached via a dongle or plain hub. */
  host_dfu_capable: boolean;
}

/** Live PD status read from a dongle over its vendor USB interface. */
export interface DongleStatus {
  pd_state: "disconnected" | "vbus-on" | "connected" | "accept" | "idle" | "unknown";
  target_attached: boolean;
  /** Cable orientation: true = CC2 (flipped), false = CC1 (normal). */
  polarity_cc2: boolean;
}

/** A connected RecoverKit dongle and the Mac (if any) cabled to it. */
export interface Dongle {
  serial: string;
  product: string;
  /** Live status; null if the vendor interface couldn't be read. */
  status: DongleStatus | null;
  /** The cabled Mac, if its USB data reaches this host; null otherwise. */
  target: Device | null;
}

export interface Firmware {
  identifier: string;
  version: string;
  build: string;
  url: string;
  size: number;
  sha256: string | null;
  sha1: string | null;
  signed: boolean;
}

export interface CacheInfo {
  path: string;
  bytes: number;
  count: number;
}

/** A parallel restore job tracked by the backend job manager. */
export interface JobView {
  id: number;
  name: string;
  ecid: string;
  /** queued | running | done | failed | canceled */
  status: string;
  step: string;
  progress: number;
  message: string;
}

/** A device ever seen by this host, deduped by ECID across modes. */
export interface SeenDevice {
  ecid: string;
  serial_number: string | null;
  model_identifier: string | null;
  name: string;
  chip: string | null;
  board: string | null;
  mode: string;
  port: string | null;
  first_seen: string;
  last_seen: string;
}

/** One row in the persistent device-history log. */
export interface HistoryEntry {
  serial_number: string | null;
  ecid: string;
  model_identifier: string | null;
  name: string;
  mode: string;
  status: string;
  timestamp_rfc3339: string;
}

/** A library progress event, discriminated by `event` (matches the CLI --json). */
export type ProgressEvent =
  | { event: "cache_hit"; path: string }
  | { event: "download_resumed"; received: number }
  | { event: "download_progress"; received: number; total: number }
  | { event: "verifying" }
  | { event: "restore_step"; step: number; name: string; progress: number }
  | { event: "done" }
  | { event: string; [k: string]: unknown };

// Running inside the Tauri webview? When not (a plain browser during
// `npm run dev`), commands fall back to sample data so the UI can be developed
// and reviewed without a full build. In the real app __TAURI_INTERNALS__ is
// always present, so the real backend is always used.
const inTauri = typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined";

/** Whether we're running inside the Tauri webview (vs a plain dev browser). */
export const isTauri = inTauri;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return browserMock(cmd) as Promise<T>;
  return invoke<T>(cmd, args);
}

/// Error string the backend returns when the privileged helper needs the
/// one-time approval; the UI matches it to show the approval screen.
export const APPROVAL_REQUIRED = "helper-approval-required";

export const api = {
  hostCanTrigger: () => call<boolean>("host_can_trigger"),
  historyEnabled: () => call<boolean>("history_enabled"),
  manualInstructions: () => call<string>("manual_instructions"),
  listDevices: () => call<Device[]>("list_devices"),
  /** Trigger DFU. Pass a dongle id to route over that dongle (any host OS, no
   *  helper); omit to use the host's electronic trigger (Apple Silicon macOS). */
  triggerDfu: (dongle?: string) => call<Device>("trigger_dfu", { dongle: dongle ?? null }),
  /** Reboot the target. Pass a dongle id to route over it; omit for the host. */
  rebootTarget: (dongle?: string) => call<void>("reboot_target", { dongle: dongle ?? null }),
  listDongles: () => call<Dongle[]>("list_dongles"),
  dongleDfu: (serial: string) => call<void>("dongle_dfu", { serial }),
  dongleReboot: (serial: string) => call<void>("dongle_reboot", { serial }),
  helperStatus: () => call<string>("helper_status"),
  approveHelper: () => call<void>("approve_helper"),
  setupDriver: () => call<void>("setup_driver"),
  focusApp: () => call<void>("focus_app"),
  resolveFirmware: (identifier: string, osVersion?: string) =>
    call<Firmware>("resolve_firmware", { identifier, osVersion: osVersion || null }),
  downloadFirmware: (firmware: Firmware) => call<string>("download_firmware", { firmware }),
  restore: (ipsw: string, serial: string, revive: boolean) =>
    call<void>("restore", { ipsw, serial, revive }),
  cacheInfo: () => call<CacheInfo>("cache_info"),
  clearCache: () => call<void>("clear_cache"),
  historyList: () => call<HistoryEntry[]>("history_list"),
  recordCapture: (entry: HistoryEntry) => call<void>("record_capture", { entry }),
  historyClear: () => call<void>("history_clear"),
  serialQrSvg: (text: string) => call<string>("serial_qr_svg", { text }),
  enqueueRestore: (ipsw: string, ecid: string, name: string, revive: boolean) =>
    call<number>("enqueue_restore", { ipsw, ecid, name, revive }),
  cancelRestore: (id: number) => call<void>("cancel_restore", { id }),
  restartRestore: (id: number) => call<void>("restart_restore", { id }),
  clearRestoreJob: (id: number) => call<void>("clear_restore_job", { id }),
  listRestoreJobs: () => call<JobView[]>("list_restore_jobs"),
  getSettings: () => call<Settings>("get_settings"),
  setAutoDfu: (enabled: boolean) => call<void>("set_auto_dfu", { enabled }),
  openAppleConfigurator: () => call<void>("open_apple_configurator"),
  recordSeenDevices: (devices: SeenDevice[]) => call<void>("record_seen_devices", { devices }),
  listSeenDevices: () => call<SeenDevice[]>("list_seen_devices"),
};

/** Persisted app settings. */
export interface Settings {
  auto_dfu: boolean;
}

/** Live restore-job status updates. No-op (never fires) outside Tauri. */
export function onRestoreJobUpdate(cb: (j: JobView) => void): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<JobView>("restore_job_update", (ev) => cb(ev.payload));
}

/** Live restore-job log lines. No-op outside Tauri. */
export function onRestoreJobLog(
  cb: (l: { id: number; level: number; line: string }) => void,
): Promise<UnlistenFn> {
  if (!inTauri) return Promise.resolve(() => {});
  return listen<{ id: number; level: number; line: string }>("restore_job_log", (ev) => cb(ev.payload));
}

/** Native save dialog (defaulting to Downloads) + backend CSV write. Returns
 *  true if a file was written, false if cancelled or running in a browser. */
async function saveCsv(cmd: string, filename: string): Promise<boolean> {
  if (!inTauri) return false;
  let defaultPath = filename;
  try {
    defaultPath = await join(await downloadDir(), filename);
  } catch {
    /* fall back to a bare filename if the Downloads dir can't be resolved */
  }
  const path = await save({ defaultPath, filters: [{ name: "CSV", extensions: ["csv"] }] });
  if (!path) return false;
  await invoke(cmd, { path });
  return true;
}

export const exportHistoryCsv = () => saveCsv("export_history_csv", "restorekit-history.csv");
export const exportDevicesCsv = () => saveCsv("export_devices_csv", "restorekit-devices.csv");
export const exportSeenCsv = () => saveCsv("export_seen_csv", "restorekit-devices-history.csv");

function browserMock(cmd: string): Promise<unknown> {
  const devices: Device[] = [
    { mode: "dfu", name: "MacBook Pro (M1, Late 2020)", identifier: "MacBookPro17,1", chip: "CPID:8103", board: "BDID:24", ecid: "0x1a2b3c4d5e6f", srtg: "iBoot-11881.60.5", serial: "SDOM:01 CPID:8103 ECID:1a2b3c4d5e6f", serial_number: null, restorable: true, port: { dfu: true, location: "left-back" }, driver_ready: true, connection: "direct", via_dongle: null, host_dfu_capable: true },
    { mode: "booted", name: "MacBook Air (M2, 2022)", identifier: "Mac14,2", chip: "CPID:8112", board: "BDID:28", ecid: "0x77aa22bb44cc", srtg: "iBoot-10151.1.1", serial: "SDOM:01 CPID:8112 ECID:77aa22bb44cc", serial_number: "C02XX1234567", restorable: false, port: { dfu: true, location: "left-back" }, driver_ready: true, connection: "dongle", via_dongle: "DPL-1A2B3C4D", host_dfu_capable: false },
    { mode: "other", name: "Apple device", identifier: null, chip: "", board: "", ecid: "", srtg: null, serial: "0x998877", serial_number: null, restorable: false, port: null, driver_ready: true, connection: "direct", via_dongle: null, host_dfu_capable: false },
  ];
  const history: HistoryEntry[] = [
    { serial_number: "C02XX1234567", ecid: "0x77aa22bb44cc", model_identifier: "Mac14,2", name: "MacBook Air (M2, 2022)", mode: "recovery", status: "captured", timestamp_rfc3339: "2026-07-07T15:04:00.000Z" },
    { serial_number: "C02YY7654321", ecid: "0x1a2b3c4d5e6f", model_identifier: "MacBookPro17,1", name: "MacBook Pro (M1, Late 2020)", mode: "booted", status: "restored", timestamp_rfc3339: "2026-07-07T14:12:00.000Z" },
  ];
  const dongles: Dongle[] = [
    { serial: "DPL-1A2B3C4D", product: "Dongle-Proto-Lite", status: { pd_state: "connected", target_attached: true, polarity_cc2: true }, target: devices[0] },
    { serial: "DPL-99887766", product: "Dongle-Proto-Lite", status: { pd_state: "disconnected", target_attached: false, polarity_cc2: false }, target: null },
  ];
  const map: Record<string, unknown> = {
    host_can_trigger: true,
    history_enabled: true,
    list_dongles: dongles,
    dongle_dfu: null,
    dongle_reboot: null,
    manual_instructions: "1. Connect the target's DFU port.\n2. Disconnect power.\n3. Hold power, reconnect, keep holding ~10s.",
    list_devices: devices,
    trigger_dfu: devices[0],
    helper_status: "enabled",
    cache_info: { path: "~/.config/restorekit/firmwares", bytes: 19_769_902_281, count: 1 },
    history_list: history,
    list_restore_jobs: [
      { id: 1, name: "MacBook Pro (M1, Late 2020)", ecid: "0x1a2b3c4d5e6f", status: "running", step: "Sending filesystem", progress: 42, message: "" },
      { id: 2, name: "MacBook Air (M2, 2022)", ecid: "0x77aa22bb44cc", status: "queued", step: "queued", progress: 0, message: "" },
      { id: 3, name: "Mac Studio (M1 Max)", ecid: "0x33bb0011", status: "failed", step: "failed", progress: 0, message: "unable to get SHSH blobs for this device" },
    ] as JobView[],
    enqueue_restore: 1,
    cancel_restore: null,
    restart_restore: null,
    clear_restore_job: null,
    get_settings: { auto_dfu: false } as Settings,
    set_auto_dfu: null,
    open_apple_configurator: null,
    record_seen_devices: null,
    list_seen_devices: [
      { ecid: "0x1a2b3c4d5e6f", serial_number: "C02YY7654321", model_identifier: "MacBookPro17,1", name: "MacBook Pro (M1, Late 2020)", chip: "CPID:8103", board: "BDID:24", mode: "dfu", port: "left-back", first_seen: "2026-07-06T09:00:00.000Z", last_seen: "2026-07-07T15:04:00.000Z" },
      { ecid: "0x77aa22bb44cc", serial_number: "C02XX1234567", model_identifier: "Mac14,2", name: "MacBook Air (M2, 2022)", chip: "CPID:8112", board: "BDID:28", mode: "recovery", port: "right", first_seen: "2026-07-07T11:00:00.000Z", last_seen: "2026-07-07T11:20:00.000Z" },
    ] as SeenDevice[],
    record_capture: null,
    history_clear: null,
    serial_qr_svg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="220" height="220"><rect width="100" height="100" fill="#fff"/><rect x="12" y="12" width="24" height="24" fill="#000"/><rect x="64" y="12" width="24" height="24" fill="#000"/><rect x="12" y="64" width="24" height="24" fill="#000"/><rect x="46" y="46" width="8" height="8" fill="#000"/><rect x="64" y="64" width="10" height="10" fill="#000"/><rect x="78" y="78" width="10" height="10" fill="#000"/></svg>',
  };
  return Promise.resolve(map[cmd] ?? null);
}

export function onProgress(cb: (e: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("progress", (ev) => cb(ev.payload));
}

/** Native file picker for a local .ipsw. Returns the path, or null if cancelled. */
export async function pickIpsw(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "IPSW firmware", extensions: ["ipsw"] }],
  });
  return typeof picked === "string" ? picked : null;
}

export function gib(bytes: number): string {
  return (bytes / 1e9).toFixed(1) + " GB";
}

export const MODES: Record<Mode, { label: string; hint: string }> = {
  dfu: { label: "DFU", hint: "ready to restore" },
  recovery: { label: "Recovery", hint: "put in DFU to restore" },
  restore: { label: "Restore", hint: "restore in progress" },
  wtf: { label: "WTF", hint: "low-level mode" },
  booted: { label: "Booted", hint: "running macOS — put in DFU to restore" },
  other: { label: "Connected", hint: "not in a restore mode" },
};
