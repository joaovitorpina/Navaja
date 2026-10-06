#![allow(unsafe_code, reason = "dlopen probe for the tray library")]

/// The names tray-icon (through libappindicator-sys) tries, in its order:
/// the Ayatana fork first, then the original, then the unversioned names an
/// AppImage may carry.
const TRAY_LIBRARIES: &[&str] = &[
    "libayatana-appindicator3.so.1",
    "libappindicator3.so.1",
    "libayatana-appindicator3.so",
    "libappindicator3.so",
];

/// Whether an appindicator library can be loaded. tray-icon panics when
/// none can, instead of returning an error, so the tray is skipped then.
pub fn tray_available() -> bool {
    TRAY_LIBRARIES.iter().any(|name| {
        // SAFETY: loading runs the library's initialisers. These are the
        // libraries tray-icon loads moments later anyway, so the probe runs
        // nothing the tray would not.
        match unsafe { libloading::Library::new(*name) } {
            // Kept loaded for tray-icon: GLib libraries are not always safe
            // to unload.
            Ok(library) => {
                std::mem::forget(library);
                true
            }
            Err(_) => false,
        }
    })
}
