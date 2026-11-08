<script lang="ts" module>
  import { ICONS, type IconName } from "../icons";

  /** True for names that have an icon, e.g. the ones in known_folders.json. */
  export function isIconName(name: string | null | undefined): name is IconName {
    return !!name && Object.hasOwn(ICONS, name);
  }
</script>

<script lang="ts">
  /** A monochrome line icon (Fluent UI System Icons) drawn in the text color.
      `filled` is the solid variant, used for the selected item. Decorative unless `label` is set. */
  let { name, size = 16, filled = false, label }: { name: IconName; size?: number; filled?: boolean; label?: string } = $props();
</script>

<svg class="icon" width={size} height={size} viewBox="0 0 20 20" fill="currentColor"
     role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : "true"}>
  <path d={ICONS[name][filled ? 1 : 0]} />
</svg>

<style>
  .icon { flex: none; display: inline-block; vertical-align: -0.2em; }
</style>
