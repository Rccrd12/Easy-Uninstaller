// Single entry point for everything the UI asks the system.
// Inside the Tauri app calls go to Rust commands; in a plain browser
// (npm run dev) they fall back to sample data so the UI can be designed fast.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { GB } from "./format";
import { sampleApps, sampleFileGroups, sampleLeftovers, sampleQuarantine, sampleSpace } from "./mock";
import type { DiskInfo, FileGroup, InstalledApp, LeftoverItem, QuarantineItem, SpaceBlock } from "./types";

export const QUARANTINE_DAYS = 7;

/** Real: implemented in src-tauri/src/disk.rs. */
export async function getSystemDisk(): Promise<DiskInfo> {
  if (isTauri()) return invoke<DiskInfo>("get_system_disk");
  return { mountPoint: "C:", totalBytes: 476 * GB, freeBytes: 64 * GB };
}

/** Sample until the registry + Store reader lands. */
export async function listInstalledApps(): Promise<InstalledApp[]> {
  return sampleApps;
}

/** Sample until the file scanner lands. */
export async function listFileGroups(): Promise<FileGroup[]> {
  return sampleFileGroups;
}

/** Sample until the leftovers scanner lands. */
export async function listLeftovers(): Promise<LeftoverItem[]> {
  return sampleLeftovers;
}

/** Sample until the space analyzer lands. */
export async function getSpaceMap(): Promise<SpaceBlock[]> {
  return sampleSpace;
}

/** Sample until quarantine lands. */
export async function listQuarantine(): Promise<QuarantineItem[]> {
  return sampleQuarantine;
}
