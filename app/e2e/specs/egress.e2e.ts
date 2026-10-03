import { browser, expect } from '@wdio/globals';

// The webview must not be able to reach the network by any route: the CSP
// covers fetch, images, sockets and beacons; the guard script removes WebRTC
// in every frame; new windows are denied. See docs/architecture.md §5.
//
// The canary host doesn't resolve, so a failed request alone proves nothing:
// each attempt must also raise a CSP violation, i.e. be blocked before any
// lookup or connection.
const CANARY = 'https://navaja-canary.example.com/';

interface Outcome {
  webrtc: string;
  frameWebrtc: string;
  windowOpen: string;
  violations: string[];
}

describe('offline guarantee', () => {
  it('blocks every way the page could reach the network', async () => {
    const outcome = await browser.executeAsync((canary: string, done: (o: Outcome) => void) => {
      const violations: string[] = [];
      document.addEventListener('securitypolicyviolation', (event) => {
        violations.push(`${event.effectiveDirective} ${new URL(event.blockedURI).protocol}`);
      });

      const g = globalThis as unknown as Record<string, unknown>;
      const frame = document.createElement('iframe');
      document.body.append(frame);
      const frameGlobal = frame.contentWindow as unknown as Record<string, unknown> | null;
      const result: Outcome = {
        webrtc: typeof g.RTCPeerConnection === 'undefined' ? 'removed' : 'present',
        frameWebrtc:
          !frameGlobal || typeof frameGlobal.RTCPeerConnection === 'undefined' ? 'removed' : 'present',
        windowOpen: window.open(canary) === null ? 'blocked' : 'opened',
        violations,
      };
      frame.remove();

      fetch(`${canary}data`, { mode: 'no-cors' }).catch(() => {});
      const img = new Image();
      img.src = `${canary}pixel.png`;
      try {
        new WebSocket(canary.replace('https', 'wss'));
      } catch {
        // Some engines throw synchronously; the violation is still reported.
      }

      // Violation events are dispatched asynchronously.
      setTimeout(() => done(result), 1500);
    }, CANARY);

    expect(outcome.webrtc).toBe('removed');
    expect(outcome.frameWebrtc).toBe('removed');
    expect(outcome.windowOpen).toBe('blocked');
    expect(outcome.violations).toEqual(
      expect.arrayContaining(['connect-src https:', 'img-src https:', 'connect-src wss:']),
    );
    // The page is still the app.
    expect(await browser.execute(() => location.hostname)).toMatch(/localhost$/);
  });
});
