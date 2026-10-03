//! Main window lifecycle. The window is declared in tauri.conf.json with
//! `"create": false` and built here, so every hardening option lives in one
//! place (docs/architecture.md §5).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::state::AppState;

pub const MAIN: &str = "main";

/// The DOM event that opens the command palette; Shell.svelte listens for it.
pub const PALETTE_EVENT: &str = "navaja:palette";

/// How long to wait for the front end's `shell_ready` before showing anyway.
const READY_TIMEOUT: Duration = Duration::from_secs(5);

/// A tray click still counts the window as focused if it lost focus this
/// recently: on Windows, clicking the notification area moves focus to the
/// taskbar before the click arrives. KeePassXC allows the same 500 ms.
const TRAY_BLUR_GRACE: Duration = Duration::from_millis(500);

/// Where the webview's web traffic goes on Linux: a closed, privileged
/// loopback port (the discard service's). WebKitGTK looks a link's host up
/// while it waits for the navigation decision, even one the guard then
/// denies; through this proxy nothing is resolved and nothing leaves the
/// machine. The app's own scheme and IPC never use the network.
#[cfg(target_os = "linux")]
const DEAD_PROXY: &str = "http://127.0.0.1:9";

/// What a second launch or the tray asks the window to open, besides
/// showing it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Request {
    /// A registered tool id.
    pub tool: Option<String>,
    /// The command palette.
    pub palette: bool,
}

/// What the shell tracks about the main window: whether the front end has
/// reported ready, what was asked for before it did, and focus as the
/// window's own events report it.
#[derive(Debug, Default)]
pub struct WindowState {
    startup: Mutex<Startup>,
    focus: Mutex<Focus>,
}

/// `ready` and `pending` share one lock, so a request is either kept for
/// [`ready`] or handled at once, never lost in between.
#[derive(Debug, Default)]
struct Startup {
    ready: bool,
    pending: Request,
}

#[derive(Debug, Default, Clone, Copy)]
struct Focus {
    focused: bool,
    /// When the window last lost focus.
    blurred_at: Option<Instant>,
}

impl Focus {
    fn since_blur(self) -> Option<Duration> {
        self.blurred_at.map(|at| at.elapsed())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl WindowState {
    /// Before the front end is ready, keeps `request` for [`ready`] (the
    /// latest tool wins) and returns true: the window shows then anyway.
    /// Returns false once it is ready, and the caller acts itself.
    fn defer(&self, request: &Request) -> bool {
        let mut startup = lock(&self.startup);
        if startup.ready {
            return false;
        }
        if let Some(tool) = &request.tool {
            startup.pending.tool = Some(tool.clone());
        }
        startup.pending.palette |= request.palette;
        true
    }

    /// Marks the front end ready and takes what was asked for until then;
    /// `None` if it was ready already.
    fn mark_ready(&self) -> Option<Request> {
        let mut startup = lock(&self.startup);
        if startup.ready {
            return None;
        }
        startup.ready = true;
        Some(std::mem::take(&mut startup.pending))
    }

    fn set_focused(&self, focused: bool) {
        let mut focus = lock(&self.focus);
        focus.focused = focused;
        if !focused {
            focus.blurred_at = Some(Instant::now());
        }
    }

    fn focus(&self) -> Focus {
        *lock(&self.focus)
    }
}

/// Builds the main window. `initial_tool` (already checked against the
/// registry) becomes the first route.
pub fn create_main<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    initial_tool: Option<&str>,
) -> tauri::Result<WebviewWindow<R>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == MAIN)
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;

    let mut builder = WebviewWindowBuilder::from_config(app, &config)?
        // No pop-ups: links and window.open never create windows.
        .on_new_window(|_url, _features| NewWindowResponse::Deny);
    if let Some(id) = initial_tool {
        builder = builder.initialization_script(route_script(id));
    }
    // Not in dev, where the front end comes from the Vite server.
    #[cfg(target_os = "linux")]
    if !tauri::is_dev()
        && let Ok(proxy) = tauri::Url::parse(DEAD_PROXY)
    {
        builder = builder.proxy_url(proxy);
    }
    let window = builder.build()?;

    // WebKit skips the guard script in `srcdoc` frames, so on macOS WebRTC is
    // also switched off in the engine (docs/architecture.md §5).
    #[cfg(target_os = "macos")]
    if let Err(error) = window.with_webview(|webview| {
        if !crate::platform::disable_peer_connections(webview.inner()) {
            tracing::warn!("this WebKit has no switch to turn WebRTC off");
        }
    }) {
        tracing::warn!(%error, "could not reach the webview to turn WebRTC off");
    }

    // The toggles read focus from the window's own events, not is_focused():
    // a tray click on Windows moves focus to the taskbar before it arrives
    // (see TRAY_BLUR_GRACE).
    let tracked = Arc::clone(state);
    window.on_window_event(move |event| {
        if let WindowEvent::Focused(focused) = event {
            tracked.window.set_focused(*focused);
        }
    });

    // Fallback: if the front end never reports ready, show the window anyway
    // rather than leaving an invisible process behind.
    let handle = window.clone();
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        std::thread::sleep(READY_TIMEOUT);
        if let Some(request) = state.window.mark_ready() {
            tracing::warn!("front end did not report ready; showing the window anyway");
            let _ = reveal(&handle).and_then(|()| navigate(&handle, &request));
        }
    });
    Ok(window)
}

