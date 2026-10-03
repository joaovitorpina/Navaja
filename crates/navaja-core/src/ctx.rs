//! The per-call context a tool receives: cancellation, progress and
//! capability-gated host services.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::error::ToolError;
use crate::id::Capability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}

/// Host services (for example the container engine), each tied to the
/// capability a tool must declare to reach it.
///
/// A service type sits behind exactly one capability. To gate different
/// operations separately, register separate types (e.g. a `ProcessInspector`
/// and a `ProcessKiller`), never one object under one capability.
#[derive(Default)]
pub struct Services {
    entries: HashMap<TypeId, (Capability, Box<dyn Any + Send + Sync>)>,
}

/// A service type was registered twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateService(pub &'static str);

impl std::fmt::Display for DuplicateService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "service {} is already registered", self.0)
    }
}

impl std::error::Error for DuplicateService {}

impl Services {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Any + Send + Sync>(
        &mut self,
        capability: Capability,
        service: T,
    ) -> Result<(), DuplicateService> {
        let key = TypeId::of::<T>();
        if self.entries.contains_key(&key) {
            return Err(DuplicateService(std::any::type_name::<T>()));
        }
        self.entries.insert(key, (capability, Box::new(service)));
        Ok(())
    }

    fn get<T: Any + Send + Sync>(&self) -> Option<(&Capability, &T)> {
        let (capability, service) = self.entries.get(&TypeId::of::<T>())?;
        Some((capability, service.downcast_ref::<T>()?))
    }
}

impl std::fmt::Debug for Services {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Services")
            .field("count", &self.entries.len())
            .finish()
    }
}

/// Minimum time between two progress reports.
const PROGRESS_INTERVAL_MS: u64 = 100;
const NEVER: u64 = u64::MAX;

pub struct Ctx<'a> {
    cancel: &'a AtomicBool,
    progress: &'a (dyn Fn(Progress) + Sync),
    services: &'a Services,
    granted: &'a [Capability],
    started: Instant,
    last_progress_ms: AtomicU64,
}

impl<'a> Ctx<'a> {
    pub fn new(
        cancel: &'a AtomicBool,
        progress: &'a (dyn Fn(Progress) + Sync),
        services: &'a Services,
        granted: &'a [Capability],
    ) -> Self {
        Self {
            cancel,
            progress,
            services,
            granted,
            started: Instant::now(),
            last_progress_ms: AtomicU64::new(NEVER),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Returns `core.cancelled` once the user cancels. Call it before any
    /// side effect and periodically in long loops.
    pub fn check(&self) -> Result<(), ToolError> {
        if self.is_cancelled() {
            Err(ToolError::cancelled())
        } else {
            Ok(())
        }
    }

    /// Reports progress, throttled to about ten reports a second. The final
    /// report (`done == total`) is always delivered.
    pub fn progress(&self, done: u64, total: Option<u64>) {
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(NEVER - 1);
        let last = self.last_progress_ms.load(Ordering::Relaxed);
        let finished = total == Some(done);
        if finished || last == NEVER || now.saturating_sub(last) >= PROGRESS_INTERVAL_MS {
            self.last_progress_ms.store(now, Ordering::Relaxed);
            (self.progress)(Progress { done, total });
        }
    }

    /// A host service, if the tool declared the capability it requires.
    pub fn service<T: Any + Send + Sync>(&self) -> Option<&T> {
        let (capability, service) = self.services.get::<T>()?;
        self.granted.contains(capability).then_some(service)
    }
}

impl std::fmt::Debug for Ctx<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ctx")
            .field("cancelled", &self.is_cancelled())
            .field("granted", &self.granted)
            .finish_non_exhaustive()
    }
}

/// What the host provides for one `Registry::run`: the cancel flag the UI can
/// set, the progress sink and the host services.
pub struct RunEnv<'a> {
    pub cancel: &'a AtomicBool,
    pub progress: &'a (dyn Fn(Progress) + Sync),
    pub services: &'a Services,
}

impl std::fmt::Debug for RunEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunEnv")
            .field("cancelled", &self.cancel.load(Ordering::Relaxed))
            .field("services", self.services)
            .finish_non_exhaustive()
    }
}

/// Runs `f` with an environment that is never cancelled, drops progress and
/// offers no services. For tests and simple callers.
pub fn with_detached_env<R>(f: impl FnOnce(&RunEnv<'_>) -> R) -> R {
    let cancel = AtomicBool::new(false);
    let services = Services::new();
    let env = RunEnv {
        cancel: &cancel,
        progress: &|_| {},
        services: &services,
    };
    f(&env)
}

/// Runs `f` with a context that is never cancelled, drops progress and offers
/// no services. For unit tests of a single tool.
pub fn with_detached_ctx<R>(f: impl FnOnce(&Ctx<'_>) -> R) -> R {
    with_detached_env(|env| f(&Ctx::new(env.cancel, env.progress, env.services, &[])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn cancellation() {
        let cancel = AtomicBool::new(false);
        let services = Services::new();
        let ctx = Ctx::new(&cancel, &|_| {}, &services, &[]);
        assert!(ctx.check().is_ok());
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(ctx.check().unwrap_err(), ToolError::cancelled());
    }

    #[test]
    fn progress_is_throttled_but_final_report_arrives() {
        let seen = Mutex::new(Vec::new());
        let report = |p: Progress| seen.lock().unwrap().push(p);
        let cancel = AtomicBool::new(false);
        let services = Services::new();
        let ctx = Ctx::new(&cancel, &report, &services, &[]);
        for done in 0..1000 {
            ctx.progress(done, Some(1000));
        }
        ctx.progress(1000, Some(1000));
        let seen = seen.into_inner().unwrap();
        assert!(seen.len() < 50, "{} reports", seen.len());
        assert_eq!(seen.first().unwrap().done, 0);
        assert_eq!(seen.last().unwrap().done, 1000);
    }

    #[test]
    fn progress_resumes_after_the_interval() {
        let seen = Mutex::new(Vec::new());
        let report = |p: Progress| seen.lock().unwrap().push(p.done);
        let cancel = AtomicBool::new(false);
        let services = Services::new();
        let ctx = Ctx::new(&cancel, &report, &services, &[]);
        ctx.progress(0, Some(10));
        ctx.progress(1, Some(10)); // within the interval: dropped
        std::thread::sleep(std::time::Duration::from_millis(PROGRESS_INTERVAL_MS + 20));
        ctx.progress(2, Some(10));
        assert_eq!(seen.into_inner().unwrap(), [0, 2]);
    }

    #[derive(Debug, PartialEq)]
    struct Engine(u8);

    #[test]
    fn services_need_the_capability() {
        let cancel = AtomicBool::new(false);
        let mut services = Services::new();
        services
            .insert(Capability::CONTAINER_ENGINE, Engine(7))
            .unwrap();
        assert!(
            services
                .insert(Capability::PROCESS_KILL, Engine(8))
                .is_err()
        );

        let without = Ctx::new(&cancel, &|_| {}, &services, &[]);
        assert_eq!(without.service::<Engine>(), None);

        let granted = [Capability::CONTAINER_ENGINE];
        let with = Ctx::new(&cancel, &|_| {}, &services, &granted);
        assert_eq!(with.service::<Engine>(), Some(&Engine(7)));
    }
}
