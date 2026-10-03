import { mount } from 'svelte';
import App from './App.svelte';
import './app.css';

// End-to-end builds only (VITE_NAVAJA_E2E=1): WebdriverIO's bridge. The
// condition is replaced at build time, so release bundles don't contain it.
if (import.meta.env.VITE_NAVAJA_E2E === '1') {
  await import('@wdio/tauri-plugin');
}

const target = document.getElementById('app');
if (!target) {
  throw new Error('#app element missing from index.html');
}

export default mount(App, { target });
