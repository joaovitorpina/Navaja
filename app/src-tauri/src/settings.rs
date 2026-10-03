//! User settings: a typed TOML file in the app directory, written atomically.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

const FILE: &str = "settings.toml";
const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Settings {
    pub version: u32,
    pub theme: Theme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            theme: Theme::System,
        }
    }
}

pub struct SettingsStore {
    path: Option<PathBuf>,
    current: Mutex<Settings>,
    /// Held across a whole save, so saves never interleave. Separate from
    /// `current`, so `get` (on the main thread) never waits for a disk write.
    saving: Mutex<()>,
}

impl SettingsStore {
    /// Loads settings; a missing file means defaults, and an unreadable one
    /// is set aside as `settings.toml.corrupt` (its content is never logged).
    pub fn load(app_dir: Option<&Path>) -> Self {
        let path = app_dir.map(|dir| dir.join(FILE));
        let current = path.as_deref().map(read).unwrap_or_default();
        Self {
            path,
            current: Mutex::new(current),
            saving: Mutex::new(()),
        }
    }

    pub fn get(&self) -> Settings {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Saves and then calls `apply` with what was saved, before any other
    /// save starts, so the file, memory and what `apply` does always agree.
    /// Must not be called on the main thread when `apply` waits on it.
    pub fn set(&self, settings: Settings, apply: impl FnOnce(&Settings)) -> Result<(), String> {
        if settings.version != CURRENT_VERSION {
            return Err(format!(
                "settings version {} is not supported",
                settings.version
            ));
        }
        let _saving = self.saving.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(path) = &self.path {
            write_atomic(path, &settings).map_err(|error| error.to_string())?;
        }
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = settings.clone();
        apply(&settings);
        Ok(())
    }
}

fn read(path: &Path) -> Settings {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Settings::default();
    };
    match toml::from_str::<Settings>(&text) {
        Ok(settings) if settings.version == CURRENT_VERSION => settings,
        _ => {
            tracing::warn!("settings file unreadable; set aside and reset to defaults");
            let _ = std::fs::rename(path, path.with_extension("toml.corrupt"));
            Settings::default()
        }
    }
}

fn write_atomic(path: &Path, settings: &Settings) -> std::io::Result<()> {
    let text = toml::to_string(settings).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        crate::paths::ensure_private_dir(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("navaja-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trip() {
        let dir = scratch("round-trip");
        let store = SettingsStore::load(Some(&dir));
        assert_eq!(store.get(), Settings::default());
        let dark = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        store.set(dark.clone(), |_| {}).unwrap();
        assert_eq!(SettingsStore::load(Some(&dir)).get(), dark);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_set_aside() {
        let dir = scratch("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "theme = [not toml").unwrap();
        let store = SettingsStore::load(Some(&dir));
        assert_eq!(store.get(), Settings::default());
        assert!(dir.join("settings.toml.corrupt").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unknown_versions() {
        let store = SettingsStore::load(None);
        let future = Settings {
            version: 99,
            ..Settings::default()
        };
        assert!(store.set(future, |_| {}).is_err());
    }

    #[test]
    fn concurrent_saves_leave_file_memory_and_apply_in_agreement() {
        let dir = scratch("concurrent");
        let store = std::sync::Arc::new(SettingsStore::load(Some(&dir)));
        let applied = std::sync::Arc::new(Mutex::new(Vec::new()));
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let (store, applied) = (store.clone(), applied.clone());
                std::thread::spawn(move || {
                    let theme = if i % 2 == 0 {
                        Theme::Dark
                    } else {
                        Theme::Light
                    };
                    let settings = Settings {
                        theme,
                        ..Settings::default()
                    };
                    store
                        .set(settings, |saved| applied.lock().unwrap().push(saved.theme))
                        .unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let last_applied = *applied.lock().unwrap().last().unwrap();
        assert_eq!(store.get().theme, last_applied);
        assert_eq!(SettingsStore::load(Some(&dir)).get().theme, last_applied);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
