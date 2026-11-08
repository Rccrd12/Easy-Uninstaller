// Checking for a newer version and installing it, shared by Settings and the
// notice that appears when the daily check finds one. Reactive.
import { checkForUpdate, installUpdate } from "./api";
import { settings } from "./settings.svelte";
import type { UpdateInfo } from "./types";

type Phase =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "upToDate" }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "downloading"; info: UpdateInfo; percent: number | null }
  | { kind: "installing"; info: UpdateInfo }
  | { kind: "failed"; offline: boolean; error: string; info: UpdateInfo | null };

export const update = $state<{ phase: Phase; noticeDismissed: boolean }>({ phase: { kind: "idle" }, noticeDismissed: false });

const LAST_CHECK_KEY = "lastUpdateCheck";
const DAY_MS = 24 * 60 * 60 * 1000;

function failed(e: unknown, info: UpdateInfo | null): Phase {
  const error = String(e);
  return { kind: "failed", offline: error === "offline", error, info };
}

/** Asks for a newer version (the "Check for updates" button). */
export async function check(): Promise<void> {
  const busy = update.phase.kind;
  if (busy === "checking" || busy === "downloading" || busy === "installing") return;
  update.phase = { kind: "checking" };
  try {
    const info = await checkForUpdate();
    update.phase = info ? { kind: "available", info } : { kind: "upToDate" };
    remember();
  } catch (e) {
    update.phase = failed(e, null);
  }
}

/** Downloads and installs the version found by the last check. */
export async function install(): Promise<void> {
  const p = update.phase;
  const info = p.kind === "available" ? p.info : p.kind === "failed" ? p.info : null;
  if (!info) return;
  update.phase = { kind: "downloading", info, percent: null };
  try {
    await installUpdate(({ downloadedBytes, totalBytes }) => {
      if (totalBytes && downloadedBytes >= totalBytes) update.phase = { kind: "installing", info };
      else update.phase = { kind: "downloading", info, percent: totalBytes ? Math.floor((downloadedBytes / totalBytes) * 100) : null };
    });
    update.phase = { kind: "installing", info };
  } catch (e) {
    update.phase = failed(e, info);
  }
}

/**
 * On start: a quiet check at most once a day, when the user left it on.
 * Failures stay quiet too (the PC may simply be offline); the button in Settings shows them.
 */
export async function checkOnStart(): Promise<void> {
  if (!settings.checkUpdates) return;
  let last = 0;
  try {
    last = Number(localStorage.getItem(LAST_CHECK_KEY)) || 0;
  } catch {
    // No storage: check every start.
  }
  if (Date.now() - last < DAY_MS) return;
  update.phase = { kind: "checking" };
  try {
    const info = await checkForUpdate();
    update.phase = info ? { kind: "available", info } : { kind: "idle" };
    remember();
  } catch {
    update.phase = { kind: "idle" };
  }
}

function remember() {
  try {
    localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
  } catch {
    // Not kept; the next start checks again.
  }
}