/// Sets the first route before the page's own scripts run. `id` is a
/// registered tool id (`[a-z][a-z0-9_]*`), so it is safe inside the string.
fn route_script(id: &str) -> String {
    format!(
        "if (!location.hash || location.hash === '#/') location.hash = '{}';",
        tool_route(id)
    )
}

/// Navigates the running page to a tool. Same constraint on `id`.
fn tool_script(id: &str) -> String {
    format!("window.location.hash = '{}';", tool_route(id))
}

/// A tool's route in the front end's router.
fn tool_route(id: &str) -> String {
    format!("#/tool/{id}")
}

fn palette_script() -> String {
    format!("window.dispatchEvent(new Event('{PALETTE_EVENT}'));")
}

pub fn main_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    app.get_webview_window(MAIN)
}

pub fn reveal<R: Runtime>(window: &WebviewWindow<R>) -> tauri::Result<()> {
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}

/// The front end has rendered (`shell_ready`): shows the window and opens
/// what was asked for in the meantime. Only the first time: after the
/// fallback has shown the window, the user may have hidden it again.
pub fn ready<R: Runtime>(window: &WebviewWindow<R>, state: &WindowState) -> tauri::Result<()> {
    let Some(request) = state.mark_ready() else {
        return Ok(());
    };
    reveal(window)?;
    navigate(window, &request)
}

/// Shows the window and opens what `request` asks for. Until the front end
/// is ready, only keeps the request for [`ready`].
pub fn open<R: Runtime>(
    app: &AppHandle<R>,
    state: &WindowState,
    request: &Request,
) -> tauri::Result<()> {
    if state.defer(request) {
        return Ok(());
    }
    let Some(window) = main_window(app) else {
        return Ok(());
    };
    reveal(&window)?;
    navigate(&window, request)
}

/// `--toggle`: hides the window if it is shown and focused, otherwise shows
/// it; then opens what `request` asks for. Until the front end is ready,
/// only keeps the request for [`ready`].
pub fn toggle<R: Runtime>(
    app: &AppHandle<R>,
    state: &WindowState,
    request: &Request,
) -> tauri::Result<()> {
    if state.defer(request) {
        return Ok(());
    }
    let Some(window) = main_window(app) else {
        return Ok(());
    };
    toggle_with(&window, state.focus().focused, None)?;
    navigate(&window, request)
}

/// The tray icon's left click (Windows): like `--toggle`, but a window that
/// lost focus just now still counts as focused (see [`TRAY_BLUR_GRACE`]).
pub fn toggle_from_tray<R: Runtime>(app: &AppHandle<R>, state: &WindowState) -> tauri::Result<()> {
    if state.defer(&Request::default()) {
        return Ok(());
    }
    let Some(window) = main_window(app) else {
        return Ok(());
    };
    let focus = state.focus();
    toggle_with(&window, focus.focused, focus.since_blur())
}

