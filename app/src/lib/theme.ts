import type { Theme } from '$bindings/Theme';

/** `system` follows the OS; light and dark override it (Rust sets the native theme). */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme);
}
