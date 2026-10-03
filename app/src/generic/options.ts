import type { OptionSpec } from '$bindings/OptionSpec';
import type { ToolInput } from '$lib/ipc';

/** The input the generic views start from: every option at its default. */
export function defaultValues(options: OptionSpec[], mode?: string): ToolInput {
  const values: ToolInput = {};
  for (const option of options) {
    if (mode && option.modes.length > 0 && !option.modes.includes(mode)) continue;
    const control = option.control;
    switch (control.kind) {
      case 'toggle':
      case 'choice':
      case 'integer':
      case 'text':
        values[option.key] = control.default;
        break;
      default:
        break;
    }
  }
  return values;
}
