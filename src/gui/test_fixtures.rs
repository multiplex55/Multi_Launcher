//! Scoped, explicit-root app fixtures for small GUI behavior tests.

use crate::performance::workloads::IsolatedWorkspace;
use crate::settings::Settings;
use eframe::egui;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::LauncherApp;

pub(crate) struct IsolatedAppWorkspace {
    workspace: Option<IsolatedWorkspace>,
    root: PathBuf,
    app: Option<LauncherApp>,
    workspace_setup: Duration,
    app_construction: Vec<(String, Duration)>,
    app_drop: Duration,
}

impl IsolatedAppWorkspace {
    pub(crate) fn new() -> Self {
        let setup_started = Instant::now();
        let workspace = IsolatedWorkspace::new();
        let root = workspace.root().to_path_buf();
        Self {
            workspace: Some(workspace),
            root,
            app: None,
            workspace_setup: setup_started.elapsed(),
            app_construction: Vec::new(),
            app_drop: Duration::ZERO,
        }
    }

    pub(crate) fn profile_root(&self, profile: &str) -> PathBuf {
        self.root.join(profile)
    }

    pub(crate) fn original_cwd(&self) -> &std::path::Path {
        self.workspace
            .as_ref()
            .expect("isolated workspace is alive")
            .original_cwd()
    }

    pub(crate) fn original_notes_directory(&self) -> Option<std::ffi::OsString> {
        self.workspace
            .as_ref()
            .expect("isolated workspace is alive")
            .original_notes_directory()
    }

    pub(crate) fn original_template_directory(&self) -> Option<std::ffi::OsString> {
        self.workspace
            .as_ref()
            .expect("isolated workspace is alive")
            .original_template_directory()
    }

    pub(crate) fn create_app(
        &mut self,
        ctx: &egui::Context,
        profile: &str,
        settings: Settings,
    ) -> Result<&mut LauncherApp, String> {
        if self.app.is_some() {
            return Err("drop the current isolated app before creating another profile".into());
        }
        let profile_root = self.profile_root(profile);
        fs::create_dir_all(&profile_root).map_err(|error| {
            format!(
                "create isolated app profile {}: {error}",
                profile_root.display()
            )
        })?;
        self.workspace
            .as_mut()
            .expect("isolated workspace is alive")
            .scope_to_profile(&profile_root)
            .map_err(|error| {
                format!(
                    "scope isolated app fixture to profile {}: {error}",
                    profile_root.display()
                )
            })?;

        let construction_started = Instant::now();
        self.app = Some(LauncherApp::new_isolated_for_test_at(
            ctx,
            &profile_root,
            settings,
        ));
        self.app_construction
            .push((profile.to_owned(), construction_started.elapsed()));
        Ok(self.app.as_mut().expect("app was just constructed"))
    }

    pub(crate) fn create_app_from_saved_profile(
        &mut self,
        ctx: &egui::Context,
        profile: &str,
    ) -> Result<&mut LauncherApp, String> {
        let settings_path = self.profile_root(profile).join("settings.json");
        let settings = Settings::load(&settings_path.to_string_lossy())
            .map_err(|error| format!("load isolated settings profile: {error}"))?;
        self.create_app(ctx, profile, settings)
    }

    pub(crate) fn drop_app(&mut self) {
        let started = Instant::now();
        drop(self.app.take());
        self.app_drop += started.elapsed();
    }

    pub(crate) fn finish(mut self) -> Result<FixtureTeardown, String> {
        self.drop_app();
        let restore_started = Instant::now();
        let workspace = self.workspace.take().expect("isolated workspace is alive");
        workspace
            .finish()
            .map_err(|error| format!("finish isolated app workspace: {error}"))?;
        let workspace_restore_and_cleanup = restore_started.elapsed();
        let root_removed = !self
            .root
            .try_exists()
            .map_err(|error| format!("verify isolated root removal: {error}"))?;
        if !root_removed {
            return Err(format!(
                "isolated root remains after fixture finish: {}",
                self.root.display()
            ));
        }

        Ok(FixtureTeardown {
            root_removed,
            cwd_restored: true,
            workspace_setup: self.workspace_setup,
            app_construction: std::mem::take(&mut self.app_construction),
            app_drop: self.app_drop,
            workspace_restore_and_cleanup,
        })
    }
}

impl Drop for IsolatedAppWorkspace {
    fn drop(&mut self) {
        // Panic fallback: application-owned services must go before the
        // process-global CWD/environment guard restores and removes its root.
        self.drop_app();
        drop(self.workspace.take());
    }
}

#[derive(Debug)]
pub(crate) struct FixtureTeardown {
    pub(crate) root_removed: bool,
    pub(crate) cwd_restored: bool,
    pub(crate) workspace_setup: Duration,
    pub(crate) app_construction: Vec<(String, Duration)>,
    pub(crate) app_drop: Duration,
    pub(crate) workspace_restore_and_cleanup: Duration,
}
