use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Frontend {
    #[default]
    Egui,
    Gpui,
}

impl Frontend {
    pub(crate) const fn selector_argument(self) -> &'static str {
        match self {
            Self::Egui => "--ui=egui",
            Self::Gpui => "--ui=gpui",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Egui => "egui",
            Self::Gpui => "GPUI",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestartContext {
    executable: PathBuf,
    working_directory: PathBuf,
}

impl RestartContext {
    pub(crate) fn capture() -> std::io::Result<Self> {
        Ok(Self {
            executable: std::env::current_exe()?,
            working_directory: std::env::current_dir()?,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_paths(executable: PathBuf, working_directory: PathBuf) -> Self {
        Self {
            executable,
            working_directory,
        }
    }

    pub(crate) fn into_plan(self, target: Frontend) -> RestartPlan {
        RestartPlan {
            executable: self.executable,
            working_directory: self.working_directory,
            target,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RestartPlan {
    pub(crate) executable: PathBuf,
    pub(crate) working_directory: PathBuf,
    pub(crate) target: Frontend,
}

impl RestartPlan {
    pub(crate) fn selector_args(&self) -> [std::ffi::OsString; 1] {
        [std::ffi::OsString::from(self.target.selector_argument())]
    }

    pub(crate) fn manual_command(&self) -> String {
        format!(
            "Command: \"{}\" {}\nWorking directory: \"{}\"",
            self.executable.display(),
            self.target.selector_argument(),
            self.working_directory.display()
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HashWork {
    Idle,
    Active,
    Retiring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestartBlocker {
    HashWork(HashWork),
    PendingEffects(usize),
    ConfirmationInactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Ready,
    Confirming,
    Fenced,
}

#[derive(Debug)]
struct AdmissionState {
    phase: Phase,
    pending_effects: usize,
}

/// Admission fence shared by GPUI UI actions and their asynchronous native effects.
///
/// Confirmation remains cancelable and effects can still be admitted while it is
/// open. Acceptance checks the pending count and fences admission in the same
/// critical section, so no new effect can race teardown after acceptance.
#[derive(Debug)]
pub(crate) struct RestartAdmission {
    state: Mutex<AdmissionState>,
}

impl Default for RestartAdmission {
    fn default() -> Self {
        Self {
            state: Mutex::new(AdmissionState {
                phase: Phase::Ready,
                pending_effects: 0,
            }),
        }
    }
}

impl RestartAdmission {
    pub(crate) fn begin_confirmation(&self) -> bool {
        let mut state = self.lock();
        if state.phase != Phase::Ready {
            return false;
        }
        state.phase = Phase::Confirming;
        true
    }

    pub(crate) fn cancel_confirmation(&self) {
        let mut state = self.lock();
        if state.phase == Phase::Confirming {
            state.phase = Phase::Ready;
        }
    }

    pub(crate) fn admit(self: &Arc<Self>) -> Option<EffectPermit> {
        let mut state = self.lock();
        if state.phase == Phase::Fenced {
            return None;
        }
        state.pending_effects += 1;
        Some(EffectPermit {
            admission: Arc::clone(self),
        })
    }

    pub(crate) fn try_accept(&self, hash_work: HashWork) -> Result<(), RestartBlocker> {
        let mut state = self.lock();
        if state.phase != Phase::Confirming {
            return Err(RestartBlocker::ConfirmationInactive);
        }
        if hash_work != HashWork::Idle {
            return Err(RestartBlocker::HashWork(hash_work));
        }
        if state.pending_effects != 0 {
            return Err(RestartBlocker::PendingEffects(state.pending_effects));
        }
        state.phase = Phase::Fenced;
        Ok(())
    }

    pub(crate) fn fence(&self) {
        self.lock().phase = Phase::Fenced;
    }

    pub(crate) fn pending_effects(&self) -> usize {
        self.lock().pending_effects
    }

    pub(crate) fn is_fenced(&self) -> bool {
        self.lock().phase == Phase::Fenced
    }

    /// Run a short synchronous claim atomically against the terminal fence.
    pub(crate) fn claim_if_unfenced<T>(&self, claim: impl FnOnce() -> T) -> Option<T> {
        let state = self.lock();
        if state.phase == Phase::Fenced { None } else { Some(claim()) }
    }

    pub(crate) fn is_ready(&self) -> bool {
        self.lock().phase == Phase::Ready
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AdmissionState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(crate) struct EffectPermit {
    admission: Arc<RestartAdmission>,
}

impl Drop for EffectPermit {
    fn drop(&mut self) {
        let mut state = self.admission.lock();
        state.pending_effects = state.pending_effects.saturating_sub(1);
    }
}

/// Release the canonical process guard before invoking the one-shot launcher.
pub(crate) fn after_guard_release<G, T, E>(
    plan: RestartPlan,
    guard: G,
    launch: impl FnOnce(&RestartPlan) -> Result<T, E>,
) -> (RestartPlan, Result<T, E>) {
    drop(guard);
    let result = launch(&plan);
    (plan, result)
}

pub(crate) fn plan_after_teardown(
    context: RestartContext,
    target: Frontend,
    teardown: Result<(), String>,
    windows_retired: bool,
) -> Result<RestartPlan, String> {
    teardown?;
    if !windows_retired {
        return Err("one or more owned GPUI windows remain alive".to_owned());
    }
    Ok(context.into_plan(target))
}

#[cfg(test)]
mod tests {
    use super::{
        Frontend, HashWork, RestartAdmission, RestartBlocker, RestartContext, after_guard_release,
        plan_after_teardown,
    };
    use multi_launcher::platform::{
        app_data::AppDataRoot,
        single_instance::{SingleInstanceAcquire, SingleInstanceGuard},
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    fn acquired(guard: SingleInstanceAcquire) -> SingleInstanceGuard {
        match guard {
            SingleInstanceAcquire::Acquired(guard) => guard,
            SingleInstanceAcquire::AlreadyRunning => panic!("expected an unowned test root"),
        }
    }

    #[test]
    fn cancellation_and_duplicate_acceptance_are_one_shot() {
        let admission = Arc::new(RestartAdmission::default());
        assert!(admission.begin_confirmation());
        assert!(!admission.begin_confirmation());
        admission.cancel_confirmation();
        assert!(admission.begin_confirmation());
        admission.try_accept(HashWork::Idle).unwrap();
        assert!(admission.admit().is_none());
        assert_eq!(
            admission.try_accept(HashWork::Idle),
            Err(RestartBlocker::ConfirmationInactive)
        );
    }

    #[test]
    fn synchronous_claim_is_denied_after_fence() {
        let admission = RestartAdmission::default();
        assert_eq!(admission.claim_if_unfenced(|| 9), Some(9));
        admission.fence();
        assert_eq!(admission.claim_if_unfenced(|| panic!("fenced claim ran")), None::<()>);
    }

    #[test]
    fn active_and_retiring_hash_work_block_acceptance() {
        for work in [HashWork::Active, HashWork::Retiring] {
            let admission = RestartAdmission::default();
            assert!(admission.begin_confirmation());
            assert_eq!(
                admission.try_accept(work),
                Err(RestartBlocker::HashWork(work))
            );
            admission.cancel_confirmation();
        }
    }

    #[test]
    fn pending_effect_blocks_until_it_retires() {
        let admission = Arc::new(RestartAdmission::default());
        assert!(admission.begin_confirmation());
        let effect = admission.admit().unwrap();
        assert_eq!(
            admission.try_accept(HashWork::Idle),
            Err(RestartBlocker::PendingEffects(1))
        );
        drop(effect);
        admission.try_accept(HashWork::Idle).unwrap();
    }

    #[test]
    fn restart_context_keeps_exact_executable_and_working_directory() {
        let context = RestartContext::from_paths(
            PathBuf::from(r"C:\Apps\Multi Launcher\multi_launcher.exe"),
            PathBuf::from(r"D:\Workspace\session"),
        );
        for (target, selector) in [(Frontend::Egui, "--ui=egui"), (Frontend::Gpui, "--ui=gpui")] {
            let plan = context.clone().into_plan(target);
            assert_eq!(
                plan.executable,
                PathBuf::from(r"C:\Apps\Multi Launcher\multi_launcher.exe")
            );
            assert_eq!(
                plan.working_directory,
                PathBuf::from(r"D:\Workspace\session")
            );
            assert_eq!(plan.target, target,);
            assert_eq!(target.selector_argument(), selector);
            assert_eq!(plan.selector_args(), [std::ffi::OsString::from(selector)]);
            assert_eq!(
                plan.manual_command(),
                format!(
                    "Command: \"C:\\Apps\\Multi Launcher\\multi_launcher.exe\" {}\nWorking directory: \"D:\\Workspace\\session\"",
                    selector
                )
            );
        }
    }

    #[test]
    fn teardown_failure_does_not_produce_a_restart_plan() {
        let context = RestartContext::from_paths(PathBuf::from("app.exe"), PathBuf::from("."));
        assert!(
            plan_after_teardown(
                context,
                Frontend::Egui,
                Err("native cleanup failed".to_owned()),
                true
            )
            .is_err()
        );
    }

    #[test]
    fn live_owned_window_does_not_produce_a_restart_plan() {
        let context = RestartContext::from_paths(PathBuf::from("app.exe"), PathBuf::from("."));
        assert!(plan_after_teardown(context, Frontend::Egui, Ok(()), false).is_err());
    }

    #[test]
    fn spawn_failure_propagates_after_guard_release() {
        let directory = tempfile::tempdir().unwrap();
        let data_root =
            AppDataRoot::from_settings_path(directory.path().join("settings.json")).unwrap();
        let guard = acquired(SingleInstanceGuard::acquire(&data_root).unwrap());
        let plan = RestartContext::from_paths(PathBuf::from("app.exe"), PathBuf::from("."))
            .into_plan(Frontend::Egui);
        let (_, result): (_, Result<(), &str>) =
            after_guard_release(plan, guard, |_| Err("spawn failed"));
        assert_eq!(result, Err("spawn failed"));
        assert!(matches!(
            SingleInstanceGuard::acquire(&data_root).unwrap(),
            SingleInstanceAcquire::Acquired(_)
        ));
    }

    #[test]
    fn injected_spawn_reacquires_the_same_data_root_after_guard_drop() {
        let directory = tempfile::tempdir().unwrap();
        let data_root =
            AppDataRoot::from_settings_path(directory.path().join("settings.json")).unwrap();
        let context = RestartContext::from_paths(
            PathBuf::from(r"C:\Apps\multi_launcher.exe"),
            directory.path().to_path_buf(),
        );
        for (target, selector) in [(Frontend::Egui, "--ui=egui"), (Frontend::Gpui, "--ui=gpui")] {
            let guard = acquired(SingleInstanceGuard::acquire(&data_root).unwrap());
            let plan = context.clone().into_plan(target);
            let spawn_count = std::cell::Cell::new(0);
            let (released_plan, observed) = after_guard_release(plan, guard, |plan| {
                spawn_count.set(spawn_count.get() + 1);
                let next_owner = SingleInstanceGuard::acquire(&data_root).unwrap();
                assert!(matches!(next_owner, SingleInstanceAcquire::Acquired(_)));
                Ok::<_, String>((
                    plan.executable.clone(),
                    plan.working_directory.clone(),
                    plan.target,
                    plan.selector_args(),
                ))
            });
            let observed = observed.unwrap();
            assert_eq!(spawn_count.get(), 1);
            assert_eq!(
                released_plan.executable,
                PathBuf::from(r"C:\Apps\multi_launcher.exe")
            );
            assert_eq!(released_plan.working_directory, directory.path());
            assert_eq!(released_plan.target, target);
            assert_eq!(target.selector_argument(), selector);
            assert_eq!(
                released_plan.selector_args(),
                [std::ffi::OsString::from(selector)]
            );
            assert_eq!(observed.0, PathBuf::from(r"C:\Apps\multi_launcher.exe"));
            assert_eq!(observed.1, directory.path());
            assert_eq!(observed.2, target);
            assert_eq!(observed.3, [std::ffi::OsString::from(selector)]);
        }
    }

}
