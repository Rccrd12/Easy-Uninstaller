// Builds the blocks of the space map from the measured folders (space.rs) and
// the installed apps (apps.rs). Pure functions: no system access here.
import type { InstalledApp, MapCategory, SpaceBlock, SpaceUsage } from "./types";

export type MapView = "apps" | "files" | "all";

/** Which categories each view shows. */
export const VIEW_CATEGORIES: Record<MapView, MapCategory[]> = {
  apps: ["apps", "games"],
  files: ["personal", "reclaimable"],
  all: ["apps", "games", "personal", "reclaimable", "system", "free"],
};

/** Publishers whose apps are games (lowercase, matched as a prefix). */
const GAME_PUBLISHERS = [
  "activision",
  "bandai namco",
  "bethesda",
  "blizzard",
  "bungie",
  "capcom",
  "cd projekt",
  "electronic arts",
  "epic games",
  "mojang",
  "paradox interactive",
  "riot games",
  "rockstar games",
  "sega",
  "square enix",
  "ubisoft",
  "valve",
  "xbox game studios",
];

/** Games are told apart from other apps by their store or publisher. */
export function isGame(app: InstalledApp): boolean {
  if (/^reg:[a-z]+:steam app \d+$/i.test(app.id)) return true;
  const publisher = app.publisher.toLowerCase();
  return GAME_PUBLISHERS.some((p) => publisher.startsWith(p));
}

/**
 * Every block of the drive: one per app, one per measured folder, what is left
 * of the used space ("Windows and other") and the free space.
 */
export function spaceBlocks(usage: SpaceUsage, apps: InstalledApp[]): SpaceBlock[] {
  const blocks: SpaceBlock[] = [];
  for (const app of apps) {
    if (app.sizeBytes <= 0) continue;
    blocks.push({
      id: `app:${app.id}`,
      labelKey: null,
      label: app.name,
      sizeBytes: app.sizeBytes,
      category: isGame(app) ? "games" : "apps",
      path: null,
      groupedCount: 0,
    });
  }
  for (const f of usage.folders) {
    if (f.sizeBytes > 0) blocks.push({ ...f, groupedCount: 0 });
  }
  const used = Math.max(0, usage.totalBytes - usage.freeBytes);
  const measured = blocks.reduce((s, b) => s + b.sizeBytes, 0);
  // Apps installed on another drive can make `measured` bigger than `used`: never go below zero.
  const rest = used - measured;
  if (rest > 0) blocks.push(fixed("system", "map.blocks.system", rest, "system"));
  if (usage.freeBytes > 0) blocks.push(fixed("free", "map.blocks.free", usage.freeBytes, "free"));
  return blocks.sort((a, b) => b.sizeBytes - a.sizeBytes);
}

function fixed(id: string, labelKey: string, sizeBytes: number, category: MapCategory): SpaceBlock {
  return { id, labelKey, label: null, sizeBytes, category, path: null, groupedCount: 0 };
}

/** App data (settings and caches of every app) cannot be uninstalled, so it sits with the files. */
const APP_DATA = "appdata";

function inView(b: SpaceBlock, view: MapView): boolean {
  if (b.id === APP_DATA) return view !== "apps";
  return VIEW_CATEGORIES[view].includes(b.category);
}

/** Categories present in a view, for the legend. */
export function legendFor(blocks: SpaceBlock[], view: MapView): MapCategory[] {
  return VIEW_CATEGORIES.all.filter((c) => blocks.some((b) => b.category === c && inView(b, view)));
}

/**
 * The blocks one view shows. Blocks too small to read are put together into one
 * "Other ..." block per category, so the map stays readable on any PC.
 */
export function blocksForView(all: SpaceBlock[], view: MapView, minShare = 0.006, maxBlocks = 36): SpaceBlock[] {
  const shown = all.filter((b) => inView(b, view)).sort((a, b) => b.sizeBytes - a.sizeBytes);
  const total = shown.reduce((s, b) => s + b.sizeBytes, 0);
  const kept: SpaceBlock[] = [];
  const small = new Map<MapCategory, SpaceBlock[]>();
  for (const b of shown) {
    const big = b.sizeBytes >= total * minShare && kept.length < maxBlocks;
    if (big || b.category === "system" || b.category === "free") kept.push(b);
    else small.set(b.category, [...(small.get(b.category) ?? []), b]);
  }
  for (const [category, items] of small) {
    if (items.length === 1) {
      kept.push(items[0]);
      continue;
    }
    kept.push({
      id: `other:${category}`,
      labelKey: `map.blocks.other.${category}`,
      label: null,
      sizeBytes: items.reduce((s, b) => s + b.sizeBytes, 0),
      category,
      path: null,
      groupedCount: items.length,
    });
  }
  return kept.sort((a, b) => b.sizeBytes - a.sizeBytes);
}
