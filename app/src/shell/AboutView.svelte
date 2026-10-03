<script lang="ts">
  import { onMount } from 'svelte';
  import type { AppInfo } from '$bindings/AppInfo';
  import { t } from '$lib/i18n';
  import { getAppInfo, openRepository } from '$lib/ipc';
  import { REPOSITORY_URL } from '$lib/links';

  let info = $state<AppInfo | null>(null);
  let openFailed = $state(false);

  // Shown without the scheme. A button, not a link: the page holds no
  // external href that a click, middle-click or prefetch could follow.
  const repository = REPOSITORY_URL.replace(/^https:\/\//, '');

  onMount(async () => {
    info = await getAppInfo();
  });

  async function openRepo() {
    try {
      await openRepository();
      openFailed = false;
    } catch {
      openFailed = true;
    }
  }
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
    {t('shell.about.license', 'MIT licence.')}
    <button
      type="button"
      class="rounded-sm text-accent underline underline-offset-2 hover:no-underline"
      aria-describedby="about-repository-hint"
      onclick={openRepo}>{repository}</button
    >
    <span id="about-repository-hint" class="sr-only"
      >{t('shell.about.repository_hint', 'Opens in your web browser.')}</span
    >
  </p>
  {#if openFailed}
    <p role="alert" class="text-sm text-danger">
      {t('shell.about.repository_failed', 'Navaja could not open your web browser.')}
    </p>
  {/if}
</div>
