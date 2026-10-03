import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

// jsdom has no layout. The palette's Command.Viewport observes its size, and
// Command scrolls the highlighted item into view.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
Element.prototype.scrollIntoView ??= () => {};

afterEach(() => {
  cleanup();
  clearMocks();
  window.location.hash = '';
});
