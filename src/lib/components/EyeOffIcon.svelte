<script lang="ts">
  /** A closed eye: the sign for "hide", drawn in the text color. */
  let { size = 16 }: { size?: number } = $props();
</script>

<svg class="eye-off" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
     stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d="M3 10c2.2 3 5.4 5 9 5s6.8-2 9-5" />
  <path d="M5.6 13.4 3.8 16M10 15l-.6 3M14 15l.6 3M18.4 13.4l1.8 2.6" />
</svg>

<style>
  .eye-off { flex: none; vertical-align: -0.2em; }
</style>
