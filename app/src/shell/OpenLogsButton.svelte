<script lang="ts">
  import { t } from '$lib/i18n';
  import { openLogs } from '$lib/ipc';

  let failed = $state(false);

  async function open() {
    try {
      await openLogs();
      failed = false;
    } catch {
      failed = true;
    }
  }
</script>

<div class="flex flex-col items-start gap-2 text-sm">
  <button
    type="button"
    class="rounded-md border border-line bg-surface px-2.5 py-1 hover:bg-subtle"
    onclick={open}>{t('shell.logs.open', 'Open logs folder')}</button
  >
  {#if failed}
    <p role="alert" class="text-danger">
      {t('shell.logs.open_failed', 'Navaja could not open its logs folder.')}
    </p>
  {/if}
</div>
