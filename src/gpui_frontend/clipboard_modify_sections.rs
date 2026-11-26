//! Named catalog sections for the retained Clipboard Modify workspace.
use super::*;
use multi_launcher::clipboard_modify::help::build_help_entries;

pub(super) struct SectionState {
    pub template_filter: Entity<InputState>,
    pub pipeline_filter: Entity<InputState>,
    pub help_filter: Entity<InputState>,
    pub template: Option<String>,
    pub pipeline: Option<String>,
    _subscriptions: Vec<Subscription>,
}
impl SectionState {
    pub fn new(window: &mut Window, cx: &mut Context<ClipboardModifyView>) -> Self {
        let template_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter templates"));
        let pipeline_filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter pipelines"));
        let help_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter help"));
        let subscriptions = [&template_filter, &pipeline_filter, &help_filter]
            .into_iter()
            .map(|input| {
                cx.subscribe(
                    input,
                    |view: &mut ClipboardModifyView,
                     input: Entity<InputState>,
                     event: &InputEvent,
                     cx| {
                        let active = match view.section {
                            Section::Templates => {
                                input.entity_id() == view.sections.template_filter.entity_id()
                            }
                            Section::SavedPipelines => {
                                input.entity_id() == view.sections.pipeline_filter.entity_id()
                            }
                            Section::Help => {
                                input.entity_id() == view.sections.help_filter.entity_id()
                            }
                            _ => false,
                        };
                        if active && matches!(event, InputEvent::Change) {
                            view.retire_effect_for_edit(cx);
                            view.section_revision = view.section_revision.wrapping_add(1);
                            cx.notify();
                        }
                    },
                )
            })
            .collect();
        Self {
            template_filter,
            pipeline_filter,
            help_filter,
            template: None,
            pipeline: None,
            _subscriptions: subscriptions,
        }
    }
}

fn matches(query: &str, id: &str, label: &str, aliases: &[String]) -> bool {
    let normalize = multi_launcher::clipboard_modify::catalog::normalize_name;
    let query = normalize(query);
    query.is_empty()
        || normalize(id).contains(&query)
        || normalize(label).contains(&query)
        || aliases
            .iter()
            .any(|alias| normalize(alias).contains(&query))
}

pub(super) fn selection_visible(view: &ClipboardModifyView, cx: &App) -> bool {
    let Some(catalog) = view.catalog.as_ref() else {
        return false;
    };
    match view.section {
        Section::Modify => true,
        Section::Templates => view.sections.template.as_ref().is_some_and(|id| {
            catalog.templates.iter().any(|item| {
                &item.id == id
                    && matches(
                        view.sections.template_filter.read(cx).value().as_str(),
                        &item.id,
                        &item.label,
                        &item.aliases,
                    )
            })
        }),
        Section::SavedPipelines => view.sections.pipeline.as_ref().is_some_and(|id| {
            catalog.pipelines.iter().any(|item| {
                &item.id == id
                    && matches(
                        view.sections.pipeline_filter.read(cx).value().as_str(),
                        &item.id,
                        &item.label,
                        &item.aliases,
                    )
            })
        }),
        _ => false,
    }
}

