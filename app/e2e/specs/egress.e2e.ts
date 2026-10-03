import { $, browser, expect } from '@wdio/globals';
import { goHome } from '../support/app';

// The webview must not be able to reach the network by any route: the CSP
// covers fetch, beacons, images and sockets; the guard script removes WebRTC
// in every frame, including frames the page makes itself; new windows are
// denied; navigation away from the app is refused. See docs/architecture.md §5.
//
// The canary hosts don't resolve, so a failed request alone proves nothing:
// each attempt must also raise its own CSP violation, i.e. be blocked before
// any lookup or connection. Each probe has its own host, the part of a blocked
// URL that every engine keeps in the report (WebKit may cut a cross-origin URL
// down to its origin).
const CANARY = 'navaja-canary.example.com';

interface Probe {
  url: string;
  /** The violation the probe must raise, as "directive protocol//host". */
  violation: string;
}

const PROBES = {
  fetch: { url: `https://fetch.${CANARY}/data`, violation: `connect-src https://fetch.${CANARY}` },
  beacon: {
    url: `https://beacon.${CANARY}/beacon`,
    violation: `connect-src https://beacon.${CANARY}`,
  },
  image: { url: `https://img.${CANARY}/pixel.png`, violation: `img-src https://img.${CANARY}` },
  socket: { url: `wss://ws.${CANARY}/socket`, violation: `connect-src wss://ws.${CANARY}` },
} satisfies Record<string, Probe>;

const POPUP = `https://open.${CANARY}/`;
const NAVIGATION = `https://navigate.${CANARY}/`;

/** The frames the page makes; each is checked as soon as it exists and once it has loaded. */
const FRAMES = ['no src', 'src=about:blank', 'srcdoc', 'nested in another frame'];

interface Outcome {
  error?: string;
  /** What the page itself still exposes of WebRTC. */
  webrtc: string;
  /** The same, per frame and moment ("<frame>" and "<frame>, loaded"). */
  frames: Record<string, string>;
  windowOpen: string;
  violations: string[];
}

/** The window is still the app: its origin and its own navigation. */
async function stillTheApp(): Promise<void> {
  expect(await browser.execute(() => window.location.hostname)).toMatch(/localhost$/);
  await expect($('nav[aria-label="Tools"]')).toBeExisting();
}

