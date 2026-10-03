//! Hands one of Navaja's own links to the default browser, or a folder to
//! the file manager, through tauri-plugin-opener's free functions. The
//! plugin is never registered, so the webview has none of its commands, and
//! only the app's own `open_url` and `open_logs` reach this module.
//!
//! What starts (docs/architecture.md §5): on Windows, `ShellExecuteExW` for
//! a URL and `SHOpenFolderAndSelectItems` for a folder; on macOS,
//! `/usr/bin/open`, which hands the target to LaunchServices and exits; on
//! Linux, `xdg-open` (or gio, gnome-open, kde-open) after a double fork and
//! `setsid`.

use std::path::Path;

/// The repository, spelled as frozen in docs/adr/0001-stack.md.
pub const REPOSITORY: &str = "https://github.com/joaovitorpina/Navaja";

/// Every URL Navaja links to, exactly as the front end sends it. Pages under
/// the repository are refused too: GitHub serves a fork's commits under the
/// parent's URLs (`/raw/<sha>/...`, `/archive/<sha>.zip`), so a rule for
/// paths would let a page open content anyone can push. A new link adds its
/// exact URL here.
const ALLOWED_URLS: &[&str] = &[REPOSITORY];

/// Whether `url` is, byte for byte, one of [`ALLOWED_URLS`].
pub fn is_allowed_url(url: &str) -> bool {
    ALLOWED_URLS.contains(&url)
}

/// Opens `url` in the default browser. The caller has checked it with
/// [`is_allowed_url`]; it is passed on exactly as given.
pub fn open_url(url: &str) -> Result<(), String> {
    let url = url.to_owned();
    hand_off("the web browser", move || {
        tauri_plugin_opener::open_url(url, None::<&str>)
    })
}

/// Opens `dir`, which must exist, in the OS file manager.
pub fn open_folder(dir: &Path) -> Result<(), String> {
    let dir = dir.to_path_buf();
    hand_off("the file manager", move || {
        tauri_plugin_opener::open_path(dir, None::<&str>)
    })
}

/// Runs `open` on a thread of its own and waits for it. On Windows, opening
/// a folder initialises COM on the calling thread and never releases it, so
/// this keeps that off the async runtime's shared worker threads.
fn hand_off(
    what: &'static str,
    open: impl FnOnce() -> Result<(), tauri_plugin_opener::Error> + Send + 'static,
) -> Result<(), String> {
    let failed = || format!("Could not open {what}.");
    let joined = std::thread::Builder::new()
        .name("navaja-opener".to_owned())
        .spawn(open)
        .map_err(|error| {
            tracing::warn!(%error, "could not start a thread to open {what}");
            failed()
        })?
        .join();
    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => {
            // The error's text can quote the URL or the path, so only its kind.
            let kind = match &error {
                tauri_plugin_opener::Error::Io(io) => io.kind().to_string(),
                _ => "other".to_owned(),
            };
            tracing::warn!(%kind, "could not open {what}");
            Err(failed())
        }
        // The panic hook has logged where, without the payload.
        Err(_) => Err(failed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn under(rest: &str) -> String {
        format!("{REPOSITORY}{rest}")
    }

    #[test]
    fn the_listed_urls_only() {
        assert_eq!(ALLOWED_URLS, [REPOSITORY]);
        assert!(is_allowed_url(REPOSITORY));
    }

    #[test]
    fn everything_else_is_refused() {
        for url in [
            String::new(),
            // Pages under the repository, a fork's commits among them.
            under("/issues"),
            under("/releases"),
            under("/blob/main/SECURITY.md"),
            under("/releases/tag/v1.0.0"),
            under("/security/advisories/new"),
            under("/raw/0123456789abcdef0123456789abcdef01234567/page.html"),
            under("/archive/0123456789abcdef0123456789abcdef01234567.zip"),
            under("/tree/0123456789abcdef0123456789abcdef01234567"),
            under("#readme"),
            under("/blob/main/docs/install.md#windows"),
            under("/..."),
            under(&format!("/{}", "a".repeat(300))),
            // Another scheme, host, port, userinfo or casing.
            REPOSITORY.replacen("https", "http", 1),
            REPOSITORY.replacen("https", "HTTPS", 1),
            REPOSITORY.replacen("github.com", "GitHub.com", 1),
            REPOSITORY.replacen("github.com", "github.com.evil.com", 1),
            REPOSITORY.replacen("github.com", "github.com:443", 1),
            REPOSITORY.replacen("github.com", "user@github.com", 1),
            REPOSITORY.replacen("joaovitorpina", "JoaoVitorPina", 1),
            REPOSITORY.to_lowercase(),
            // Another repository, or the right one with something glued on.
            under("X"),
            under(".evil"),
            under(".git"),
            under("@evil.example"),
            // Paths a browser would normalise or decode.
            under("/"),
            under("//issues"),
            under("/issues/"),
            under("/../../x"),
            under("/./x"),
            under("/%2e%2e/x"),
            under("/issues%2F1"),
            // Queries, fragments and characters that don't belong.
            under("?tab=readme"),
            under("/issues?q=is%3Aopen"),
            under("#"),
            under("#a#b"),
            under("#a/b"),
            under("\\issues"),
            under("/issues\\x"),
            under("/is sues"),
            under(" "),
            under("/issues\n"),
            under("/issues\t"),
            under("/issues\u{0}"),
            under("/\u{0131}ssues"),
            " ".to_owned() + REPOSITORY,
            REPOSITORY.to_owned() + "\n",
            "javascript:alert(1)".to_owned(),
            "file:///C:/Windows/System32/".to_owned(),
            "file:///etc/passwd".to_owned(),
        ] {
            assert!(!is_allowed_url(&url), "{url:?}");
        }
    }
}
