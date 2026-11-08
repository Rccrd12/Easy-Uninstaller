<script lang="ts">
  /** A crossed-out eye: the sign for "hide", drawn in the text color. */
  let { size = 16 }: { size?: number } = $props();
  // The slash cuts a gap through the eye; a mask keeps it see-through on any background.
  const uid = $props.id();
  const mask = `eye-off-${uid}`;
</script>

<svg class="eye-off" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
     stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <mask id={mask} maskUnits="userSpaceOnUse" x="0" y="0" width="24" height="24">
    <rect width="24" height="24" fill="white" />
    <path d="M4.5 20.5 19.5 3.5" stroke="black" stroke-width="4.6" />
  </mask>
  <g mask="url(#{mask})">
    <path d="M2 12c2.4-4.2 5.8-6.3 10-6.3s7.6 2.1 10 6.3c-2.4 4.2-5.8 6.3-10 6.3S4.4 16.2 2 12Z" />
    <circle cx="12" cy="12" r="3.4" fill="currentColor" stroke="none" />
  </g>
  <path d="M4.5 20.5 19.5 3.5" />
</svg>

<style>
  .eye-off { flex: none; vertical-align: -0.2em; }
</style>
