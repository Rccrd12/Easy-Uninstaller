import type { IconName } from "./icons";
// The AI help for unknown folders, shared by the Files screen, the "What is this?"
// panel and Settings: which services exist, the answers kept on this PC, and the
// setup window. Everything reaches Rust through api.ts.

import { aiAsk, aiForgetAnswers, aiSavedAnswers, onAiStart } from "./api";
import { getLanguage, t } from "./i18n/index.svelte";
import { saveSettings, settings } from "./settings.svelte";
import type { AiAnswer, AiConfig, FileItem } from "./types";

export type AiKind = "cli" | "api" | "local";

export interface AiServiceInfo {
  id: string;
  kind: AiKind;
  icon: IconName;
  /** It cannot answer without a key (Unsloth Studio wants its sk-unsloth one). */
  needsKey: boolean;
  /** A key can be given but is not required. */
  optionalKey?: boolean;
  /** The user types its address ("other service"), or may change it (servers on this PC). */
  url?: "required" | "optional";
  /** Where the address points by default, shown under the name. */
  defaultUrl?: string;
  /** A help page Rust can open (src-tauri/src/ai.rs, PAGES). */
  keyPage?: string;
  installPage?: string;
}

/** Same ids and order as src-tauri/src/ai/services.rs. Names are i18n keys `ai.services.<id>`. */
export const AI_SERVICES: AiServiceInfo[] = [
  { id: "claude-code", kind: "cli", icon: "console", needsKey: false, installPage: "install:claude-code" },
  { id: "codex", kind: "cli", icon: "console", needsKey: false, installPage: "install:codex" },
  { id: "antigravity-cli", kind: "cli", icon: "console", needsKey: false, installPage: "install:antigravity-cli" },
  { id: "anthropic", kind: "api", icon: "cloud", needsKey: true, keyPage: "key:anthropic" },
  { id: "openai", kind: "api", icon: "cloud", needsKey: true, keyPage: "key:openai" },
  { id: "google", kind: "api", icon: "cloud", needsKey: true, keyPage: "key:google" },
  { id: "openrouter", kind: "api", icon: "cloud", needsKey: true, keyPage: "key:openrouter" },
  { id: "custom", kind: "api", icon: "settings", needsKey: false, optionalKey: true, url: "required" },
  { id: "ollama", kind: "local", icon: "laptop", needsKey: false, url: "optional", defaultUrl: "localhost:11434", installPage: "install:ollama" },
  { id: "lmstudio", kind: "local", icon: "laptop", needsKey: false, url: "optional", defaultUrl: "localhost:1234", installPage: "install:lmstudio" },
  { id: "unsloth", kind: "local", icon: "laptop", needsKey: true, url: "optional", defaultUrl: "localhost:8888", installPage: "install:unsloth" },
  { id: "local-custom", kind: "local", icon: "settings", needsKey: false, optionalKey: true, url: "required" },
];

/** Models we suggest first, when the service offers them. */
const RECOMMENDED: Record<string, string> = {
  "claude-code": "sonnet",
  anthropic: "claude-sonnet-5-5",
};

export function serviceInfo(id: string): AiServiceInfo | undefined {
  return AI_SERVICES.find((s) => s.id === id);
}

export function recommendedModel(service: string): string | undefined {
  return RECOMMENDED[service];
}

/** The service and model picked in Settings, as Rust takes them. */
export function aiConfig(): AiConfig {
  const { service, model, urls, autoStart } = settings.ai;
  return { service, model, url: urls[service] ?? "", autoStart };
}

/** The AI is on and has a service and a model. */
export function aiReady(): boolean {
  const a = settings.ai;
  const kind = serviceInfo(a.service)?.kind;
  if (a.mode === "off" || !kind || !a.model) return false;
  return a.mode === "local" ? kind === "local" : kind !== "local";
}

