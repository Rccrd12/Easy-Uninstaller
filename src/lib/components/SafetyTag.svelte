<script lang="ts">
  import { t } from "../i18n";
  import type { Safety } from "../types";

  let { safety }: { safety: Safety } = $props();
  const labels: Record<Safety, string> = { safe: "common.safe", check: "common.check", personal: "common.personal" };
  const classes: Record<Safety, string> = { safe: "tag-safe", check: "tag-check", personal: "tag-keep" };
</script>

<span class="tag {classes[safety]}">{t(labels[safety])}</span>
