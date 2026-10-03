<script lang="ts">
  import type { Catalog } from '$bindings/Catalog';
  import { t } from '$lib/i18n';
  import { router } from '$lib/router.svelte';
  import AboutView from './AboutView.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Home from './Home.svelte';
  import SettingsView from './SettingsView.svelte';
  import Sidebar from './Sidebar.svelte';
  import ToolHost from './ToolHost.svelte';
  import { isPaletteShortcut } from './keys';

  let { catalog }: { catalog: Catalog } = $props();

  let paletteOpen = $state(false);

  const route = $derived(router.route);
  const tool = $derived(
    route.kind === 'tool' ? catalog.tools.find((candidate) => candidate.id === route.id) : undefined,
  );

  function onKeydown(event: KeyboardEvent) {
    if (isPaletteShortcut(event)) {
      event.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full">
  <Sidebar {catalog} onOpenPalette={() => (paletteOpen = true)} />
  <main class="flex-1 overflow-auto bg-canvas">
    {#if route.kind === 'tool'}
      {#if tool}
        <ToolHost meta={tool} />
      {:else}
        <p class="p-6 text-muted">{t('shell.not_found', 'No such tool.')}</p>
      {/if}
    {:else if route.kind === 'settings'}
      <SettingsView />
    {:else if route.kind === 'about'}
      <AboutView />
    {:else}
      <Home {catalog} />
    {/if}
  </main>
</div>

<CommandPalette {catalog} bind:open={paletteOpen} />
