// The shell API a custom view (tools/<id>/ui/) may use. Besides this module a
// view imports only `./`, `svelte` and `$bindings/*` (docs/architecture.md §4),
// so the shell can change behind these names without touching any tool.
import type { ToolMeta } from '$bindings/ToolMeta';

export { default as CopyButton } from '../CopyButton.svelte';
export { default as ToolIcon } from '../ToolIcon.svelte';
export { copySelection } from '../copy';
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