pub(super) fn render(
    view: &mut ClipboardModifyView,
    mut panel: Stateful<Div>,
    cx: &mut Context<ClipboardModifyView>,
) -> Stateful<Div> {
    panel = match view.section {
        Section::Templates => {
            let manage = view.mount.clone();
            let revision = view.section_revision;
            panel = panel.child(
                Button::new("cm-manage-templates")
                    .label("Manage Templates")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.blocked()
                            && manage.is_current()
                            && manage.matches(&view.mount)
                            && view.section == Section::Templates
                            && view.section_revision == revision
                        {
                            view.select_section(Section::ManageTemplates, window, cx);
                        }
                    })),
            );
            panel.child(Input::new(&view.sections.template_filter).aria_label("Filter templates"))
        }
        Section::SavedPipelines => {
            let receipt = view.mount.clone();
            let revision = view.section_revision;
            panel = panel.child(
                Button::new("cm-manage-pipelines")
                    .label("Manage Pipelines")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.blocked()
                            && receipt.is_current()
                            && receipt.matches(&view.mount)
                            && view.section == Section::SavedPipelines
                            && view.section_revision == revision
                        {
                            view.select_section(Section::ManagePipelines, window, cx);
                        }
                    })),
            );
            panel.child(
                Input::new(&view.sections.pipeline_filter).aria_label("Filter saved pipelines"),
            )
        }
        Section::Help => panel.child(
            Input::new(&view.sections.help_filter).aria_label("Filter Clipboard Modify help"),
        ),
        _ => panel,
    };
    let Some(catalog) = view.catalog.clone() else {
        return panel.child(Label::new("Loading Clipboard Modify catalog…"));
    };
    let mount = view.mount.clone();
    let launcher_effect_pending = view
        .root
        .upgrade()
        .is_some_and(|root| root.read(cx).external_launch.clipboard_effect_pending());
    let revision = view.section_revision;
    match view.section {
        Section::Templates => {
            let query = view.sections.template_filter.read(cx).value().to_string();
            let mut count = 0;
            for template in &catalog.templates {
                if !matches(&query, &template.id, &template.label, &template.aliases) {
                    continue;
                }
                count += 1;
                let id = template.id.clone();
                let expected = query.clone();
                let receipt = mount.clone();
                let catalog_receipt = catalog.clone();
                let selected = view.sections.template.as_deref() == Some(&id);
                let apply_id = id.clone();
                let apply_filter = query.clone();
                let apply_mount = mount.clone();
                let apply_catalog = catalog.clone();
                let expected_preview = selected.then(|| view.effect_receipt()).flatten();
                let mut row = div()
                    .id(SharedString::from(format!("cm-template-{}", id)))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(Label::new(format!("{} ({})", template.label, template.id)))
                    .child(Label::new(format!(
                        "Aliases: {} · Processor: {:?}",
                        template.aliases.join(", "),
                        template.processor
                    )))
                    .child(Label::new(
                        template.template.chars().take(200).collect::<String>(),
                    ))
                    .child(
                        Button::new(SharedString::from(format!("cm-template-select-{id}")))
                            .label(if selected {
                                "Selected"
                            } else {
                                "Select template"
                            })
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if view.blocked()
                                    || !receipt.is_current()
                                    || !receipt.matches(&view.mount)
                                    || view.section != Section::Templates
                                    || view.section_revision != revision
                                    || !view.catalog.as_ref().is_some_and(|current| {
                                        Arc::ptr_eq(current, &catalog_receipt)
                                    })
                                    || view.sections.template_filter.read(cx).value().as_str()
                                        != expected
                                {
                                    return;
                                }
                                view.sections.template = Some(id.clone());
                                view.section_revision = view.section_revision.wrapping_add(1);
                                view.schedule(cx);
                            })),
                    );
                if selected {
                    row = row.child(Label::new("Preview uses the current source."));
                }
                row = row.child(
                    Button::new(SharedString::from(format!("cm-template-apply-{apply_id}")))
                        .label("Apply Template")
                        .disabled(expected_preview.is_none() || launcher_effect_pending)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.blocked()
                                || !apply_mount.is_current()
                                || !apply_mount.matches(&view.mount)
                                || view.section != Section::Templates
                                || view.section_revision != revision
                                || view.sections.template.as_deref() != Some(apply_id.as_str())
                                || view.sections.template_filter.read(cx).value().as_str()
                                    != apply_filter
                                || !view
                                    .catalog
                                    .as_ref()
                                    .is_some_and(|current| Arc::ptr_eq(current, &apply_catalog))
                            {
                                return;
                            }
                            if let Some(expected) = expected_preview {
                                view.effect(EffectKind::Apply, expected, None, window, cx);
                            }
                        })),
                );
                panel = panel.child(row);
            }
            if count == 0 {
                panel = panel.child(Label::new("No matching templates."));
            }
        }
        Section::SavedPipelines => {
            let query = view.sections.pipeline_filter.read(cx).value().to_string();
            let source_preview = multi_launcher::clipboard_modify::catalog::capped_source_preview(
                view.source.read(cx).value().as_str(),
            );
            panel = panel.child(Label::new(format!("Source: {source_preview}")));
            let mut count = 0;
            for pipeline in &catalog.pipelines {
                if !matches(&query, &pipeline.id, &pipeline.label, &pipeline.aliases) {
                    continue;
                }
                count += 1;
                let id = pipeline.id.clone();
                let expected = query.clone();
                let receipt = mount.clone();
                let catalog_receipt = catalog.clone();
                let selected = view.sections.pipeline.as_deref() == Some(&id);
                let apply_id = id.clone();
                let apply_filter = query.clone();
                let apply_mount = mount.clone();
                let apply_catalog = catalog.clone();
                let expected_preview = selected.then(|| view.effect_receipt()).flatten();
                let stages = pipeline
                    .stages
                    .iter()
                    .map(|stage| {
                        multi_launcher::clipboard_modify::catalog::canonical_command(
                            stage.operation,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" → ");
                panel = panel.child(
                    div()
                        .id(SharedString::from(format!("cm-pipeline-{}", id)))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new(format!("{} ({})", pipeline.label, pipeline.id)))
                        .child(Label::new(format!(
                            "Aliases: {}",
                            pipeline.aliases.join(", ")
                        )))
                        .child(Label::new(stages))
                        .child(
                            Button::new(SharedString::from(format!("cm-pipeline-select-{id}")))
                                .label(if selected {
                                    "Selected"
                                } else {
                                    "Select pipeline"
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view.blocked()
                                        || !receipt.is_current()
                                        || !receipt.matches(&view.mount)
                                        || view.section != Section::SavedPipelines
                                        || view.section_revision != revision
                                        || !view.catalog.as_ref().is_some_and(|current| {
                                            Arc::ptr_eq(current, &catalog_receipt)
                                        })
                                        || view.sections.pipeline_filter.read(cx).value().as_str()
                                            != expected
                                    {
                                        return;
                                    }
                                    view.sections.pipeline = Some(id.clone());
                                    view.section_revision = view.section_revision.wrapping_add(1);
                                    view.schedule(cx);
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!(
                                "cm-pipeline-execute-{apply_id}"
                            )))
                            .label(format!("Execute {}", pipeline.label))
                            .disabled(expected_preview.is_none() || launcher_effect_pending)
                            .on_click(cx.listener(
                                move |view, _, window, cx| {
                                    if view.blocked()
                                        || !apply_mount.is_current()
                                        || !apply_mount.matches(&view.mount)
                                        || view.section != Section::SavedPipelines
                                        || view.section_revision != revision
                                        || view.sections.pipeline.as_deref()
                                            != Some(apply_id.as_str())
                                        || view.sections.pipeline_filter.read(cx).value().as_str()
                                            != apply_filter
                                        || !view.catalog.as_ref().is_some_and(|current| {
                                            Arc::ptr_eq(current, &apply_catalog)
                                        })
                                    {
                                        return;
                                    }
                                    if let Some(expected) = expected_preview {
                                        view.effect(EffectKind::Apply, expected, None, window, cx);
                                    }
                                },
                            )),
                        ),
                );
            }
            if count == 0 {
                panel = panel.child(Label::new("No matching saved pipelines."));
            }
        }
        Section::Help => {
            let query = view.sections.help_filter.read(cx).value().to_string();
            let entries = build_help_entries(&catalog);
            let mut count = 0;
            for entry in entries
                .into_iter()
                .filter(|entry| entry.matches_filter(&query))
            {
                count += 1;
                panel = panel.child(
                    div()
                        .id(SharedString::from(format!("cm-help-{:?}", entry.source)))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Label::new(entry.canonical_syntax))
                        .child(Label::new(entry.description))
                        .child(Label::new(format!(
                            "{} · Pipeline: {} · {} · Aliases: {}",
                            entry.category,
                            if entry.pipeline_allowed {
                                "allowed"
                            } else {
                                "not allowed"
                            },
                            entry.arguments,
                            entry.aliases.join(", ")
                        )))
                        .child(Label::new(entry.examples.join(" · "))),
                );
            }
            if count == 0 {
                panel = panel.child(Label::new("No matching help entries."));
            }
        }
        _ => {}
    }
    panel
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_filter_matches_id_label_and_alias() {
        assert!(super::matches("LOUD", "loud", "Upper", &[]));
        assert!(super::matches("upper", "loud", "Upper", &[]));
        assert!(super::matches(
            "short",
            "loud",
            "Upper",
            &["shortcut".into()]
        ));
        assert!(!super::matches("missing", "loud", "Upper", &[]));
    }
    #[test]
    fn source_card_is_unicode_bounded() {
        use multi_launcher::clipboard_modify::catalog::capped_source_preview;
        assert_eq!(capped_source_preview(&"é".repeat(80)), "é".repeat(80));
        assert_eq!(
            capped_source_preview(&"é".repeat(81)),
            format!("{}…", "é".repeat(80))
        );
    }
}
