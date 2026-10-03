import { copyText } from './ipc';

/** Marks a region that shows tool output; custom views add it to theirs. */
export const OUTPUT_ATTRIBUTE = 'data-output';

/**
 * Sends native copies (Ctrl/Cmd+C, the context menu) of tool output through
 * Rust, like the Copy button, so they get the clipboard-history exclusion
 * markers. The copy event fires where the selection starts, which can be
 * outside the output (Ctrl+A starts in the sidebar), so this listens on the
 * whole document and acts when the selection touches any output region.
 * Copies from text fields stay native: they hold the user's own input.
 * Installed once by the shell; returns the uninstaller.
 */
export function routeOutputCopies(doc: Document = document): () => void {
  const onCopy = (event: ClipboardEvent) => {
    const active = doc.activeElement;
    if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) return;
    const selection = doc.getSelection();
    if (!selection || selection.isCollapsed || !touchesOutput(doc, selection)) return;
    const text = selection.toString();
    if (!text) return;
    event.preventDefault();
    // Nothing to show on failure: the clipboard keeps what it had.
    copyText(text).catch(() => {});
  };
  doc.addEventListener('copy', onCopy);
  return () => doc.removeEventListener('copy', onCopy);
}

function touchesOutput(doc: Document, selection: Selection): boolean {
  const regions = doc.querySelectorAll(`[${OUTPUT_ATTRIBUTE}]`);
  for (let i = 0; i < selection.rangeCount; i++) {
    const range = selection.getRangeAt(i);
    for (const region of regions) {
      if (range.intersectsNode(region)) return true;
    }
  }
  return false;
}
