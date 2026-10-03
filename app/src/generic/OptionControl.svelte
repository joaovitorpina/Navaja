<script lang="ts">
  // One option of a generic view, rendered from its declarative Control.
  // Native controls: accessible and native-looking on all three webviews.
  import type { OptionSpec } from '$bindings/OptionSpec';
  import type { JsonValue } from '$bindings/serde_json/JsonValue';
  import { t } from '$lib/i18n';

  let {
    toolId,
    option,
    value = $bindable(),
  }: { toolId: string; option: OptionSpec; value: JsonValue | undefined } = $props();

  const id = $derived(`opt-${toolId}-${option.key}`);
  const label = $derived(t(`tool.${toolId}.option.${option.key}.label`, option.label));
  const control = $derived(option.control);
</script>

{#if control.kind === 'toggle'}
  <label class="flex items-center gap-2 text-sm" for={id}>
    <input
      {id}
      type="checkbox"
      class="size-4 accent-accent"
      checked={value === true}
      onchange={(e) => (value = e.currentTarget.checked)}
    />
    {label}
  </label>
{:else if control.kind === 'choice'}
  <label class="flex flex-col gap-1 text-sm" for={id}>
    <span class="text-muted">{label}</span>
    <select
      {id}
      class="rounded-md border border-muted bg-surface px-2 py-1.5"
      value={typeof value === 'string' ? value : control.default}
      onchange={(e) => (value = e.currentTarget.value)}
    >
      {#each control.choices as choice (choice.value)}
        <option value={choice.value}>
          {t(`tool.${toolId}.option.${option.key}.choice.${choice.value}`, choice.label)}
        </option>
      {/each}
    </select>
  </label>
{:else if control.kind === 'integer'}
  <label class="flex flex-col gap-1 text-sm" for={id}>
    <span class="text-muted">{label}</span>
    <!-- Required: a blank or non-numeric field is invalid and blocks the form,
         instead of silently sending the last number it held. -->
    <input
      {id}
      type="number"
      inputmode="numeric"
      class="w-32 rounded-md border border-muted bg-surface px-2 py-1.5 user-invalid:border-danger"
      required
      min={control.min}
      max={control.max}
      step="1"
      value={typeof value === 'number' ? value : control.default}
      oninput={(e) => {
        const next = e.currentTarget.valueAsNumber;
        if (Number.isInteger(next)) value = next;
      }}
    />
  </label>
{:else if control.kind === 'text'}
  <label class="flex flex-col gap-1 text-sm" for={id}>
    <span class="text-muted">{label}</span>
    <input
      {id}
      type="text"
      class="rounded-md border border-muted bg-surface px-2 py-1.5"
      maxlength={control.limit}
      value={typeof value === 'string' ? value : control.default}
      oninput={(e) => (value = e.currentTarget.value)}
    />
  </label>
{/if}
