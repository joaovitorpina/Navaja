// Navaja webview guard. Runs in every frame before any page script.
(() => {
  // WebRTC can open network connections (ICE/STUN) that the CSP does not
  // govern, so the constructors are removed for good.
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

  // No native context menu (reload, inspect, open in browser) outside
  // editable fields, where cut, copy and paste stay available.
  addEventListener(
    'contextmenu',
    (event) => {
      const target = event.target;
      const editable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        (target instanceof HTMLElement && target.isContentEditable);
      if (!editable) event.preventDefault();
    },
    { capture: true },
  );
})();
