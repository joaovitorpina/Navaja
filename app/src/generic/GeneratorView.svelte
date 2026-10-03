<script lang="ts">
  // Options -> output, no input (e.g. UUID). Rendered from GeneratorSpec.
  import { onMount, untrack } from 'svelte';
  import type { GeneratorSpec } from '$bindings/GeneratorSpec';
  import type { ToolError } from '$bindings/ToolError';
  import type { ToolMeta } from '$bindings/ToolMeta';
  import { t } from '$lib/i18n';
  import { type Run, type ToolInput, type ToolOutput, isToolOutput, runTool } from '$lib/ipc';
  import { defaultValues } from './options';
  import OptionControl from './OptionControl.svelte';
  import OutputView from './OutputView.svelte';

  let { meta, spec }: { meta: ToolMeta; spec: GeneratorSpec } = $props();

  // Initial values only: ToolHost remounts this view for each tool.
  let values = $state<ToolInput>(untrack(() => defaultValues(spec.options)));
  let output = $state.raw<ToolOutput | null>(null);
  let error = $state.raw<ToolError | null>(null);
  let running = $state(false);
  let current: Run | null = null;

  const action = $derived(meta.actions.find((a) => a.id === spec.action));
  // Destructive actions need the confirmation dialog, which doesn't exist yet
  // (Rust rejects such generators too), so they never run from here.
  const blocked = $derived(action?.destructive ?? false);

  async function generate() {
    if (blocked) return;
    void current?.cancel();
    // Option values are primitives, so a shallow copy is a plain snapshot.
    const run = runTool(meta.id, spec.action, { ...values });
    current = run;
    running = true;
    const result = await run.result;
    if (current !== run) return; // superseded by a newer run
    running = false;
    if (!result.ok) {
      error = result.error;
    } else if (isToolOutput(result.value)) {
      output = result.value;
      error = null;
    } else {
      error = { code: 'core.ipc', message: 'malformed tool output', details: null };
    }
  }

  onMount(() => {
    if (spec.runOnOpen) void generate();
    return () => void current?.cancel();
  });
</script>

<form
  class="flex flex-wrap items-end gap-4"
  onsubmit={(e) => {
    e.preventDefault();
    void generate();
  }}
>
  {#each spec.options as option (option.key)}
    <OptionControl toolId={meta.id} {option} bind:value={values[option.key]} />
  {/each}
  <button
    type="submit"
    class="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-canvas hover:opacity-90 disabled:opacity-50"
    aria-busy={running}
    disabled={blocked}
    aria-describedby={blocked ? `confirm-${meta.id}` : undefined}
  >
    {action ? t(`tool.${meta.id}.action.${action.id}`, action.label) : spec.action}
  </button>
</form>

{#if blocked}
  <p id="confirm-{meta.id}" class="mt-4 text-sm text-muted">
    {t(
      'shell.confirm_unavailable',
      'This action needs a confirmation step that this version of Navaja doesn’t have yet.',
    )}
  </p>
{/if}

{#if error}
  <p role="alert" class="mt-4 text-sm text-danger">{t(`error.${error.code}`, error.message)}</p>
{/if}

<div class="mt-6 flex flex-col gap-6">
  {#each spec.outputs as out (out.key)}
    <OutputView toolId={meta.id} spec={out} value={output?.[out.key]} />
  {/each}
</div>
