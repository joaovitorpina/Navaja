//! The tray icon. Its menu lists tools whose metadata sets `tray: true`, so
//! a tool appears here without changing this file.
//!
//! - Windows: left click toggles the window; right click opens the menu.
//! - macOS: template icon, tinted by the system; click opens the menu.
//! - Linux (StatusNotifierItem): menu only, since click events don't arrive.

use std::sync::Arc;

use navaja_core::ToolMeta;
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::state::AppState;
use crate::window::{self, Request};

const OPEN: &str = "open";
const SEARCH: &str = "search";
const QUIT: &str = "quit";
const TOOL: &str = "tool:";

#[cfg(target_os = "macos")]
const ICON: &[u8] = include_bytes!("../icons/tray/template.png");
#[cfg(not(target_os = "macos"))]
const ICON: &[u8] = include_bytes!("../icons/tray/color-32.png");

/// The menu's tool items: `(menu id, label)` for each tool whose metadata
/// sets `tray: true`, in registration order. Pure, so the tests cover it
/// while no shipped tool sets `tray` yet.
pub fn tool_entries<'a>(metas: impl IntoIterator<Item = &'a ToolMeta>) -> Vec<(String, String)> {
    metas
        .into_iter()
        .filter(|meta| meta.tray)
        .map(|meta| (format!("{TOOL}{}", meta.id), meta.name.clone()))
        .collect()
}

/// What a menu item asks for.
#[derive(Debug, PartialEq, Eq)]
enum MenuAction {
    Quit,
    /// Show the window and open what the request names.
    Open(Request),
}

/// Reads a clicked item's menu id. `None` for an id this menu never sets.
fn menu_action(id: &str) -> Option<MenuAction> {
    match id {
        QUIT => Some(MenuAction::Quit),
        OPEN => Some(MenuAction::Open(Request::default())),
        SEARCH => Some(MenuAction::Open(Request {
            tool: None,
            palette: true,
        })),
        _ => id.strip_prefix(TOOL).map(|tool| {
            MenuAction::Open(Request {
                tool: Some(tool.to_owned()),
                palette: false,
            })
        }),
    }
}

/// `tools` are the entries `tool_entries` built from the registry.
pub fn create<R: Runtime>(
    app: &AppHandle<R>,
    state: &Arc<AppState>,
    tools: &[(String, String)],
) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, OPEN, "Open Navaja", true, None::<&str>)?;
    let search = MenuItem::with_id(app, SEARCH, "Search tools…", true, None::<&str>)?;
    let tool_items = tools
        .iter()
        .map(|(id, label)| MenuItem::with_id(app, id.as_str(), label, true, None::<&str>))
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
        .on_menu_event({
            let state = Arc::clone(state);
            move |app, event| on_menu_event(app, &state, &event)
        })
        .on_tray_icon_event({
            let state = Arc::clone(state);
            move |tray, event| on_tray_icon_event(tray, &state, &event)
        })
        .build(app)?;
    Ok(())
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, state: &AppState, event: &MenuEvent) {
    let request = match menu_action(event.id().as_ref()) {
        Some(MenuAction::Quit) => {
            crate::quit(app, crate::QuitFrom::Tray);
            return;
        }
        Some(MenuAction::Open(request)) => request,
        None => return,
    };
    if let Err(error) = window::open(app, &state.window, &request) {
        tracing::warn!(%error, "tray action failed");
    }
}

fn on_tray_icon_event<R: Runtime>(tray: &TrayIcon<R>, state: &AppState, event: &TrayIconEvent) {
    if !cfg!(windows) {
        return;
    }
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
        && let Err(error) = window::toggle_from_tray(tray.app_handle(), &state.window)
    {
        tracing::warn!(%error, "tray toggle failed");
    }
}

#[cfg(test)]
mod tests {
    use navaja_core::{
        ActionMeta, Category, Ctx, GeneratorSpec, OutputKind, OutputSpec, Registry, Tool,
        ToolError, ToolId, UiSpec, Value,
    };

    use super::*;

    /// A tool that asks for a tray entry, which no shipped tool does yet.
    struct Pinned;

    impl Tool for Pinned {
        fn meta(&self) -> ToolMeta {
            ToolMeta {
                spec_version: navaja_core::SPEC_VERSION,
                id: ToolId::from_static("pinned"),
                name: "Pinned tool".into(),
                description: "Listed in the tray.".into(),
                category: Category::GENERATORS,
                keywords: vec![],
                icon: r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M4 12h16"/></svg>"#.into(),
                capabilities: vec![],
                actions: vec![ActionMeta::new("make", "Make")],
                tray: true,
                ui: UiSpec::Generator(GeneratorSpec {
                    action: "make".into(),
                    options: vec![],
                    outputs: vec![OutputSpec {
                        key: "text".into(),
                        label: "Text".into(),
                        format: OutputKind::Text,
                    }],
                    run_on_open: false,
                }),
            }
        }

        fn invoke(&self, _: &str, _: Value, _: &Ctx<'_>) -> Result<Value, ToolError> {
            Ok(serde_json::json!({ "text": "" }))
        }
    }

    /// The shipped tools plus `Pinned`, as `run` builds the registry.
    fn registry() -> Registry {
        let mut tools = navaja_tools::all();
        tools.push(Arc::new(Pinned));
        Registry::new(tools).unwrap()
    }

    /// Checked per tool, so a shipped tool that sets `tray` later keeps it
    /// passing.
    #[test]
    fn only_tray_tools_get_an_entry() {
        let registry = registry();
        let entries = tool_entries(registry.metas());
        assert!(
            entries.contains(&("tool:pinned".to_owned(), "Pinned tool".to_owned())),
            "{entries:?}"
        );
        for meta in registry.metas() {
            let id = format!("tool:{}", meta.id);
            let listed = entries.iter().filter(|(entry, _)| *entry == id).count();
            assert_eq!(listed, usize::from(meta.tray), "{}", meta.id);
        }
        assert_eq!(
            entries.len(),
            registry.metas().filter(|meta| meta.tray).count()
        );
    }

    #[test]
    fn an_entry_opens_its_tool() {
        let registry = registry();
        let entries = tool_entries(registry.metas());
        assert!(!entries.is_empty());
        for (id, label) in &entries {
            let Some(MenuAction::Open(Request {
                tool: Some(tool),
                palette: false,
            })) = menu_action(id)
            else {
                panic!("{id} does not open a tool");
            };
            let meta = registry.meta(&tool).expect("a registered tool");
            assert!(meta.tray, "{tool}");
            assert_eq!(&meta.name, label);
        }
    }

    #[test]
    fn fixed_items_and_unknown_ids() {
        assert_eq!(menu_action(QUIT), Some(MenuAction::Quit));
        assert_eq!(
            menu_action(OPEN),
            Some(MenuAction::Open(Request::default()))
        );
        assert_eq!(
            menu_action(SEARCH),
            Some(MenuAction::Open(Request {
                tool: None,
                palette: true,
            }))
        );
        // Ids the menu never sets, a bare tool id among them.
        for id in ["pinned", "", "tools:pinned"] {
            assert_eq!(menu_action(id), None, "{id:?}");
        }
    }
}
