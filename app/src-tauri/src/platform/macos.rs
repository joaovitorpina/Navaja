#![allow(unsafe_code, reason = "Objective-C messages to the window's WKWebView")]

use std::ffi::c_void;

use objc2::msg_send;
use objc2::runtime::{AnyObject, Sel};
use objc2::sel;

/// Turns WebRTC off in WebKit itself, for every document the webview creates
/// from now on. The guard script removes the constructors only in frames
/// where WebKit injects it, and WebKit skips `srcdoc` frames. Returns false
/// when this WebKit has no such switch.
///
/// The switch is `WKPreferences`' private `_setPeerConnectionEnabled:`, the
/// same kind of private preference wry sets for the inspector. It is checked
/// with `respondsToSelector:` first, so a WebKit without it changes nothing.
pub fn disable_peer_connections(wk_webview: *mut c_void) -> bool {
    // SAFETY: `wk_webview` is the WKWebView wry created for the main window,
    // handed over by `with_webview` on the main thread, where AppKit objects
    // may be used. Each message is a public accessor (`configuration`,
    // `preferences`, `respondsToSelector:`) or the setter just confirmed to
    // exist, with the types WebKit declares: objects in, objects or BOOL out.
    unsafe {
        let Some(webview) = wk_webview.cast::<AnyObject>().as_ref() else {
            return false;
        };
        let configuration: *mut AnyObject = msg_send![webview, configuration];
        let Some(configuration) = configuration.as_ref() else {
            return false;
        };
        let preferences: *mut AnyObject = msg_send![configuration, preferences];
        let Some(preferences) = preferences.as_ref() else {
            return false;
        };
        let setter: Sel = sel!(_setPeerConnectionEnabled:);
        let supported: bool = msg_send![preferences, respondsToSelector: setter];
        if supported {
            let _: () = msg_send![preferences, _setPeerConnectionEnabled: false];
        }
        supported
    }
}
