// The shell API a custom view (tools/<id>/ui/) may use, and the only app module
// it may import, so the shell can change behind these names without touching
// any tool. What else a view may import is in docs/architecture.md §4, and
// app/eslint/view-imports.js enforces it.
import type { ToolMeta } from '$bindings/ToolMeta';

export { default as CopyButton } from '../CopyButton.svelte';
export { default as ToolIcon } from '../ToolIcon.svelte';
export { OUTPUT_ATTRIBUTE } from '../copy';
export { t } from '../i18n';
export {
  copyText,
  runTool,
  type Run,
  type RunResult,
  type ToolInput,
  type ToolOutput,
} from '../ipc';

export type { Progress } from '$bindings/Progress';
export type { ToolError } from '$bindings/ToolError';
export type { ToolMeta };

/** What ToolHost passes to every custom view. */
export interface ViewProps {
  meta: ToolMeta;
}
