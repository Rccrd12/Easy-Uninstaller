// "What is this?": what the app knows about common folders, without AI.
// The list is shared with Rust (which matches paths and sends `explainId`);
// the texts are i18n keys under explain.folders.<id>.
import known from "../../src-tauri/src/known_folders.json";

export type Verdict = "safe" | "check" | "keep";

export interface KnownFolder {
  id: string;
  icon: string;
  verdict: Verdict;
  /** Product names, not translated. */
  madeBy: string;
}

const byId = new Map<string, KnownFolder>(known.folders.map((f) => [f.id, f as KnownFolder]));

export function knownFolder(id: string | null | undefined): KnownFolder | null {
  return id ? (byId.get(id) ?? null) : null;
}
