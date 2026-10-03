import { describe, expect, it } from 'vitest';
import { isMac, isPaletteShortcut } from './keys';

const press = (init: KeyboardEventInit) =>
  isPaletteShortcut(
    new KeyboardEvent('keydown', { ...init, [isMac() ? 'metaKey' : 'ctrlKey']: true }),
  );

describe('palette shortcut', () => {
  it('matches the letter K with the platform modifier', () => {
    expect(press({ key: 'k', code: 'KeyK' })).toBe(true);
    expect(press({ key: 'K', code: 'KeyK' })).toBe(true);
    expect(isPaletteShortcut(new KeyboardEvent('keydown', { key: 'k', code: 'KeyK' }))).toBe(false);
    expect(press({ key: 'k', code: 'KeyK', shiftKey: true })).toBe(false);
    expect(press({ key: 'k', code: 'KeyK', altKey: true })).toBe(false);
  });

  it('falls back to the K position on non-Latin layouts', () => {
    expect(press({ key: 'л', code: 'KeyK' })).toBe(true);
    expect(press({ key: 'κ', code: 'KeyK' })).toBe(true);
  });

  it('follows the letter, not the position, on Latin layouts', () => {
    // Dvorak: the K position types "t", and K sits where QWERTY has V.
    expect(press({ key: 't', code: 'KeyK' })).toBe(false);
    expect(press({ key: 'k', code: 'KeyV' })).toBe(true);
  });
});
