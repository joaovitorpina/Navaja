<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { Catalog } from '$bindings/Catalog';
  import { t } from '$lib/i18n';
  import { getSettings, listTools, shellReady } from '$lib/ipc';
  import { applyTheme } from '$lib/theme';
  import Shell from './shell/Shell.svelte';

  let catalog = $state.raw<Catalog | null>(null);
  let failed = $state<string | null>(null);

  onMount(async () => {
    try {
      const [loaded, settings] = await Promise.all([listTools(), getSettings()]);
      applyTheme(settings.theme);
      catalog = loaded;
    } catch (error) {
      failed = String(error);
    }
    // Show the window only once the first real frame is ready.
    await tick();
    await shellReady().catch(() => {});
  });
</script>

{#if catalog}
  <Shell {catalog} />
{:else if failed}
  <main class="p-6">
    <h1 class="text-xl font-semibold">Navaja</h1>
    <p role="alert" class="text-danger">
      {t('shell.start_failed', 'Navaja could not load its tools.')}
      {failed}
    </p>
  </main>
{/if}
