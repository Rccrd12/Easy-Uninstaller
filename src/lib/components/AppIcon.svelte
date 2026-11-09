<script lang="ts">
  /** An app's own icon, or the first letter of its name on a colored tile. */
  let { app, size = 38 }: { app: { name: string; icon: string | null; iconColor: string }; size?: number } = $props();
</script>

{#if app.icon}
  <img class="ico" src={app.icon} alt="" width={size} height={size} style="width:{size}px;height:{size}px" />
{:else}
  <div class="ico letter" style="background:{app.iconColor};width:{size}px;height:{size}px;font-size:{size * 0.42}px">{app.name[0]}</div>
{/if}

<style>
  .ico { flex: none; border-radius: 24%; object-fit: contain; }
  img.ico { padding: 3px; }
  .letter { display: flex; align-items: center; justify-content: center; color: #fff; font-weight: 700; }
</style>
