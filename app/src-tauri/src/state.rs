//! Shared app state: the tool registry, in-flight runs, settings, the
//! clipboard and the main window's state.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use navaja_core::{Registry, Services};

use crate::clipboard::Clipboard;
use crate::settings::SettingsStore;
use crate::window::WindowState;

/// Cancels kept for runs that haven't started yet; the oldest is dropped
/// first.
const MAX_EARLY_CANCELS: usize = 32;

pub struct AppState {
    pub registry: Registry,
    pub services: Services,
    pub settings: SettingsStore,
    pub clipboard: Clipboard,
    pub window: WindowState,
    runs: Mutex<Runs>,
}

#[derive(Default)]
struct Runs {
    active: HashMap<String, Arc<AtomicBool>>,
    /// `run_tool` and `cancel_run` are separate IPC calls, so a cancel can
    /// arrive before its run is registered. Such ids wait here, and the run
    /// starts already cancelled.
    early_cancels: VecDeque<String>,
}

/// A registered run. Dropping it frees the run id, whichever way the run
/// ended.
pub struct RunGuard<'a> {
    state: &'a AppState,
    run_id: &'a str,
    cancel: Arc<AtomicBool>,
}

impl RunGuard<'_> {
    /// The flag `cancel_run` sets; tools see it through `ctx.check()`.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.state.lock_runs().active.remove(self.run_id);
    }
}

impl AppState {
    pub fn new(registry: Registry, settings: SettingsStore) -> Self {
        Self {
            registry,
            services: Services::new(),
            settings,
            clipboard: Clipboard::default(),
            window: WindowState::default(),
            runs: Mutex::new(Runs::default()),
        }
    }

    fn lock_runs(&self) -> MutexGuard<'_, Runs> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Registers a run until the guard drops; `None` if the id is taken.
    pub fn begin_run<'a>(&'a self, run_id: &'a str) -> Option<RunGuard<'a>> {
        let mut runs = self.lock_runs();
        if runs.active.contains_key(run_id) {
            return None;
        }
        let early = runs.early_cancels.iter().position(|id| id == run_id);
        let cancelled = early
            .and_then(|index| runs.early_cancels.remove(index))
            .is_some();
        let cancel = Arc::new(AtomicBool::new(cancelled));
        runs.active.insert(run_id.to_owned(), Arc::clone(&cancel));
        Some(RunGuard {
            state: self,
            run_id,
            cancel,
        })
    }

    /// Asks a run to stop; tools notice at their next `ctx.check()`. Returns
    /// false if the run isn't registered (yet): its id is then kept, so a run
    /// that starts later with it starts cancelled.
    pub fn cancel_run(&self, run_id: &str) -> bool {
        let mut runs = self.lock_runs();
        if let Some(flag) = runs.active.get(run_id) {
            flag.store(true, Ordering::Relaxed);
            return true;
        }
        if !runs.early_cancels.iter().any(|id| id == run_id) {
            if runs.early_cancels.len() == MAX_EARLY_CANCELS {
                runs.early_cancels.pop_front();
            }
            runs.early_cancels.push_back(run_id.to_owned());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        let registry = Registry::new(Vec::new()).unwrap();
        AppState::new(registry, SettingsStore::load(None))
    }

    fn is_cancelled(run: &RunGuard<'_>) -> bool {
        run.cancel_flag().load(Ordering::Relaxed)
    }

    #[test]
    fn duplicate_ids_are_rejected_until_the_run_ends() {
        let state = state();
        let run = state.begin_run("a").unwrap();
        assert!(state.begin_run("a").is_none());
        assert!(state.begin_run("b").is_some());
        drop(run);
        assert!(state.begin_run("a").is_some());
    }

    #[test]
    fn cancel_reaches_the_runs_flag() {
        let state = state();
        let run = state.begin_run("a").unwrap();
        let other = state.begin_run("b").unwrap();
        assert!(!is_cancelled(&run));
        assert!(state.cancel_run("a"));
        assert!(is_cancelled(&run));
        assert!(!is_cancelled(&other));
    }

    #[test]
    fn ended_runs_are_forgotten() {
        let state = state();
        drop(state.begin_run("a").unwrap());
        assert!(!state.cancel_run("a"));
        assert!(state.begin_run("a").is_some());
    }

    #[test]
    fn a_cancel_that_overtakes_its_run_still_applies() {
        let state = state();
        assert!(!state.cancel_run("early"));
        let run = state.begin_run("early").unwrap();
        assert!(is_cancelled(&run));
        // Consumed: the next run with that id starts normally.
        drop(run);
        assert!(!is_cancelled(&state.begin_run("early").unwrap()));
    }

    #[test]
    fn early_cancels_are_bounded_oldest_first() {
        let state = state();
        for i in 0..=MAX_EARLY_CANCELS {
            assert!(!state.cancel_run(&format!("run-{i}")));
        }
        assert_eq!(state.lock_runs().early_cancels.len(), MAX_EARLY_CANCELS);
        assert!(!is_cancelled(&state.begin_run("run-0").unwrap()));
        let newest = format!("run-{MAX_EARLY_CANCELS}");
        assert!(is_cancelled(&state.begin_run(&newest).unwrap()));
    }
}
