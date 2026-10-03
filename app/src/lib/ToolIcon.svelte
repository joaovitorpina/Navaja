<script lang="ts">
  // Tool icons are drawn through a CSS mask, never inserted as markup, so an
  // icon can't run script even if the allowlist check in Rust were bypassed.
  let { svg, class: className = '' }: { svg: string; class?: string } = $props();

  const url = $derived(`url("data:image/svg+xml;utf8,${encodeURIComponent(svg)}")`);
</script>

<span
  aria-hidden="true"
  class="inline-block size-5 shrink-0 bg-current {className}"
  style:mask-image={url}
  style:-webkit-mask-image={url}
  style:mask-repeat="no-repeat"
  style:mask-size="contain"
></span>
