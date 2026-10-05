//! Gallery interest/cache and menu-scoped Simple authoring. The bounded gallery
//! worker prepares/rasterizes tiles; this owner uploads completed bounded pixels.

use super::*;
use crate::radial::{appearance::*, gallery::*};
use std::collections::BTreeMap;

#[derive(Clone)]
struct Entry {
    skin: crate::radial::model::SkinDefinition,
    builtin: Option<usize>,
    key: GalleryContentKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::Override;
    fn session() -> RadialAuthoringSession {
        RadialAuthoringSession::new(crate::radial::authoring::AuthoringSnapshot::new(
            Arc::new(RadialDocument::starter()),
            "appearance-test",
        ))
    }
    fn tile(correlation: GalleryCorrelation, dimension: u32) -> GalleryReply {
        use crate::radial::{compositor::RasterFrame, geometry::*};
        GalleryReply {
            correlation,
            result: Ok(GalleryTile {
                raster: Arc::new(RasterFrame {
                    logical_bounds: LogicalRect {
                        min: LogicalPoint { x: 0.0, y: 0.0 },
                        max: LogicalPoint { x: 360.0, y: 360.0 },
                    },
                    scale_factor: ScaleFactor::new(1.0).unwrap(),
                    image: image::RgbaImage::new(dimension, dimension),
                    generation: 1,
                    animation_deadline_ms: None,
                }),
                scene_digest: 1,
                geometry_digest: 2,
                selected_emphasis: false,
                diagnostic_count: 0,
                diagnostic_summary: None,
            }),
        }
    }
    #[test]
    fn retained_ring_proposal_gallery_preview_cancel_and_apply_share_one_candidate_owner() {
        let mut editor = RadialEditorState::default();
        editor.open_test_snapshot();
        let session = editor.session.as_ref().unwrap();
        let menu = session.draft.default_menu_id.clone();
        let ring = session.draft.menus[0].rings[0].id.clone();
        let proposal = super::super::menu::propose_ring_resize(
            &session.draft,
            session.generation,
            &menu,
            &ring,
            session.draft.menus[0].rings[0].cells.len() + 1,
            vec![CellId::new("retained-proposal-cell")],
        )
        .unwrap();
        let proposal_document = proposal.document.clone();
        editor.install_ring_proposal(proposal);
        let before = Arc::clone(&editor.session.as_ref().unwrap().draft);
        let depths = editor.session.as_ref().unwrap().acceptance_history_depths();
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        editor.client = Some(client);
        editor.show_resources = true;
        editor.appearance.destination = Some(menu.clone());
        editor
            .appearance
            .sync(editor.session.as_ref().unwrap(), 1.0);
        editor
            .appearance
            .choose(editor.session.as_ref().unwrap(), 1)
            .unwrap();
        let candidate = Arc::clone(&editor.appearance.choice.as_ref().unwrap().candidate);
        let context = egui::Context::default();
        let frame = DesignerFrameContext {
            feature_defaults: Default::default(),
            expected_diagnostics: Vec::new(),
            action_catalog: Arc::new(
                crate::gui::universal_action_catalog::UniversalActionCatalogSnapshot {
                    entries: Vec::new(),
                    recent_entries: Vec::new(),
                    dashboard: Arc::new(Default::default()),
                },
            ),
            require_confirm_destructive: false,
            intent_bridge: Arc::clone(&editor.intent_bridge),
            root_window_bridge: Default::default(),
        };
        let draw = |editor: &mut RadialEditorState| {
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 1000.0),
                    )),
                    ..Default::default()
                },
                |ctx| editor.viewport_ui(ctx, &frame, egui::ViewportClass::Deferred),
            );
        };
        let prepare = |editor: &mut RadialEditorState| {
            let request = endpoint.request_rx.try_recv().unwrap();
            let crate::radial::authoring::AuthoringRequest::PrepareEmbeddedPreview {
                id,
                generation,
                editor_session,
                candidate,
                menu_id,
                selected,
                anchor,
                work_area,
                scale,
                token,
                projection,
            } = request
            else {
                panic!("expected existing embedded preparation owner")
            };
            let input = Arc::new(
                crate::radial::preparation::PreviewFramePreparer::new(Default::default())
                    .prepare(
                        &candidate,
                        &menu_id,
                        anchor,
                        work_area,
                        scale,
                        generation.0,
                        selected.as_ref(),
                        &projection,
                    )
                    .unwrap(),
            );
            assert!(editor.session.as_mut().unwrap().accept_reply(
                crate::radial::authoring::AuthoringReply::EmbeddedPreviewPrepared {
                    id,
                    generation,
                    editor_session,
                    token,
                    input
                }
            ));
            candidate
        };
        draw(&mut editor);
        let crate::radial::authoring::AuthoringRequest::FontCatalog {
            id,
            generation,
            editor_session,
        } = endpoint.request_rx.try_recv().unwrap()
        else {
            panic!("initial existing font catalogue owner")
        };
        assert!(
            editor.session.as_mut().unwrap().accept_reply(
                crate::radial::authoring::AuthoringReply::FontCatalog {
                    id,
                    generation,
                    editor_session,
                    families: crate::radial::font_cache::SystemFontCatalog::discover()
                        .family_names()
                        .into()
                }
            )
        );
        draw(&mut editor);
        assert_eq!(prepare(&mut editor), candidate);
        draw(&mut editor);
        let prepared = editor
            .appearance
            .prepared_identity(editor.session.as_ref().unwrap(), &editor.preview)
            .unwrap();
        assert!(editor.appearance.choice_is_prepared(Some(prepared)));
        assert_eq!(
            editor.ring_proposal.as_ref().unwrap().document,
            proposal_document
        );
        // An asset-watch epoch must not steal an in-flight ordinary embedded
        // request. Its old completion cannot make the refreshed choice ready.
        editor
            .appearance
            .choose(editor.session.as_ref().unwrap(), 3)
            .unwrap();
        draw(&mut editor);
        let pending = editor.session.as_ref().unwrap().pending_request.clone();
        editor.invalidate_appearance_resources();
        assert_eq!(editor.session.as_ref().unwrap().pending_request, pending);
        assert!(
            editor
                .preview
                .prepared_frame(editor.session.as_ref().unwrap())
                .is_none()
        );
        prepare(&mut editor);
        draw(&mut editor);
        for _ in 0..3 {
            if editor
                .appearance
                .prepared_identity(editor.session.as_ref().unwrap(), &editor.preview)
                .is_some()
            {
                break;
            }
            prepare(&mut editor);
            draw(&mut editor);
        }
        assert!(
            editor
                .appearance
                .prepared_identity(editor.session.as_ref().unwrap(), &editor.preview)
                .is_some()
        );
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
        editor.appearance.choice = None;
        draw(&mut editor);
        assert_eq!(&*prepare(&mut editor), &proposal_document);
        draw(&mut editor);
        assert_eq!(editor.session.as_ref().unwrap().draft, before);
        assert_eq!(
            editor.session.as_ref().unwrap().acceptance_history_depths(),
            depths
        );
        editor
            .appearance
            .choose(editor.session.as_ref().unwrap(), 2)
            .unwrap();
        draw(&mut editor);
        prepare(&mut editor);
        draw(&mut editor);
        assert!(
            editor
                .appearance
                .prepared_identity(editor.session.as_ref().unwrap(), &editor.preview)
                .is_some()
        );
        editor
            .appearance
            .apply(editor.session.as_mut().unwrap())
            .unwrap();
        assert_eq!(
            editor.session.as_ref().unwrap().draft.menus[0].rings[0].cells,
            before.menus[0].rings[0].cells
        );
        assert_eq!(
            editor
                .session
                .as_ref()
                .unwrap()
                .acceptance_history_depths()
                .0,
            depths.0 + 1
        );
        assert_eq!(
            editor.ring_proposal.as_ref().unwrap().document,
            proposal_document
        );
    }
    #[test]
    fn destination_navigation_and_prepared_choice_identity_cannot_apply_an_old_menu_or_tile() {
        let mut session = session();
        let root = session.draft.default_menu_id.clone();
        let mut dirty = (*session.draft).clone();
        dirty.menus[0].name = "Retained dirty root".into();
        let mut other = dirty.menus[0].clone();
        other.id = MenuId::new("other-appearance-menu");
        other.name = "Retained dirty destination".into();
        for ring in &mut other.rings {
            ring.id = RingId::new(format!("other-{}", ring.id));
            for cell in &mut ring.cells {
                cell.id = CellId::new(format!("other-{}", cell.id));
            }
        }
        let destination = other.id.clone();
        dirty.menus.push(other);
        session.replace_document_atomic(dirty).unwrap();
        let before = Arc::clone(&session.draft);
        let depths = session.acceptance_history_depths();
        let mut gallery = AppearanceGallery::default();
        gallery.destination = Some(root.clone());
        gallery.sync(&session, 1.0);
        gallery.choose(&session, 1).unwrap();
        let choice = gallery.choice.as_ref().unwrap();
        let completed_a = PreparedAppearance {
            session: choice.session.0,
            menu: digest(&choice.destination),
            candidate: choice.candidate_digest,
            key: digest(&choice.key),
            token: digest(&choice.token),
        };
        assert!(gallery.choice_is_prepared(Some(completed_a)));
        gallery.choose(&session, 2).unwrap();
        assert!(!gallery.choice_is_prepared(Some(completed_a)));
        session.selection = Some(StableSelection::Menu(destination.clone()));
        assert!(gallery.candidate(&session).is_none());
        assert!(gallery.apply(&mut session).is_err());
        gallery.sync(&session, 1.0);
        assert!(gallery.choice.is_none());
        assert_eq!(gallery.destination, Some(destination.clone()));
        session.selection = Some(StableSelection::Skin(session.draft.skins[0].id.clone()));
        gallery.sync(&session, 1.0);
        gallery.choose(&session, 1).unwrap();
        gallery.apply(&mut session).unwrap();
        assert_eq!(session.draft.menus[0], before.menus[0]);
        assert_eq!(
            session
                .draft
                .menus
                .iter()
                .find(|menu| menu.id == destination)
                .unwrap()
                .name,
            "Retained dirty destination"
        );
        assert_eq!(session.acceptance_history_depths().0, depths.0 + 1);
        assert!(session.undo());
        assert_eq!(session.draft, before);
    }
    #[test]
    fn masking_descriptions_identify_bounded_authored_ring_and_cell_owners() {
        let mut document = RadialDocument::starter();
        document.menus[0].name = "Workbench".into();
        document.menus[0].rings[0].style.text.color =
            Override::Value(crate::radial::model::ColorRgba {
                red: 1,
                green: 2,
                blue: 3,
                alpha: 255,
            });
        let template = document.menus[0].rings[0].cells[0].clone();
        document.menus[0].rings[0].cells = (0..10)
            .map(|index| {
                let mut cell = template.clone();
                cell.id = CellId::new(format!("masked-{index}"));
                cell.label = format!("Authored {index}");
                cell.style.text.color = Override::Clear;
                cell
            })
            .collect();
        let state = simple_state(&document, &document.default_menu_id).unwrap();
        let (owners, overflow) = mask_descriptions(&state, SimpleControl::Accent, &document);
        assert_eq!(owners.len(), 8);
        assert!(overflow >= 2);
        assert!(owners.iter().all(|owner| owner.contains("Workbench")));
        assert!(owners.iter().any(|owner| owner.contains("Authored")));
    }
    #[test]
    fn retained_simple_panel_identifies_duplicate_empty_labels_before_long_menu_names() {
        let mut document = RadialDocument::starter();
        document.menus[0].name = "Long authored menu ".repeat(12);
        let template = document.menus[0].rings[0].cells[0].clone();
        document.menus[0].rings[0].cells = ["Duplicate", "Duplicate", ""]
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let mut cell = template.clone();
                cell.id = CellId::new(format!("masked-{index}"));
                cell.label = label.into();
                cell.style.text.color = Override::Clear;
                cell
            })
            .collect();
        let destination = document.default_menu_id.clone();
        let state = simple_state(&document, &destination).unwrap();
        let (owners, overflow) = mask_descriptions(&state, SimpleControl::Accent, &document);
        assert_eq!(overflow, 0);
        assert_eq!(owners.len(), 3);
        for (index, owner) in owners.iter().enumerate() {
            assert!(owner.starts_with(&format!(
                "Ring 1 / slot {} · Cell [masked-{index}]",
                index + 1
            )));
            assert!(owner.chars().count() <= 160);
            assert!(owner.contains("Long authored menu"));
        }
        assert_ne!(owners[0], owners[1]);
        assert!(owners[2].contains("(unnamed)"));
        let mut session = RadialAuthoringSession::new(
            crate::radial::authoring::AuthoringSnapshot::new(Arc::new(document), "mask-panel"),
        );
        let before = Arc::clone(&session.draft);
        let depths = session.acceptance_history_depths();
        let mut gallery = AppearanceGallery::default();
        let context = egui::Context::default();
        context.style_mut(|style| style.animation_time = 0.0);
        fn text(
            shape: &egui::epaint::Shape,
            labels: &mut Vec<String>,
            header: &mut Option<egui::Pos2>,
        ) {
            match shape {
                egui::epaint::Shape::Text(shape) => {
                    if shape.galley.text() == "Inheritance and explicit resets" {
                        *header = Some(shape.pos + shape.galley.rect.center().to_vec2());
                    }
                    labels.push(shape.galley.text().into());
                }
                egui::epaint::Shape::Vec(shapes) => {
                    for shape in shapes {
                        text(shape, labels, header);
                    }
                }
                _ => {}
            }
        }
        let mut labels = Vec::new();
        let mut header = None;
        for frame in 0..4 {
            let events = if matches!(frame, 1 | 2) {
                let pos = header.expect("actual painted inheritance header");
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 1,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            } else {
                Vec::new()
            };
            labels.clear();
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 2000.0),
                    )),
                    // Match the retained GUI driver's frame cadence. A
                    // one-second held press exceeds egui's click duration.
                    time: Some(frame as f64 / 60.0),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        gallery.simple_ui(ui, &mut session, &destination, ViewportClass::Deferred);
                    });
                },
            );
            for shape in &output.shapes {
                text(&shape.shape, &mut labels, &mut header);
            }
            if frame == 0 {
                assert!(!labels.iter().any(|label| label.starts_with("Masked by ")));
            } else if frame == 2 {
                assert!(context.input(|input| input.pointer.primary_clicked()));
            }
        }
        for owner in owners {
            assert!(
                labels
                    .iter()
                    .any(|label| label == &format!("Masked by {owner}")),
                "missing visible owner {owner}; painted labels={labels:?}"
            );
        }
        assert_eq!(session.draft, before);
        assert_eq!(session.acceptance_history_depths(), depths);
    }
    #[test]
    fn authored_style_copy_keeps_custom_origin_and_resource_refresh_retires_old_prepared_identity()
    {
        let session = session();
        let before = Arc::clone(&session.draft);
        let depths = session.acceptance_history_depths();
        let mut gallery = AppearanceGallery::default();
        gallery.destination = Some(session.draft.default_menu_id.clone());
        gallery.sync(&session, 1.0);
        assert_eq!(gallery.entries[0].key, gallery.entries[5].key);
        gallery.choose(&session, 5).unwrap();
        let choice = gallery.choice.as_ref().unwrap();
        assert_eq!(choice.builtin, None);
        let prepared = PreparedAppearance {
            session: choice.session.0,
            menu: digest(&choice.destination),
            candidate: choice.candidate_digest,
            key: digest(&choice.key),
            token: digest(&choice.token),
        };
        assert_eq!(
            gallery
                .observation(&session, None, None, None)
                .preview_builtin,
            -1
        );
        gallery.invalidate_resources();
        gallery.sync(&session, 1.0);
        assert!(!gallery.choice_is_prepared(Some(prepared)));
        assert_eq!(gallery.choice.as_ref().unwrap().builtin, None);
        assert_ne!(digest(&gallery.choice.as_ref().unwrap().key), prepared.key);
        assert_eq!(session.draft, before);
        assert_eq!(session.acceptance_history_depths(), depths);
    }
    #[test]
    fn appearance_preview_cancel_and_single_apply_preserve_dirty_work_and_explicit_destination() {
        let mut session = session();
        let destination = session.draft.default_menu_id.clone();
        let mut dirty = (*session.draft).clone();
        dirty.menus[0].name = "Unrelated dirty work".into();
        session.replace_document_atomic(dirty).unwrap();
        session.selection = Some(StableSelection::Skin(session.draft.skins[0].id.clone()));
        let before = Arc::clone(&session.draft);
        let history = session.acceptance_history_depths();
        let selection = session.selection.clone();
        let mut gallery = AppearanceGallery::default();
        gallery.destination = Some(destination.clone());
        gallery.sync(&session, 1.0);
        gallery.choose(&session, 1).unwrap();
        assert_eq!(session.draft, before);
        assert_eq!(session.acceptance_history_depths(), history);
        assert_eq!(
            gallery.candidate(&session).unwrap().2,
            StableSelection::Menu(destination)
        );
        gallery.choice = None;
        assert_eq!(session.draft, before);
        assert!(session.is_dirty());
        gallery.choose(&session, 1).unwrap();
        gallery.apply(&mut session).unwrap();
        assert_eq!(session.acceptance_history_depths().0, history.0 + 1);
        assert_eq!(session.selection, selection);
        assert_eq!(session.draft.menus[0].name, "Unrelated dirty work");
        let applied = Arc::clone(&session.draft);
        assert!(session.undo());
        assert_eq!(session.draft, before);
        assert!(session.redo());
        assert_eq!(session.draft, applied);
        gallery.sync(&session, 1.0);
        gallery.choose(&session, 2).unwrap();
        let mut later = (*session.draft).clone();
        later.menus[0].name = "Later dirty work".into();
        session.replace_document_atomic(later).unwrap();
        assert!(gallery.apply(&mut session).is_err());
        assert_eq!(session.draft.menus[0].name, "Later dirty work");
    }
    #[test]
    fn gallery_demand_reply_correlation_retirement_and_negative_cache_are_bounded() {
        let session = session();
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut gallery = AppearanceGallery::default();
        gallery.sync(&session, 4.0);
        let before = Arc::clone(&session.draft);
        gallery.request_visible(&[1], &session, Some(&client));
        let request = endpoint.gallery_rx.try_recv().unwrap();
        gallery.request_visible(&[1, 2], &session, Some(&client));
        assert!(endpoint.gallery_rx.try_recv().is_err());
        let mut wrong = request.correlation;
        wrong.key.demo_version += 1;
        assert!(!gallery.accept(tile(wrong, 360), &session));
        assert_eq!(gallery.pending, Some(request.correlation));
        assert!(gallery.accept(tile(request.correlation, 360), &session));
        let count = gallery.requested;
        gallery.request_visible(&[1], &session, Some(&client));
        assert_eq!(gallery.requested, count);
        gallery.request_visible(&[2], &session, Some(&client));
        let retired = endpoint.gallery_rx.try_recv().unwrap();
        gallery.retire_interest();
        assert!(!retired.interest.load(std::sync::atomic::Ordering::Acquire));
        assert!(gallery.pending.is_none());
        assert!(!gallery.accept(tile(retired.correlation, 360), &session));
        gallery.request_visible(&[3], &session, Some(&client));
        assert!(endpoint.gallery_rx.try_recv().is_err());
        gallery.sync(&session, 4.0);
        gallery.request_visible(&[2], &session, Some(&client));
        let oversized = endpoint.gallery_rx.try_recv().unwrap();
        assert!(gallery.accept(tile(oversized.correlation, 1500), &session));
        assert!(gallery.cache[&oversized.correlation.key].result.is_err());
        let count = gallery.requested;
        gallery.request_visible(&[2], &session, Some(&client));
        assert_eq!(gallery.requested, count);
        for i in 0..MAX_GALLERY_ENTRIES + 3 {
            let mut key = oversized.correlation.key;
            key.appearance = i as u64;
            gallery.cache.insert(
                key,
                CachedTile {
                    result: Err("missing".into()),
                    texture: None,
                    bytes: 600_000,
                    touched: i as u64,
                },
            );
        }
        gallery.evict();
        assert!(gallery.cache.len() <= MAX_GALLERY_ENTRIES);
        assert!(gallery.cache.values().map(|tile| tile.bytes).sum::<usize>() <= MAX_GALLERY_BYTES);
        gallery.invalidate_resources();
        assert!(gallery.cache.is_empty());
        gallery.dispose();
        assert!(gallery.entries.is_empty());
        assert_eq!(session.draft, before);
    }
    #[test]
    fn gallery_replacement_terminalizes_retired_pending_without_waiting_for_late_reply() {
        let old_session = session();
        let current_session = session();
        assert_ne!(old_session.editor_session, current_session.editor_session);
        let (client, endpoint) = crate::radial::authoring::authoring_control_service();
        let mut gallery = AppearanceGallery::default();
        gallery.sync(&old_session, 1.0);
        gallery.request_visible(&[1], &old_session, Some(&client));
        let old = endpoint.gallery_rx.try_recv().unwrap();
        gallery.retire_interest();
        assert!(gallery.pending.is_none());
        gallery.sync(&current_session, 1.0);
        gallery.request_visible(&[1], &current_session, Some(&client));
        let current = endpoint.gallery_rx.try_recv().unwrap();
        endpoint.send_gallery_reply(GalleryReply {
            correlation: current.correlation,
            result: Err("current dispatch rejected".into()),
        });
        endpoint.gallery_reply_sink()(tile(old.correlation, 360));
        assert!(gallery.accept(client.try_recv_gallery().unwrap(), &current_session));
        assert!(gallery.pending.is_none());
        assert_eq!(
            gallery.cache[&current.correlation.key]
                .result
                .as_ref()
                .unwrap_err(),
            "current dispatch rejected"
        );
        assert!(client.try_recv_gallery().is_none());
        gallery.invalidate_resources();
        gallery.sync(&current_session, 1.0);
        gallery.request_visible(&[1], &current_session, Some(&client));
        let resources = endpoint.gallery_rx.try_recv().unwrap();
        gallery.invalidate_resources();
        assert!(gallery.pending.is_none());
        assert!(
            !resources
                .interest
                .load(std::sync::atomic::Ordering::Acquire)
        );
        gallery.sync(&current_session, 1.25);
        gallery.request_visible(&[1], &current_session, Some(&client));
        let changed = endpoint.gallery_rx.try_recv().unwrap();
        assert_eq!(gallery.pending, Some(changed.correlation));
        assert!(!gallery.accept(tile(resources.correlation, 360), &current_session));
        assert_eq!(gallery.pending, Some(changed.correlation));
        assert!(gallery.accept(tile(changed.correlation, 360), &current_session));
        assert!(gallery.pending.is_none());
        assert!(endpoint.request_rx.try_recv().is_err());
    }
}
struct CachedTile {
    result: Result<GalleryTile, String>,
    texture: Option<egui::TextureHandle>,
    bytes: usize,
    touched: u64,
}
#[derive(Clone)]
struct PreviewChoice {
    skin: crate::radial::model::SkinDefinition,
    builtin: Option<usize>,
    session: crate::radial::authoring::AuthoringSessionId,
    generation: crate::radial::authoring::DraftGeneration,
    destination: MenuId,
    key: GalleryContentKey,
    candidate: Arc<RadialDocument>,
    token: String,
    candidate_digest: u64,
}

