fn main() {
    // Declaring the app's commands makes tauri-build generate one permission per
    // command ("allow-<command>"), which capabilities/main.json grants one by one.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "app_info",
        "shell_ready",
        "list_tools",
        "search",
        "run_tool",
        "cancel_run",
        "copy_text",
        "settings_get",
        "settings_set",
    ]);
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        panic!("tauri-build failed: {error:#}");
    }
}
