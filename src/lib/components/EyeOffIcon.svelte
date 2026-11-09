<script lang="ts">
  /** A crossed-out eye: the sign for "hide", drawn in the text color. */
  import Icon from "./Icon.svelte";

  let { size = 16 }: { size?: number } = $props();
</script>

<Icon name="eyeOff" {size} />
