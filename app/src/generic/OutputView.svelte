<script lang="ts">
  // One declared output, rendered by its format. The value's shape is
  // guaranteed by the registry's checks (crates/navaja-core/src/payload.rs).
  import type { BinaryValue } from '$bindings/BinaryValue';
  import type { Diagnostic } from '$bindings/Diagnostic';
  import type { KeyValueRow } from '$bindings/KeyValueRow';
  import type { OutputSpec } from '$bindings/OutputSpec';
  import type { JsonValue } from '$bindings/serde_json/JsonValue';
  import CopyButton from '$lib/CopyButton.svelte';
  import { copySelection } from '$lib/copy';
  import { t } from '$lib/i18n';

  let { toolId, spec, value }: { toolId: string; spec: OutputSpec; value: JsonValue | undefined } =
    $props();

  const label = $derived(t(`tool.${toolId}.output.${spec.key}`, spec.label));
  // Reveals belong to one result: every new value starts masked again.
  let revealed = $derived.by((): Record<number, boolean> => {
    void value;
    return {};
  });
</script>

<!-- Native copies go through Rust too, so they get the same exclusion markers. -->
<section class="flex flex-col gap-2" aria-label={label} oncopy={copySelection}>
  <h2 class="text-sm font-medium text-muted">{label}</h2>
  {#if value === null || value === undefined}
    <p class="text-sm text-muted">—</p>
  {:else if spec.format.kind === 'text' || spec.format.kind === 'code'}
    {@const text = String(value)}
    <!-- Focusable so keyboard users can scroll long output. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <pre
      class="max-h-[60vh] overflow-auto rounded-md border border-line bg-surface p-3 font-mono text-sm break-all whitespace-pre-wrap"
      tabindex="0">{text}</pre>
    <div><CopyButton {text} /></div>
  {:else if spec.format.kind === 'key_value'}
    {@const rows = value as unknown as KeyValueRow[]}
    <table class="w-full border-collapse text-sm">
      <tbody>
        {#each rows as row, i (i)}
          <tr class="border-b border-line">
            <th scope="row" class="py-1.5 pr-4 text-left align-top font-medium">{row.key}</th>
            <td class="py-1.5 font-mono break-all">
              <!-- Secrets can't be selected, so they never reach the X11/Wayland primary selection. -->
              <span class={row.secret ? 'select-none' : undefined}>
                {#if row.secret && !revealed[i]}
                  <span aria-label={t('shell.hidden_value', 'Hidden value')}>••••••••</span>
                  <button
                    type="button"
                    class="ml-2 text-xs underline"
                    onclick={() => (revealed = { ...revealed, [i]: true })}
                    >{t('shell.reveal', 'Reveal')}</button
                  >
                {:else}
                  {row.value}
                {/if}
              </span>
              {#if row.note}<div class="font-sans text-xs text-muted">{row.note}</div>{/if}
            </td>
            <!-- Copies without revealing. -->
            <td class="py-1.5 pl-2 text-right align-top"><CopyButton text={row.value} /></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if spec.format.kind === 'diagnostics'}
    {@const items = value as unknown as Diagnostic[]}
    <ul class="flex flex-col gap-1 text-sm">
      {#each items as item, i (i)}
        <li class:text-danger={item.severity === 'error'}>
          {#if item.line !== null}<span class="font-mono"
              >{item.line}{item.column !== null ? `:${item.column}` : ''}</span
            >
          {/if}
          {t(`error.${item.code}`, item.message)}
        </li>
      {/each}
    </ul>
  {:else if spec.format.kind === 'binary'}
    {@const binary = value as unknown as BinaryValue}
    <p class="text-sm text-muted">
      {t('shell.binary_bytes', 'Binary data')} · {binary.len}
      {t('shell.bytes', 'bytes')}
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <pre
      class="max-h-[40vh] overflow-auto rounded-md border border-line bg-surface p-3 font-mono text-sm break-all whitespace-pre-wrap"
      tabindex="0">{binary.hex}</pre>
    <div><CopyButton text={binary.hex} label={t('shell.copy_hex', 'Copy hex')} /></div>
  {:else}
    <p class="text-sm text-muted">
      {t('shell.unsupported_output', 'This output needs a newer version of Navaja.')}
    </p>
  {/if}
</section>
