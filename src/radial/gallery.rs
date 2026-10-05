//! Fixed, execution-free gallery inputs and bounded service transport.

use super::{
    authoring::{AuthoringSessionId, DraftGeneration},
    model::*,
    preparation::PreviewProjection,
};
use std::sync::Arc;

pub const DEMO_VERSION: u32 = 1;
pub const TILE_LOGICAL_SIZE: u32 = 360;
pub const MAX_GALLERY_ENTRIES: usize = 12;
pub const MAX_GALLERY_BYTES: usize = 8 * 1024 * 1024;

/// Payload-free facts from the actual appearance owners, used by native
/// qualification. Digests describe content; counters describe bounded work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceObservation {
    /// Filled from the enclosing trace envelope by the acceptance reader.
    pub trace_sequence: u64,
    pub session_id: u64,
    pub generation: u64,
    pub menu_digest: u64,
    pub catalog_count: usize,
    pub current_builtin: i32,
    pub preview_builtin: i32,
    pub preview_active: bool,
    pub preview_key: u64,
    pub preview_candidate_digest: u64,
    pub preview_token_digest: u64,
    pub prepared_menu_digest: u64,
    pub prepared_candidate_digest: u64,
    pub prepared_content_key: u64,
    pub prepared_token_digest: u64,
    pub prepared_owner_session: u64,
    pub effective_digest: u64,
    pub bindings_digest: u64,
    pub other_menus_digest: u64,
    pub raw_overrides_digest: u64,
    pub opacity_milli: i32,
    pub scale_milli: i32,
    pub spacing_milli: i32,
    pub label_size_milli: i32,
    pub labels_visible: bool,
    pub masked_fields: usize,
    pub gallery_active: bool,
    pub requested: u64,
    pub completed: u64,
    pub rejected: u64,
    pub in_flight: usize,
    pub visible_pending: usize,
    pub cached_tiles: usize,
    pub cpu_bytes: usize,
    pub gpu_bytes: usize,
    pub failure_count: usize,
    pub tile_key: u64,
    pub tile_scene_digest: u64,
    pub tile_geometry_digest: u64,
    pub tile_selected_emphasis: bool,
    pub prepared_generation: u64,
    pub geometry_digest: u64,
    pub scene_digest: u64,
    pub authored_cell_count: usize,
    pub density_warnings: usize,
    pub diagnostic_digest: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAppearanceTarget {
    pub session_id: u64,
    pub generation: u64,
    pub menu_digest: u64,
    pub cell_digest: u64,
    pub geometry_digest: u64,
    pub point_x: i32,
    pub point_y: i32,
    pub dpi_milli: u32,
    pub work_area: [i32; 4],
    pub kind: u8,
    pub clicked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GalleryContentKey {
    pub appearance: u64,
    pub assets: u64,
    pub resource_revision: u64,
    pub dpi_milli: u32,
    pub geometry: u32,
    pub demo_version: u32,
}

/// Change-time fingerprint. Names and unrelated assets are deliberately
/// excluded; declared managed dependencies include their staged versions.
pub fn content_key(
    skin: &SkinDefinition,
    document: &RadialDocument,
    staged: &super::authoring::AssetMutations,
    dpi_milli: u32,
    resource_revision: u64,
) -> GalleryContentKey {
    use std::hash::{Hash, Hasher};
    let hash = |value: &dyn std::fmt::Debug| {
        let mut h = std::hash::DefaultHasher::new();
        format!("{value:?}").hash(&mut h);
        h.finish()
    };
    let mut style = skin.style.clone();
    style.values.sounds = SoundStyleOverrides::default();
    let mut defaults = document.user_style_defaults.clone();
    defaults.values.sounds = SoundStyleOverrides::default();
    let mut ids = std::collections::BTreeSet::new();
    for images in [&style.values.images, &defaults.values.images] {
        for slot in [
            &images.item_glow,
            &images.menu_outer_rim,
            &images.menu_background,
            &images.item_background,
            &images.item_foreground,
            &images.item_shadow,
            &images.menu_foreground,
            &images.center_background,
            &images.center_image,
            &images.submenu_indicator,
        ] {
            if let Override::Value(MediaReference::Managed { asset_id }) = slot {
                ids.insert(asset_id.clone());
            }
        }
    }
    GalleryContentKey {
        appearance: hash(&(&style, &defaults)),
        assets: hash(&(
            document
                .assets
                .iter()
                .filter(|record| ids.contains(&record.id))
                .collect::<Vec<_>>(),
            staged
                .additions
                .iter()
                .filter(|asset| ids.contains(&asset.record.id))
                .map(|asset| (&asset.record, asset.bytes.len()))
                .collect::<Vec<_>>(),
            staged
                .deletions
                .iter()
                .filter(|id| ids.contains(id))
                .collect::<Vec<_>>(),
            &document.media_search_roots.image_directories,
        )),
        resource_revision,
        dpi_milli,
        geometry: TILE_LOGICAL_SIZE,
        demo_version: DEMO_VERSION,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GalleryCorrelation {
    pub request_id: u64,
    pub editor_session: AuthoringSessionId,
    pub draft_generation: DraftGeneration,
    pub interest_generation: u64,
    pub key: GalleryContentKey,
}

#[derive(Clone, Debug)]
pub struct GalleryRequest {
    pub correlation: GalleryCorrelation,
    pub document: Arc<RadialDocument>,
    pub projection: PreviewProjection,
    /// Retired on tab/session/resource changes before queued cold work starts.
    pub interest: Arc<std::sync::atomic::AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct GalleryTile {
    pub raster: Arc<super::compositor::RasterFrame>,
    pub scene_digest: u64,
    pub geometry_digest: u64,
    pub selected_emphasis: bool,
    pub diagnostic_count: usize,
    pub diagnostic_summary: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GalleryReply {
    pub correlation: GalleryCorrelation,
    pub result: Result<GalleryTile, String>,
}

enum GalleryWork {
    Prepare(GalleryRequest),
    Invalidate,
}
type GalleryRenderer =
    Box<dyn FnMut(Option<&GalleryRequest>, bool) -> Result<Option<GalleryTile>, String> + Send>;

/// One bounded cold-work owner. A legal decoder/font discovery never blocks
/// the application's input/control owner, and retired work cannot publish pixels.
pub struct GalleryPreparationService {
    tx: std::sync::mpsc::SyncSender<GalleryWork>,
    resource_epoch: Arc<std::sync::atomic::AtomicU64>,
    stopping: Arc<std::sync::atomic::AtomicBool>,
}
impl GalleryPreparationService {
    pub fn new(
        application_data: std::path::PathBuf,
        reply: Arc<dyn Fn(GalleryReply) + Send + Sync>,
    ) -> std::io::Result<Self> {
        let mut preparer = super::preparation::PreviewFramePreparer::new(application_data);
        let mut compositor = super::compositor::CompositorCache::with_budget(MAX_GALLERY_BYTES);
        Self::spawn(
            reply,
            Box::new(move |request, reset| {
                if reset {
                    preparer.release();
                    compositor = super::compositor::CompositorCache::with_budget(MAX_GALLERY_BYTES);
                }
                request
                    .map(|request| prepare_tile(&mut preparer, &mut compositor, request))
                    .transpose()
            }),
        )
    }
    fn spawn(
        reply: Arc<dyn Fn(GalleryReply) + Send + Sync>,
        mut renderer: GalleryRenderer,
    ) -> std::io::Result<Self> {
        use std::sync::atomic::Ordering;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let resource_epoch = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let epoch = Arc::clone(&resource_epoch);
        let stop = Arc::clone(&stopping);
        std::thread::Builder::new()
            .name("radial-gallery".into())
            .spawn(move || {
                let mut applied = 0;
                while let Ok(work) = rx.recv() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let version = epoch.load(Ordering::Acquire);
                    let reset = version != applied;
                    applied = version;
                    match work {
                        GalleryWork::Invalidate => {
                            let _ = renderer(None, reset);
                        }
                        GalleryWork::Prepare(request) => {
                            let result = if !request.interest.load(Ordering::Acquire) {
                                if reset {
                                    let _ = renderer(None, true);
                                }
                                Err("gallery interest was retired".into())
                            } else {
                                renderer(Some(&request), reset).and_then(|tile| {
                                    tile.ok_or_else(|| "gallery renderer returned no tile".into())
                                })
                            };
                            let result = if !request.interest.load(Ordering::Acquire)
                                || epoch.load(Ordering::Acquire) != version
                            {
                                Err("gallery interest or resources changed during preparation"
                                    .into())
                            } else {
                                result
                            };
                            if !stop.load(Ordering::Acquire)
                                && request.interest.load(Ordering::Acquire)
                            {
                                reply(GalleryReply {
                                    correlation: request.correlation,
                                    result,
                                });
                            }
                        }
                    }
                }
            })?;
        Ok(Self {
            tx,
            resource_epoch,
            stopping,
        })
    }
    pub fn try_prepare(&self, request: GalleryRequest) -> Result<(), String> {
        self.tx
            .try_send(GalleryWork::Prepare(request))
            .map_err(|error| error.to_string())
    }
    pub fn invalidate_resources(&self) {
        self.resource_epoch
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        // A queued request also notices the epoch; invalidation never waits for
        // a decoder or occupies an unbounded control queue.
        let _ = self.tx.try_send(GalleryWork::Invalidate);
    }
}
impl Drop for GalleryPreparationService {
    fn drop(&mut self) {
        self.stopping
            .store(true, std::sync::atomic::Ordering::Release);
        let _ = self.tx.try_send(GalleryWork::Invalidate);
        // No UI/control owner joins a decoder. Its bounded in-progress work
        // observes stopping before publication, then drops all resources.
    }
}

fn prepare_tile(
    preparer: &mut super::preparation::PreviewFramePreparer,
    compositor: &mut super::compositor::CompositorCache,
    request: &GalleryRequest,
) -> Result<GalleryTile, String> {
    use super::geometry::*;
    use super::render::VectorPrimitive;
    use std::hash::{Hash, Hasher};
    let scale = ScaleFactor::new(request.correlation.key.dpi_milli as f64 / 1000.0)
        .ok_or("invalid gallery DPI")?;
    let size = request.correlation.key.geometry as f64 * scale.get();
    let input = preparer.prepare(
        &request.document,
        &request.document.default_menu_id,
        PhysicalPoint {
            x: size * 0.5,
            y: size * 0.5,
        },
        PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: size, y: size },
        },
        scale,
        request.correlation.request_id,
        Some(&CellId::new("gallery-0")),
        &request.projection,
    )?;
    if !request.interest.load(std::sync::atomic::Ordering::Acquire) {
        return Err("gallery interest was retired".into());
    }
    let digest = |value: &str| {
        let mut hash = std::hash::DefaultHasher::new();
        value.hash(&mut hash);
        hash.finish()
    };
    let selected_emphasis=input.scene.primitives.iter().any(|primitive|matches!(primitive,VectorPrimitive::Text{text,bold:true,underline:true,..} if text=="Home"));
    // Fixed raster resolution bounds all twelve retained tiles at every DPI.
    let raster = compositor
        .compose(&input.scene, ScaleFactor::new(1.0).unwrap_or(scale), 0)
        .map_err(|error| format!("gallery composition failed: {error:?}"))?;
    Ok(GalleryTile {
        raster,
        scene_digest: digest(&format!("{:?}", input.scene.primitives)),
        geometry_digest: digest(&format!("{:?}", input.layout)),
        selected_emphasis,
        diagnostic_count: input.diagnostics.len(),
        diagnostic_summary: input
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.severity != super::diagnostics::RadialDiagnosticSeverity::Info
            })
            .map(|diagnostic| diagnostic.message.chars().take(160).collect()),
    })
}

