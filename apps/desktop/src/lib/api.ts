import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export type Mode = "dfu" | "recovery" | "wtf" | "restore" | "other";

export interface Device {
  mode: Mode;
  name: string;
  identifier: string | null;
  chip: string;
  board: string;
  ecid: string;
  srtg: string | null;
  serial: string;
  restorable: boolean;
  /** Windows: false until WinUSB is bound. Always true on macOS/Linux. */
  driver_ready: boolean;
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

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return browserMock(cmd) as Promise<T>;
  return invoke<T>(cmd, args);
}

/// Error string the backend returns when the privileged helper needs the
/// one-time approval; the UI matches it to show the approval screen.
export const APPROVAL_REQUIRED = "helper-approval-required";

export const api = {
  hostCanTrigger: () => call<boolean>("host_can_trigger"),
  manualInstructions: () => call<string>("manual_instructions"),
  listDevices: () => call<Device[]>("list_devices"),
  triggerDfu: () => call<void>("trigger_dfu"),
  rebootTarget: () => call<void>("reboot_target"),
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
};

function browserMock(cmd: string): Promise<unknown> {
  const devices: Device[] = [
    { mode: "dfu", name: "MacBook Pro (M1, Late 2020)", identifier: "MacBookPro17,1", chip: "CPID:8103", board: "BDID:24", ecid: "0x1a2b3c4d5e6f", srtg: "iBoot-11881.60.5", serial: "SDOM:01 CPID:8103 ECID:1a2b3c4d5e6f", restorable: true, driver_ready: true },
    { mode: "recovery", name: "MacBook Air (M2, 2022)", identifier: "Mac14,2", chip: "CPID:8112", board: "BDID:28", ecid: "0x77aa22bb44cc", srtg: "iBoot-10151.1.1", serial: "SDOM:01 CPID:8112 ECID:77aa22bb44cc", restorable: false, driver_ready: true },
    { mode: "other", name: "Apple device", identifier: null, chip: "", board: "", ecid: "", srtg: null, serial: "0x998877", restorable: false, driver_ready: true },
  ];
  const map: Record<string, unknown> = {
    host_can_trigger: true,
    manual_instructions: "1. Connect the target's DFU port.\n2. Disconnect power.\n3. Hold power, reconnect, keep holding ~10s.",
    list_devices: devices,
    helper_status: "enabled",
    cache_info: { path: "~/.config/restorekit/firmwares", bytes: 19_769_902_281, count: 1 },
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
  other: { label: "Connected", hint: "not in a restore mode" },
};
