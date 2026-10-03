// Test fixtures typed against the generated bindings, so a Rust-side change
// to the wire format breaks these at type-check time.
import type { Catalog } from '$bindings/Catalog';
import type { RunEnvelope } from '$bindings/RunEnvelope';
import type { SearchHit } from '$bindings/SearchHit';
import type { ToolMeta } from '$bindings/ToolMeta';
import { mockIPC } from '@tauri-apps/api/mocks';

const ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M4 12h16"/></svg>';

export const uuidTool = {
  specVersion: 1,
  id: 'uuid',
  name: 'UUID generator',
  description: 'Generates random (v4) or time-ordered (v7) UUIDs, one or many.',
  category: 'generators',
  keywords: ['uuid', 'guid'],
  icon: ICON,
  capabilities: [],
  actions: [{ id: 'generate', label: 'Generate', destructive: false }],
  tray: false,
  ui: {
    kind: 'generator',
    action: 'generate',
    options: [
      {
        key: 'version',
        label: 'Version',
        control: {
          kind: 'choice',
          choices: [
            { value: 'v4', label: 'Random (v4)' },
            { value: 'v7', label: 'Time-ordered (v7)' },
          ],
          default: 'v4',
        },
        modes: [],
      },
      {
        key: 'count',
        label: 'How many',
        control: { kind: 'integer', min: 1, max: 10000, default: 1 },
        modes: [],
      },
      {
        key: 'uppercase',
        label: 'Upper case',
        control: { kind: 'toggle', default: false },
        modes: [],
      },
    ],
    outputs: [{ key: 'uuids', label: 'UUIDs', format: { kind: 'text' } }],
    runOnOpen: true,
  },
} satisfies ToolMeta;

export const otherTool = {
  ...uuidTool,
  id: 'other',
  name: 'Other encoder',
  description: 'Another tool.',
  category: 'encoders',
  keywords: ['other'],
} satisfies ToolMeta;

export const catalog = {
  tools: [uuidTool, otherTool],
  categories: [
    { id: 'encoders', label: 'Encoders', order: 10 },
    { id: 'generators', label: 'Generators', order: 30 },
  ],
} satisfies Catalog;

export function envelopeBytes(envelope: RunEnvelope): ArrayBuffer {
  const bytes = new TextEncoder().encode(JSON.stringify(envelope));
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
}

export interface Recorded {
  cmd: string;
  args: Record<string, unknown>;
}

/** Answers the `index`-th `run_tool` call (0 is the first, often the run on open). */
export type RunAnswer = (
  index: number,
  args: Record<string, unknown>,
) => RunEnvelope | Promise<RunEnvelope>;

/**
 * Mocks the app's commands; `run_tool` answers with `runResult`. `commands`
 * replaces any command's answer: throw (or reject) to make it fail.
 */
export function mockApp(options: {
  runResult?: RunEnvelope | RunAnswer;
  search?: (query: string) => SearchHit[];
  commands?: Record<string, (args: Record<string, unknown>) => unknown>;
}): Recorded[] {
  const calls: Recorded[] = [];
  let runs = 0;
  mockIPC(async (cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    const override = options.commands?.[cmd];
    if (override) return override(args);
    switch (cmd) {
      case 'list_tools':
        return catalog;
      case 'settings_get':
        return { version: 1, theme: 'system' };
      case 'search': {
        const query = String(args.query ?? '');
        return options.search ? options.search(query) : [];
      }
      case 'run_tool': {
        const { runResult } = options;
        const envelope =
          typeof runResult === 'function' ? await runResult(runs++, args) : runResult;
        return envelopeBytes(envelope ?? { ok: { uuids: null } });
      }
      case 'cancel_run':
        return true;
      default:
        return null;
    }
  });
  return calls;
}
