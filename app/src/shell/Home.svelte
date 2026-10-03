<script lang="ts">
  import type { Catalog } from '$bindings/Catalog';
  import ToolIcon from '$lib/ToolIcon.svelte';
  import { t } from '$lib/i18n';
  import { href } from '$lib/router.svelte';
  import { shortcutLabel } from './keys';
  import { toolGroups } from './order';

  let { catalog }: { catalog: Catalog } = $props();

  const groups = $derived(toolGroups(catalog));
</script>

<div class="mx-auto flex max-w-5xl flex-col gap-6 p-6">
  <header>
    <h1 class="text-2xl font-semibold">Navaja</h1>
    <p class="text-muted">
      {t('shell.home.tagline', 'Offline developer toolbox. Nothing you paste leaves this machine.')}
      {t('shell.home.palette_hint', 'Press')}
      <kbd class="rounded border border-line px-1 text-xs">{shortcutLabel()}</kbd>
      {t('shell.home.palette_hint_end', 'to jump to a tool.')}
    </p>
  </header>

  {#each groups as { category, tools } (category.id)}
    <section>
      <h2 class="mb-2 text-sm font-semibold text-muted">
        {t(`category.${category.id}`, category.label)}
      </h2>
      <ul class="grid grid-cols-[repeat(auto-fill,minmax(14rem,1fr))] gap-3">
        {#each tools as tool (tool.id)}
          <li>
            <a
              href={href({ kind: 'tool', id: tool.id })}
              class="flex h-full items-start gap-3 rounded-lg border border-line bg-surface p-3 hover:border-accent"
            >
              <ToolIcon svg={tool.icon} class="mt-0.5 text-accent" />
              <span>
                <span class="block font-medium">{t(`tool.${tool.id}.name`, tool.name)}</span>
                <span class="block text-sm text-muted"
                  >{t(`tool.${tool.id}.description`, tool.description)}</span
                >
              </span>
            </a>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>
