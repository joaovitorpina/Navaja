<script lang="ts">
  // Shows one tool: header plus the view its UiSpec asks for.
  import type { Component } from 'svelte';
  import type { ToolMeta } from '$bindings/ToolMeta';
  import GeneratorView from '../generic/GeneratorView.svelte';
  import ToolIcon from '$lib/ToolIcon.svelte';
  import { t } from '$lib/i18n';
  import type { ViewProps } from '$lib/view-kit';

  let { meta }: { meta: ToolMeta } = $props();

  // Custom views live with their tool and are found here, so adding one
  // never edits the shell (docs/architecture.md §4).
  const customViews = import.meta.glob<Component<ViewProps>>('@tools/*/ui/View.svelte', {
    import: 'default',
  });

  function customView(id: string) {
    const key = Object.keys(customViews).find((k) => k.endsWith(`/${id}/ui/View.svelte`));
    return key ? customViews[key] : undefined;
  }
</script>

<article class="mx-auto flex max-w-5xl flex-col gap-6 p-6">
  <header class="flex items-start gap-3">
    <ToolIcon svg={meta.icon} class="mt-1 size-6 text-accent" />
    <div>
      <h1 class="text-xl font-semibold">{t(`tool.${meta.id}.name`, meta.name)}</h1>
      <p class="text-sm text-muted">{t(`tool.${meta.id}.description`, meta.description)}</p>
    </div>
  </header>

  {#key meta.id}
    {#if meta.ui.kind === 'generator'}
      <GeneratorView {meta} spec={meta.ui} />
    {:else if meta.ui.kind === 'custom'}
      {@const load = customView(meta.ui.view)}
      {#if load}
        {#await load() then View}
          <View {meta} />
        {/await}
      {:else}
        <p class="text-sm text-danger">
          {t('shell.view_missing', 'This tool’s view is missing from this build.')}
        </p>
      {/if}
    {:else}
      <p class="text-sm text-muted">
        {t('shell.view_unsupported', 'This tool needs a newer version of Navaja.')}
      </p>
    {/if}
  {/key}
</article>
