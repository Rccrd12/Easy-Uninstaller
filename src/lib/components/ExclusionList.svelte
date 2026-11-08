<script lang="ts">
  import { listExclusions, removeExclusion } from "../api";
  import { t } from "../i18n/index.svelte";
  import { displayPath } from "../paths";
  import type { Exclusion } from "../types";

  let list = $state<Exclusion[]>([]);
  let open = $state(false);

  function load() {
    listExclusions()
      .then((l) => (list = l))
      .catch(() => (list = []));
  }

  $effect(() => load());

  async function showAgain(path: string) {
    await removeExclusion(path).catch(() => {});
    load();
  }
</script>

<div class="exclusions">
  <div class="top">
    <span>
      {t("settings.hidden")}
      <span class="muted hint">{t("settings.hiddenHint")}</span>
    </span>
    {#if list.length}
      <button class="btn btn-ghost" aria-expanded={open} onclick={() => (open = !open)}>
        {open ? t("settings.hiddenClose") : t("settings.hiddenCount", { count: list.length })}
      </button>
    {:else}
      <span class="muted">{t("settings.hiddenNone")}</span>
    {/if}
  </div>
  {#if open && list.length}
    <ul>
      {#each list as item (item.path)}
        <li>
          <span aria-hidden="true">{item.folder ? "📁" : "📄"}</span>
          <span class="path" title={item.path}>{displayPath(item.path)}</span>
          <button class="link" onclick={() => showAgain(item.path)}>{t("settings.showAgain")}</button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .exclusions { padding: 14px 18px; border-bottom: 1px solid var(--border); }
  .exclusions:last-child { border-bottom: none; }
  .top { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .top .btn { flex: none; }
  .hint { display: block; font-size: 12px; margin-top: 2px; }
  ul { list-style: none; margin-top: 10px; border-top: 1px solid var(--border); }
  li { display: flex; align-items: center; gap: 8px; padding: 7px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
  .path { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .link { border: none; background: none; color: var(--accent); font-weight: 600; font-size: 12.5px; padding: 2px 4px; flex: none; }
</style>
