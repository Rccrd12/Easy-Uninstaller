<script lang="ts">
  import EyeOffIcon from "../components/EyeOffIcon.svelte";
  import { addExclusions, listExclusions, removeExclusions } from "../api";
  import { relativeTime } from "../format";
  import { locale, t } from "../i18n/index.svelte";
  import { displayPath, folderOf, nameOf } from "../paths";
  import type { Screen } from "../screens";
  import type { Exclusion, HiddenKind } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  const KINDS: HiddenKind[] = ["file", "folder", "group", "leftover"];
  const ICONS: Record<HiddenKind, string> = { file: "📄", folder: "📁", group: "🗂️", leftover: "🧹" };

  let list = $state<Exclusion[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let tab = $state<HiddenKind | "all">("all");
  let busy = $state(false);
  /** What was just shown again, so it can be hidden again with one click. */
  let shown = $state<Exclusion[] | null>(null);

  function load() {
    listExclusions()
      .then((l) => (list = l))
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  }

  $effect(() => load());

  const countOf = (kind: HiddenKind) => list.filter((e) => e.kind === kind).length;
  const tabs = $derived(KINDS.filter((k) => countOf(k) > 0));
  const visible = $derived(
    list.filter((e) => tab === "all" || e.kind === tab).sort((a, b) => b.hiddenAt - a.hiddenAt || a.path.localeCompare(b.path)),
  );
  // A tab that emptied falls back to "all".
  $effect(() => {
    if (tab !== "all" && !tabs.includes(tab)) tab = "all";
  });

  function nameOfItem(e: Exclusion): string {
    if (e.kind === "group") return t(e.label ?? e.path);
    if (e.kind === "leftover") return e.label ? t("hidden.leftoverOf", { app: e.label }) : nameOf(e.path) || e.path;
    return nameOf(e.path);
  }

  function whereOf(e: Exclusion): string {
    if (e.kind === "group") return t("hidden.groupNote");
    if (e.kind === "folder") return `${displayPath(e.path)} · ${t("hidden.folderNote")}`;
    if (e.kind === "leftover") return displayPath(e.path);
    return displayPath(folderOf(e.path));
  }

  async function showAgain(items: Exclusion[]) {
    if (!items.length || busy) return;
    busy = true;
    error = null;
    try {
      await removeExclusions(items);
      shown = items;
      load();
    } catch (e) {
      error = t("hidden.failed", { error: String(e) });
    } finally {
      busy = false;
    }
  }

  /** Hides again what was just shown, with the date it had. */
  async function hideAgain(items: Exclusion[]) {
    busy = true;
    try {
      await addExclusions(items);
      shown = null;
      load();
    } catch (e) {
      error = t("hidden.failed", { error: String(e) });
    } finally {
      busy = false;
    }
  }

  const count = (n: number) => n.toLocaleString(locale());
</script>

<button class="back" onclick={() => onNavigate("settings")}>‹ {t("settings.title")}</button>
<h1>{t("hidden.title")}</h1>
<p class="muted sub">{t("hidden.subtitle")}</p>

{#if loading}
  <div class="card message" role="status">…</div>
{:else if list.length === 0}
  <div class="card message">
    <span class="big-icon"><EyeOffIcon size={26} /></span>
    <p>{t("hidden.empty")}</p>
  </div>
{:else}
  {#if tabs.length > 1}
    <div class="segmented" role="tablist" aria-label={t("hidden.title")}>
      <button role="tab" aria-selected={tab === "all"} class:on={tab === "all"} onclick={() => (tab = "all")}>
        {t("hidden.tabs.all")} ({count(list.length)})
      </button>
      {#each tabs as kind (kind)}
        <button role="tab" aria-selected={tab === kind} class:on={tab === kind} onclick={() => (tab = kind)}>
          {t(`hidden.tabs.${kind}`)} ({count(countOf(kind))})
        </button>
      {/each}
    </div>
  {/if}

  <div class="card rows">
    {#each visible as item (`${item.kind}:${item.path}`)}
      <div class="row">
        <span class="icon" aria-hidden="true">{ICONS[item.kind]}</span>
        <div class="text">
          <div class="name" title={item.kind === "group" ? undefined : item.path}>{nameOfItem(item)}</div>
          <div class="muted small">{whereOf(item)}</div>
        </div>
        <span class="tag kind">{t(`hidden.kinds.${item.kind}`)}</span>
        <span class="muted when">{item.hiddenAt ? relativeTime(item.hiddenAt) : ""}</span>
        <button class="btn btn-ghost" disabled={busy} onclick={() => showAgain([item])}>{t("hidden.showAgain")}</button>
      </div>
    {/each}
  </div>

  <div class="footer">
    <span class="muted small">{t("hidden.note")}</span>
    <span class="grow"></span>
    <button class="btn btn-ghost" disabled={busy} onclick={() => showAgain(visible)}>{t("hidden.showAll")}</button>
  </div>
{/if}

{#if error}<p class="error" role="alert">{error}</p>{/if}

{#if shown}
  {@const items = shown}
  <div class="card toast" role="status" aria-live="polite">
    <span class="toast-icon" aria-hidden="true">✅</span>
    <p class="toast-text">{items.length === 1 ? t("hidden.shownAgain") : t("hidden.shownAgainMany", { count: count(items.length) })}</p>
    <button class="btn btn-ghost" disabled={busy} onclick={() => hideAgain(items)}>↩ {t("files.undo")}</button>
    <button class="close" aria-label={t("common.close")} onclick={() => (shown = null)}>✕</button>
  </div>
{/if}

<style>
  .back { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 2px 0; margin-bottom: 4px; }
  .back:hover { color: var(--accent); }
  .sub { margin: 3px 0 14px; max-width: 640px; }
  .message { padding: 22px 24px; display: flex; align-items: center; gap: 14px; }
  .big-icon { display: flex; color: var(--text-muted); }
  .segmented { display: inline-flex; background: var(--track); border-radius: 8px; padding: 3px; margin-bottom: 12px; flex-wrap: wrap; }
  .segmented button { border: none; background: none; padding: 5px 12px; font-size: 12.5px; border-radius: 6px; }
  .segmented .on { background: var(--raised); font-weight: 600; box-shadow: var(--shadow); }
  .rows { overflow: hidden; }
  .row { display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .row:last-child { border-bottom: none; }
  .icon { font-size: 20px; width: 26px; text-align: center; flex: none; }
  .text { flex: 1; min-width: 0; }
  .name { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .small { font-size: 12px; }
  .text .small { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kind { flex: none; background: var(--track); color: var(--text-muted); }
  .when { flex: none; width: 96px; font-size: 12px; text-align: right; }
  .row .btn { flex: none; }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .footer { display: flex; align-items: center; gap: 12px; margin-top: 12px; }
  .grow { flex: 1; }
  .error { color: var(--danger); margin-top: 10px; font-size: 13px; }
  .toast { position: fixed; right: 30px; bottom: 22px; max-width: 520px; display: flex; align-items: center; gap: 12px; padding: 12px 14px 12px 16px; box-shadow: var(--shadow-pop); border-left: 4px solid var(--safe-fg); z-index: 10; }
  .toast-icon { font-size: 18px; }
  .toast-text { flex: 1; font-size: 13px; }
  .toast .btn { flex: none; }
  .close { border: none; background: none; color: var(--text-muted); font-size: 13px; padding: 4px; }
</style>
