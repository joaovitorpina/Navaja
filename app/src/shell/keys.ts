export const isMac = (): boolean => /Mac|iPhone|iPad/.test(navigator.userAgent);

/** "⌘K" on macOS, "Ctrl K" elsewhere. */
export const shortcutLabel = (): string => (isMac() ? '⌘K' : 'Ctrl K');

/** What `aria-keyshortcuts` should announce on this platform. */
export const shortcutAria = (): string => (isMac() ? 'Meta+K' : 'Control+K');

const PRINTABLE_ASCII = /^[\x20-\x7e]$/;

/** Ctrl+K (Cmd+K on macOS) opens the command palette. */
export function isPaletteShortcut(event: KeyboardEvent): boolean {
  const modifier = isMac() ? event.metaKey : event.ctrlKey;
  if (!modifier || event.altKey || event.shiftKey) return false;
  // The letter K wherever the layout puts it (Dvorak, AZERTY). Non-Latin
  // layouts report their own letter (Cyrillic "л"), so fall back to the key
  // in the K position.
  return (
    event.key.toLowerCase() === 'k' || (!PRINTABLE_ASCII.test(event.key) && event.code === 'KeyK')
  );
}
