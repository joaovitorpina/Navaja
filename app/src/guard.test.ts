// The webview guard (src-tauri/src/guard.js) runs here once, before the tests,
// as the engine runs it before page scripts. It pins globals that cannot be
// restored; Vitest gives every test file its own environment, so they stay
// in this file.
import { beforeAll, describe, expect, it } from 'vitest';
import guard from '../src-tauri/src/guard.js?raw';

const webrtc = ['RTCPeerConnection', 'RTCDataChannel', 'webkitRTCPeerConnection'];

beforeAll(() => {
  // jsdom has no WebRTC: stand in for the engine's constructors.
  for (const name of webrtc) Reflect.set(globalThis, name, class {});
  new Function(guard)();
});

/** Right-clicks `element` and says whether the native menu was suppressed. */
function menuSuppressed(element: HTMLElement): boolean {
  document.body.append(element);
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
  element.dispatchEvent(event);
  element.remove();
  return event.defaultPrevented;
}

function input(type?: string): HTMLInputElement {
  const element = document.createElement('input');
  if (type !== undefined) element.setAttribute('type', type);
  return element;
}

describe('webview guard', () => {
  it('removes the WebRTC constructors', () => {
    for (const name of webrtc) {
      expect(Reflect.get(globalThis, name), name).toBeUndefined();
      expect(Object.getOwnPropertyDescriptor(globalThis, name), name).toMatchObject({
        writable: false,
        configurable: false,
      });
      expect(Reflect.set(globalThis, name, class {}), name).toBe(false);
      expect(() => Object.defineProperty(globalThis, name, { value: class {} }), name).toThrow(
        TypeError,
      );
      expect(Reflect.get(globalThis, name), name).toBeUndefined();
    }
  });

  it('suppresses the native context menu outside text fields', () => {
    const targets: [string, HTMLElement][] = [
      ['div', document.createElement('div')],
      ['button', document.createElement('button')],
      ...['checkbox', 'radio', 'range', 'color', 'button', 'submit', 'file'].map(
        (type): [string, HTMLElement] => [`input[type=${type}]`, input(type)],
      ),
    ];
    for (const [label, element] of targets) {
      expect(menuSuppressed(element), label).toBe(true);
    }
  });

  it('keeps the native context menu in text fields', () => {
    const readonly = document.createElement('textarea');
    readonly.readOnly = true;
    // jsdom does not implement isContentEditable.
    const editable = document.createElement('div');
    editable.contentEditable = 'true';
    Object.defineProperty(editable, 'isContentEditable', { value: true });
    const targets: [string, HTMLElement][] = [
      ['input', input()],
      ['input[type=unknown]', input('unknown')],
      ...['text', 'search', 'url', 'tel', 'email', 'password', 'number'].map(
        (type): [string, HTMLElement] => [`input[type=${type}]`, input(type)],
      ),
      ['textarea', document.createElement('textarea')],
      ['readonly textarea', readonly],
      ['contenteditable', editable],
    ];
    for (const [label, element] of targets) {
      expect(menuSuppressed(element), label).toBe(false);
    }
  });
});
