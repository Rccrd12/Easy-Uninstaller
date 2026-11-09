<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import type { Screen } from "../screens";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";

  let { current, onNavigate }: { current: Screen; onNavigate: (s: Screen) => void } = $props();

  const main: { id: Screen; icon: IconName }[] = [
    { id: "home", icon: "home" },
    { id: "apps", icon: "apps" },
    { id: "files", icon: "folder" },
    { id: "leftovers", icon: "broom" },
    { id: "map", icon: "map" },
  ];
  const bottom: { id: Screen; icon: IconName }[] = [
    { id: "quarantine", icon: "quarantine" },
    { id: "settings", icon: "settings" },
  ];
</script>

{#snippet item(entry: { id: Screen; icon: IconName })}
  <button class="item" class:on={current === entry.id} aria-current={current === entry.id ? "page" : undefined} onclick={() => onNavigate(entry.id)}>
    <span class="icon"><Icon name={entry.icon} size={20} filled={current === entry.id} /></span>{t(`nav.${entry.id}`)}
  </button>
{/snippet}

<nav>
  <div class="brand"><span class="logo"></span>Easy Uninstaller</div>
  {#each main as entry (entry.id)}{@render item(entry)}{/each}
  <div class="spacer"></div>
  {#each bottom as entry (entry.id)}{@render item(entry)}{/each}
</nav>

<style>
  nav { width: 210px; flex: none; background: var(--bg-nav); padding: 12px 10px; display: flex; flex-direction: column; gap: 2px; }
  .brand { display: flex; align-items: center; gap: 10px; font-weight: 600; font-size: 13px; padding: 4px 12px 14px; }
  .logo { width: 20px; height: 20px; border-radius: 6px; background: linear-gradient(135deg, #2f7cf6, #28c2a0); }
  .item { display: flex; align-items: center; gap: 12px; padding: 9px 12px; border: none; background: none; border-radius: var(--radius-sm); font-size: 14px; color: var(--text); text-align: left; position: relative; }
  .item:hover { background: var(--hover); }
  .item.on { background: var(--surface); font-weight: 600; box-shadow: var(--shadow); }
  .item.on::before { content: ""; position: absolute; left: 0; top: 10px; bottom: 10px; width: 3px; border-radius: 2px; background: var(--accent); }
  .icon { width: 20px; height: 20px; display: flex; color: var(--text); }
  .item.on .icon { color: var(--accent); }
  .spacer { flex: 1; }
</style>
