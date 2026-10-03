import { copyText } from './ipc';

/**
 * `oncopy` handler for regions that show tool output. A native copy (Ctrl/Cmd+C,
 * the context menu) would skip the clipboard-history exclusion markers, so the
 * selection is copied through Rust instead, like the Copy button.
 */
export function copySelection(event: ClipboardEvent): void {
  const text = window.getSelection()?.toString() ?? '';
  if (!text) return;
  event.preventDefault();
  // Nothing to show on failure: the clipboard keeps what it had.
  copyText(text).catch(() => {});
}
