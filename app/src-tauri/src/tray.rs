//! The tray icon. Its menu lists tools whose metadata sets `tray: true`, so
//! a tool appears here without changing this file.
//!
//! - Windows: left click toggles the window; right click opens the menu.
//! - macOS: template icon, tinted by the system; click opens the menu.
//! - Linux (StatusNotifierItem): menu only, since click events don't arrive.

use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::window;

const OPEN: &str = "open";
const SEARCH: &str = "search";
const QUIT: &str = "quit";
const TOOL: &str = "tool:";

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../icons/tray/template.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../icons/tray/color-32.png");

/// `tools` are `(id, name)` pairs of registered tools with `tray: true`.
pub fn create<R: Runtime>(app: &AppHandle<R>, tools: &[(String, String)]) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open Navaja", true, None::<&str>)?;
    let search = MenuItem::with_id(app, SEARCH, "Search tools…", true, None::<&str>)?;
    let tool_items = tools
        .iter()
        .map(|(id, name)| MenuItem::with_id(app, format!("{TOOL}{id}"), name, true, None::<&str>))
        .collect::<tauri::Result<Vec<_>>>()?;
    let separator = PredefinedMenuItem::separator(app)?;
    let tools_separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Navaja", true, None::<&str>)?;

    let mut items: Vec<&dyn IsMenuItem<R>> = vec![&open, &search];
    if !tool_items.is_empty() {
        items.push(&tools_separator);
        items.extend(tool_items.iter().map(|item| item as &dyn IsMenuItem<R>));
    }
    items.push(&separator);
    items.push(&quit);
    let menu = Menu::with_items(app, &items)?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(ICON)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Navaja")
        .menu(&menu)
        .show_menu_on_left_click(!cfg!(windows))
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_icon_event)
        .build(app)?;
    Ok(())
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    if id == QUIT {
        app.exit(0);
        return;
    }
    let Some(main) = window::main_window(app) else {
        return;
    };
    let result = match id {
        OPEN => window::reveal(&main),
        SEARCH => window::reveal(&main).and_then(|()| window::open_palette(&main)),
        _ => match id.strip_prefix(TOOL) {
            Some(tool) => window::reveal(&main).and_then(|()| window::open_tool(&main, tool)),
            None => Ok(()),
        },
    };
    if let Err(error) = result {
        tracing::warn!(%error, "tray action failed");
    }
}

fn on_tray_icon_event<R: Runtime>(tray: &TrayIcon<R>, event: TrayIconEvent) {
    if !cfg!(windows) {
        return;
    }
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
        && let Some(main) = window::main_window(tray.app_handle())
        && let Err(error) = window::toggle(&main)
    {
        tracing::warn!(%error, "tray toggle failed");
    }
}
