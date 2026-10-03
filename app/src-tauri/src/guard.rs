//! The webview guard plugin: a navigation allowlist and a script, injected
//! into every frame before any page script, that removes WebRTC (whose
//! ICE/STUN traffic the CSP does not govern) and the native context menu.
//! See docs/architecture.md §5.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, Url};

const SCRIPT: &str = include_str!("guard.js");

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navaja-guard")
        .js_init_script_on_all_frames(SCRIPT)
        .on_navigation(|_webview, url| {
            let allowed = navigation_allowed(url);
            if !allowed {
                // The URL itself is not logged: it could carry pasted data.
                tracing::warn!(
                    scheme = url.scheme(),
                    "blocked a navigation away from the app"
                );
            }
            allowed
        })
        .build()
}

/// Only the app's own origin: `tauri://localhost` (macOS, Linux),
/// `http://tauri.localhost` (Windows) and, in debug builds, the Vite dev server.
pub fn navigation_allowed(url: &Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("tauri", Some("localhost")) => true,
        ("http" | "https", Some("tauri.localhost")) => true,
        ("http", Some("localhost")) => cfg!(debug_assertions) && url.port() == Some(1420),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str) -> bool {
        navigation_allowed(&Url::parse(url).unwrap())
    }

    #[test]
    fn only_the_app_origin() {
        assert!(allowed("tauri://localhost/index.html"));
        assert!(allowed("http://tauri.localhost/#/tool/uuid"));
        for blocked in [
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "file:///C:/Windows/System32/",
            "javascript:alert(1)",
            "data:text/html,<p>x</p>",
            "http://localhost:8080/",
            "ftp://tauri.localhost/",
        ] {
            assert!(!allowed(blocked), "{blocked}");
        }
    }

    #[test]
    fn dev_server_only_in_debug_builds() {
        assert_eq!(allowed("http://localhost:1420/"), cfg!(debug_assertions));
    }
}
