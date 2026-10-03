<script lang="ts">
  import { onMount, tick } from 'svelte';
  import type { AppInfo } from '$bindings/AppInfo';
  import type { Catalog } from '$bindings/Catalog';
  import { t } from '$lib/i18n';
  import { getAppInfo, getSettings, listTools, quitApp, shellReady } from '$lib/ipc';
  import { applyTheme } from '$lib/theme';
  import OpenLogsButton from './shell/OpenLogsButton.svelte';
  import Shell from './shell/Shell.svelte';

  let catalog = $state.raw<Catalog | null>(null);
  let info = $state.raw<AppInfo | null>(null);
  let failed = $state<string | null>(null);
  let quitFailed = $state(false);

  onMount(async () => {
    try {
      const [loaded, settings, appInfo] = await Promise.all([
        listTools(),
        getSettings(),
        getAppInfo(),
      ]);
      applyTheme(settings.theme);
      info = appInfo;
      catalog = loaded;
    } catch (error) {
      failed = String(error);
    }
    // Show the window only once the first real frame is ready.
    await tick();
    await shellReady().catch(() => {});
  });

  async function quit() {
    try {
      await quitApp();
    } catch {
      quitFailed = true;
    }
  }
</script>

{#if catalog}
  <div class="flex h-full flex-col">
    <Shell {catalog} elevated={info?.elevated ?? false} />
  </div>
{:else if failed}
  <main class="flex flex-col items-start gap-3 p-6">
    <h1 class="text-xl font-semibold">Navaja</h1>
    <p role="alert" class="text-danger">
      {t('shell.start_failed', 'Navaja could not load its tools.')}
      {failed}
    </p>
    <OpenLogsButton />
    <button
      type="button"
      class="rounded-md border border-line bg-surface px-2.5 py-1 text-sm hover:bg-subtle"
      onclick={quit}>{t('shell.quit', 'Quit Navaja')}</button
    >
    {#if quitFailed}
      <p role="alert" class="text-sm text-danger">
        {t('shell.quit_failed', 'Navaja could not quit.')}
      </p>
    {/if}
  </main>
{/if}