#[derive(Default)]
pub(super) struct AppearanceGallery {
    active: bool,
    pub(super) destination: Option<MenuId>,
    source: Option<(
        crate::radial::authoring::AuthoringSessionId,
        crate::radial::authoring::DraftGeneration,
        u32,
        u64,
    )>,
    entries: Vec<Entry>,
    cache: BTreeMap<GalleryContentKey, CachedTile>,
    pending: Option<GalleryCorrelation>,
    interest: u64,
    interest_token: Arc<std::sync::atomic::AtomicBool>,
    visible_pending: usize,
    next_request: u64,
    clock: u64,
    choice: Option<PreviewChoice>,
    resource_revision: u64,
    pub(super) requested: u64,
    pub(super) completed: u64,
    pub(super) rejected: u64,
    pub(super) feedback: Option<String>,
    reset_descendants: bool,
    observed_base: Option<(
        crate::radial::authoring::AuthoringSessionId,
        crate::radial::authoring::DraftGeneration,
        MenuId,
        AppearanceObservation,
    )>,
    observed_frame: Option<Arc<crate::radial::preparation::PreparedFrameInput>>,
    frame_facts: (u64, u64, u64, usize, u64),
}

fn digest(value: &impl std::fmt::Debug) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    format!("{value:?}").hash(&mut h);
    h.finish()
}

