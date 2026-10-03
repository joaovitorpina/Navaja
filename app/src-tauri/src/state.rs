//! Shared app state: the tool registry, in-flight runs, settings and the
//! clipboard.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use navaja_core::{Registry, Services};

use crate::clipboard::Clipboard;
use crate::settings::SettingsStore;

pub struct AppState {
    pub registry: Registry,
    pub services: Services,
    pub settings: SettingsStore,
    pub clipboard: Clipboard,
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new(registry: Registry, settings: SettingsStore) -> Self {
        Self {
            registry,
            services: Services::new(),
            settings,
            clipboard: Clipboard::default(),
            runs: Mutex::new(HashMap::new()),
        }
    }

    /// Registers a run and returns its cancel flag; `None` if the id is taken.
    pub fn begin_run(&self, run_id: &str) -> Option<Arc<AtomicBool>> {
        let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        if runs.contains_key(run_id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        runs.insert(run_id.to_owned(), Arc::clone(&flag));
        Some(flag)
    }

    pub fn end_run(&self, run_id: &str) {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(run_id);
    }

    /// Asks a run to stop; tools notice at their next `ctx.check()`.
    pub fn cancel_run(&self, run_id: &str) -> bool {
        let runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        runs.get(run_id)
            .map(|flag| flag.store(true, Ordering::Relaxed))
            .is_some()
    }
}
