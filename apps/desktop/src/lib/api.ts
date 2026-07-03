import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Device {
  name: string;
  identifier: string | null;
  chip: string;
  board: string;
  ecid: string;
  srtg: string | null;
  serial: string;
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

/** A library progress event, discriminated by `event` (matches the CLI --json). */
export type ProgressEvent =
  | { event: "dfu_trigger_stage"; stage: string }
  | { event: "cache_hit"; path: string }
  | { event: "download_resumed"; received: number }
  | { event: "download_progress"; received: number; total: number }
  | { event: "verifying" }
  | { event: "restore_step"; step: number; name: string; progress: number }
  | { event: "done" }
  | { event: string; [k: string]: unknown };

export const api = {
  hostCanTrigger: () => invoke<boolean>("host_can_trigger"),
  manualInstructions: () => invoke<string>("manual_instructions"),
  listDevices: () => invoke<Device[]>("list_devices"),
  cacheDir: () => invoke<string>("cache_dir"),
  triggerDfu: () => invoke<Device>("trigger_dfu"),
  resolveFirmware: (identifier: string, osVersion?: string) =>
    invoke<Firmware>("resolve_firmware", { identifier, osVersion: osVersion ?? null }),
  downloadFirmware: (firmware: Firmware) => invoke<string>("download_firmware", { firmware }),
  restore: (ipsw: string, serial: string, revive: boolean) =>
    invoke<void>("restore", { ipsw, serial, revive }),
};

export function onProgress(cb: (e: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>("progress", (ev) => cb(ev.payload));
}

export function gib(bytes: number): string {
  return (bytes / 1e9).toFixed(1) + " GB";
}
