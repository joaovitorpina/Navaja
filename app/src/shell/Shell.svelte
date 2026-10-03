<script lang="ts">
  import { onMount } from 'svelte';
  import type { Catalog } from '$bindings/Catalog';
  import { routeOutputCopies } from '$lib/copy';
  import { t } from '$lib/i18n';
  import { router } from '$lib/router.svelte';
  import AboutView from './AboutView.svelte';
  import CommandPalette from './CommandPalette.svelte';
  import Home from './Home.svelte';
  import SettingsView from './SettingsView.svelte';
  import Sidebar from './Sidebar.svelte';
  import ToolHost from './ToolHost.svelte';
  import { isPaletteShortcut } from './keys';

  let { catalog, elevated = false }: { catalog: Catalog; elevated?: boolean } = $props();

  let paletteOpen = $state(false);

  // The tray's "Search tools…" opens the palette through this event.
  $effect(() => {
    const open = () => (paletteOpen = true);
    window.addEventListener('navaja:palette', open);
    return () => window.removeEventListener('navaja:palette', open);
  });

  const route = $derived(router.route);
  const tool = $derived(
    route.kind === 'tool' ? catalog.tools.find((candidate) => candidate.id === route.id) : undefined,
  );

  // Native copies of tool output go through Rust (see lib/copy.ts).
  onMount(() => routeOutputCopies());

  function onKeydown(event: KeyboardEvent) {
    if (isPaletteShortcut(event)) {
      event.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if elevated}
  <div role="status" class="border-b border-line bg-subtle px-4 py-2 text-sm">
    {t(
      'shell.elevated',
      'Navaja is running with administrator or root rights. It doesn’t need them.',
    )}
  </div>
{/if}

<div class="flex min-h-0 flex-1">
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
