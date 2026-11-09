// The local index (src-tauri/src/semantic.rs) as the UI sees it: one shared,
// reactive status, kept current by the events Rust sends while it downloads and indexes.
import { indexCancelDownload, indexInstall, indexRemove, indexSetSettings, indexStatus, onIndexStatus } from "./api";
import { t } from "./i18n/index.svelte";
import type { IndexSettings, IndexStatus } from "./types";

export const localIndex = $state<{ status: IndexStatus | null; busy: boolean }>({ status: null, busy: false });

let started = false;

/** Loads the status once and follows it from then on. */
export function watchIndex(): void {
  if (started) return;
  started = true;
  onIndexStatus((s) => (localIndex.status = s)).catch(() => {});
  indexStatus()
    .then((s) => (localIndex.status = s))
    .catch(() => {});
}

async function run(action: () => Promise<IndexStatus | void>) {
  localIndex.busy = true;
  try {
    const s = await action();
    if (s) localIndex.status = s;
  } catch (e) {
    if (localIndex.status) localIndex.status = { ...localIndex.status, error: String(e) };
  } finally {
    localIndex.busy = false;
  }
}

export const installIndex = () => run(indexInstall);
export const cancelIndexDownload = () => run(indexCancelDownload);
export const removeIndex = () => run(indexRemove);

/** Saves a change of the choices. */
export function changeIndex(change: Partial<IndexSettings>): Promise<void> {
  const current = localIndex.status?.settings;
  if (!current) return Promise.resolve();
  return run(() => indexSetSettings({ ...current, ...change }));
}

/** The model is downloaded: a topic can be looked for. */
export const indexReady = () => !!localIndex.status?.ready;

/** An `index.error.*` key (maybe with `:detail`) as text for the user. */
export function indexErrorText(e: string): string {
  const at = e.indexOf(":");
  const key = at < 0 ? e : e.slice(0, at);
  if (!key.startsWith("index.error.")) return e;
  return t(key, { detail: at < 0 ? "" : e.slice(at + 1) });
}
