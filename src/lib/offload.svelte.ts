// "Free up space without deleting": the OneDrive and "move to another drive" cards
// of the Files screen share one scan, kept here until a new scan is asked for.
import { currentOffload, scanOffload } from "./api";
import type { OffloadScan } from "./types";

export const offload = $state({
  scan: null as OffloadScan | null,
  scanning: false,
  error: null as string | null,
});

let pending: Promise<void> | null = null;

/** Scans once; `rescan` scans again (the Files screen's "Scan again"). */
export function loadOffload(rescan = false): Promise<void> {
  if (pending && !rescan) return pending;
  if (offload.scan && !rescan) return Promise.resolve();
  offload.scanning = true;
  offload.error = null;
  const run = scanOffload()
    .then((s) => {
      offload.scan = s;
    })
    .catch((e) => {
      offload.error = String(e);
    })
    .finally(() => {
      if (pending === run) pending = null;
      offload.scanning = false;
    });
  pending = run;
  return run;
}

/** Reads the cards again after a change, without scanning. */
export async function refreshOffload(): Promise<void> {
  offload.scan = await currentOffload();
}

/** "D: (Data)", or "D:" when the drive has no name. */
export const driveName = (d: { mountPoint: string; label: string }) => (d.label ? `${d.mountPoint} (${d.label})` : d.mountPoint);
