<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import { listLeftovers } from "../api";
  import { formatBytes } from "../format";
  import { t } from "../i18n";
  import type { LeftoverItem } from "../types";

  let items = $state<LeftoverItem[]>([]);
  $effect(() => {
    listLeftovers().then((l) => (items = l));
  });

  const icons: Record<LeftoverItem["kind"], string> = { folder: "📂", registry: "🔑", shortcut: "🔗" };
  const total = $derived(items.reduce((s, x) => s + x.sizeBytes, 0));
</script>

<h1>{t("leftovers.title")}</h1>
<p class="muted sub">{t("leftovers.subtitle")}</p>
<SampleBanner />

{#each items as item (item.id)}
  <div class="card row">
    <span class="kind" aria-hidden="true">{icons[item.kind]}</span>
    <div class="info">
      <div class="app">{item.appName}</div>
      <div class="muted path">{item.path}</div>
    </div>
    <span class="size">{item.sizeBytes ? formatBytes(item.sizeBytes) : "—"}</span>
  </div>
{/each}

<div class="footer">
  <span class="grow"></span>
  <button class="btn btn-primary">{t("common.moveToQuarantine")} ({formatBytes(total)})</button>
</div>

<style>
  .sub { margin: 3px 0 10px; }
  .row { display: flex; align-items: center; gap: 12px; padding: 11px 14px; margin-bottom: 6px; }
  .kind { font-size: 20px; }
  .info { min-width: 0; flex: 1; }
  .app { font-weight: 600; }
  .path { font-family: Consolas, monospace; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .size { font-weight: 600; width: 80px; text-align: right; }
  .footer { display: flex; margin-top: 12px; }
  .grow { flex: 1; }
</style>
