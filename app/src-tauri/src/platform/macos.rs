#![allow(unsafe_code, reason = "Objective-C messages to the window's WKWebView")]

use std::ffi::c_void;

use objc2::msg_send;
use objc2::runtime::{AnyObject, Sel};
use objc2::sel;

/// The `WKPreferences` of the window's `WKWebView`, which every document the
/// webview creates from now on reads.
///
/// # Safety
///
/// `wk_webview` is the WKWebView wry created for the main window, handed
/// over by `with_webview` on the main thread, where AppKit objects may be
/// used; the reference is used only there and then.
unsafe fn preferences<'a>(wk_webview: *mut c_void) -> Option<&'a AnyObject> {
    // SAFETY: see above. `configuration` and `preferences` are public
    // accessors that take nothing and return an object, as WebKit declares.
    unsafe {
        let webview = wk_webview.cast::<AnyObject>().as_ref()?;
        let configuration: *mut AnyObject = msg_send![webview, configuration];
        let configuration = configuration.as_ref()?;
        let preferences: *mut AnyObject = msg_send![configuration, preferences];
        preferences.as_ref()
    }
}

/// Turns WebRTC off in WebKit itself, for every document the webview creates
/// from now on. The guard script removes the constructors only in frames
/// where WebKit injects it, and WebKit skips `srcdoc` frames. Returns false
/// when this WebKit has no such switch.
///
/// The switch is `WKPreferences`' private `_setPeerConnectionEnabled:`, the
/// same kind of private preference wry sets for the inspector. It is checked
/// with `respondsToSelector:` first, so a WebKit without it changes nothing.
pub fn disable_peer_connections(wk_webview: *mut c_void) -> bool {
    // SAFETY: `wk_webview` comes from `with_webview` on the main thread (see
    // `preferences`). The messages are `respondsToSelector:` (a selector in,
    // a BOOL out) and the setter just confirmed to exist, with a BOOL.
    unsafe {
        let Some(preferences) = preferences(wk_webview) else {
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

/// Turns WebKit's fraudulent-website warnings off, so that WebKit never
/// asks the system's Safe Browsing service about a page the webview loads.
/// That service is a separate daemon, outside the app's process tree, which
/// fetches its lists from Apple (spike S2.7). The webview only ever shows
/// Navaja's own pages, so there is nothing for it to check. Returns false
/// when the webview could not be reached.
///
/// `fraudulentWebsiteWarningEnabled` is public API (macOS 10.15 and later)
/// and on by default.
pub fn disable_fraudulent_website_warnings(wk_webview: *mut c_void) -> bool {
    // SAFETY: `wk_webview` comes from `with_webview` on the main thread (see
    // `preferences`). The setter is public on every macOS Navaja runs on,
    // and takes a BOOL.
    unsafe {
        let Some(preferences) = preferences(wk_webview) else {
            return false;
        };
        let _: () = msg_send![preferences, setFraudulentWebsiteWarningEnabled: false];
        true
    }
}
