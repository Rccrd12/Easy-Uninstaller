<script lang="ts">
  import EyeOffIcon from "./EyeOffIcon.svelte";
  import { formatBytes } from "../format";
  import { t } from "../i18n/index.svelte";
  import type { LeftoverItem } from "../types";

  /** Without `onToggle` the row only shows the item, with no checkbox. With `onHide`, "Hide" shows on hover. */
  let {
    item,
    checked = false,
    onToggle,
    onHide,
  }: { item: LeftoverItem; checked?: boolean; onToggle?: () => void; onHide?: () => void } = $props();

  const icons: Record<LeftoverItem["kind"], string> = { folder: "📂", registry: "🔑", shortcut: "🔗" };
</script>

<label class="row" class:on={checked} class:readonly={!onToggle} title={item.path}>
  {#if onToggle}<input type="checkbox" {checked} onchange={onToggle} />{/if}
  <span class="kind" aria-hidden="true">{icons[item.kind]}</span>
  <span class="info">
    <span class="reason">
      {t(`leftovers.reasons.${item.reason}`)}
      {#if item.safety === "check" && !item.unowned}<span class="tag tag-check">{t("common.check")}</span>{/if}
      {#if item.needsAdmin}<span class="shield" title={t("common.needsAdmin")} aria-label={t("common.needsAdmin")}>🛡️</span>{/if}
    </span>
    <span class="muted path">{item.path}</span>
  </span>
  {#if onHide}
    <button class="hide-pill" aria-label={t("files.hideItem", { name: t(`leftovers.reasons.${item.reason}`) })}
            onclick={(e) => { e.preventDefault(); e.stopPropagation(); onHide(); }}>
      <EyeOffIcon size={14} /> {t("files.hide")}
    </button>
  {/if}
  <span class="size" class:muted={!item.sizeBytes}>{item.sizeBytes ? formatBytes(item.sizeBytes) : "—"}</span>
</label>

<style>
  .row { display: flex; align-items: center; gap: 11px; padding: 9px 4px; border-bottom: 1px solid var(--border); cursor: pointer; }
  .row.readonly { cursor: default; }
  .row:last-child { border-bottom: none; }
  .row input { width: 18px; height: 18px; accent-color: var(--accent); flex: none; }
  .kind { font-size: 18px; flex: none; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .reason { font-size: 13.5px; font-weight: 600; display: flex; align-items: center; gap: 6px; }
  .shield { font-size: 12px; }
  .path { font-family: Consolas, "Cascadia Mono", monospace; font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hide-pill { flex: none; display: inline-flex; align-items: center; gap: 5px; border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); border-radius: 999px; padding: 3px 10px; font-size: 12px; opacity: 0; transition: opacity 0.12s; }
  .row:hover .hide-pill, .hide-pill:focus-visible { opacity: 1; }
  .hide-pill:hover { background: var(--hover); }
  .size { font-size: 13px; font-weight: 600; width: 72px; text-align: right; flex: none; }
</style>
