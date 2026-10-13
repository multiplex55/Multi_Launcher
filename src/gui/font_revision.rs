use eframe::egui::{self, FontDefinitions, Plugin, RawInput, Ui};

const FONT_REVISION_ID: &str = "multi_launcher_font_revision";

#[derive(Clone, Copy, Default)]
struct FontRevision {
    requested: u64,
    active: u64,
}

#[derive(Clone)]
struct FontInstallation {
    revision: u64,
    definitions: FontDefinitions,
}

#[derive(Default)]
struct FontRevisionPlugin {
    next_revision: u64,
    active_fonts_initialized: bool,
    pending: Option<FontInstallation>,
    submitted: Option<FontInstallation>,
}

impl FontRevisionPlugin {
    fn install(&mut self, ctx: &egui::Context, definitions: FontDefinitions) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.definitions == definitions)
        {
            return;
        }

        // The newest request replaces any earlier staged request, even when
        // it matches a request already submitted to egui. In particular this
        // cancels a later pending replacement before egui activates it.
        if let Some(submitted) = self
            .submitted
            .as_ref()
            .filter(|submitted| submitted.definitions == definitions)
        {
            let revision = submitted.revision;
            self.pending = None;
            ctx.data_mut(|data| {
                let active = data
                    .get_temp::<FontRevision>(egui::Id::new(FONT_REVISION_ID))
                    .unwrap_or_default()
                    .active;
                data.insert_temp(
                    egui::Id::new(FONT_REVISION_ID),
                    FontRevision {
                        requested: revision,
                        active,
                    },
                );
            });
            return;
        }

        // egui only compares a new request with the currently active fonts.
        // Keeping this check here lets us cancel a staged replacement before
        // the next pass without ever queueing a definition that is no longer
        // wanted. Definitions are compared only when a caller installs fonts.
        let active_definitions = if self.active_fonts_initialized {
            ctx.fonts(|fonts| fonts.definitions().clone())
        } else {
            FontDefinitions::default()
        };
        if active_definitions == definitions && self.submitted.is_none() {
            self.pending = None;
            let active = self.active_revision(ctx);
            ctx.data_mut(|data| {
                data.insert_temp(
                    egui::Id::new(FONT_REVISION_ID),
                    FontRevision {
                        requested: active,
                        active,
                    },
                );
            });
            return;
        }

        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        let revision = self.next_revision;
        self.pending = Some(FontInstallation {
            revision,
            definitions,
        });
        ctx.data_mut(|data| {
            let active = data
                .get_temp::<FontRevision>(egui::Id::new(FONT_REVISION_ID))
                .unwrap_or_default()
                .active;
            data.insert_temp(
                egui::Id::new(FONT_REVISION_ID),
                FontRevision {
                    requested: revision,
                    active,
                },
            );
        });
        ctx.request_repaint();
    }

    fn active_revision(&self, ctx: &egui::Context) -> u64 {
        ctx.data(|data| {
            data.get_temp::<FontRevision>(egui::Id::new(FONT_REVISION_ID))
                .unwrap_or_default()
                .active
        })
    }

    fn on_begin_pass(&mut self, ctx: &egui::Context) {
        self.active_fonts_initialized = true;
        if let Some(submitted) = self.submitted.take() {
            ctx.data_mut(|data| {
                let requested = data
                    .get_temp::<FontRevision>(egui::Id::new(FONT_REVISION_ID))
                    .unwrap_or_default()
                    .requested;
                data.insert_temp(
                    egui::Id::new(FONT_REVISION_ID),
                    FontRevision {
                        requested,
                        active: submitted.revision,
                    },
                );
            });
        }
    }
}

impl Plugin for FontRevisionPlugin {
    fn debug_name(&self) -> &'static str {
        "Multi Launcher font revision"
    }

    fn input_hook(&mut self, ctx: &egui::Context, _input: &mut RawInput) {
        if self.submitted.is_some() {
            return;
        }
        let Some(installation) = self.pending.take() else {
            return;
        };

        // egui applies the definitions later in this same begin_pass cycle.
        // Keep the submitted revision separate until on_begin_pass confirms
        // that activation boundary has been crossed.
        ctx.set_fonts(installation.definitions.clone());
        self.submitted = Some(installation);
    }

    fn on_begin_pass(&mut self, ui: &mut Ui) {
        self.on_begin_pass(ui.ctx());
    }
}

/// Ensure the pass hooks exist before eframe captures the plugin list for a UI run.
pub(crate) fn register(ctx: &egui::Context) {
    if ctx.with_plugin::<FontRevisionPlugin, _>(|_| ()).is_none() {
        ctx.add_plugin(FontRevisionPlugin::default());
    }
}

/// Request a font-definition change. The final request staged before an egui
/// pass is applied by the plugin input hook, and becomes active at that pass.
pub(crate) fn install(ctx: &egui::Context, definitions: FontDefinitions) {
    register(ctx);
    ctx.with_plugin::<FontRevisionPlugin, _>(|plugin| plugin.install(ctx, definitions));
}

/// Return the font revision active at the current pass boundary.
pub(crate) fn active_revision(ctx: &egui::Context) -> u64 {
    ctx.data(|data| {
        data.get_temp::<FontRevision>(egui::Id::new(FONT_REVISION_ID))
            .unwrap_or_default()
            .active
    })
}

#[cfg(test)]
mod tests {
    use super::{active_revision, install, register};
    use eframe::egui::{self, FontDefinitions, FontFamily};

    fn changed_definitions() -> FontDefinitions {
        let mut definitions = FontDefinitions::default();
        let proportional = definitions
            .families
            .get_mut(&FontFamily::Proportional)
            .expect("default proportional font family exists");
        proportional.push(proportional[0].clone());
        definitions
    }

    #[test]
    fn staged_font_replacement_activates_on_next_actual_pass() {
        let ctx = egui::Context::default();
        register(&ctx);
        let definitions = changed_definitions();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(active_revision(ui.ctx()), 0);
            install(ui.ctx(), definitions.clone());
            assert_eq!(active_revision(ui.ctx()), 0);
        });
        output.drop_without_applying_deltas();
        assert_eq!(active_revision(&ctx), 0);

        // A hidden native tick reads visibility and repaint state only. It
        // must not submit fonts or promote the active revision.
        drop(ctx.run_logic(&egui::RawInput::default(), |_| {}));
        assert_eq!(active_revision(&ctx), 0);

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(active_revision(ui.ctx()), 1);
        });
        output.drop_without_applying_deltas();
        assert_eq!(active_revision(&ctx), 1);
    }

    #[test]
    fn staged_replacement_canceled_back_to_active_fonts_keeps_revision() {
        let ctx = egui::Context::default();
        register(&ctx);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(active_revision(ui.ctx()), 0);
            install(ui.ctx(), changed_definitions());
            install(ui.ctx(), FontDefinitions::default());
            assert_eq!(active_revision(ui.ctx()), 0);
        });
        output.drop_without_applying_deltas();

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert_eq!(active_revision(ui.ctx()), 0);
            assert_eq!(
                ui.ctx().fonts(|fonts| fonts.definitions().clone()),
                FontDefinitions::default()
            );
        });
        output.drop_without_applying_deltas();
        assert_eq!(active_revision(&ctx), 0);
    }
}
