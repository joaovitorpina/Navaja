import { invoke } from '@tauri-apps/api/core';

/** Mirrors `commands::AppInfo` in app/src-tauri (generated bindings replace this in a later PR). */
export interface AppInfo {
  name: string;
  version: string;
  specVersion: number;
}

export const getAppInfo = (): Promise<AppInfo> => invoke<AppInfo>('app_info');

/** Tells the shell the first frame is rendered, so the window can be shown. */
export const shellReady = (): Promise<void> => invoke<void>('shell_ready');
