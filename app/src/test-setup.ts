import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/svelte';
import { afterEach } from 'vitest';

afterEach(() => {
  cleanup();
  clearMocks();
  window.location.hash = '';
});
