<script lang="ts">
  /** A round status mark: a tick when something went well, "!" when it did not. */
  let { kind = "ok", size = 20 }: { kind?: "ok" | "warn"; size?: number } = $props();
</script>

<svg class="status {kind}" width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
  <circle cx="12" cy="12" r="12" />
  {#if kind === "ok"}
    <path d="M7 12.4l3.2 3.2L17 8.8" />
  {:else}
    <path d="M12 6.8v6.4M12 17.1v.1" />
  {/if}
</svg>

<style>
  .status { flex: none; display: inline-block; vertical-align: middle; }
  circle { fill: var(--safe-bg); }
  path { fill: none; stroke: var(--safe-fg); stroke-width: 2.4; stroke-linecap: round; stroke-linejoin: round; }
  .warn circle { fill: var(--check-bg); }
  .warn path { stroke: var(--check-fg); }
</style>
