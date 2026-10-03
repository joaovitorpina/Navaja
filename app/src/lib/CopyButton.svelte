<script lang="ts">
  import { copyText } from './ipc';
  import { t } from './i18n';

  let { text, label = t('shell.copy', 'Copy') }: { text: string; label?: string } = $props();

  let state = $state<'idle' | 'copied' | 'failed'>('idle');

  async function copy() {
    try {
      await copyText(text);
      state = 'copied';
    } catch {
      state = 'failed';
    }
    setTimeout(() => (state = 'idle'), 1500);
  }
</script>

<button
  type="button"
  class="rounded-md border border-line bg-surface px-2.5 py-1 text-sm hover:bg-subtle"
  onclick={copy}
>
  {#if state === 'copied'}{t('shell.copied', 'Copied')}{:else if state === 'failed'}{t(
      'shell.copy_failed',
      'Copy failed',
    )}{:else}{label}{/if}
</button>
<span class="sr-only" aria-live="polite">{state === 'copied' ? t('shell.copied', 'Copied') : ''}</span>
