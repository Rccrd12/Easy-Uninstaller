<script lang="ts">
  import Icon from "../components/Icon.svelte";
  import SampleBanner from "../components/SampleBanner.svelte";
  import { clearHistory, getHistory, usesSampleData } from "../api";
  import { formatBytes } from "../format";
  import { historyAmount, historyDetail, historyTitle, kindIcons, dayLabel, monthLabel, shortDate, timeOf } from "../history";
  import { locale, t } from "../i18n/index.svelte";
  import type { Screen } from "../screens";
  import type { HistoryItem, HistorySummary } from "../types";

  let { onNavigate }: { onNavigate: (s: Screen) => void } = $props();

  let history = $state<HistorySummary | null>(null);
  let error = $state<string | null>(null);
  let confirming = $state(false);
  let clearing = $state(false);

  function load() {
    return getHistory()
      .then((h) => {
        history = h;
        error = null;
      })
      .catch((e) => (error = String(e)));
  }

  $effect(() => {
    load();
  });

  async function clear() {
    clearing = true;
    try {
      await clearHistory();
    } catch (e) {
      error = String(e);
    } finally {
      clearing = false;
      confirming = false;
      await load();
    }
  }

  /** The rows grouped by month, newest first. */
  const months = $derived.by(() => {
    const out: { label: string; items: HistoryItem[] }[] = [];
    for (const item of history?.items ?? []) {
      const label = monthLabel(item.at);
      const last = out[out.length - 1];
      if (last?.label === label) last.items.push(item);
      else out.push({ label, items: [item] });
    }
    return out;
  });

  const actions = (n: number) => (n === 1 ? t("history.actionsOne") : t("history.actions", { count: n.toLocaleString(locale()) }));
  const inQuarantineOnly = (item: HistoryItem) => item.freedBytes === 0 && item.pendingBytes > 0;
</script>

<h1>{t("history.title")}</h1>
{#if usesSampleData()}<SampleBanner />{/if}

{#if error}
  <div class="card message"><p>{t("history.failed", { error })}</p></div>
{:else if history && history.actionCount === 0}
  <div class="card message">
    <span class="big-icon"><Icon name="history" size={26} /></span>
    <p>{t("history.empty")}</p>
  </div>
{:else if history}
  <section class="card total">
    <span class="total-icon"><Icon name="checkCircle" size={26} /></span>
    <div class="total-text">
      <div class="total-line"><span class="big">{formatBytes(history.freedBytes)}</span><span class="muted">{t("history.freed")}</span></div>
      {#if history.since}
        <div class="muted small">{t("history.since", { count: actions(history.actionCount), date: shortDate(history.since) })}</div>
      {/if}
    </div>
  </section>

  {#if history.pendingBytes > 0}
    <div class="card pending">
      <span class="pending-icon"><Icon name="quarantine" size={18} /></span>
      <span class="grow">{t("history.pending", { size: formatBytes(history.pendingBytes) })}</span>
      <button class="btn btn-ghost" onclick={() => onNavigate("quarantine")}>{t("history.openQuarantine")}</button>
    </div>
  {/if}

  {#each months as month (month.label)}
    <h2>{month.label}</h2>
    <div class="card list">
      {#each month.items as item (item.id)}
        <div class="row">
          <div class="when">
            <div>{dayLabel(item.at)}</div>
            <div class="muted time">{timeOf(item.at)}</div>
          </div>
          <span class="kind"><Icon name={kindIcons[item.kind]} size={18} /></span>
          <div class="info">
            <div class="title">{historyTitle(item)}</div>
            <div class="muted detail">{historyDetail(item)}</div>
            {#if item.pendingBytes > 0 && item.pendingUntil}
              <span class="tag tag-info">{t("history.pendingUntil", { date: shortDate(item.pendingUntil) })}</span>
            {/if}
            {#if item.restoredAt && item.restoredBytes > 0}
              <span class="tag tag-gray">
                {item.freedBytes + item.pendingBytes > 0
                  ? t("history.partRestored", { size: formatBytes(item.restoredBytes), date: shortDate(item.restoredAt) })
                  : t("history.restored", { date: shortDate(item.restoredAt) })}
              </span>
            {/if}
          </div>
          <span class="amount" class:waiting={inQuarantineOnly(item)} class:none={item.freedBytes + item.pendingBytes === 0}>{historyAmount(item)}</span>
        </div>
      {/each}
    </div>
  {/each}

  <div class="foot">
    <span class="grow"></span>
    {#if confirming}
      <div class="confirm">
        <span>{t("history.clearConfirm")}</span>
        <button class="btn btn-ghost" disabled={clearing} onclick={() => (confirming = false)}>{t("common.cancel")}</button>
        <button class="btn btn-danger" disabled={clearing} onclick={clear}>{t("history.clear")}</button>
      </div>
    {:else}
      <button class="btn btn-ghost" onclick={() => (confirming = true)}>{t("history.clear")}</button>
    {/if}
  </div>
{/if}

<style>
  .small { font-size: 13px; }
  .grow { flex: 1; }

  .total { display: flex; align-items: center; gap: 16px; padding: 18px 22px; }
  .total-icon { width: 48px; height: 48px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; flex: none; background: var(--safe-bg); color: var(--safe-fg); }
  .total-line { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .big { font-size: 32px; font-weight: 700; color: var(--safe-fg); line-height: 1.1; }

  .pending { display: flex; align-items: center; gap: 12px; margin-top: 10px; padding: 10px 14px; font-size: 13px; line-height: 1.4; }
  .pending-icon { display: flex; color: var(--accent); }
  .btn { flex: none; }
  .btn:disabled { opacity: 0.5; cursor: default; }

  h2 { font-size: 13px; font-weight: 600; color: var(--text-muted); margin: 20px 0 8px; }
  .list { padding: 2px 16px; }
  .row { display: flex; align-items: center; gap: 14px; padding: 11px 0; }
  .row + .row { border-top: 1px solid var(--border); }
  .when { width: 74px; flex: none; font-size: 13px; }
  .time { font-size: 11.5px; }
  .kind { width: 34px; height: 34px; border-radius: var(--radius-sm); display: flex; align-items: center; justify-content: center; flex: none; background: var(--icon-tile-bg); color: var(--icon-tile-fg); }
  .info { flex: 1; min-width: 0; }
  .title { font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .detail { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .info .tag { display: inline-block; margin-top: 5px; margin-right: 6px; }
  .amount { flex: none; font-size: 14px; font-weight: 700; text-align: right; min-width: 80px; }
  .amount.waiting, .amount.none { color: var(--text-muted); font-weight: 600; }

  .foot { display: flex; align-items: center; gap: 12px; margin: 18px 0 6px; font-size: 12.5px; flex-wrap: wrap; }
  .foot > .grow { flex: 1; }
  .confirm { display: flex; align-items: center; gap: 10px; padding: 8px 10px 8px 12px; border-radius: var(--radius-sm); background: var(--keep-bg); color: var(--keep-fg); font-weight: 600; flex-wrap: wrap; }

  .message { padding: 22px 24px; margin-top: 8px; display: flex; align-items: center; gap: 14px; }
  .message p { flex: 1; }
  .big-icon { width: 44px; height: 44px; border-radius: var(--radius); display: flex; align-items: center; justify-content: center; flex: none; background: var(--icon-tile-bg); color: var(--icon-tile-fg); }
</style>