/// Only these six fixed labels/controls and a safe submenu exist. No user's
/// action graph, providers, shortcuts, sounds or history enter the fixture.
pub fn demonstration(skin: SkinDefinition, document: &RadialDocument) -> RadialDocument {
    let mut demo = RadialDocument::starter();
    let mut menu = demo.menus[0].clone();
    menu.id = MenuId::new("gallery-demo");
    menu.name = "Skin demonstration".into();
    menu.skin_id = skin.id.clone();
    menu.style = MenuStyleLayer::default();
    menu.center_action = None;
    menu.center_secondary_action = None;
    menu.background_action = None;
    menu.background_secondary_action = None;
    menu.center_control = Some(Control::Back);
    menu.center_secondary_control = None;
    menu.background_control = None;
    menu.background_secondary_control = None;
    menu.rings.truncate(1);
    let template = menu.rings[0].cells[0].clone();
    menu.rings[0].id = RingId::new("gallery-ring");
    menu.rings[0].cells = ["Home", "Tools", "Notes", "Window", "More", "Close"]
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let mut cell = template.clone();
            cell.id = CellId::new(format!("gallery-{index}"));
            cell.label = (*label).into();
            cell.content = if index == 4 {
                CellContent::Submenu {
                    menu_id: MenuId::new("gallery-child"),
                }
            } else {
                CellContent::Control {
                    control: Control::Close,
                }
            };
            cell.alternate_clicks.clear();
            cell.alternate_controls.clear();
            cell.shortcuts.clear();
            cell.hotstrings.clear();
            cell.icon = Override::Clear;
            cell.tooltip = Override::Clear;
            cell.style = CellStyleLayer::default();
            cell
        })
        .collect();
    let mut child = menu.clone();
    child.id = MenuId::new("gallery-child");
    child.rings[0].cells.truncate(1);
    child.rings[0].id = RingId::new("gallery-child-ring");
    child.rings[0].cells[0].id = CellId::new("gallery-child-cell");
    demo.default_menu_id = menu.id.clone();
    demo.menus = vec![menu, child];
    demo.skins = vec![skin];
    demo.user_style_defaults = document.user_style_defaults.clone();
    // Sound/window behavior is outside appearance. The fixture does not load
    // or audition sound dependencies, even if an authored skin declares them.
    demo.user_style_defaults.values.sounds = SoundStyleOverrides::default();
    demo.skins[0].style.values.sounds = SoundStyleOverrides::default();
    demo.assets = document.assets.clone();
    demo.media_search_roots = document.media_search_roots.clone();
    demo.context_rules.clear();
    demo.custom_triggers.clear();
    demo
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_cold_work_keeps_control_nonblocking_and_retires_queued_work_with_bounded_mailboxes()
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (reply_tx, reply_rx) = mpsc::sync_channel(2);
        let (reset_tx, reset_rx) = mpsc::sync_channel(1);
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_calls = Arc::clone(&calls);
        let service = GalleryPreparationService::spawn(
            Arc::new(move |reply| reply_tx.send(reply).unwrap()),
            Box::new(move |request, reset| {
                if request.is_some() {
                    worker_calls.fetch_add(1, Ordering::AcqRel);
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                } else if reset {
                    reset_tx.send(()).unwrap();
                }
                Err("bounded decoder fixture".into())
            }),
        )
        .unwrap();
        let document = Arc::new(RadialDocument::starter());
        let key = content_key(&document.skins[0], &document, &Default::default(), 1000, 0);
        let request = |id, interest| GalleryRequest {
            correlation: GalleryCorrelation {
                request_id: id,
                editor_session: AuthoringSessionId(1),
                draft_generation: DraftGeneration(1),
                interest_generation: 1,
                key,
            },
            document: Arc::clone(&document),
            projection: PreviewProjection::default(),
            interest,
        };
        let first = Arc::new(AtomicBool::new(true));
        service.try_prepare(request(1, Arc::clone(&first))).unwrap();
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let queued = Arc::new(AtomicBool::new(true));
        // These control-owner operations complete while the decoder is still
        // waiting for our explicit release, with one active and one queued job.
        service
            .try_prepare(request(2, Arc::clone(&queued)))
            .unwrap();
        assert!(
            service
                .try_prepare(request(3, Arc::new(AtomicBool::new(true))))
                .is_err()
        );
        service.invalidate_resources();
        queued.store(false, Ordering::Release);
        release_tx.send(()).unwrap();
        let first_reply = reply_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(first_reply.correlation.request_id, 1);
        assert!(
            first_reply
                .result
                .unwrap_err()
                .contains("resources changed")
        );
        reset_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert!(reply_rx.try_recv().is_err());
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }
    #[test]
    fn blocked_retired_work_cannot_replace_reopened_interest_dispatch_rejection() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        };
        let (client, endpoint) = super::super::authoring::authoring_control_service();
        let sink = endpoint.gallery_reply_sink();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (drained_tx, drained_rx) = mpsc::sync_channel(1);
        let publications = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let worker_publications = Arc::clone(&publications);
        let worker_sink = Arc::clone(&sink);
        let service = GalleryPreparationService::spawn(
            Arc::new(move |reply| {
                worker_publications.fetch_add(1, Ordering::AcqRel);
                worker_sink(reply);
            }),
            Box::new(move |request, _| {
                if request.is_some() {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                } else {
                    drained_tx.send(()).unwrap();
                }
                Err("blocked old decoder".into())
            }),
        )
        .unwrap();
        let document = Arc::new(RadialDocument::starter());
        let old_interest = Arc::new(AtomicBool::new(true));
        let old = GalleryRequest {
            correlation: GalleryCorrelation {
                request_id: 1,
                editor_session: AuthoringSessionId(1),
                draft_generation: DraftGeneration(1),
                interest_generation: 1,
                key: content_key(&document.skins[0], &document, &Default::default(), 1000, 0),
            },
            document,
            projection: PreviewProjection::default(),
            interest: Arc::clone(&old_interest),
        };
        client.try_send_gallery(old.clone()).unwrap();
        service
            .try_prepare(endpoint.gallery_rx.try_recv().unwrap())
            .unwrap();
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        // Close/reopen while the cold decoder is blocked. Resource retirement
        // occupies its sole queued slot, so the new main-owner dispatch fails.
        old_interest.store(false, Ordering::Release);
        service.invalidate_resources();
        let current = GalleryCorrelation {
            editor_session: AuthoringSessionId(2),
            ..old.correlation
        };
        client
            .try_send_gallery(GalleryRequest {
                correlation: current,
                interest: Arc::new(AtomicBool::new(true)),
                ..old.clone()
            })
            .unwrap();
        assert!(
            service
                .try_prepare(endpoint.gallery_rx.try_recv().unwrap())
                .is_err()
        );
        endpoint.send_gallery_reply(GalleryReply {
            correlation: current,
            result: Err("current dispatch rejected".into()),
        });
        release_tx.send(()).unwrap();
        drained_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(publications.load(Ordering::Acquire), 0);
        // Publication itself also rejects a stale completion, even if another
        // producer invokes the callback after the newer terminal result.
        sink(GalleryReply {
            correlation: old.correlation,
            result: Err("late retired completion".into()),
        });
        let reply = client.try_recv_gallery().unwrap();
        assert_eq!(reply.correlation, current);
        assert_eq!(reply.result.unwrap_err(), "current dispatch rejected");
        assert!(client.try_recv_gallery().is_none());
        assert!(endpoint.request_rx.try_recv().is_err());
        assert!(client.try_recv().is_none());
    }
    #[test]
    fn keys_preserve_name_only_and_unrelated_assets_but_track_referenced_versions_dpi_and_resources()
     {
        let mut document = RadialDocument::starter();
        let mut skin = super::super::appearance::BuiltinSkin::Compact.definition();
        let staged = super::super::authoring::AssetMutations::default();
        let original = content_key(&skin, &document, &staged, 1000, 1);
        skin.name = "Renamed custom copy".into();
        assert_eq!(content_key(&skin, &document, &staged, 1000, 1), original);
        document.assets.push(AssetRecord {
            id: AssetId::new("unrelated"),
            kind: MediaKind::Image,
            relative_path: "unrelated.png".into(),
            content_sha256: "a".repeat(64),
            byte_len: 1,
        });
        assert_eq!(content_key(&skin, &document, &staged, 1000, 1), original);
        skin.style.values.images.center_image = Override::Value(MediaReference::Managed {
            asset_id: AssetId::new("unrelated"),
        });
        let referenced = content_key(&skin, &document, &staged, 1000, 1);
        assert_ne!(referenced, original);
        document.assets[0].content_sha256 = "b".repeat(64);
        assert_ne!(content_key(&skin, &document, &staged, 1000, 1), referenced);
        assert_ne!(
            content_key(&skin, &document, &staged, 1250, 1),
            content_key(&skin, &document, &staged, 1000, 1)
        );
        assert_ne!(
            content_key(&skin, &document, &staged, 1000, 2),
            content_key(&skin, &document, &staged, 1000, 1)
        );
    }
    #[test]
    fn shared_preparation_and_composition_prove_distinct_presets_and_noncolor_selection_at_fractional_negative_monitor_contexts()
     {
        use super::super::{appearance::*, compositor::*, geometry::*, preparation::*, render::*};
        let source = RadialDocument::starter();
        let selected = CellId::new("gallery-0");
        let mut signatures = std::collections::BTreeSet::new();
        let mut preparer = PreviewFramePreparer::new(std::path::PathBuf::new());
        for scale in [1.0, 1.25, 2.0] {
            for preset in BUILTIN_SKINS {
                let demo = demonstration(preset.definition(), &source);
                let frame = preparer
                    .prepare(
                        &demo,
                        &demo.default_menu_id,
                        PhysicalPoint {
                            x: -900.0,
                            y: 700.0,
                        },
                        PhysicalRect {
                            min: PhysicalPoint { x: -2560.0, y: 0.0 },
                            max: PhysicalPoint { x: 0.0, y: 1440.0 },
                        },
                        ScaleFactor::new(scale).unwrap(),
                        1,
                        Some(&selected),
                        &PreviewProjection::default(),
                    )
                    .unwrap();
                let selected_text=frame.scene.primitives.iter().find(|primitive|matches!(primitive,VectorPrimitive::Text{text,..} if text=="Home")).unwrap();
                if let VectorPrimitive::Text {
                    bold,
                    underline,
                    color,
                    ..
                } = selected_text
                {
                    assert_eq!(*bold, preset == BuiltinSkin::HighContrast);
                    assert_eq!(*underline, preset == BuiltinSkin::HighContrast);
                    signatures.insert(format!(
                        "{scale}:{:?}:{color:?}",
                        frame.layout.cells[0].visual.font_size
                    ));
                }
                let idle = build_scene_prepared_selected(&frame.layout, 2, &frame.resources, None);
                assert!(idle.primitives.iter().any(|primitive|matches!(primitive,VectorPrimitive::Text{text,bold:false,underline:false,..} if text=="Home")));
                let mut compositor = CompositorCache::with_budget(MAX_GALLERY_BYTES);
                let raster = compositor
                    .compose(&frame.scene, ScaleFactor::new(1.0).unwrap(), 0)
                    .unwrap();
                assert!(raster.image.width() > 0 && raster.image.height() > 0);
                assert!(raster.image.as_raw().len() <= MAX_GALLERY_BYTES);
                let cell = frame
                    .layout
                    .cells
                    .iter()
                    .find(|cell| cell.cell_id == selected)
                    .unwrap();
                let center = match cell.shape {
                    HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
                };
                assert_eq!(
                    frame.layout.hit_test(center).map(|hit| &hit.cell_id),
                    Some(&selected)
                );
            }
        }
        assert_eq!(signatures.len(), 15);
    }
    #[test]
    fn gallery_fixed_demo_has_no_actions_providers_or_behavioral_inputs() {
        let document = RadialDocument::starter();
        for builtin in super::super::appearance::BUILTIN_SKINS {
            let demo = demonstration(builtin.definition(), &document);
            super::super::validation::validate(&demo).unwrap();
            for menu in &demo.menus {
                assert!(menu.center_action.is_none() && menu.background_action.is_none());
                for cell in menu.rings.iter().flat_map(|ring| &ring.cells) {
                    assert!(matches!(
                        cell.content,
                        CellContent::Control { .. } | CellContent::Submenu { .. }
                    ));
                    assert!(
                        cell.shortcuts.is_empty()
                            && cell.hotstrings.is_empty()
                            && cell.alternate_clicks.is_empty()
                    );
                }
            }
        }
    }
}