/// Small explicit steps complement dragging and preserve keyboard-accessible
/// single-transaction edits. Each response is traced before its mutation.
fn step_buttons(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    target: DesignerAuthoringTarget,
    viewport: ViewportClass,
    session: &RadialAuthoringSession,
) -> bool {
    let mut changed = false;
    let step = if *range.end() > 8.0 { 1.0 } else { 0.05 };
    ui.horizontal(|ui| {
        for (index, delta, label) in [(0, -step, "−"), (1, step, "+")] {
            let next = (*value + delta).clamp(*range.start(), *range.end());
            let enabled = next != *value;
            let response = ui.add_enabled(enabled, egui::Button::new(label));
            trace_designer_authoring_control(
                ui,
                &response,
                target,
                DesignerAuthoringRole::Button,
                Some(index),
                enabled,
                false,
                viewport,
                trace_correlation(Some(session)),
            );
            if response.clicked() {
                *value = next;
                changed = true;
            }
        }
    });
    changed
}

fn source_description(
    source: &crate::radial::skin::StyleSource,
    document: &RadialDocument,
) -> String {
    use crate::radial::skin::StyleSource;
    let bounded = |text: &str, limit: usize| {
        let mut chars = text.chars();
        let mut label: String = chars.by_ref().take(limit).collect();
        if chars.next().is_some() {
            label.push('…');
        }
        if label.is_empty() {
            label.push_str("(unnamed)");
        }
        label
    };
    let menu_name = |id: &MenuId| {
        document
            .menus
            .iter()
            .find(|menu| &menu.id == id)
            .map_or("Missing menu", |menu| menu.name.as_str())
    };
    match source {
        StyleSource::ApplicationFallback => "Application defaults".into(),
        StyleSource::UserDefaults => "User defaults".into(),
        StyleSource::SelectedSkin(id) => format!(
            "Skin [{}]: {}",
            bounded(id.as_str(), 36),
            bounded(
                document
                    .skins
                    .iter()
                    .find(|skin| &skin.id == id)
                    .map_or("Missing skin", |skin| skin.name.as_str()),
                64
            )
        ),
        StyleSource::Menu(id) => format!(
            "Menu [{}]: {}",
            bounded(id.as_str(), 36),
            bounded(menu_name(id), 64)
        ),
        StyleSource::Ring { menu_id, ring_id } => {
            let position = document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().position(|ring| &ring.id == ring_id))
                .map_or_else(|| "?".into(), |i| (i + 1).to_string());
            format!(
                "Ring {position} [{}] · Menu: {}",
                bounded(ring_id.as_str(), 36),
                bounded(menu_name(menu_id), 64)
            )
        }
        StyleSource::Cell {
            menu_id,
            ring_id,
            cell_id,
        } => {
            let ring = document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| {
                    menu.rings
                        .iter()
                        .enumerate()
                        .find(|(_, ring)| &ring.id == ring_id)
                });
            let cell = ring.and_then(|(_, ring)| {
                ring.cells
                    .iter()
                    .enumerate()
                    .find(|(_, cell)| &cell.id == cell_id)
            });
            let ring_position = ring.map_or_else(|| "?".into(), |(i, _)| (i + 1).to_string());
            let slot_position = cell.map_or_else(|| "?".into(), |(i, _)| (i + 1).to_string());
            format!(
                "Ring {ring_position} / slot {slot_position} · Cell [{}]: {} · Menu: {}",
                bounded(cell_id.as_str(), 36),
                bounded(
                    cell.map_or("Missing cell", |(_, cell)| cell.label.as_str()),
                    28
                ),
                bounded(menu_name(menu_id), 40),
            )
        }
    }
    .chars()
    .take(160)
    .collect()
}

