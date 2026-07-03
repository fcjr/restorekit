import { check } from "@tauri-apps/plugin-updater";
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";

export async function checkForUpdates() {
  try {
    const update = await check();
    if (!update) return;
    const install = await ask(
      `RestoreKit ${update.version} is available (you have ${update.currentVersion}). Install and restart now?`,
      { title: "Update available", kind: "info", okLabel: "Install", cancelLabel: "Later" },
    );
    if (!install) return;
    await update.downloadAndInstall();
    await relaunch();
  } catch {
    // offline or the release feed is unreachable — check again next launch
  }
}
