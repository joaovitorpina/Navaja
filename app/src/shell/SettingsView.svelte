<script lang="ts">
  import { onMount } from 'svelte';
  import type { Settings } from '$bindings/Settings';
  import type { Theme } from '$bindings/Theme';
  import { t } from '$lib/i18n';
  import { getSettings, saveSettings } from '$lib/ipc';
  import { applyTheme } from '$lib/theme';
  import OpenLogsButton from './OpenLogsButton.svelte';

  let settings = $state<Settings | null>(null);
  let error = $state<string | null>(null);

  onMount(async () => {
    settings = await getSettings();
  });

  async function setTheme(select: HTMLSelectElement) {
    if (!settings) return;
    const theme = select.value as Theme;
    const next = { ...settings, theme };
    try {
      await saveSettings(next);
      settings = next;
      applyTheme(theme);
      error = null;
    } catch (e) {
      // The select still shows the rejected pick; show what is stored instead.
      select.value = settings.theme;
      error = String(e);
    }
  }
</script>

<div class="mx-auto flex max-w-2xl flex-col gap-6 p-6">
  <h1 class="text-xl font-semibold">{t('shell.settings', 'Settings')}</h1>
  {#if settings}
    <label class="flex flex-col gap-1 text-sm" for="setting-theme">
      <span class="text-muted">{t('shell.settings.theme', 'Theme')}</span>
      <select
        id="setting-theme"
        class="w-48 rounded-md border border-muted bg-surface px-2 py-1.5"
        value={settings.theme}
        onchange={(e) => void setTheme(e.currentTarget)}
      >
        <option value="system">{t('shell.settings.theme.system', 'Same as the system')}</option>
        <option value="light">{t('shell.settings.theme.light', 'Light')}</option>
        <option value="dark">{t('shell.settings.theme.dark', 'Dark')}</option>
      </select>
    </label>
  {/if}
  {#if error}<p role="alert" class="text-sm text-danger">{error}</p>{/if}

  <section class="flex flex-col gap-2" aria-labelledby="settings-logs">
    <h2 id="settings-logs" class="text-sm font-medium">{t('shell.settings.logs', 'Logs')}</h2>
    <p class="text-sm text-muted">
      {t(
        'shell.settings.logs.about',
        'Navaja keeps up to seven daily log files. They never leave this computer.',
      )}
    </p>
    <OpenLogsButton />
  </section>
</div>
