<script lang="ts">
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Home from "./lib/screens/Home.svelte";
  import Apps from "./lib/screens/Apps.svelte";
  import Files from "./lib/screens/Files.svelte";
  import Leftovers from "./lib/screens/Leftovers.svelte";
  import SpaceMap from "./lib/screens/SpaceMap.svelte";
  import Quarantine from "./lib/screens/Quarantine.svelte";
  import Settings from "./lib/screens/Settings.svelte";
  import type { Screen } from "./lib/screens";

  let screen = $state<Screen>("home");
  const go = (next: Screen) => (screen = next);
</script>

<div class="shell">
  <Sidebar current={screen} onNavigate={go} />
  <main>
    {#if screen === "home"}<Home onNavigate={go} />
    {:else if screen === "apps"}<Apps />
    {:else if screen === "files"}<Files />
    {:else if screen === "leftovers"}<Leftovers />
    {:else if screen === "map"}<SpaceMap onNavigate={go} />
    {:else if screen === "quarantine"}<Quarantine />
    {:else if screen === "settings"}<Settings />
    {/if}
  </main>
</div>

<style>
  .shell { display: flex; height: 100%; }
  main { flex: 1; min-width: 0; overflow-y: auto; padding: 24px 30px; }
</style>
