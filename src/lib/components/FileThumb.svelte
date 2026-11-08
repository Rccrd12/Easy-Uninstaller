<script module lang="ts">
  import { fileThumbnail } from "../api";

  // One request per file and size for the whole session: scrolling back is instant.
  const cache = new Map<string, Promise<string | null>>();

  function load(path: string, px: number): Promise<string | null> {
    const key = `${px}|${path}`;
    let pending = cache.get(key);
    if (!pending) {
      pending = fileThumbnail(path, px).catch(() => null);
      cache.set(key, pending);
    }
    return pending;
  }
</script>

<script lang="ts">
  import { fileKind, type FileKind } from "../paths";

  /** `size` is the tile size in CSS pixels; previews are asked at twice that for sharp screens. */
  let { path, size = 44 }: { path: string; size?: number } = $props();

  const icons: Record<FileKind, string> = {
    image: "🖼️",
    video: "🎬",
    audio: "🎵",
    document: "📄",
    archive: "🗜️",
    program: "📦",
    disc: "💿",
    other: "📁",
  };

  let el: HTMLElement | undefined = $state();
  let url = $state<string | null>(null);
  const kind = $derived(fileKind(path));

  // Previews are made only when the tile scrolls into view.
  $effect(() => {
    const target = el;
    const file = path;
    const px = Math.min(size * 2, 256);
    url = null;
    if (!target) return;
    let live = true;
    const seen = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return;
      seen.disconnect();
      load(file, px).then((u) => live && (url = u));
    });
    seen.observe(target);
    return () => {
      live = false;
      seen.disconnect();
    };
  });
</script>

<span bind:this={el} class="thumb kind-{kind}" style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.42)}px" aria-hidden="true">
  {#if url}<img src={url} alt="" />{:else}{icons[kind]}{/if}
  {#if kind === "video"}<i class="play" style="font-size:{Math.max(10, Math.round(size * 0.24))}px">▶</i>{/if}
</span>

<style>
  .thumb { position: relative; flex: none; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--radius-sm); overflow: hidden; background: var(--kind-other); }
  img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .play { position: absolute; inset: 0; margin: auto; width: 2em; height: 2em; border-radius: 50%; background: rgba(0, 0, 0, 0.45); color: #fff; font-style: normal; display: flex; align-items: center; justify-content: center; padding-left: 0.15em; }
  .kind-image { background: var(--kind-image); }
  .kind-video { background: var(--kind-video); }
  .kind-audio { background: var(--kind-audio); }
  .kind-document { background: var(--kind-document); }
  .kind-archive { background: var(--kind-archive); }
  .kind-program { background: var(--kind-program); }
  .kind-disc { background: var(--kind-disc); }
</style>
