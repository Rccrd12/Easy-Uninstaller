<script lang="ts">
  import SampleBanner from "../components/SampleBanner.svelte";
  import { listQuarantine, QUARANTINE_DAYS } from "../api";
  import { formatBytes } from "../format";
  import { t } from "../i18n";
  import type { QuarantineItem } from "../types";

  let items = $state<QuarantineItem[]>([]);
  $effect(() => {
    listQuarantine().then((q) => (items = q));
  });
</script>

<h1>{t("quarantine.title")}</h1>
<p class="muted sub">{t("quarantine.subtitle", { days: QUARANTINE_DAYS })}</p>
<SampleBanner />

{#each items as item (item.id)}
  <div class="card row">
    <div class="info">
      <div class="label">{item.label}</div>
      <div class="muted meta">{item.originalPath}</div>
      <div class="muted meta">{t("quarantine.removedOn", { when: item.removedOn })} · {t("quarantine.expires", { days: item.expiresInDays })}</div>
    </div>
    <span class="size">{formatBytes(item.sizeBytes)}</span>
    <button class="btn btn-ghost">{t("common.restore")}</button>
  </div>
{/each}

<style>
  .sub { margin: 3px 0 10px; }
  .row { display: flex; align-items: center; gap: 14px; padding: 12px 16px; margin-bottom: 6px; }
  .info { flex: 1; min-width: 0; }
  .label { font-weight: 600; }
  .meta { font-size: 12px; }
  .size { font-weight: 600; }
</style>
