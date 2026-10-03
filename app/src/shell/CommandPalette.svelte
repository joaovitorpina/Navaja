<script lang="ts">
  // Ctrl/Cmd+K: jump to any tool. Ranking comes from Rust (`search`), so the
  // palette and the sidebar filter always agree.
  import { Command, Dialog } from 'bits-ui';
  import type { Catalog } from '$bindings/Catalog';
  import type { SearchHit } from '$bindings/SearchHit';
  import ToolIcon from '$lib/ToolIcon.svelte';
  import { t } from '$lib/i18n';
  import { searchTools } from '$lib/ipc';
  import { router } from '$lib/router.svelte';

  let { catalog, open = $bindable(false) }: { catalog: Catalog; open?: boolean } = $props();

  let query = $state('');
  let hits = $state.raw<SearchHit[]>([]);

  const byId = $derived(new Map(catalog.tools.map((tool) => [tool.id, tool])));
  const categoryLabel = $derived(
    new Map(catalog.categories.map((c) => [c.id, t(`category.${c.id}`, c.label)])),
  );

  $effect(() => {
    if (!open) return;
    const current = query;
    void searchTools(current).then((result) => {
      if (query === current) hits = result;
    });
  });

  function choose(id: string) {
    open = false;
    query = '';
    router.go({ kind: 'tool', id });
  }
</script>

<Dialog.Root bind:open>
  <Dialog.Portal>
    <Dialog.Overlay class="fixed inset-0 bg-black/40" />
    <Dialog.Content
      class="fixed top-24 left-1/2 w-[min(36rem,90vw)] -translate-x-1/2 overflow-hidden rounded-lg border border-line bg-surface text-fg shadow-xl"
    >
      <Dialog.Title class="sr-only">{t('shell.palette.title', 'Search tools')}</Dialog.Title>
      <Command.Root shouldFilter={false} label={t('shell.palette.title', 'Search tools')}>
        <Command.Input
          bind:value={query}
          class="w-full border-b border-line bg-transparent px-4 py-3 text-base outline-none"
          placeholder={t('shell.palette.placeholder', 'Search tools…')}
        />
        <Command.List class="max-h-80 overflow-auto p-1">
          <!-- The viewport gives the input its aria-controls and aria-activedescendant. -->
          <Command.Viewport>
            <Command.Empty class="px-3 py-6 text-center text-sm text-muted">
              {t('shell.nav.no_match', 'No tools match.')}
            </Command.Empty>
            {#each hits as hit (hit.id)}
              {@const tool = byId.get(hit.id)}
              {#if tool}
                <!-- Selected: a tint plus the accent focus ring, which also survives forced colours. -->
                <Command.Item
                  value={tool.id}
                  onSelect={() => choose(tool.id)}
                  class="flex cursor-default items-center gap-3 rounded-md px-3 py-2 text-sm aria-selected:bg-subtle aria-selected:outline-2 aria-selected:-outline-offset-2 aria-selected:outline-accent"
                >
                  <ToolIcon svg={tool.icon} class="size-4 text-muted" />
                  <span class="flex-1">{t(`tool.${tool.id}.name`, tool.name)}</span>
                  <span class="text-xs text-muted">{categoryLabel.get(tool.category)}</span>
                </Command.Item>
              {/if}
            {/each}
          </Command.Viewport>
        </Command.List>
      </Command.Root>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>
