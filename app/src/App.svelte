<script lang="ts">
  import { onMount } from 'svelte';
  import { getAppInfo, shellReady, type AppInfo } from './lib/ipc';

  let info = $state<AppInfo | null>(null);

  onMount(async () => {
    try {
      info = await getAppInfo();
    } finally {
      await shellReady();
    }
  });
</script>

<main>
  <h1>Navaja</h1>
  <p>Offline developer toolbox.</p>
  {#if info}
    <p class="version">Version {info.version}</p>
  {/if}
</main>

<style>
  main {
    padding: 2rem;
  }
  .version {
    color: var(--nv-steel-600);
  }
</style>
