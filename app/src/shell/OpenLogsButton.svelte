<script lang="ts">
  import { t } from '$lib/i18n';
  import { openLogs } from '$lib/ipc';

  // Failed tries in a row. Each failure renders a new alert node, so a
  // screen reader announces a repeated failure again.
  let failures = $state(0);

  async function open() {
    try {
      await openLogs();
      failures = 0;
    } catch {
      failures += 1;
    }
  }
</script>

<div class="flex flex-col items-start gap-2 text-sm">
  <button
    type="button"
    class="rounded-md border border-line bg-surface px-2.5 py-1 hover:bg-subtle"
    onclick={open}>{t('shell.logs.open', 'Open logs folder')}</button
  >
  {#if failures > 0}
    {#key failures}
      <p role="alert" class="text-danger">
        {t('shell.logs.open_failed', 'Navaja could not open its logs folder.')}
      </p>
    {/key}
  {/if}
</div>