/** "Claude Code · Sonnet": who answered. */
export function serviceLabel(service: string, model: string): string {
  const name = t(`ai.services.${service}.name`);
  const a = settings.ai;
  const label = a.service === service && a.model === model && a.modelLabel ? a.modelLabel : model;
  return label ? `${name} · ${label}` : name;
}

/** An error from Rust ("ai.error.x" or "ai.error.x|details") as a sentence. */
export function aiErrorText(error: unknown): string {
  const text = String(error);
  const [key, ...rest] = text.split("|");
  if (!key.startsWith("ai.error.")) return t("ai.error.other", { error: text });
  const detail = rest.join("|");
  const name = t(`ai.services.${settings.ai.service}.name`);
  const sentence = t(key, { service: name });
  return detail ? `${sentence} (${detail})` : sentence;
}

/** Reactive UI state for the AI help. */
export const ai = $state({
  /** Answers by lower-case path. */
  answers: {} as Record<string, AiAnswer>,
  /** Paths being asked about now (lower-case). */
  asking: {} as Record<string, boolean>,
  /** "Ask for all": how far it got. */
  batch: null as { done: number; total: number } | null,
  /** The setup window, and what to do once the AI is set up. */
  setup: null as { then?: () => void } | null,
  /** Seconds Unsloth Studio has been starting for, while the app starts it. */
  starting: null as number | null,
});

onAiStart((s) => (ai.starting = s.state === "starting" ? s.seconds : null)).catch(() => {});

const key = (path: string) => path.toLowerCase();

/** Which "Ask for all" is running, so a stopped one does not go on. */
let batchRun = 0;

export function answerFor(path: string): AiAnswer | undefined {
  return ai.answers[key(path)];
}

/** Loads the answers kept on this PC for these folders. */
export async function loadSavedAnswers(paths: string[]): Promise<void> {
  if (!paths.length) return;
  const saved = await aiSavedAnswers(paths).catch(() => []);
  for (const a of saved) ai.answers[key(a.path)] = a;
}

/** Asks about one folder and keeps the answer. Throws the error key on failure. */
export async function askAbout(item: FileItem): Promise<AiAnswer> {
  const k = key(item.path);
  ai.asking[k] = true;
  try {
    const answer = await aiAsk(aiConfig(), item, getLanguage());
    ai.answers[k] = answer;
    return answer;
  } finally {
    delete ai.asking[k];
  }
}

/**
 * Asks about every folder in `items` with no answer yet, `settings.ai.parallel`
 * at a time. Returns the failures.
 */
export async function askAll(items: FileItem[]): Promise<number> {
  const todo = items.filter((i) => !answerFor(i.path));
  const id = ++batchRun;
  const running = () => batchRun === id && ai.batch !== null;
  let failed = 0;
  let next = 0;
  ai.batch = { done: 0, total: todo.length };
  const worker = async () => {
    while (running() && next < todo.length) {
      const item = todo[next++];
      try {
        await askAbout(item);
      } catch {
        failed++;
      }
      if (running() && ai.batch) ai.batch.done++;
    }
  };
  const lanes = Math.max(1, Math.min(settings.ai.parallel || 1, todo.length));
  await Promise.all(Array.from({ length: lanes }, worker));
  if (batchRun === id) ai.batch = null;
  return failed;
}

/** Stops "Ask for all": the questions already sent still finish. */
export function stopAskAll(): void {
  batchRun++;
  ai.batch = null;
}

/** Forgets the answers of these folders, or all of them. */
export async function forgetAnswers(paths?: string[]): Promise<void> {
  await aiForgetAnswers(paths);
  if (!paths) ai.answers = {};
  else for (const p of paths) delete ai.answers[key(p)];
}

/** Opens the setup window; `then` runs once the AI is on. */
export function openSetup(then?: () => void): void {
  ai.setup = { then };
}

/** Turns the AI off (the "No thanks" choice). */
export function turnOff(): void {
  settings.ai.mode = "off";
  saveSettings();
}
