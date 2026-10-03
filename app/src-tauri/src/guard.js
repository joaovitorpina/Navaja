// Navaja webview guard. The engine injects it before any page script in each
// frame it covers. It is one layer: CSP script-src 'self' is the boundary that
// keeps foreign code out of the app.
(() => {
  // WebRTC can open network connections (ICE/STUN) that the CSP does not
  // govern, so the constructors are removed in every frame this script runs
  // in. A frame the engine never injects it into keeps them.
  const webrtc = [
    'RTCPeerConnection',
    'webkitRTCPeerConnection',
    'RTCDataChannel',
    'RTCSessionDescription',
    'RTCIceCandidate',
    'RTCRtpSender',
    'RTCRtpReceiver',
    'RTCRtpTransceiver',
  ];
  for (const name of webrtc) {
    try {
      Object.defineProperty(globalThis, name, {
        value: undefined,
        writable: false,
        configurable: false,
      });
    } catch {
      // Already non-configurable in this engine; nothing more to do.
    }
  }

  // No native context menu (reload, inspect, open in browser) outside text
  // fields, where cut, copy and paste stay available. A missing or unknown
  // input type reads as 'text'. Readonly fields count, so outputs can be copied.
  const textTypes = new Set(['text', 'search', 'url', 'tel', 'email', 'password', 'number']);
  addEventListener(
    'contextmenu',
    (event) => {
      const target = event.target;
      const editable =
        (target instanceof HTMLInputElement && textTypes.has(target.type)) ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable);
      if (!editable) event.preventDefault();
    },
    { capture: true },
  );
})();