fn toggle_with<R: Runtime>(
    window: &WebviewWindow<R>,
    focused: bool,
    since_blur: Option<Duration>,
) -> tauri::Result<()> {
    let visible = window.is_visible().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    if should_hide(visible, minimized, focused, since_blur) {
        window.hide()
    } else {
        reveal(window)
    }
}

/// Whether a toggle hides the window rather than showing it. `since_blur`
/// is how long ago the window lost focus, given only for a tray click. A
/// shown but unfocused window comes to the front instead of hiding.
fn should_hide(
    visible: bool,
    minimized: bool,
    focused: bool,
    since_blur: Option<Duration>,
) -> bool {
    visible && !minimized && (focused || since_blur.is_some_and(|age| age < TRAY_BLUR_GRACE))
}

/// Opens `request`'s tool, then the palette, in the running page.
fn navigate<R: Runtime>(window: &WebviewWindow<R>, request: &Request) -> tauri::Result<()> {
    if let Some(id) = &request.tool {
        window.eval(tool_script(id))?;
    }
    if request.palette {
        window.eval(palette_script())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG_AGO: Duration = Duration::from_secs(60);
    const JUST_NOW: Duration = Duration::from_millis(50);

    fn tool(id: &str) -> Request {
        Request {
            tool: Some(id.to_owned()),
            palette: false,
        }
    }

    #[test]
    fn requests_wait_for_the_front_end() {
        let state = WindowState::default();
        assert!(state.defer(&tool("uuid")));
        assert!(state.defer(&Request {
            tool: None,
            palette: true,
        }));
        assert!(state.defer(&tool("json")));
        assert_eq!(
            state.mark_ready(),
            Some(Request {
                tool: Some("json".to_owned()),
                palette: true,
            })
        );
        // Once ready, callers act themselves and nothing is kept.
        assert!(!state.defer(&tool("uuid")));
        assert_eq!(state.mark_ready(), None);
    }

    #[test]
    fn ready_with_nothing_asked_for() {
        let state = WindowState::default();
        assert!(state.defer(&Request::default()));
        assert_eq!(state.mark_ready(), Some(Request::default()));
    }

    #[test]
    fn focus_follows_window_events() {
        let state = WindowState::default();
        assert!(!state.focus().focused);
        assert_eq!(state.focus().since_blur(), None);
        state.set_focused(true);
        assert!(state.focus().focused);
        state.set_focused(false);
        let focus = state.focus();
        assert!(!focus.focused);
        assert!(focus.since_blur().is_some_and(|age| age < LONG_AGO));
    }

    #[test]
    fn toggle_hides_only_a_shown_focused_window() {
        // --toggle passes no blur age.
        assert!(should_hide(true, false, true, None));
        assert!(!should_hide(true, false, false, None));
        assert!(!should_hide(false, false, true, None));
        assert!(!should_hide(true, true, true, None));
    }

    #[test]
    fn tray_click_counts_a_window_that_just_lost_focus() {
        // Focused, or blurred by the click itself: hide.
        assert!(should_hide(true, false, true, Some(LONG_AGO)));
        assert!(should_hide(true, false, false, Some(JUST_NOW)));
        // Blurred long ago, e.g. behind another window: bring it forward.
        assert!(!should_hide(true, false, false, Some(LONG_AGO)));
        assert!(!should_hide(true, false, false, Some(TRAY_BLUR_GRACE)));
        // Hidden or minimized: show.
        assert!(!should_hide(false, false, false, Some(JUST_NOW)));
        assert!(!should_hide(false, false, true, Some(JUST_NOW)));
        assert!(!should_hide(true, true, false, Some(JUST_NOW)));
        assert!(!should_hide(true, true, true, None));
    }

    #[test]
    fn both_tool_scripts_use_the_router_format() {
        assert!(route_script("uuid").contains("'#/tool/uuid'"));
        assert!(tool_script("uuid").contains("'#/tool/uuid'"));
    }

    #[test]
    fn the_shell_listens_for_the_palette_event() {
        let shell = include_str!("../../src/shell/Shell.svelte");
        assert!(shell.contains(&format!("'{PALETTE_EVENT}'")));
        assert!(palette_script().contains(&format!("'{PALETTE_EVENT}'")));
    }
}