describe('offline guarantee', () => {
  beforeEach(goHome);

  it('blocks every way the page could reach the network', async () => {
    const outcome = await browser.executeAsync(
      (probes: typeof PROBES, popup: string, done: (outcome: Outcome) => void) => {
        const violations: string[] = [];
        document.addEventListener('securitypolicyviolation', (event) => {
          try {
            const blocked = new URL(event.blockedURI);
            violations.push(`${event.effectiveDirective} ${blocked.protocol}//${blocked.host}`);
          } catch {
            violations.push(`${event.effectiveDirective} ${event.blockedURI}`);
          }
        });

        // A frame without a window counts as a failure, not as "removed".
        const webrtcIn = (realm: Window | null | undefined): string => {
          if (!realm) return 'no window';
          try {
            const scope = realm as unknown as Record<string, unknown>;
            const present = ['RTCPeerConnection', 'RTCDataChannel', 'webkitRTCPeerConnection'].filter(
              (name) => typeof scope[name] !== 'undefined',
            );
            return present.length === 0 ? 'removed' : `present: ${present.join(', ')}`;
          } catch (error) {
            return `unreadable: ${String(error)}`;
          }
        };

        // Resolves once the frame has loaded, or after 2 s if it never does.
        const addFrame = (doc: Document, setup: (frame: HTMLIFrameElement) => void) => {
          const frame = doc.createElement('iframe');
          setup(frame);
          const loaded = new Promise<void>((resolve) => {
            frame.addEventListener('load', () => resolve(), { once: true });
            setTimeout(resolve, 2_000);
          });
          (doc.body ?? doc.documentElement).append(frame);
          return { frame, loaded };
        };

        const run = async (): Promise<Outcome> => {
          const frames: Record<string, string> = {};
          const bare = addFrame(document, () => {});
          const blank = addFrame(document, (frame) => {
            frame.src = 'about:blank';
          });
          const srcdoc = addFrame(document, (frame) => {
            frame.srcdoc = '<p>srcdoc</p>';
          });
          const outer = addFrame(document, () => {});
          frames['no src'] = webrtcIn(bare.frame.contentWindow);
          frames['src=about:blank'] = webrtcIn(blank.frame.contentWindow);
          frames.srcdoc = webrtcIn(srcdoc.frame.contentWindow);

          const windowOpen = window.open(popup) === null ? 'blocked' : 'opened';

          fetch(probes.fetch.url, { mode: 'no-cors' }).catch(() => {});
          try {
            navigator.sendBeacon(probes.beacon.url, 'x');
          } catch {
            // Some engines throw; the violation is still reported.
          }
          const img = new Image();
          img.src = probes.image.url;
          try {
            new WebSocket(probes.socket.url);
          } catch {
            // Some engines throw synchronously; the violation is still reported.
          }

          // The nested frame goes into the outer frame's document once that
          // has settled, so a late load can't replace it.
          await Promise.all([bare.loaded, blank.loaded, srcdoc.loaded, outer.loaded]);
          const outerDocument = outer.frame.contentDocument;
          const nested = outerDocument ? addFrame(outerDocument, () => {}) : undefined;
          frames['nested in another frame'] = webrtcIn(nested?.frame.contentWindow);
          await nested?.loaded;

          frames['no src, loaded'] = webrtcIn(bare.frame.contentWindow);
          frames['src=about:blank, loaded'] = webrtcIn(blank.frame.contentWindow);
          frames['srcdoc, loaded'] = webrtcIn(srcdoc.frame.contentWindow);
          frames['nested in another frame, loaded'] = webrtcIn(nested?.frame.contentWindow);

          // Violation events arrive asynchronously: wait for every probe's.
          const expected = Object.values(probes).map((probe) => probe.violation);
          const deadline = Date.now() + 10_000;
          while (!expected.every((v) => violations.includes(v)) && Date.now() < deadline) {
            await new Promise((resolve) => setTimeout(resolve, 50));
          }

          for (const { frame } of [bare, blank, srcdoc, outer]) frame.remove();
          return { webrtc: webrtcIn(window), frames, windowOpen, violations: [...violations] };
        };

        run().then(done, (error: unknown) =>
          done({ error: String(error), webrtc: '', frames: {}, windowOpen: '', violations }),
        );
      },
      PROBES,
      POPUP,
    );

    expect(outcome.error).toBeUndefined();
    expect(outcome.webrtc).toBe('removed');
    expect(outcome.frames).toEqual(
      Object.fromEntries(
        FRAMES.flatMap((frame) => [
          [frame, 'removed'],
          [`${frame}, loaded`, 'removed'],
        ]),
      ),
    );
    expect(outcome.windowOpen).toBe('blocked');
    for (const probe of Object.values(PROBES)) {
      expect(outcome.violations).toContain(probe.violation);
    }
    await stillTheApp();
  });

  it('refuses to navigate away from the app', async () => {
    await browser.execute((url: string) => {
      window.location.href = url;
    }, NAVIGATION);
    await browser.pause(1_000);
    await stillTheApp();

    // A link with no target, clicked like a user would.
    await browser.execute((url: string) => {
      const link = document.createElement('a');
      link.id = 'navaja-canary-link';
      link.href = url;
      link.textContent = 'canary';
      link.style.cssText = 'position:fixed;top:0;left:0;z-index:2147483647';
      document.body.append(link);
    }, NAVIGATION);
    await $('#navaja-canary-link').click();
    await browser.pause(1_000);
    await stillTheApp();
    await browser.execute(() => document.getElementById('navaja-canary-link')?.remove());
  });
});
