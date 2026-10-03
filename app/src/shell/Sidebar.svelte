<script lang="ts">
  // Category navigation built from the registry, with a search filter.
  import type { Catalog } from '$bindings/Catalog';
  import type { SearchHit } from '$bindings/SearchHit';
  import ToolIcon from '$lib/ToolIcon.svelte';
  import { t } from '$lib/i18n';
  import { searchTools } from '$lib/ipc';
  import { href, router } from '$lib/router.svelte';
  import { shortcutAria, shortcutLabel } from './keys';
  import { toolGroups } from './order';

  let { catalog, onOpenPalette }: { catalog: Catalog; onOpenPalette: () => void } = $props();

  let filter = $state('');
  let hits = $state.raw<SearchHit[] | null>(null);

  $effect(() => {
    const query = filter.trim();
    if (!query) {
      hits = null;
      return;
    }
    const timer = setTimeout(async () => {
      const result = await searchTools(query);
      if (filter.trim() === query) hits = result;
    }, 80);
    return () => clearTimeout(timer);
  });

  const byId = $derived(new Map(catalog.tools.map((tool) => [tool.id, tool])));
  const groups = $derived(toolGroups(catalog));
  const activeId = $derived(router.route.kind === 'tool' ? router.route.id : null);
</script>

{#snippet toolLink(id: string)}
  {@const tool = byId.get(id)}
  {#if tool}
    <li>
      <a
        href={href({ kind: 'tool', id })}
        aria-current={activeId === id ? 'page' : undefined}
        class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-subtle aria-[current=page]:bg-subtle aria-[current=page]:font-medium"
      >
        <ToolIcon svg={tool.icon} class="size-4 text-muted" />
        {t(`tool.${tool.id}.name`, tool.name)}
      </a>
    </li>
  {/if}
{/snippet}

<nav
  class="flex w-64 shrink-0 flex-col gap-3 border-r border-line bg-canvas p-3"
  aria-label={t('shell.nav.tools', 'Tools')}
>
  <a href={href({ kind: 'home' })} class="px-2 text-lg font-semibold">Navaja</a>

  <div class="flex gap-2">
    <input
      type="search"
      class="min-w-0 flex-1 rounded-md border border-muted bg-surface px-2 py-1.5 text-sm"
      placeholder={t('shell.nav.filter', 'Filter tools')}
      aria-label={t('shell.nav.filter', 'Filter tools')}
      bind:value={filter}
    />
    <button
      type="button"
      class="rounded-md border border-line px-2 text-xs text-muted hover:bg-subtle"
      aria-label={t('shell.palette.open', 'Search all tools')}
      aria-keyshortcuts={shortcutAria()}
      onclick={onOpenPalette}>{shortcutLabel()}</button
    >
  </div>

  <div class="flex-1 overflow-auto">
    {#if hits}
      <ul aria-label={t('shell.nav.results', 'Matching tools')}>
        {#each hits as hit (hit.id)}
          {@render toolLink(hit.id)}
        {:else}
          <li class="px-2 py-1.5 text-sm text-muted">
            {t('shell.nav.no_match', 'No tools match.')}
          </li>
        {/each}
      </ul>
      <p class="sr-only" aria-live="polite">
        {hits.length}
        {t('shell.nav.result_count', 'tools match')}
      </p>
    {:else}
      {#each groups as group (group.category.id)}
        <section class="mb-3">
          <h2 class="px-2 pb-1 text-xs font-semibold tracking-wide text-muted uppercase">
            {t(`category.${group.category.id}`, group.category.label)}
          </h2>
          <ul>
            {#each group.tools as tool (tool.id)}
              {@render toolLink(tool.id)}
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>

  <ul class="border-t border-line pt-2 text-sm">
    <li>
      <a class="block rounded-md px-2 py-1.5 hover:bg-subtle" href={href({ kind: 'settings' })}
        >{t('shell.settings', 'Settings')}</a
      >
    </li>
    <li>
      <a class="block rounded-md px-2 py-1.5 hover:bg-subtle" href={href({ kind: 'about' })}
        >{t('shell.about', 'About')}</a
      >
    </li>
  </ul>
</nav>
