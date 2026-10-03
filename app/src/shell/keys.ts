export const isMac = (): boolean => /Mac|iPhone|iPad/.test(navigator.userAgent);

/** "⌘K" on macOS, "Ctrl K" elsewhere. */
export const shortcutLabel = (): string => (isMac() ? '⌘K' : 'Ctrl K');

/** Ctrl+K (Cmd+K on macOS) opens the command palette. */
export function isPaletteShortcut(event: KeyboardEvent): boolean {
  const modifier = isMac() ? event.metaKey : event.ctrlKey;
  return modifier && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k';
}
