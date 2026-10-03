//! The webview guard plugin: a navigation allowlist and a script, injected
//! before any page script in each frame the engine covers, that hides WebRTC
//! (whose ICE/STUN traffic the CSP does not govern) and the native context
//! menu.
//! The script is one layer of defence, not a guarantee: the boundary is the
//! CSP's `script-src 'self'`, which keeps foreign script out of the app.
//! See docs/architecture.md §5.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, Url};

const SCRIPT: &str = include_str!("guard.js");

/// The app's own origin as (scheme, host). `useHttpsScheme` is off, so
/// Windows serves it over plain http.
#[cfg(windows)]
const APP_ORIGIN: (&str, &str) = ("http", "tauri.localhost");
#[cfg(not(windows))]
const APP_ORIGIN: (&str, &str) = ("tauri", "localhost");

/// The Vite dev server (`build.devUrl` in tauri.conf.json).
const DEV_SERVER_PORT: u16 = 1420;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("navaja-guard")
        .js_init_script_on_all_frames(SCRIPT)
        .on_navigation(|_webview, url| {
            let allowed = navigation_allowed(url, tauri::is_dev());
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

/// Only this platform's app origin, with no explicit port:
/// `http://tauri.localhost` on Windows, `tauri://localhost` elsewhere.
/// `dev_server` also allows the Vite dev server, which `tauri dev` loads the
/// front end from (a build that embeds the front end passes false).
pub fn navigation_allowed(url: &Url, dev_server: bool) -> bool {
    match (url.scheme(), url.host_str(), url.port()) {
        (scheme, Some(host), None) => (scheme, host) == APP_ORIGIN,
        ("http", Some("localhost"), Some(DEV_SERVER_PORT)) => dev_server,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(url: &str, dev_server: bool) -> bool {
        navigation_allowed(&Url::parse(url).unwrap(), dev_server)
    }

    #[test]
    fn only_this_platforms_app_origin() {
        let (own, other) = if cfg!(windows) {
            (
                "http://tauri.localhost/#/tool/uuid",
                "tauri://localhost/index.html",
            )
        } else {
            (
                "tauri://localhost/index.html",
                "http://tauri.localhost/#/tool/uuid",
            )
        };
        assert!(allowed(own, false));
        assert!(allowed(own, true));
        for blocked in [
            other,
            "https://tauri.localhost/",
            "http://tauri.localhost:8080/",
            "tauri://localhost:8080/",
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "file:///C:/Windows/System32/",
            "javascript:alert(1)",
            "data:text/html,<p>x</p>",
            "http://localhost/",
            "http://localhost:8080/",
            "ftp://tauri.localhost/",
        ] {
            assert!(!allowed(blocked, false), "{blocked}");
            assert!(!allowed(blocked, true), "{blocked} (dev)");
        }
    }

    #[test]
    fn dev_server_only_in_dev() {
        assert!(allowed("http://localhost:1420/", true));
        assert!(!allowed("http://localhost:1420/", false));
        assert!(!allowed("https://localhost:1420/", true));
        assert!(!allowed("http://localhost:1421/", true));
    }
}