fn mask_descriptions(
    state: &SimpleAppearance,
    control: SimpleControl,
    document: &RadialDocument,
) -> (Vec<String>, usize) {
    let owners = control
        .fields()
        .iter()
        .filter_map(|field| state.masks.get(field))
        .flatten()
        .collect::<std::collections::BTreeSet<_>>();
    (
        owners
            .iter()
            .take(8)
            .map(|source| source_description(source, document))
            .collect(),
        owners.len().saturating_sub(8),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PreparedAppearance {
    session: u64,
    menu: u64,
    candidate: u64,
    key: u64,
    token: u64,
}

impl AppearanceGallery {
    /// Capture the existing embedded preview owner's completed candidate before
    /// controls can change the choice during this frame.
    pub(super) fn prepared_identity(
        &self,
        session: &RadialAuthoringSession,
        preview: &super::preview::EmbeddedPreview,
    ) -> Option<PreparedAppearance> {
        let choice = self.choice.as_ref()?;
        self.candidate(session)?;
        if preview.current_menu() != Some(&choice.destination)
            || !preview.candidate_is_prepared(session, &choice.token)
        {
            return None;
        }
        Some(PreparedAppearance {
            session: session.editor_session.0,
            menu: digest(&choice.destination),
            candidate: choice.candidate_digest,
            key: digest(&choice.key),
            token: digest(&choice.token),
        })
    }
    fn choice_is_prepared(&self, prepared: Option<PreparedAppearance>) -> bool {
        self.choice
            .as_ref()
            .zip(prepared)
            .is_some_and(|(choice, prepared)| {
                prepared.session == choice.session.0
                    && prepared.menu == digest(&choice.destination)
                    && prepared.candidate == choice.candidate_digest
                    && prepared.key == digest(&choice.key)
                    && prepared.token == digest(&choice.token)
            })
    }
    pub(super) fn observation(
        &mut self,
        session: &RadialAuthoringSession,
        frame: Option<&Arc<crate::radial::preparation::PreparedFrameInput>>,
        prepared_menu: Option<&MenuId>,
        prepared: Option<PreparedAppearance>,
    ) -> AppearanceObservation {
        let destination = selected_menu_id(session).or_else(|| self.destination.clone());
        let mut state = AppearanceObservation::default();
        state.current_builtin = -1;
        state.preview_builtin = -1;
        state.opacity_milli = -1;
        if let Some(menu_id) = destination {
            if self
                .observed_base
                .as_ref()
                .is_none_or(|(id, generation, menu, _)| {
                    *id != session.editor_session
                        || *generation != session.generation
                        || *menu != menu_id
                })
            {
                if let Some(menu) = session.draft.menus.iter().find(|menu| menu.id == menu_id) {
                    state.session_id = session.editor_session.0;
                    state.generation = session.generation.0;
                    state.menu_digest = digest(&menu.id);
                    state.bindings_digest = digest(&(
                        &menu.center_action,
                        &menu.center_secondary_action,
                        &menu.background_action,
                        &menu.background_secondary_action,
                        menu.rings
                            .iter()
                            .flat_map(|ring| &ring.cells)
                            .map(|cell| {
                                (
                                    &cell.id,
                                    &cell.content,
                                    &cell.alternate_clicks,
                                    &cell.alternate_controls,
                                    &cell.shortcuts,
                                    &cell.hotstrings,
                                )
                            })
                            .collect::<Vec<_>>(),
                    ));
                    state.other_menus_digest = digest(
                        &session
                            .draft
                            .menus
                            .iter()
                            .filter(|other| other.id != menu_id)
                            .collect::<Vec<_>>(),
                    );
                    state.raw_overrides_digest = digest(&(
                        &menu.style,
                        menu.rings
                            .iter()
                            .map(|ring| {
                                (
                                    &ring.style,
                                    ring.cells
                                        .iter()
                                        .map(|cell| &cell.style)
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ));
                    state.authored_cell_count =
                        menu.rings.iter().map(|ring| ring.cells.len()).sum();
                    if let Some(skin) = session
                        .draft
                        .skins
                        .iter()
                        .find(|skin| skin.id == menu.skin_id)
                    {
                        state.current_builtin = BUILTIN_SKINS
                            .iter()
                            .position(|preset| preset.definition().style == skin.style)
                            .map_or(-1, |i| i as i32);
                    }
                    if let Ok(simple) = simple_state(&session.draft, &menu_id) {
                        state.effective_digest = digest(&simple.values);
                        state.opacity_milli =
                            simple.opacity.map_or(-1, |v| (v * 1000.0).round() as i32);
                        state.scale_milli = value(&simple.values.geometry.menu_scale)
                            .map_or(0, |v| (v * 1000.0).round() as i32);
                        state.spacing_milli = value(&simple.values.geometry.radius_scale)
                            .map_or(0, |v| (v * 1000.0).round() as i32);
                        state.label_size_milli = value(&simple.values.text.font_size)
                            .map_or(0, |v| (v * 1000.0).round() as i32);
                        state.labels_visible = value(&simple.values.text.visible).unwrap_or(false);
                        state.masked_fields = simple.masks.len();
                    }
                    self.observed_base =
                        Some((session.editor_session, session.generation, menu_id, state));
                }
            }
            if let Some((_, _, _, base)) = &self.observed_base {
                state = *base;
            }
        }
        if let Some(frame) = frame {
            if self
                .observed_frame
                .as_ref()
                .is_none_or(|previous| !Arc::ptr_eq(previous, frame))
            {
                self.frame_facts = (
                    frame.scene.generation,
                    digest(&frame.layout),
                    digest(&frame.scene.primitives),
                    frame
                        .diagnostics
                        .iter()
                        .filter(|d| {
                            matches!(
                                d.kind,
                                crate::radial::diagnostics::RadialDiagnosticKind::DensityPressure(
                                    _
                                )
                            )
                        })
                        .count(),
                    digest(&frame.diagnostics),
                );
                self.observed_frame = Some(Arc::clone(frame));
            }
        } else {
            self.observed_frame = None;
            self.frame_facts = Default::default();
        }
        (
            state.prepared_generation,
            state.geometry_digest,
            state.scene_digest,
            state.density_warnings,
            state.diagnostic_digest,
        ) = self.frame_facts;
        if frame.is_some() {
            state.prepared_menu_digest = prepared_menu.map_or(0, digest);
            state.prepared_owner_session = session.editor_session.0;
            if let Some(prepared) = prepared {
                state.prepared_candidate_digest = prepared.candidate;
                state.prepared_content_key = prepared.key;
                state.prepared_token_digest = prepared.token;
                state.prepared_owner_session = prepared.session;
            }
        }
        state.catalog_count = BUILTIN_SKINS.len();
        state.gallery_active = self.active;
        state.requested = self.requested;
        state.completed = self.completed;
        state.rejected = self.rejected;
        state.in_flight = usize::from(self.pending.is_some());
        state.visible_pending = self.visible_pending;
        state.cached_tiles = self.cache.len();
        state.cpu_bytes = self.cache.values().map(|tile| tile.bytes).sum();
        state.gpu_bytes = self
            .cache
            .values()
            .filter(|tile| tile.texture.is_some())
            .map(|tile| tile.bytes)
            .sum();
        state.failure_count = self
            .cache
            .values()
            .filter(|tile| tile.result.is_err())
            .count();
        let selected_key = self.choice.as_ref().map(|choice| choice.key).or_else(|| {
            self.entries
                .iter()
                .find(|entry| entry.builtin == usize::try_from(state.current_builtin).ok())
                .map(|entry| entry.key)
        });
        if let Some(choice) = &self.choice {
            state.preview_active = true;
            state.preview_key = digest(&choice.key);
            state.preview_candidate_digest = choice.candidate_digest;
            state.preview_token_digest = digest(&choice.token);
            state.preview_builtin = choice.builtin.map_or(-1, |i| i as i32);
        }
        if let Some((key, tile)) = selected_key
            .and_then(|key| self.cache.get(&key).map(|tile| (key, tile)))
            .filter(|(_, tile)| tile.result.is_ok())
        {
            if let Ok(tile) = &tile.result {
                state.tile_key = digest(&key);
                state.tile_scene_digest = tile.scene_digest;
                state.tile_geometry_digest = tile.geometry_digest;
                state.tile_selected_emphasis = tile.selected_emphasis;
            }
        }
        state
    }
    pub(super) fn retire_interest(&mut self) {
        self.interest_token
            .store(false, std::sync::atomic::Ordering::Release);
        // Cancellation terminalizes this separate gallery slot locally. The
        // worker/mailbox may suppress obsolete replies; ordinary embedded and
        // native pending requests retain their existing owners.
        self.pending = None;
        self.visible_pending = 0;
        if self.active {
            self.interest = self.interest.wrapping_add(1);
            self.active = false;
            self.source = None;
        }
    }
    pub(super) fn dispose(&mut self) {
        self.retire_interest();
        self.cache.clear();
        self.entries.clear();
        self.choice = None;
        self.destination = None;
        self.pending = None;
        self.observed_base = None;
        self.observed_frame = None;
        self.frame_facts = Default::default();
    }
    pub(super) fn invalidate_resources(&mut self) {
        self.resource_revision = self.resource_revision.wrapping_add(1);
        self.interest_token
            .store(false, std::sync::atomic::Ordering::Release);
        self.source = None;
        self.pending = None;
        self.visible_pending = 0;
        self.cache.clear();
    }
    fn sync(&mut self, session: &RadialAuthoringSession, dpi: f32) {
        if let Some(menu) = selected_menu_id(session) {
            self.destination = Some(menu);
        }
        self.destination = self
            .destination
            .take()
            .filter(|id| session.draft.menus.iter().any(|menu| &menu.id == id));
        if self
            .choice
            .as_ref()
            .is_some_and(|choice| Some(&choice.destination) != self.destination.as_ref())
        {
            self.choice = None;
            self.feedback =
                Some("Destination changed; choose the appearance for this menu again.".into());
        }
        let dpi = (dpi * 1000.0).round().clamp(1.0, 4000.0) as u32;
        let source = (
            session.editor_session,
            session.generation,
            dpi,
            self.resource_revision,
        );
        self.active = true;
        if self.source == Some(source) {
            return;
        }
        self.source = Some(source);
        self.interest = self.interest.wrapping_add(1);
        self.interest_token
            .store(false, std::sync::atomic::Ordering::Release);
        self.pending = None;
        self.interest_token = Arc::new(std::sync::atomic::AtomicBool::new(true));
        if self.choice.as_ref().is_some_and(|choice| {
            choice.session != session.editor_session || choice.generation != session.generation
        }) {
            self.choice = None;
            self.feedback = Some("Draft changed; choose the appearance again before Apply.".into());
        }
        let mut skins: Vec<_> = BUILTIN_SKINS
            .iter()
            .enumerate()
            .map(|(index, preset)| (preset.definition(), Some(index)))
            .collect();
        skins.extend(session.draft.skins.iter().cloned().map(|skin| (skin, None)));
        self.entries = skins
            .into_iter()
            .map(|(skin, builtin)| {
                let key = content_key(
                    &skin,
                    &session.draft,
                    &session.pending_assets,
                    dpi,
                    self.resource_revision,
                );
                Entry { skin, builtin, key }
            })
            .collect();
        // Keep the chosen logical tile distinct from its shared pixel key.
        // Resource/DPI changes rebuild that exact choice and its preparation
        // token; a cached frame from the previous resource epoch cannot Apply.
        let refresh = self.choice.as_ref().and_then(|choice| {
            self.entries.iter().position(|entry| {
                entry.builtin == choice.builtin
                    && entry.skin == choice.skin
                    && entry.key != choice.key
            })
        });
        if let Some(index) = refresh
            && let Err(error) = self.choose(session, index)
        {
            self.choice = None;
            self.feedback = Some(error);
        }
    }
    pub(super) fn accept(&mut self, reply: GalleryReply, session: &RadialAuthoringSession) -> bool {
        if self.pending != Some(reply.correlation) {
            self.rejected += 1;
            return false;
        }
        self.pending = None;
        let c = reply.correlation;
        if !self.active
            || c.editor_session != session.editor_session
            || c.draft_generation != session.generation
            || c.interest_generation != self.interest
            || !self.entries.iter().any(|entry| entry.key == c.key)
        {
            self.rejected += 1;
            return false;
        }
        let mut bytes = reply
            .result
            .as_ref()
            .map_or(0, |tile| tile.raster.image.as_raw().len());
        let result = if bytes > MAX_GALLERY_BYTES {
            bytes = 0;
            Err("thumbnail exceeds gallery pixel budget".into())
        } else {
            reply.result
        };
        self.clock += 1;
        self.cache.insert(
            c.key,
            CachedTile {
                result,
                texture: None,
                bytes,
                touched: self.clock,
            },
        );
        self.completed += 1;
        self.evict();
        true
    }
    fn evict(&mut self) {
        while self.cache.len() > MAX_GALLERY_ENTRIES
            || self.cache.values().map(|tile| tile.bytes).sum::<usize>() > MAX_GALLERY_BYTES
        {
            let Some(key) = self
                .cache
                .iter()
                .min_by_key(|(key, tile)| (tile.touched, **key))
                .map(|(key, _)| *key)
            else {
                break;
            };
            self.cache.remove(&key);
        }
    }
    pub(super) fn candidate(
        &self,
        session: &RadialAuthoringSession,
    ) -> Option<(Arc<RadialDocument>, String, StableSelection)> {
        let choice = self.choice.as_ref()?;
        if choice.session != session.editor_session || choice.generation != session.generation {
            return None;
        }
        if selected_menu_id(session)
            .as_ref()
            .is_some_and(|menu| menu != &choice.destination)
            || self.destination.as_ref() != Some(&choice.destination)
        {
            return None;
        }
        Some((
            Arc::clone(&choice.candidate),
            choice.token.clone(),
            StableSelection::Menu(choice.destination.clone()),
        ))
    }
    fn apply(&mut self, session: &mut RadialAuthoringSession) -> Result<(), String> {
        let choice = self.choice.as_ref().ok_or("choose an appearance first")?;
        if choice.session != session.editor_session || choice.generation != session.generation {
            return Err("appearance preview is stale".into());
        }
        if selected_menu_id(session)
            .as_ref()
            .is_some_and(|menu| menu != &choice.destination)
            || self.destination.as_ref() != Some(&choice.destination)
        {
            return Err("appearance destination changed".into());
        }
        let candidate = preset_candidate(&session.draft, &choice.destination, &choice.skin)?;
        crate::radial::validation::validate(&candidate).map_err(|error| format!("{error:?}"))?;
        session
            .replace_document_atomic(candidate)
            .map_err(|error| format!("{error:?}"))?;
        self.choice = None;
        self.source = None;
        Ok(())
    }
    fn choose(&mut self, session: &RadialAuthoringSession, index: usize) -> Result<(), String> {
        let destination = self
            .destination
            .clone()
            .ok_or("select a destination menu first")?;
        let entry = self
            .entries
            .get(index)
            .ok_or("appearance tile no longer exists")?;
        let candidate = preset_candidate(&session.draft, &destination, &entry.skin)?;
        crate::radial::validation::validate(&candidate).map_err(|error| format!("{error:?}"))?;
        let token = format!("appearance:{destination}:{:?}", entry.key);
        let candidate_digest = serde_json::to_vec(&candidate)
            .map(|bytes| acceptance_observation_digest(&bytes))
            .map_err(|error| error.to_string())?;
        self.choice = Some(PreviewChoice {
            skin: entry.skin.clone(),
            builtin: entry.builtin,
            session: session.editor_session,
            generation: session.generation,
            destination,
            key: entry.key,
            candidate: Arc::new(candidate),
            token,
            candidate_digest,
        });
        self.feedback = None;
        Ok(())
    }
    fn request_visible(
        &mut self,
        visible: &[usize],
        session: &RadialAuthoringSession,
        client: Option<&AuthoringClient>,
    ) {
        let mut demand = Vec::new();
        for entry in visible.iter().filter_map(|index| self.entries.get(*index)) {
            if !demand.contains(&entry.key) && demand.len() < MAX_GALLERY_ENTRIES {
                demand.push(entry.key);
            }
        }
        self.visible_pending = demand
            .iter()
            .filter(|key| !self.cache.contains_key(key))
            .count();
        if self.pending.is_some() || !self.active {
            return;
        }
        let Some(entry) = demand
            .iter()
            .find(|key| !self.cache.contains_key(key))
            .and_then(|key| self.entries.iter().find(|entry| entry.key == *key))
        else {
            return;
        };
        let Some(client) = client else {
            return;
        };
        self.next_request = self.next_request.wrapping_add(1);
        let c = GalleryCorrelation {
            request_id: self.next_request,
            editor_session: session.editor_session,
            draft_generation: session.generation,
            interest_generation: self.interest,
            key: entry.key,
        };
        let assets = match crate::radial::assets::ManagedAssetOverlay::validated(
            session
                .pending_assets
                .additions
                .iter()
                .map(|addition| (addition.record.clone(), Arc::clone(&addition.bytes))),
        ) {
            Ok(assets) => assets,
            Err(error) => {
                self.feedback = Some(format!("Invalid staged gallery assets: {error}"));
                return;
            }
        };
        let request = GalleryRequest {
            correlation: c,
            document: Arc::new(demonstration(entry.skin.clone(), &session.draft)),
            projection: crate::radial::preparation::PreviewProjection {
                assets,
                ..Default::default()
            },
            interest: Arc::clone(&self.interest_token),
        };
        match client.try_send_gallery(request) {
            Ok(()) => {
                self.pending = Some(c);
                self.requested += 1;
            }
            Err(error) => self.feedback = Some(format!("Gallery service unavailable: {error}")),
        }
    }
    pub(super) fn ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut RadialAuthoringSession,
        client: Option<&AuthoringClient>,
        viewport: ViewportClass,
        prepared: Option<PreparedAppearance>,
    ) {
        self.sync(session, ui.ctx().pixels_per_point());
        ui.heading("Appearance gallery");
        if let Some(id) = &self.destination {
            let name = session
                .draft
                .menus
                .iter()
                .find(|menu| &menu.id == id)
                .map(|menu| menu.name.as_str())
                .unwrap_or("missing");
            ui.label(format!("Apply to current menu: {name}"));
        } else {
            ui.label("Select a menu in Design to set the destination.");
        }
        ui.small("Fixed demonstration, shared radial renderer. Menu/ring/cell overrides can change the actual menu. Document skin copies are Custom.");
        let mut visible = Vec::new();
        let columns = ((ui.available_width() / 155.0).floor() as usize).clamp(1, 4);
        let entries = self.entries.clone();
        egui::Grid::new("radial-appearance-gallery")
            .num_columns(columns)
            .show(ui, |ui| {
                for (index, entry) in entries.iter().enumerate() {
                    ui.vertical(|ui| {
                        let response = ui.add(egui::SelectableLabel::new(
                            self.choice.as_ref().is_some_and(|choice| {
                                choice.key == entry.key
                                    && choice.builtin == entry.builtin
                                    && choice.skin.id == entry.skin.id
                            }),
                            &entry.skin.name,
                        ));
                        trace_designer_authoring_control(
                            ui,
                            &response,
                            DesignerAuthoringTarget::AppearanceTile,
                            DesignerAuthoringRole::Selectable,
                            Some(index),
                            self.destination.is_some(),
                            session
                                .draft
                                .menus
                                .iter()
                                .find(|menu| Some(&menu.id) == self.destination.as_ref())
                                .is_some_and(|menu| menu.skin_id == entry.skin.id),
                            viewport,
                            trace_correlation(Some(session)),
                        );
                        ui.small(if entry.builtin.is_some() {
                            "Built-in"
                        } else {
                            "Custom authored copy"
                        });
                        let rect = ui
                            .allocate_exact_size(egui::vec2(140.0, 120.0), egui::Sense::hover())
                            .0;
                        if ui.is_rect_visible(rect) {
                            visible.push(index);
                            if let Some(tile) = self.cache.get_mut(&entry.key) {
                                self.clock += 1;
                                tile.touched = self.clock;
                                if let Ok(prepared) = &tile.result {
                                    if tile.texture.is_none() {
                                        let pixels = &prepared.raster.image;
                                        tile.texture = Some(ui.ctx().load_texture(
                                            format!("radial-gallery-{:?}", entry.key),
                                            egui::ColorImage::from_rgba_unmultiplied(
                                                [pixels.width() as usize, pixels.height() as usize],
                                                pixels.as_raw(),
                                            ),
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                    if let Some(texture) = &tile.texture {
                                        ui.painter().image(
                                            texture.id(),
                                            rect,
                                            egui::Rect::from_min_max(
                                                egui::Pos2::ZERO,
                                                egui::pos2(1.0, 1.0),
                                            ),
                                            egui::Color32::WHITE,
                                        );
                                    }
                                    if let Some(reason) = &prepared.diagnostic_summary {
                                        ui.small(format!("Preview fallback: {reason}"));
                                    }
                                } else if let Err(error) = &tile.result {
                                    ui.put(
                                        rect,
                                        egui::Label::new(format!("Preview unavailable: {error}")),
                                    );
                                }
                            } else {
                                ui.put(rect, egui::Label::new("Preview pending"));
                            }
                        }
                        if response.clicked()
                            && let Err(error) = self.choose(session, index)
                        {
                            self.feedback = Some(error);
                        }
                    });
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        self.request_visible(&visible, session, client);
        ui.horizontal(|ui| {
            let can_apply = self.choice_is_prepared(prepared);
            let response = ui.add_enabled(can_apply, egui::Button::new("Apply appearance"));
            trace_designer_authoring_control(
                ui,
                &response,
                DesignerAuthoringTarget::AppearanceApply,
                DesignerAuthoringRole::Button,
                None,
                can_apply,
                false,
                viewport,
                trace_correlation(Some(session)),
            );
            if response.clicked()
                && let Err(error) = self.apply(session)
            {
                self.feedback = Some(error);
            }
            let response = ui.add_enabled(
                self.choice.is_some(),
                egui::Button::new("Cancel appearance preview"),
            );
            trace_designer_authoring_control(
                ui,
                &response,
                DesignerAuthoringTarget::AppearanceCancel,
                DesignerAuthoringRole::Button,
                None,
                self.choice.is_some(),
                false,
                viewport,
                trace_correlation(Some(session)),
            );
            if response.clicked() {
                self.choice = None;
                self.feedback = None;
            }
        });
        if let Some(feedback) = &self.feedback {
            ui.colored_label(ui.visuals().error_fg_color, feedback);
        }
        if let Some(destination) = self.destination.clone() {
            self.simple_ui(ui, session, &destination, viewport);
        }
    }

    fn simple_ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut RadialAuthoringSession,
        destination: &MenuId,
        viewport: ViewportClass,
    ) {
        let Ok(state) = simple_state(&session.draft, destination) else {
            return;
        };
        ui.separator();
        ui.heading("Simple appearance · current menu");
        ui.small("Accent changes label RGB. Opacity changes label alpha, icons, glow and decorative image channels; fallback wheel fills stay opaque. Radial spacing changes effective radius geometry.");
        let mut edit = None;
        let mut key = "";
        let mut phase = EditPhase::Atomic;
        macro_rules! control {
            ($target:ident,$label:expr,$slot:expr,$range:expr,$variant:ident,$key:expr) => {{
                if let Ok(mut v) = value($slot) {
                    ui.label(format!("{}: {:.2}", $label, v));
                    let response = ui.add(egui::Slider::new(&mut v, $range).show_value(false));
                    trace_designer_authoring_control(
                        ui,
                        &response,
                        DesignerAuthoringTarget::$target,
                        DesignerAuthoringRole::DragValue,
                        None,
                        true,
                        false,
                        viewport,
                        trace_correlation(Some(session)),
                    );
                    if response.changed() || response.drag_stopped() {
                        edit = Some(SimpleEdit::$variant(v));
                        key = $key;
                        phase = widget_edit_phase(&response);
                    }
                    if step_buttons(
                        ui,
                        &mut v,
                        $range,
                        DesignerAuthoringTarget::$target,
                        viewport,
                        session,
                    ) {
                        edit = Some(SimpleEdit::$variant(v));
                        key = $key;
                        phase = EditPhase::Atomic;
                    }
                }
            }};
        }
        if let Ok(color) = value(&state.values.text.color) {
            let mut rgb = [color.red, color.green, color.blue];
            let response = ui.color_edit_button_srgb(&mut rgb);
            ui.label("Accent (labels)");
            trace_designer_authoring_control(
                ui,
                &response,
                DesignerAuthoringTarget::SimpleAccent,
                DesignerAuthoringRole::Button,
                None,
                true,
                false,
                viewport,
                trace_correlation(Some(session)),
            );
            if response.changed() {
                edit = Some(SimpleEdit::Accent(rgb));
                key = "accent";
                phase = widget_edit_phase(&response);
            }
        }
        let mut opacity = state.opacity.unwrap_or(1.0);
        ui.label(if state.opacity.is_none() {
            "Opacity · Mixed".into()
        } else {
            format!(
                "Opacity (labels, icons, decoration): {:.0}%",
                opacity * 100.0
            )
        });
        let response = ui.add(egui::Slider::new(&mut opacity, 0.0..=1.0).show_value(false));
        trace_designer_authoring_control(
            ui,
            &response,
            DesignerAuthoringTarget::SimpleOpacity,
            DesignerAuthoringRole::DragValue,
            None,
            true,
            false,
            viewport,
            trace_correlation(Some(session)),
        );
        if response.changed() || response.drag_stopped() {
            edit = Some(SimpleEdit::Opacity(opacity));
            key = "opacity";
            phase = widget_edit_phase(&response);
        }
        if step_buttons(
            ui,
            &mut opacity,
            0.0..=1.0,
            DesignerAuthoringTarget::SimpleOpacity,
            viewport,
            session,
        ) {
            edit = Some(SimpleEdit::Opacity(opacity));
            key = "opacity";
            phase = EditPhase::Atomic;
        }
        control!(
            SimpleScale,
            "Menu scale",
            &state.values.geometry.menu_scale,
            0.55..=2.0,
            Scale,
            "scale"
        );
        control!(
            SimpleSpacing,
            "Radial spacing",
            &state.values.geometry.radius_scale,
            0.55..=2.0,
            Spacing,
            "spacing"
        );
        control!(
            SimpleLabelSize,
            "Label size",
            &state.values.text.font_size,
            8.0..=32.0,
            LabelSize,
            "label-size"
        );
        for (target, label, slot, kind) in [
            (
                DesignerAuthoringTarget::SimpleLabels,
                "Labels visible",
                &state.values.text.visible,
                0,
            ),
            (
                DesignerAuthoringTarget::SimpleBold,
                "Bold labels",
                &state.values.text.bold,
                1,
            ),
            (
                DesignerAuthoringTarget::SimpleShadow,
                "Label shadow",
                &state.values.text.shadow_enabled,
                2,
            ),
        ] {
            if let Ok(mut v) = value(slot) {
                let response = ui.checkbox(&mut v, label);
                trace_designer_authoring_control(
                    ui,
                    &response,
                    target,
                    DesignerAuthoringRole::Checkbox,
                    None,
                    true,
                    v,
                    viewport,
                    trace_correlation(Some(session)),
                );
                if response.changed() {
                    edit = Some(match kind {
                        0 => SimpleEdit::Labels(v),
                        1 => SimpleEdit::Bold(v),
                        _ => SimpleEdit::Shadow(v),
                    });
                    key = label;
                }
            }
        }
        egui::CollapsingHeader::new("Inheritance and explicit resets").show(ui, |ui| {
            ui.checkbox(
                &mut self.reset_descendants,
                "Also reset masking ring/cell overrides (explicit opt-in)",
            );
            for control in [
                SimpleControl::Accent,
                SimpleControl::Opacity,
                SimpleControl::Scale,
                SimpleControl::Spacing,
                SimpleControl::LabelSize,
                SimpleControl::Labels,
                SimpleControl::Bold,
                SimpleControl::Shadow,
            ] {
                let sources: Vec<_> = control
                    .fields()
                    .iter()
                    .filter_map(|field| state.provenance.get(field))
                    .map(|source| source_description(source, &session.draft))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect();
                ui.label(format!("{control:?}: {}", sources.join(", ")));
                let (masked, overflow) = mask_descriptions(&state, control, &session.draft);
                for owner in masked {
                    ui.small(format!("Masked by {owner}"));
                }
                if overflow > 0 {
                    ui.small(format!("and {overflow} more masking owners"));
                }
                if ui
                    .button(format!("Reset {control:?} to Inherit at menu scope"))
                    .clicked()
                {
                    edit = Some(SimpleEdit::Reset {
                        control,
                        descendants: self.reset_descendants,
                    });
                    key = "reset";
                }
            }
        });
        if let Some(edit) = edit {
            match simple_candidate(&session.draft, destination, &edit).and_then(|candidate| {
                session
                    .replace_document_edit(
                        candidate,
                        EditKey {
                            entity: format!("appearance:{destination}"),
                            field: key.into(),
                        },
                        phase,
                    )
                    .map_err(|error| format!("{error:?}"))
            }) {
                Ok(()) => {
                    self.choice = None;
                    self.source = None;
                    self.feedback = None;
                }
                Err(error) => self.feedback = Some(error),
            }
        }
    }
}
