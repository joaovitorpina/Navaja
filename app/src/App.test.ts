import { render, screen } from '@testing-library/svelte';
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

describe('App', () => {
  afterEach(() => clearMocks());

  it('shows the version from app_info and reports ready', async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === 'app_info') return { name: 'Navaja', version: '9.9.9', specVersion: 1 };
      return null;
    });

    render(App);

    expect(screen.getByRole('heading', { name: 'Navaja' })).toBeTruthy();
    expect(await screen.findByText('Version 9.9.9')).toBeTruthy();
    await vi.waitFor(() => expect(calls).toContain('shell_ready'));
  });
});
