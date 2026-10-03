<script lang="ts">
  import { onMount } from 'svelte';
  import type { AppInfo } from '$bindings/AppInfo';
  import { t } from '$lib/i18n';
  import { getAppInfo } from '$lib/ipc';

  let info = $state<AppInfo | null>(null);

  onMount(async () => {
    info = await getAppInfo();
  });
</script>

<div class="mx-auto flex max-w-2xl flex-col gap-3 p-6">
  <h1 class="text-xl font-semibold">{t('shell.about', 'About')} Navaja</h1>
  {#if info}
    <p>{t('shell.about.version', 'Version')} {info.version}</p>
  {/if}
  <p class="text-muted">
    {t(
      'shell.about.offline',
      'An offline developer toolbox. No tool makes a network request; the only network call is an update check you opt in to.',
    )}
  </p>
  <p class="text-sm text-muted">
    {t('shell.about.license', 'MIT licence.')} github.com/joaovitorpina/Navaja
  </p>
</div>
