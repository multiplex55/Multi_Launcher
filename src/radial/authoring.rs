//! Pure editor-draft state and the typed main-owner authoring protocol.
//!
//! The session/protocol layer deliberately contains no GUI, filesystem, native-window,
//! or [`RadialStore`](super::store::RadialStore) ownership. The GUI owns a
//! `RadialAuthoringSession`; the process main loop owns the service receiver, creates
//! the optional [`native_preview::NativePreviewCoordinator`], and is the only
//! component allowed to execute persistence requests or create preview surfaces.

use super::acceptance_trace::{
    self, AuthoringEdge, Correlation, Event, MutationResult, RequestKind,
};
use super::diagnostics::{
    MAX_EXPECTED_LAYOUT_DIAGNOSTICS, MAX_RADIAL_DIAGNOSTICS, bound_diagnostics,
};
use super::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor};
use super::model::{
    AssetId, AssetRecord, CellDefinition, CellId, ConfigRevision, ContextRuleId, HotstringId,
    MenuId, RadialDocument, RingId, ShortcutId, SkinId, TriggerId,
};
use super::package::ImportPlan;
use super::preparation::{PreparedFrameInput, PreviewProjection, synthetic_preview_dynamic};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};

pub mod menu;
pub mod native_preview;

pub const MAX_UNDO_ENTRIES: usize = 128;
pub const MAX_UNDO_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PENDING_ASSET_BYTES: usize = 256 * 1024 * 1024;

/// Session-lifetime allocator for every author-created stable entity ID.
///
/// IDs remain reserved after deletion, so a later add cannot silently reuse the
/// identity of an entity still referenced by UI state or an asynchronous reply.
#[derive(Clone, Debug, Default)]
pub struct AuthoringIdAllocator {
    menus: BTreeSet<String>,
    rings: BTreeSet<String>,
    cells: BTreeSet<String>,
    shortcuts: BTreeSet<String>,
    hotstrings: BTreeSet<String>,
    context_rules: BTreeSet<String>,
    triggers: BTreeSet<String>,
}

impl AuthoringIdAllocator {
    pub fn from_document(document: &RadialDocument) -> Self {
        let mut allocator = Self::default();
        allocator.reserve_document(document);
        allocator
    }

    pub fn reserve_document(&mut self, document: &RadialDocument) {
        for menu in &document.menus {
            self.menus.insert(menu.id.as_str().to_owned());
            for ring in &menu.rings {
                self.rings.insert(ring.id.as_str().to_owned());
                for cell in &ring.cells {
                    self.cells.insert(cell.id.as_str().to_owned());
                    self.shortcuts.extend(
                        cell.shortcuts
                            .iter()
                            .map(|shortcut| shortcut.id.as_str().to_owned()),
                    );
                    self.hotstrings.extend(
                        cell.hotstrings
                            .iter()
                            .map(|hotstring| hotstring.id.as_str().to_owned()),
                    );
                }
            }
        }
        self.context_rules.extend(
            document
                .context_rules
                .iter()
                .map(|rule| rule.id.as_str().to_owned()),
        );
        self.triggers.extend(
            document
                .custom_triggers
                .iter()
                .map(|trigger| trigger.id.as_str().to_owned()),
        );
    }

    fn allocate(set: &mut BTreeSet<String>, base: &str) -> String {
        let base = if base.trim().is_empty() {
            "entity"
        } else {
            base.trim()
        };
        if set.insert(base.to_owned()) {
            return base.to_owned();
        }
        for ordinal in 2usize.. {
            let candidate = format!("{base}-{ordinal}");
            if set.insert(candidate.clone()) {
                return candidate;
            }
        }
        unreachable!()
    }

    pub fn menu(&mut self, base: &str) -> MenuId {
        MenuId::new(Self::allocate(&mut self.menus, base))
    }
    pub fn ring(&mut self, base: &str) -> RingId {
        RingId::new(Self::allocate(&mut self.rings, base))
    }
    pub fn cell(&mut self, base: &str) -> CellId {
        CellId::new(Self::allocate(&mut self.cells, base))
    }
    pub fn shortcut(&mut self, base: &str) -> ShortcutId {
        ShortcutId::new(Self::allocate(&mut self.shortcuts, base))
    }
    pub fn hotstring(&mut self, base: &str) -> HotstringId {
        HotstringId::new(Self::allocate(&mut self.hotstrings, base))
    }
    pub fn context_rule(&mut self, base: &str) -> ContextRuleId {
        ContextRuleId::new(Self::allocate(&mut self.context_rules, base))
    }
    pub fn trigger(&mut self, base: &str) -> TriggerId {
        TriggerId::new(Self::allocate(&mut self.triggers, base))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthoringRequestId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DraftGeneration(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthoringSessionId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePreviewLease {
    pub editor_session: AuthoringSessionId,
    pub generation: DraftGeneration,
    pub request_id: AuthoringRequestId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskSha256(pub String);

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringSnapshot {
    pub document: Arc<RadialDocument>,
    pub revision: ConfigRevision,
    pub disk_sha256: DiskSha256,
}

impl AuthoringSnapshot {
    pub fn new(document: Arc<RadialDocument>, disk_sha256: impl Into<String>) -> Self {
        Self {
            revision: document.revision,
            document,
            disk_sha256: DiskSha256(disk_sha256.into()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StableSelection {
    Menu(MenuId),
    Ring {
        menu_id: MenuId,
        ring_id: RingId,
    },
    Cell {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    Skin(SkinId),
    Asset(AssetId),
}

impl StableSelection {
    pub fn exists_in(&self, document: &RadialDocument) -> bool {
        match self {
            Self::Menu(id) => document.menus.iter().any(|menu| &menu.id == id),
            Self::Ring { menu_id, ring_id } => document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .is_some_and(|menu| menu.rings.iter().any(|ring| &ring.id == ring_id)),
            Self::Cell {
                menu_id,
                ring_id,
                cell_id,
            } => document
                .menus
                .iter()
                .find(|menu| &menu.id == menu_id)
                .and_then(|menu| menu.rings.iter().find(|ring| &ring.id == ring_id))
                .is_some_and(|ring| ring.cells.iter().any(|cell| &cell.id == cell_id)),
            Self::Skin(id) => document.skins.iter().any(|skin| &skin.id == id),
            Self::Asset(id) => document.assets.iter().any(|asset| &asset.id == id),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedAssetAddition {
    pub record: AssetRecord,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssetMutations {
    pub additions: Vec<ManagedAssetAddition>,
    pub deletions: Vec<AssetId>,
}

impl AssetMutations {
    pub fn byte_len(&self) -> usize {
        self.additions.iter().map(|asset| asset.bytes.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.additions.is_empty() && self.deletions.is_empty()
    }

    /// Stable metadata identity for preview retry/cache correlation. Managed
    /// bytes are already content-addressed, so hashing them again per frame is
    /// both redundant and unnecessarily expensive.
    pub fn preview_identity(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for addition in &self.additions {
            addition.record.id.as_str().hash(&mut hasher);
            format!("{:?}", addition.record.kind).hash(&mut hasher);
            addition.record.relative_path.hash(&mut hasher);
            addition.record.content_sha256.hash(&mut hasher);
            addition.record.byte_len.hash(&mut hasher);
            addition.bytes.len().hash(&mut hasher);
        }
        for deletion in &self.deletions {
            deletion.as_str().hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DocumentMutation {
    RenameMenu {
        id: MenuId,
        name: String,
    },
    RenameSkin {
        id: SkinId,
        name: String,
    },
    SetDefaultMenu {
        id: MenuId,
    },
    ReplaceRingCells {
        menu_id: MenuId,
        ring_id: RingId,
        cells: Vec<CellDefinition>,
    },
    ReplaceDocument {
        document: RadialDocument,
    },
}

impl DocumentMutation {
    fn apply(self, document: &mut RadialDocument) -> Result<(), AuthoringError> {
        match self {
            Self::RenameMenu { id, name } => {
                let menu = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == id)
                    .ok_or(AuthoringError::MissingEntity)?;
                menu.name = name;
            }
            Self::RenameSkin { id, name } => {
                let skin = document
                    .skins
                    .iter_mut()
                    .find(|skin| skin.id == id)
                    .ok_or(AuthoringError::MissingEntity)?;
                skin.name = name;
            }
            Self::SetDefaultMenu { id } => {
                if !document.menus.iter().any(|menu| menu.id == id) {
                    return Err(AuthoringError::MissingEntity);
                }
                document.default_menu_id = id;
            }
            Self::ReplaceRingCells {
                menu_id,
                ring_id,
                cells,
            } => {
                let ring = document
                    .menus
                    .iter_mut()
                    .find(|menu| menu.id == menu_id)
                    .and_then(|menu| menu.rings.iter_mut().find(|ring| ring.id == ring_id))
                    .ok_or(AuthoringError::MissingEntity)?;
                ring.cells = cells;
            }
            Self::ReplaceDocument {
                document: replacement,
            } => *document = replacement,
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditKey {
    pub entity: String,
    pub field: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditPhase {
    Atomic,
    Begin,
    Update,
    End,
}

#[derive(Clone, Debug, PartialEq)]
struct HistoryEntry {
    before: Arc<RadialDocument>,
    after: Arc<RadialDocument>,
    before_assets: AssetMutations,
    after_assets: AssetMutations,
    key: Option<EditKey>,
    open: bool,
    bytes: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct BoundedHistory {
    undo: VecDeque<HistoryEntry>,
    redo: VecDeque<HistoryEntry>,
    bytes: usize,
}

impl BoundedHistory {
    fn push(&mut self, entry: HistoryEntry) {
        self.redo.clear();
        if entry.key.is_some()
            && let Some(last) = self.undo.back_mut()
            && last.open
            && last.key == entry.key
        {
            self.bytes = self.bytes.saturating_sub(last.bytes);
            last.after = entry.after;
            last.after_assets = entry.after_assets;
            last.open = entry.open;
            last.bytes =
                estimate_document_bytes(&last.before) + estimate_document_bytes(&last.after);
            last.bytes += last.before_assets.byte_len() + last.after_assets.byte_len();
            self.bytes = self.bytes.saturating_add(last.bytes);
            self.trim();
            return;
        }
        self.bytes = self.bytes.saturating_add(entry.bytes);
        self.undo.push_back(entry);
        self.trim();
    }

    fn close_group(&mut self, key: &EditKey) {
        if let Some(last) = self.undo.back_mut()
            && last.key.as_ref() == Some(key)
        {
            last.open = false;
        }
    }

    fn trim(&mut self) {
        while self.undo.len() > MAX_UNDO_ENTRIES || self.bytes > MAX_UNDO_BYTES {
            let Some(entry) = self.undo.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.bytes);
        }
    }

    fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.bytes = 0;
    }
}

fn estimate_document_bytes(document: &RadialDocument) -> usize {
    serde_json::to_vec(document).map_or(0, |bytes| bytes.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitDisposition {
    Apply,
    Save,
    RevertAppliedAndClose,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthoringRequest {
    Snapshot {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    Commit {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        disposition: CommitDisposition,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        candidate: RadialDocument,
        assets: AssetMutations,
    },
    LivePreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        candidate: Arc<RadialDocument>,
    },
    CancelPreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    ExportPackage {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        roots: Vec<MenuId>,
    },
    ExportSkin {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        skin_id: SkinId,
    },
    AuditionManagedAsset {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        asset_id: AssetId,
    },
    FontCatalog {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    PrepareEmbeddedPreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        candidate: Arc<RadialDocument>,
        menu_id: MenuId,
        selected: Option<CellId>,
        anchor: PhysicalPoint,
        work_area: PhysicalRect,
        scale: ScaleFactor,
        token: String,
        projection: PreviewProjection,
    },
    ReplacePackage {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        plan: ImportPlan,
        backup_path: PathBuf,
        confirmed: bool,
    },
    StartNativePreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        candidate: Arc<RadialDocument>,
        menu_id: MenuId,
        sample_external_context: bool,
        projection: PreviewProjection,
    },
    UpdateNativePreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        expected_revision: ConfigRevision,
        expected_disk_sha256: DiskSha256,
        previous: NativePreviewLease,
        candidate: Arc<RadialDocument>,
        menu_id: MenuId,
        sample_external_context: bool,
        projection: PreviewProjection,
    },
    StopNativePreview {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        lease: Option<NativePreviewLease>,
    },
}

impl AuthoringRequest {
    pub fn id(&self) -> AuthoringRequestId {
        match self {
            Self::Snapshot { id, .. }
            | Self::Commit { id, .. }
            | Self::LivePreview { id, .. }
            | Self::CancelPreview { id, .. }
            | Self::ExportPackage { id, .. }
            | Self::ExportSkin { id, .. }
            | Self::AuditionManagedAsset { id, .. }
            | Self::FontCatalog { id, .. }
            | Self::PrepareEmbeddedPreview { id, .. }
            | Self::ReplacePackage { id, .. }
            | Self::StartNativePreview { id, .. }
            | Self::UpdateNativePreview { id, .. }
            | Self::StopNativePreview { id, .. } => *id,
        }
    }

    pub fn generation(&self) -> DraftGeneration {
        match self {
            Self::Snapshot { generation, .. }
            | Self::Commit { generation, .. }
            | Self::LivePreview { generation, .. }
            | Self::CancelPreview { generation, .. }
            | Self::ExportPackage { generation, .. }
            | Self::ExportSkin { generation, .. }
            | Self::AuditionManagedAsset { generation, .. }
            | Self::FontCatalog { generation, .. }
            | Self::PrepareEmbeddedPreview { generation, .. }
            | Self::ReplacePackage { generation, .. }
            | Self::StartNativePreview { generation, .. }
            | Self::UpdateNativePreview { generation, .. }
            | Self::StopNativePreview { generation, .. } => *generation,
        }
    }

    pub fn editor_session(&self) -> AuthoringSessionId {
        match self {
            Self::Snapshot { editor_session, .. }
            | Self::Commit { editor_session, .. }
            | Self::LivePreview { editor_session, .. }
            | Self::CancelPreview { editor_session, .. }
            | Self::ExportPackage { editor_session, .. }
            | Self::ExportSkin { editor_session, .. }
            | Self::AuditionManagedAsset { editor_session, .. }
            | Self::FontCatalog { editor_session, .. }
            | Self::PrepareEmbeddedPreview { editor_session, .. }
            | Self::ReplacePackage { editor_session, .. }
            | Self::StartNativePreview { editor_session, .. }
            | Self::UpdateNativePreview { editor_session, .. }
            | Self::StopNativePreview { editor_session, .. } => *editor_session,
        }
    }

    fn trace_kind(&self) -> RequestKind {
        match self {
            Self::Snapshot { .. } => RequestKind::Snapshot,
            Self::Commit { disposition, .. } => match disposition {
                CommitDisposition::Apply => RequestKind::CommitApply,
                CommitDisposition::Save => RequestKind::CommitSave,
                CommitDisposition::RevertAppliedAndClose => RequestKind::CommitRevertAndClose,
            },
            Self::LivePreview { .. } => RequestKind::LivePreview,
            Self::CancelPreview { .. } => RequestKind::CancelPreview,
            Self::ExportPackage { .. } => RequestKind::ExportPackage,
            Self::ExportSkin { .. } => RequestKind::ExportSkin,
            Self::AuditionManagedAsset { .. } => RequestKind::AuditionManagedAsset,
            Self::FontCatalog { .. } => RequestKind::FontCatalog,
            Self::PrepareEmbeddedPreview { .. } => RequestKind::PrepareEmbeddedPreview,
            Self::ReplacePackage { .. } => RequestKind::ReplacePackage,
            Self::StartNativePreview { .. } => RequestKind::StartNativePreview,
            Self::UpdateNativePreview { .. } => RequestKind::UpdateNativePreview,
            Self::StopNativePreview { .. } => RequestKind::StopNativePreview,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthoringReply {
    ExternalPublished {
        editor_session: AuthoringSessionId,
        snapshot: AuthoringSnapshot,
    },
    Snapshot {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        snapshot: AuthoringSnapshot,
    },
    Published {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        disposition: CommitDisposition,
        snapshot: AuthoringSnapshot,
        rollback_assets: AssetMutations,
    },
    PreviewAccepted {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    PreviewCancelled {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    PackageExported {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        bytes: Arc<[u8]>,
    },
    AssetAuditioned {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    FontCatalog {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        families: Arc<[String]>,
    },
    EmbeddedPreviewPrepared {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        token: String,
        input: Arc<PreparedFrameInput>,
    },
    PackageReplaced {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        snapshot: AuthoringSnapshot,
        backup_path: PathBuf,
    },
    NativePreviewStarted {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        lease: NativePreviewLease,
        sampled_context: super::context::InvocationContext,
        diagnostics: Vec<super::diagnostics::RadialDiagnostic>,
    },
    NativePreviewUpdated {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        lease: NativePreviewLease,
        sampled_context: super::context::InvocationContext,
        diagnostics: Vec<super::diagnostics::RadialDiagnostic>,
    },
    NativePreviewDiagnostics {
        editor_session: AuthoringSessionId,
        lease: NativePreviewLease,
        diagnostics: Vec<super::diagnostics::RadialDiagnostic>,
    },
    NativePreviewStopped {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    },
    NativePreviewFailed {
        editor_session: AuthoringSessionId,
        lease: NativePreviewLease,
        message: String,
    },
    Failed {
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
        message: String,
    },
}

impl AuthoringReply {
    pub fn id(&self) -> AuthoringRequestId {
        match self {
            Self::ExternalPublished { .. } => AuthoringRequestId(0),
            Self::Snapshot { id, .. }
            | Self::Published { id, .. }
            | Self::PreviewAccepted { id, .. }
            | Self::PreviewCancelled { id, .. }
            | Self::PackageExported { id, .. }
            | Self::AssetAuditioned { id, .. }
            | Self::FontCatalog { id, .. }
            | Self::EmbeddedPreviewPrepared { id, .. }
            | Self::PackageReplaced { id, .. }
            | Self::NativePreviewStarted { id, .. }
            | Self::NativePreviewUpdated { id, .. }
            | Self::NativePreviewStopped { id, .. }
            | Self::Failed { id, .. } => *id,
            Self::NativePreviewFailed { lease, .. }
            | Self::NativePreviewDiagnostics { lease, .. } => lease.request_id,
        }
    }

    pub fn generation(&self) -> DraftGeneration {
        match self {
            Self::ExternalPublished { .. } => DraftGeneration(0),
            Self::Snapshot { generation, .. }
            | Self::Published { generation, .. }
            | Self::PreviewAccepted { generation, .. }
            | Self::PreviewCancelled { generation, .. }
            | Self::PackageExported { generation, .. }
            | Self::AssetAuditioned { generation, .. }
            | Self::FontCatalog { generation, .. }
            | Self::EmbeddedPreviewPrepared { generation, .. }
            | Self::PackageReplaced { generation, .. }
            | Self::NativePreviewStarted { generation, .. }
            | Self::NativePreviewUpdated { generation, .. }
            | Self::NativePreviewStopped { generation, .. }
            | Self::Failed { generation, .. } => *generation,
            Self::NativePreviewFailed { lease, .. }
            | Self::NativePreviewDiagnostics { lease, .. } => lease.generation,
        }
    }

    pub fn editor_session(&self) -> AuthoringSessionId {
        match self {
            Self::ExternalPublished { editor_session, .. }
            | Self::Snapshot { editor_session, .. }
            | Self::Published { editor_session, .. }
            | Self::PreviewAccepted { editor_session, .. }
            | Self::PreviewCancelled { editor_session, .. }
            | Self::PackageExported { editor_session, .. }
            | Self::AssetAuditioned { editor_session, .. }
            | Self::FontCatalog { editor_session, .. }
            | Self::EmbeddedPreviewPrepared { editor_session, .. }
            | Self::PackageReplaced { editor_session, .. }
            | Self::NativePreviewStarted { editor_session, .. }
            | Self::NativePreviewUpdated { editor_session, .. }
            | Self::NativePreviewStopped { editor_session, .. }
            | Self::NativePreviewDiagnostics { editor_session, .. }
            | Self::NativePreviewFailed { editor_session, .. }
            | Self::Failed { editor_session, .. } => *editor_session,
        }
    }

    fn trace_kind(&self) -> RequestKind {
        match self {
            Self::ExternalPublished { .. } => RequestKind::None,
            Self::Snapshot { .. } => RequestKind::Snapshot,
            Self::Published { disposition, .. } => match disposition {
                CommitDisposition::Apply => RequestKind::CommitApply,
                CommitDisposition::Save => RequestKind::CommitSave,
                CommitDisposition::RevertAppliedAndClose => RequestKind::CommitRevertAndClose,
            },
            Self::PreviewAccepted { .. } => RequestKind::LivePreview,
            Self::PreviewCancelled { .. } => RequestKind::CancelPreview,
            Self::PackageExported { .. } => RequestKind::ExportPackage,
            Self::AssetAuditioned { .. } => RequestKind::AuditionManagedAsset,
            Self::FontCatalog { .. } => RequestKind::FontCatalog,
            Self::EmbeddedPreviewPrepared { .. } => RequestKind::PrepareEmbeddedPreview,
            Self::PackageReplaced { .. } => RequestKind::ReplacePackage,
            Self::NativePreviewStarted { .. } => RequestKind::StartNativePreview,
            Self::NativePreviewUpdated { .. } => RequestKind::UpdateNativePreview,
            Self::NativePreviewDiagnostics { lease, .. }
            | Self::NativePreviewFailed { lease, .. } => {
                if lease.request_id.0 == 0 {
                    RequestKind::None
                } else {
                    RequestKind::UpdateNativePreview
                }
            }
            Self::NativePreviewStopped { .. } => RequestKind::StopNativePreview,
            Self::Failed { .. } => RequestKind::None,
        }
    }
}

#[derive(Clone)]
pub struct AuthoringClient {
    request_tx: mpsc::Sender<AuthoringRequest>,
    reply_rx: Arc<std::sync::Mutex<mpsc::Receiver<AuthoringReply>>>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    /// An editor viewport may be created after the main-owned service.  Keep
    /// its repaint callback separate from the root wake so replies can wake
    /// the actual owner without changing the service's lifetime boundary.
    reply_wake: Arc<std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    resource_tx: mpsc::Sender<AuthoringResourceDemand>,
}

pub struct AuthoringMainEndpoint {
    pub request_rx: mpsc::Receiver<AuthoringRequest>,
    pub reply_tx: AuthoringReplySender,
    pub resource_rx: mpsc::Receiver<AuthoringResourceDemand>,
}

/// Main-owner side of the authoring reply channel.  Sending a reply wakes the
/// root and the currently registered deferred Designer viewport, so a reply
/// never depends on an unrelated timer or pointer event to become visible.
pub struct AuthoringReplySender {
    tx: mpsc::Sender<AuthoringReply>,
    root_wake: Option<Arc<dyn Fn() + Send + Sync>>,
    reply_wake: Arc<std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
}

impl AuthoringReplySender {
    pub fn send(&self, reply: AuthoringReply) -> Result<(), mpsc::SendError<AuthoringReply>> {
        let correlation = Correlation {
            request_id: reply.id().0,
            request_kind: reply.trace_kind(),
            session_id: reply.editor_session().0,
            generation: reply.generation().0,
            terminal: true,
        };
        self.tx.send(reply)?;
        acceptance_trace::emit(Event::Authoring {
            edge: AuthoringEdge::ReplyTerminal,
            correlation,
        });
        if let Some(wake) = &self.root_wake {
            wake();
        }
        let wake = self
            .reply_wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthoringResourceDemand {
    Acquire(AuthoringSessionId),
    Release(AuthoringSessionId),
}

pub fn authoring_control_service() -> (AuthoringClient, AuthoringMainEndpoint) {
    authoring_control_service_with_wake(None)
}

pub fn authoring_control_service_with_wake(
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (AuthoringClient, AuthoringMainEndpoint) {
    let (request_tx, request_rx) = mpsc::channel();
    let (reply_tx, reply_rx) = mpsc::channel();
    let (resource_tx, resource_rx) = mpsc::channel();
    let reply_wake = Arc::new(std::sync::Mutex::new(None));
    (
        AuthoringClient {
            request_tx,
            reply_rx: Arc::new(std::sync::Mutex::new(reply_rx)),
            wake: wake.clone(),
            reply_wake: Arc::clone(&reply_wake),
            resource_tx,
        },
        AuthoringMainEndpoint {
            request_rx,
            reply_tx: AuthoringReplySender {
                tx: reply_tx,
                root_wake: wake,
                reply_wake,
            },
            resource_rx,
        },
    )
}

impl AuthoringClient {
    /// Install or clear the callback for the currently open deferred editor
    /// viewport.  Cloned clients share this slot, while the root service wake
    /// remains unchanged.  Replacing a closed viewport callback is therefore
    /// safe and does not retain an old egui context.
    pub fn set_reply_wake(&self, wake: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let Ok(mut slot) = self.reply_wake.lock() {
            *slot = wake;
        }
    }

    fn wake_reply_owner(&self) {
        let wake = self
            .reply_wake
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(Arc::clone));
        if let Some(wake) = wake {
            wake();
        }
    }

    pub fn acquire_resources(&self, editor_session: AuthoringSessionId) {
        let _ = self
            .resource_tx
            .send(AuthoringResourceDemand::Acquire(editor_session));
        if let Some(wake) = &self.wake {
            wake();
        }
        self.wake_reply_owner();
    }

    pub fn release_resources(&self, editor_session: AuthoringSessionId) {
        let _ = self
            .resource_tx
            .send(AuthoringResourceDemand::Release(editor_session));
        if let Some(wake) = &self.wake {
            wake();
        }
        self.wake_reply_owner();
    }

    pub fn send(&self, request: AuthoringRequest) -> Result<(), AuthoringError> {
        let correlation = Correlation {
            request_id: request.id().0,
            request_kind: request.trace_kind(),
            session_id: request.editor_session().0,
            generation: request.generation().0,
            terminal: false,
        };
        self.request_tx
            .send(request)
            .map_err(|_| AuthoringError::ServiceClosed)?;
        acceptance_trace::emit(Event::Authoring {
            edge: AuthoringEdge::RequestSent,
            correlation,
        });
        if let Some(wake) = &self.wake {
            wake();
        }
        self.wake_reply_owner();
        Ok(())
    }

    pub fn try_recv(&self) -> Option<AuthoringReply> {
        self.reply_rx.lock().ok()?.try_recv().ok()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AuthoringConflict {
    pub external: AuthoringSnapshot,
    pub reason: String,
}

#[derive(Clone, Debug)]
pub struct ConflictComparison {
    pub clean_base: Arc<RadialDocument>,
    pub local_draft: Arc<RadialDocument>,
    pub external: Arc<RadialDocument>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictResolution {
    Reload,
    DiscardDraft,
    Rebase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDecision {
    CloseClean,
    PromptDirty,
    AwaitingRequest,
}

/// Persistent intent carried across request/reply turns while the Designer is
/// waiting for a safe terminal close.  This is deliberately separate from the
/// dirty prompt: a clean close and a dirty Save/Discard close both need to
/// suppress new preparation work until their owned resources are retired.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseIntent {
    #[default]
    None,
    Requested,
    DiscardRequested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingRequestKind {
    Snapshot,
    Commit(CommitDisposition),
    LivePreview,
    CancelPreview,
    ExportPackage,
    ExportSkin,
    AuditionManagedAsset,
    FontCatalog,
    PrepareEmbeddedPreview,
    ReplacePackage,
    StartNativePreview,
    UpdateNativePreview,
    StopNativePreview,
}

impl PendingRequestKind {
    pub fn is_disposable(self) -> bool {
        matches!(
            self,
            Self::Snapshot
                | Self::FontCatalog
                | Self::PrepareEmbeddedPreview
                | Self::ExportPackage
                | Self::ExportSkin
                | Self::AuditionManagedAsset
                | Self::LivePreview
                | Self::CancelPreview
        )
    }

    pub fn is_preview_lifecycle(self) -> bool {
        matches!(
            self,
            Self::LivePreview
                | Self::CancelPreview
                | Self::StartNativePreview
                | Self::UpdateNativePreview
                | Self::StopNativePreview
        )
    }

    pub fn is_durable(self) -> bool {
        matches!(self, Self::Commit(_) | Self::ReplacePackage)
    }

    fn blocks_draft_generation_change(self) -> bool {
        matches!(self, Self::Snapshot) || self.is_durable()
    }

    fn invalidated_by_draft_generation_change(self) -> bool {
        self.is_disposable() && !matches!(self, Self::Snapshot | Self::FontCatalog)
    }
}

fn reply_matches_pending_kind(reply: &AuthoringReply, kind: PendingRequestKind) -> bool {
    match (kind, reply) {
        (PendingRequestKind::Snapshot, AuthoringReply::Snapshot { .. })
        | (PendingRequestKind::LivePreview, AuthoringReply::PreviewAccepted { .. })
        | (PendingRequestKind::CancelPreview, AuthoringReply::PreviewCancelled { .. })
        | (PendingRequestKind::ExportPackage, AuthoringReply::PackageExported { .. })
        | (PendingRequestKind::ExportSkin, AuthoringReply::PackageExported { .. })
        | (PendingRequestKind::AuditionManagedAsset, AuthoringReply::AssetAuditioned { .. })
        | (PendingRequestKind::FontCatalog, AuthoringReply::FontCatalog { .. })
        | (
            PendingRequestKind::PrepareEmbeddedPreview,
            AuthoringReply::EmbeddedPreviewPrepared { .. },
        )
        | (PendingRequestKind::ReplacePackage, AuthoringReply::PackageReplaced { .. })
        | (PendingRequestKind::StartNativePreview, AuthoringReply::NativePreviewStarted { .. })
        | (PendingRequestKind::UpdateNativePreview, AuthoringReply::NativePreviewUpdated { .. })
        | (PendingRequestKind::StopNativePreview, AuthoringReply::NativePreviewStopped { .. })
        | (_, AuthoringReply::Failed { .. }) => true,
        (PendingRequestKind::Commit(expected), AuthoringReply::Published { disposition, .. }) => {
            expected == *disposition
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingAuthoringRequest {
    pub id: AuthoringRequestId,
    pub generation: DraftGeneration,
    pub editor_session: AuthoringSessionId,
    pub kind: PendingRequestKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthoringError {
    MissingEntity,
    AssetBudgetExceeded,
    RequestPending,
    NoPendingRequest,
    NothingToRevert,
    RebaseConflict,
    ServiceClosed,
    ConfirmationRequired,
    StalePreview,
    AssetOverlayInvalid(String),
}

#[derive(Clone, Debug)]
pub struct RadialAuthoringSession {
    pub editor_session: AuthoringSessionId,
    pub baseline: AuthoringSnapshot,
    pub draft: Arc<RadialDocument>,
    pub generation: DraftGeneration,
    pub clean_checkpoint: Arc<RadialDocument>,
    pub selection: Option<StableSelection>,
    pub pending_assets: AssetMutations,
    pub conflict: Option<AuthoringConflict>,
    pub pending_request: Option<PendingAuthoringRequest>,
    pub last_error: Option<String>,
    pub exported_package: Option<Arc<[u8]>>,
    pub last_backup_path: Option<PathBuf>,
    pub native_preview_lease: Option<NativePreviewLease>,
    pub sampled_preview_context: Option<super::context::InvocationContext>,
    pub font_families: Arc<[String]>,
    pub font_catalog_loaded: bool,
    pub embedded_preview: Option<(String, Arc<PreparedFrameInput>)>,
    pub native_preview_diagnostics: Vec<super::diagnostics::RadialDiagnostic>,
    pub pending_native_preview: Option<PendingAuthoringRequest>,
    pub native_preview_may_be_open: bool,
    pub close_intent: CloseIntent,
    pending_native_context_sample: bool,
    cancel_checkpoint: Option<AuthoringSnapshot>,
    rollback_assets: AssetMutations,
    history: BoundedHistory,
    ids: AuthoringIdAllocator,
    next_request_id: u64,
    closed: bool,
}

impl RadialAuthoringSession {
    pub fn new(snapshot: AuthoringSnapshot) -> Self {
        static NEXT_AUTHORING_SESSION: AtomicU64 = AtomicU64::new(1);
        let editor_session = AuthoringSessionId(
            NEXT_AUTHORING_SESSION
                .fetch_add(1, Ordering::Relaxed)
                .max(1),
        );
        let ids = AuthoringIdAllocator::from_document(&snapshot.document);
        Self {
            editor_session,
            draft: Arc::clone(&snapshot.document),
            clean_checkpoint: Arc::clone(&snapshot.document),
            baseline: snapshot,
            generation: DraftGeneration(1),
            selection: None,
            pending_assets: AssetMutations::default(),
            conflict: None,
            pending_request: None,
            last_error: None,
            exported_package: None,
            last_backup_path: None,
            native_preview_lease: None,
            sampled_preview_context: None,
            font_families: Arc::from([]),
            font_catalog_loaded: false,
            embedded_preview: None,
            native_preview_diagnostics: Vec::new(),
            pending_native_preview: None,
            native_preview_may_be_open: false,
            close_intent: CloseIntent::None,
            pending_native_context_sample: false,
            rollback_assets: AssetMutations::default(),
            cancel_checkpoint: None,
            history: BoundedHistory::default(),
            ids,
            next_request_id: 1,
            closed: false,
        }
    }

    fn refresh_reserved_ids(&mut self) {
        self.ids.reserve_document(&self.draft);
    }

    pub fn allocate_menu_id(&mut self, base: &str) -> MenuId {
        self.refresh_reserved_ids();
        self.ids.menu(base)
    }
    pub fn allocate_ring_id(&mut self, base: &str) -> RingId {
        self.refresh_reserved_ids();
        self.ids.ring(base)
    }
    pub fn allocate_cell_id(&mut self, base: &str) -> CellId {
        self.refresh_reserved_ids();
        self.ids.cell(base)
    }
    pub fn allocate_shortcut_id(&mut self, base: &str) -> ShortcutId {
        self.refresh_reserved_ids();
        self.ids.shortcut(base)
    }
    pub fn allocate_hotstring_id(&mut self, base: &str) -> HotstringId {
        self.refresh_reserved_ids();
        self.ids.hotstring(base)
    }
    pub fn allocate_context_rule_id(&mut self, base: &str) -> ContextRuleId {
        self.refresh_reserved_ids();
        self.ids.context_rule(base)
    }
    pub fn allocate_trigger_id(&mut self, base: &str) -> TriggerId {
        self.refresh_reserved_ids();
        self.ids.trigger(base)
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.clean_checkpoint || !self.pending_assets.is_empty()
    }

    pub fn editor_session(&self) -> AuthoringSessionId {
        self.editor_session
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn is_initial_snapshot_pending(&self) -> bool {
        self.pending_request.is_some_and(|pending| {
            pending.editor_session == self.editor_session
                && pending.kind == PendingRequestKind::Snapshot
        })
    }

    /// Gate and advance one successful user-owned draft change.  Durable
    /// requests stay correlated until their service reply arrives, while
    /// disposable draft-bound work is terminalized before the new generation
    /// becomes observable.  Font catalogs are session-stable and deliberately
    /// survive the generation change.
    fn ensure_draft_generation_change_allowed(&self) -> Result<(), AuthoringError> {
        if self.pending_request.is_some_and(|pending| {
            pending.editor_session == self.editor_session
                && pending.kind.blocks_draft_generation_change()
        }) {
            return Err(AuthoringError::RequestPending);
        }
        Ok(())
    }

    fn advance_draft_generation(&mut self) -> Result<(), AuthoringError> {
        self.ensure_draft_generation_change_allowed()?;
        self.generation.0 = self.generation.0.wrapping_add(1).max(1);
        self.invalidate_draft_bound_pending_request();
        Ok(())
    }

    fn invalidate_draft_bound_pending_request(&mut self) {
        if self.pending_request.is_some_and(|pending| {
            pending.editor_session == self.editor_session
                && pending.kind.invalidated_by_draft_generation_change()
        }) {
            self.pending_request = None;
        }
    }

    pub fn close_decision(&self) -> CloseDecision {
        if self.pending_request.is_some() {
            CloseDecision::AwaitingRequest
        } else if self.is_dirty() {
            CloseDecision::PromptDirty
        } else {
            CloseDecision::CloseClean
        }
    }

    pub fn request_close_intent(&mut self) {
        self.close_intent = CloseIntent::Requested;
    }

    pub fn request_discard_close_intent(&mut self) {
        self.close_intent = CloseIntent::DiscardRequested;
    }

    pub fn clear_close_intent(&mut self) {
        self.close_intent = CloseIntent::None;
    }

    pub fn close_requested(&self) -> bool {
        self.close_intent != CloseIntent::None
    }

    pub fn select(&mut self, selection: Option<StableSelection>) {
        if self.is_initial_snapshot_pending() {
            return;
        }
        self.selection = selection.filter(|selection| selection.exists_in(&self.draft));
    }

    pub fn mutate(
        &mut self,
        mutation: DocumentMutation,
        key: Option<EditKey>,
        phase: EditPhase,
    ) -> Result<(), AuthoringError> {
        let result = (|| {
            self.ensure_draft_generation_change_allowed()?;
            let before = Arc::clone(&self.draft);
            let mut after = (*before).clone();
            mutation.apply(&mut after)?;
            let after = Arc::new(after);
            if after == before {
                if phase == EditPhase::End
                    && let Some(key) = &key
                {
                    self.history.close_group(key);
                }
                return Ok(());
            }
            self.advance_draft_generation()?;
            let open = matches!(phase, EditPhase::Begin | EditPhase::Update);
            let entry = HistoryEntry {
                bytes: estimate_document_bytes(&before)
                    + estimate_document_bytes(&after)
                    + self.pending_assets.byte_len() * 2,
                before,
                after: Arc::clone(&after),
                before_assets: self.pending_assets.clone(),
                after_assets: self.pending_assets.clone(),
                key: key.clone(),
                open,
            };
            self.history.push(entry);
            if phase == EditPhase::End
                && let Some(key) = &key
            {
                self.history.close_group(key);
            }
            self.draft = after;
            if self
                .selection
                .as_ref()
                .is_some_and(|selected| !selected.exists_in(&self.draft))
            {
                self.selection = None;
            }
            Ok(())
        })();
        acceptance_trace::emit(Event::DesignerMutation {
            result: if result.is_ok() {
                MutationResult::Accepted
            } else {
                MutationResult::Rejected
            },
            correlation: Correlation {
                request_id: 0,
                request_kind: RequestKind::None,
                session_id: self.editor_session.0,
                generation: self.generation.0,
                terminal: true,
            },
        });
        result
    }

    /// Imports, duplication graphs, and relocation/count previews commit to the
    /// draft as one undoable unit after their caller has built the complete
    /// stable-ID document.
    pub fn replace_document_atomic(
        &mut self,
        document: RadialDocument,
    ) -> Result<(), AuthoringError> {
        self.mutate(
            DocumentMutation::ReplaceDocument { document },
            None,
            EditPhase::Atomic,
        )
    }

    /// Apply a widget-owned document edit while preserving its focus/drag
    /// coalescing identity. Repeated updates from one gesture become one undo
    /// entry; ending the gesture seals that entry before older history changes.
    pub fn replace_document_edit(
        &mut self,
        document: RadialDocument,
        key: EditKey,
        phase: EditPhase,
    ) -> Result<(), AuthoringError> {
        self.mutate(
            DocumentMutation::ReplaceDocument { document },
            Some(key),
            phase,
        )
    }

    /// Replace a complete imported graph and its managed assets as one bounded
    /// undo entry. Import preview remains read-only until this boundary.
    pub fn replace_document_and_assets_atomic(
        &mut self,
        document: RadialDocument,
        assets: AssetMutations,
    ) -> Result<(), AuthoringError> {
        self.ensure_draft_generation_change_allowed()?;
        if assets.byte_len() > MAX_PENDING_ASSET_BYTES {
            return Err(AuthoringError::AssetBudgetExceeded);
        }
        let before = Arc::clone(&self.draft);
        let before_assets = self.pending_assets.clone();
        let after = Arc::new(document);
        if before == after && before_assets == assets {
            return Ok(());
        }
        self.advance_draft_generation()?;
        let entry = HistoryEntry {
            bytes: estimate_document_bytes(&before)
                + estimate_document_bytes(&after)
                + before_assets.byte_len()
                + assets.byte_len(),
            before,
            after: Arc::clone(&after),
            before_assets,
            after_assets: assets.clone(),
            key: None,
            open: false,
        };
        self.history.push(entry);
        self.draft = after;
        self.pending_assets = assets;
        self.selection = self
            .selection
            .take()
            .filter(|selection| selection.exists_in(&self.draft));
        Ok(())
    }

    pub fn stage_asset_addition(
        &mut self,
        addition: ManagedAssetAddition,
    ) -> Result<(), AuthoringError> {
        self.ensure_draft_generation_change_allowed()?;
        let old_len = self
            .pending_assets
            .additions
            .iter()
            .find(|asset| asset.record.id == addition.record.id)
            .map_or(0, |asset| asset.bytes.len());
        let projected = self.pending_assets.byte_len() - old_len + addition.bytes.len();
        if projected > MAX_PENDING_ASSET_BYTES {
            return Err(AuthoringError::AssetBudgetExceeded);
        }
        self.advance_draft_generation()?;
        let before_assets = self.pending_assets.clone();
        self.pending_assets
            .additions
            .retain(|asset| asset.record.id != addition.record.id);
        self.pending_assets
            .deletions
            .retain(|id| id != &addition.record.id);
        self.pending_assets.additions.push(addition);
        self.history.push(HistoryEntry {
            before: Arc::clone(&self.draft),
            after: Arc::clone(&self.draft),
            before_assets: before_assets.clone(),
            after_assets: self.pending_assets.clone(),
            key: None,
            open: false,
            bytes: before_assets.byte_len() + self.pending_assets.byte_len(),
        });
        Ok(())
    }

    pub fn stage_asset_delete(&mut self, id: AssetId) {
        if self.advance_draft_generation().is_err() {
            return;
        }
        let before_assets = self.pending_assets.clone();
        self.pending_assets
            .additions
            .retain(|asset| asset.record.id != id);
        if !self.pending_assets.deletions.contains(&id) {
            self.pending_assets.deletions.push(id);
        }
        self.history.push(HistoryEntry {
            before: Arc::clone(&self.draft),
            after: Arc::clone(&self.draft),
            before_assets: before_assets.clone(),
            after_assets: self.pending_assets.clone(),
            key: None,
            open: false,
            bytes: before_assets.byte_len() + self.pending_assets.byte_len(),
        });
    }

    pub fn undo(&mut self) -> bool {
        if self.history.undo.is_empty() || self.advance_draft_generation().is_err() {
            return false;
        }
        let Some(entry) = self.history.undo.pop_back() else {
            return false;
        };
        self.history.bytes = self.history.bytes.saturating_sub(entry.bytes);
        self.draft = Arc::clone(&entry.before);
        self.pending_assets = entry.before_assets.clone();
        self.history.redo.push_back(entry);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.history.redo.is_empty() || self.advance_draft_generation().is_err() {
            return false;
        }
        let Some(entry) = self.history.redo.pop_back() else {
            return false;
        };
        self.draft = Arc::clone(&entry.after);
        self.pending_assets = entry.after_assets.clone();
        self.history.bytes = self.history.bytes.saturating_add(entry.bytes);
        self.history.undo.push_back(entry);
        self.history.trim();
        true
    }

    fn next_id(&mut self) -> AuthoringRequestId {
        let id = AuthoringRequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        id
    }

    pub fn request_snapshot(&mut self) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::Snapshot,
        });
        Ok(AuthoringRequest::Snapshot {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
        })
    }

    pub fn request_commit(
        &mut self,
        disposition: CommitDisposition,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        let (candidate, assets) = if disposition == CommitDisposition::RevertAppliedAndClose {
            let checkpoint = self
                .cancel_checkpoint
                .as_ref()
                .ok_or(AuthoringError::NothingToRevert)?;
            ((*checkpoint.document).clone(), self.rollback_assets.clone())
        } else {
            ((*self.draft).clone(), self.pending_assets.clone())
        };
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::Commit(disposition),
        });
        Ok(AuthoringRequest::Commit {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            disposition,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            candidate,
            assets,
        })
    }

    pub fn request_live_preview(&mut self) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::LivePreview,
        });
        Ok(AuthoringRequest::LivePreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            candidate: Arc::clone(&self.draft),
        })
    }

    pub fn request_cancel_preview(&mut self) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::CancelPreview,
        });
        Ok(AuthoringRequest::CancelPreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
        })
    }

    pub fn request_export_package(
        &mut self,
        roots: Vec<MenuId>,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::ExportPackage,
        });
        Ok(AuthoringRequest::ExportPackage {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            roots,
        })
    }

    pub fn request_export_skin(
        &mut self,
        skin_id: SkinId,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        if !self.draft.skins.iter().any(|skin| skin.id == skin_id) {
            return Err(AuthoringError::MissingEntity);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::ExportSkin,
        });
        Ok(AuthoringRequest::ExportSkin {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            skin_id,
        })
    }

    pub fn request_audition_managed_asset(
        &mut self,
        asset_id: AssetId,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::AuditionManagedAsset,
        });
        Ok(AuthoringRequest::AuditionManagedAsset {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            asset_id,
        })
    }

    pub fn request_font_catalog(&mut self) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::FontCatalog,
        });
        Ok(AuthoringRequest::FontCatalog {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
        })
    }

    pub fn request_embedded_preview(
        &mut self,
        candidate: Arc<RadialDocument>,
        menu_id: MenuId,
        selected: Option<CellId>,
        anchor: PhysicalPoint,
        work_area: PhysicalRect,
        scale: ScaleFactor,
        token: String,
        page: usize,
        selected_skin: Option<SkinId>,
    ) -> Result<AuthoringRequest, AuthoringError> {
        self.request_embedded_preview_placed(
            candidate,
            menu_id,
            selected,
            anchor,
            work_area,
            scale,
            token,
            page,
            selected_skin,
            crate::radial::preparation::PreviewPlacement::FlexibleRoot,
        )
    }

    pub fn request_embedded_preview_placed(
        &mut self,
        candidate: Arc<RadialDocument>,
        menu_id: MenuId,
        selected: Option<CellId>,
        anchor: PhysicalPoint,
        work_area: PhysicalRect,
        scale: ScaleFactor,
        token: String,
        page: usize,
        selected_skin: Option<SkinId>,
        placement: crate::radial::preparation::PreviewPlacement,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        let id = self.next_id();
        let mut projection = self.preview_projection(&candidate, &menu_id, page, selected_skin)?;
        projection.placement = placement;
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::PrepareEmbeddedPreview,
        });
        Ok(AuthoringRequest::PrepareEmbeddedPreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            candidate,
            menu_id,
            selected,
            anchor,
            work_area,
            scale,
            token,
            projection,
        })
    }

    pub fn cancel_pending_request(
        &mut self,
        id: AuthoringRequestId,
        generation: DraftGeneration,
        editor_session: AuthoringSessionId,
    ) -> bool {
        if self.pending_request.is_some_and(|pending| {
            pending.id == id
                && pending.generation == generation
                && pending.editor_session == editor_session
        }) {
            self.pending_request = None;
            true
        } else {
            false
        }
    }

    /// Cancel only the exact request represented by `pending`.  Native
    /// preview requests use a separate slot, so closing code must reconcile
    /// both slots without touching a newer request or a different session.
    pub fn cancel_pending_request_exact(&mut self, pending: PendingAuthoringRequest) -> bool {
        if self.pending_request == Some(pending) {
            self.pending_request = None;
            return true;
        }
        if self.pending_native_preview == Some(pending) {
            self.pending_native_preview = None;
            self.pending_native_context_sample = false;
            if matches!(
                pending.kind,
                PendingRequestKind::StartNativePreview | PendingRequestKind::UpdateNativePreview
            ) && self.native_preview_lease.is_none()
            {
                self.native_preview_may_be_open = false;
            }
            return true;
        }
        false
    }

    /// Reconcile a request which could not be delivered to the authoring
    /// service.  Matching all correlation fields prevents a failed old send
    /// from clearing a newer request.  The error remains visible to the
    /// editor so the user can retry or keep editing.
    pub fn reconcile_request_delivery_failure(
        &mut self,
        pending: PendingAuthoringRequest,
        message: impl Into<String>,
    ) -> bool {
        let matched = self.cancel_pending_request_exact(pending);
        if !matched {
            return false;
        }
        if matches!(pending.kind, PendingRequestKind::StopNativePreview)
            && self.native_preview_lease.is_none()
        {
            self.native_preview_may_be_open = false;
        }
        self.last_error = Some(message.into());
        true
    }

    pub fn request_replace_package(
        &mut self,
        plan: ImportPlan,
        reviewed_revision: ConfigRevision,
        reviewed_disk_sha256: &str,
        reviewed_generation: DraftGeneration,
        backup_path: PathBuf,
        confirmed: bool,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if !confirmed {
            return Err(AuthoringError::ConfirmationRequired);
        }
        if self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        if self.conflict.is_some() {
            return Err(AuthoringError::RebaseConflict);
        }
        if self.baseline.revision != reviewed_revision
            || self.baseline.disk_sha256.0 != reviewed_disk_sha256
            || self.generation != reviewed_generation
        {
            return Err(AuthoringError::StalePreview);
        }
        let id = self.next_id();
        self.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::ReplacePackage,
        });
        Ok(AuthoringRequest::ReplacePackage {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            plan,
            backup_path,
            confirmed,
        })
    }

    pub fn request_start_native_preview(
        &mut self,
        menu_id: MenuId,
        sample_external_context: bool,
        selected_skin: Option<SkinId>,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_native_preview.is_some() || self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        if self.conflict.is_some() {
            return Err(AuthoringError::RebaseConflict);
        }
        let id = self.next_id();
        let projection = self.preview_projection(&self.draft, &menu_id, 0, selected_skin)?;
        self.pending_native_preview = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::StartNativePreview,
        });
        self.pending_native_context_sample = sample_external_context;
        self.native_preview_may_be_open = true;
        Ok(AuthoringRequest::StartNativePreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            candidate: Arc::clone(&self.draft),
            menu_id,
            sample_external_context,
            projection,
        })
    }

    pub fn request_update_native_preview(
        &mut self,
        menu_id: MenuId,
        sample_external_context: bool,
        selected_skin: Option<SkinId>,
    ) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_native_preview.is_some() || self.pending_request.is_some() {
            return Err(AuthoringError::RequestPending);
        }
        if self.conflict.is_some() {
            return Err(AuthoringError::RebaseConflict);
        }
        let previous = self
            .native_preview_lease
            .clone()
            .ok_or(AuthoringError::NoPendingRequest)?;
        let id = self.next_id();
        let projection = self.preview_projection(&self.draft, &menu_id, 0, selected_skin)?;
        self.pending_native_preview = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::UpdateNativePreview,
        });
        self.pending_native_context_sample = sample_external_context;
        self.native_preview_may_be_open = true;
        Ok(AuthoringRequest::UpdateNativePreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            expected_revision: self.baseline.revision,
            expected_disk_sha256: self.baseline.disk_sha256.clone(),
            previous,
            candidate: Arc::clone(&self.draft),
            menu_id,
            sample_external_context,
            projection,
        })
    }

    fn preview_projection(
        &self,
        candidate: &RadialDocument,
        menu_id: &MenuId,
        page: usize,
        selected_skin: Option<SkinId>,
    ) -> Result<PreviewProjection, AuthoringError> {
        let menu = candidate
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .ok_or(AuthoringError::MissingEntity)?;
        let assets = super::assets::ManagedAssetOverlay::validated(
            self.pending_assets
                .additions
                .iter()
                .map(|addition| (addition.record.clone(), Arc::clone(&addition.bytes))),
        )
        .map_err(|error| AuthoringError::AssetOverlayInvalid(error.to_string()))?;
        Ok(PreviewProjection {
            page,
            placement: super::preparation::PreviewPlacement::FlexibleRoot,
            dynamic: synthetic_preview_dynamic(menu),
            selected_skin,
            assets,
            tooltip_preferences: super::tooltip::TooltipPreferences::default(),
        })
    }

    pub fn request_stop_native_preview(&mut self) -> Result<AuthoringRequest, AuthoringError> {
        if self.pending_request.is_some()
            || self
                .pending_native_preview
                .is_some_and(|pending| pending.kind == PendingRequestKind::StopNativePreview)
        {
            return Err(AuthoringError::RequestPending);
        }
        // A stop is the terminal operation for a preview lifecycle.  It
        // supersedes an in-flight start/update correlation so a late success
        // cannot reopen or move a closing preview.
        self.pending_native_preview = None;
        self.pending_native_context_sample = false;
        let id = self.next_id();
        self.pending_native_preview = Some(PendingAuthoringRequest {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            kind: PendingRequestKind::StopNativePreview,
        });
        self.pending_native_context_sample = false;
        Ok(AuthoringRequest::StopNativePreview {
            id,
            generation: self.generation,
            editor_session: self.editor_session,
            lease: self.native_preview_lease.clone(),
        })
    }

    /// Returns false for stale request IDs or generations; such replies are
    /// intentionally ignored without perturbing the current draft.
    pub fn accept_reply(&mut self, reply: AuthoringReply) -> bool {
        if reply.editor_session() != self.editor_session {
            return false;
        }
        if match &reply {
            AuthoringReply::NativePreviewStarted {
                id,
                generation,
                editor_session,
                lease,
                ..
            }
            | AuthoringReply::NativePreviewUpdated {
                id,
                generation,
                editor_session,
                lease,
                ..
            } => {
                lease.request_id != *id
                    || lease.generation != *generation
                    || lease.editor_session != *editor_session
            }
            AuthoringReply::NativePreviewFailed {
                editor_session,
                lease,
                ..
            } => lease.editor_session != *editor_session,
            AuthoringReply::NativePreviewDiagnostics {
                editor_session,
                lease,
                ..
            } => lease.editor_session != *editor_session,
            _ => false,
        } {
            return false;
        }
        if let AuthoringReply::ExternalPublished { snapshot, .. } = &reply {
            if self.is_initial_snapshot_pending() {
                // This unsolicited publication is itself an authoritative
                // current snapshot for the session. Consume the bootstrap
                // request so its older correlated reply cannot strand the UI.
                self.pending_request = None;
            }
            self.native_preview_lease = None;
            self.pending_native_preview = None;
            self.pending_native_context_sample = false;
            self.observe_external(snapshot.clone());
            return true;
        }
        if let AuthoringReply::NativePreviewFailed { lease, message, .. } = &reply {
            if self.native_preview_lease.as_ref() != Some(lease) {
                return false;
            }
            self.native_preview_lease = None;
            self.pending_native_preview = None;
            self.pending_native_context_sample = false;
            self.native_preview_may_be_open = false;
            self.last_error = Some(message.clone());
            return true;
        }
        if let AuthoringReply::NativePreviewDiagnostics {
            lease, diagnostics, ..
        } = &reply
        {
            if self.native_preview_lease.as_ref() != Some(lease)
                || lease.generation != self.generation
            {
                return false;
            }
            self.native_preview_diagnostics = bound_diagnostics(
                diagnostics.clone(),
                MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
                MAX_RADIAL_DIAGNOSTICS,
            );
            return true;
        }
        if let AuthoringReply::NativePreviewStopped {
            id,
            generation,
            editor_session,
        } = &reply
            && self.native_preview_lease.as_ref().is_some_and(|lease| {
                lease.request_id == *id
                    && lease.generation == *generation
                    && lease.editor_session == *editor_session
            })
        {
            self.native_preview_lease = None;
            self.native_preview_may_be_open = false;
            return true;
        }
        if self.pending_native_preview.is_some_and(|pending| {
            pending.editor_session == self.editor_session
                && pending.id == reply.id()
                && pending.generation == reply.generation()
                && reply.generation() == self.generation
                && reply_matches_pending_kind(&reply, pending.kind)
        }) {
            let pending = self.pending_native_preview.take().unwrap();
            debug_assert_eq!(pending.editor_session, self.editor_session);
            match reply {
                AuthoringReply::NativePreviewStarted {
                    lease,
                    sampled_context,
                    diagnostics,
                    ..
                }
                | AuthoringReply::NativePreviewUpdated {
                    lease,
                    sampled_context,
                    diagnostics,
                    ..
                } => {
                    self.native_preview_lease = Some(lease);
                    self.native_preview_may_be_open = true;
                    if self.pending_native_context_sample {
                        self.sampled_preview_context = Some(sampled_context);
                    }
                    self.pending_native_context_sample = false;
                    self.native_preview_diagnostics = bound_diagnostics(
                        diagnostics,
                        MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
                        MAX_RADIAL_DIAGNOSTICS,
                    );
                }
                AuthoringReply::NativePreviewStopped { editor_session, .. } => {
                    if editor_session == self.editor_session {
                        self.native_preview_lease = None;
                        self.native_preview_may_be_open = false;
                    }
                    self.pending_native_context_sample = false;
                }
                AuthoringReply::Failed { message, .. } => {
                    self.native_preview_lease = None;
                    self.native_preview_may_be_open = false;
                    self.pending_native_context_sample = false;
                    self.last_error = Some(message);
                }
                AuthoringReply::NativePreviewFailed { .. } => return false,
                AuthoringReply::NativePreviewDiagnostics { .. } => return false,
                _ => return false,
            }
            return true;
        }
        let Some(pending) = self.pending_request else {
            return false;
        };
        if pending.editor_session == self.editor_session
            && reply.id() == pending.id
            && reply.generation() == pending.generation
            && reply.generation() != self.generation
            && pending.kind.invalidated_by_draft_generation_change()
            && reply_matches_pending_kind(&reply, pending.kind)
        {
            // A matching disposable operation can finish after its draft was
            // superseded.  Retire only that exact terminal slot; do not let
            // its draft-specific payload enter the newer generation.
            self.pending_request = None;
            return false;
        }
        if pending.editor_session != self.editor_session
            || reply.id() != pending.id
            || reply.generation() != pending.generation
            || (reply.generation() != self.generation
                && pending.kind != PendingRequestKind::FontCatalog)
            || !reply_matches_pending_kind(&reply, pending.kind)
        {
            return false;
        }
        self.pending_request = None;
        match reply {
            AuthoringReply::ExternalPublished { .. } => unreachable!(),
            AuthoringReply::Snapshot { snapshot, .. } => self.observe_external(snapshot),
            AuthoringReply::Published {
                disposition,
                snapshot,
                rollback_assets,
                ..
            } => {
                if disposition == CommitDisposition::Apply {
                    self.cancel_checkpoint = Some(self.baseline.clone());
                }
                self.baseline = snapshot.clone();
                self.draft = Arc::clone(&snapshot.document);
                self.clean_checkpoint = Arc::clone(&snapshot.document);
                self.pending_assets = AssetMutations::default();
                self.rollback_assets = rollback_assets;
                self.history.clear();
                self.conflict = None;
                self.native_preview_lease = None;
                self.native_preview_may_be_open = false;
                self.pending_native_preview = None;
                self.generation.0 = self.generation.0.wrapping_add(1).max(1);
                if matches!(
                    disposition,
                    CommitDisposition::Save | CommitDisposition::RevertAppliedAndClose
                ) {
                    self.closed = true;
                }
            }
            AuthoringReply::PreviewAccepted { .. } | AuthoringReply::PreviewCancelled { .. } => {}
            AuthoringReply::PackageExported { bytes, .. } => {
                self.exported_package = Some(bytes);
            }
            AuthoringReply::AssetAuditioned { .. } => {}
            AuthoringReply::FontCatalog { families, .. } => {
                self.font_families = families;
                self.font_catalog_loaded = true;
            }
            AuthoringReply::EmbeddedPreviewPrepared { token, input, .. } => {
                self.embedded_preview = Some((token, input));
            }
            AuthoringReply::PackageReplaced {
                snapshot,
                backup_path,
                ..
            } => {
                self.baseline = snapshot.clone();
                self.draft = Arc::clone(&snapshot.document);
                self.clean_checkpoint = Arc::clone(&snapshot.document);
                self.pending_assets = AssetMutations::default();
                self.rollback_assets = AssetMutations::default();
                self.cancel_checkpoint = None;
                self.last_backup_path = Some(backup_path);
                self.history.clear();
                self.conflict = None;
                self.native_preview_lease = None;
                self.native_preview_may_be_open = false;
                self.pending_native_preview = None;
                self.generation.0 = self.generation.0.wrapping_add(1).max(1);
            }
            AuthoringReply::NativePreviewStarted { .. }
            | AuthoringReply::NativePreviewUpdated { .. }
            | AuthoringReply::NativePreviewStopped { .. }
            | AuthoringReply::NativePreviewDiagnostics { .. }
            | AuthoringReply::NativePreviewFailed { .. } => return false,
            AuthoringReply::Failed { message, .. } => {
                if matches!(
                    pending.kind,
                    PendingRequestKind::Commit(_) | PendingRequestKind::ReplacePackage
                ) {
                    self.native_preview_lease = None;
                    self.native_preview_may_be_open = false;
                    self.pending_native_preview = None;
                }
                self.last_error = Some(message);
            }
        }
        true
    }

    pub fn observe_external(&mut self, snapshot: AuthoringSnapshot) {
        self.native_preview_lease = None;
        self.native_preview_may_be_open = false;
        self.pending_native_preview = None;
        self.generation.0 = self.generation.0.wrapping_add(1).max(1);
        self.invalidate_draft_bound_pending_request();
        if !self.is_dirty() {
            self.baseline = snapshot.clone();
            self.draft = Arc::clone(&snapshot.document);
            self.clean_checkpoint = Arc::clone(&snapshot.document);
            self.cancel_checkpoint = None;
            self.rollback_assets = AssetMutations::default();
            self.history.clear();
            self.selection = self
                .selection
                .take()
                .filter(|selection| selection.exists_in(&self.draft));
        } else {
            self.conflict = Some(AuthoringConflict {
                external: snapshot,
                reason: "radial configuration changed while this draft has local edits".into(),
            });
        }
    }

    pub fn conflict_comparison(&self) -> Option<ConflictComparison> {
        let conflict = self.conflict.as_ref()?;
        Some(ConflictComparison {
            clean_base: Arc::clone(&self.clean_checkpoint),
            local_draft: Arc::clone(&self.draft),
            external: Arc::clone(&conflict.external.document),
        })
    }

    pub fn resolve_conflict(
        &mut self,
        resolution: ConflictResolution,
    ) -> Result<(), AuthoringError> {
        let conflict = self.conflict.clone().ok_or(AuthoringError::MissingEntity)?;
        self.ensure_draft_generation_change_allowed()?;
        match resolution {
            ConflictResolution::Reload => {
                self.advance_draft_generation()?;
                self.baseline = conflict.external.clone();
                self.draft = Arc::clone(&conflict.external.document);
                self.clean_checkpoint = Arc::clone(&conflict.external.document);
                self.pending_assets = AssetMutations::default();
                self.cancel_checkpoint = None;
                self.rollback_assets = AssetMutations::default();
                self.history.clear();
                self.conflict = None;
            }
            ConflictResolution::DiscardDraft => {
                self.advance_draft_generation()?;
                self.baseline = conflict.external.clone();
                self.draft = Arc::clone(&conflict.external.document);
                self.clean_checkpoint = Arc::clone(&conflict.external.document);
                self.pending_assets = AssetMutations::default();
                self.cancel_checkpoint = None;
                self.rollback_assets = AssetMutations::default();
                self.history.clear();
                self.conflict = None;
                self.closed = true;
            }
            ConflictResolution::Rebase => {
                let merged = three_way_merge(
                    &self.clean_checkpoint,
                    &self.draft,
                    &conflict.external.document,
                )?;
                self.advance_draft_generation()?;
                self.baseline = conflict.external.clone();
                self.clean_checkpoint = Arc::clone(&conflict.external.document);
                self.draft = Arc::new(merged);
                self.cancel_checkpoint = None;
                self.rollback_assets = AssetMutations::default();
                self.history.clear();
                self.conflict = None;
            }
        }
        Ok(())
    }
}

fn three_way_merge(
    base: &RadialDocument,
    local: &RadialDocument,
    external: &RadialDocument,
) -> Result<RadialDocument, AuthoringError> {
    fn merge(
        base: &serde_json::Value,
        local: &serde_json::Value,
        external: &serde_json::Value,
    ) -> Result<serde_json::Value, AuthoringError> {
        if local == base {
            return Ok(external.clone());
        }
        if external == base || local == external {
            return Ok(local.clone());
        }
        if let (
            serde_json::Value::Object(base),
            serde_json::Value::Object(local),
            serde_json::Value::Object(external),
        ) = (base, local, external)
        {
            let mut keys = base
                .keys()
                .chain(local.keys())
                .chain(external.keys())
                .collect::<Vec<_>>();
            keys.sort();
            keys.dedup();
            let mut result = serde_json::Map::new();
            for key in keys {
                let missing = serde_json::Value::Null;
                let value = merge(
                    base.get(key).unwrap_or(&missing),
                    local.get(key).unwrap_or(&missing),
                    external.get(key).unwrap_or(&missing),
                )?;
                if !value.is_null() || local.contains_key(key) || external.contains_key(key) {
                    result.insert(key.clone(), value);
                }
            }
            return Ok(serde_json::Value::Object(result));
        }
        Err(AuthoringError::RebaseConflict)
    }

    let value = merge(
        &serde_json::to_value(base).map_err(|_| AuthoringError::RebaseConflict)?,
        &serde_json::to_value(local).map_err(|_| AuthoringError::RebaseConflict)?,
        &serde_json::to_value(external).map_err(|_| AuthoringError::RebaseConflict)?,
    )?;
    serde_json::from_value(value).map_err(|_| AuthoringError::RebaseConflict)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview_resource_diagnostic(identity: &str) -> super::super::diagnostics::RadialDiagnostic {
        super::super::diagnostics::RadialDiagnostic::new(
            super::super::diagnostics::RadialDiagnosticSeverity::Error,
            super::super::diagnostics::RadialDiagnosticKind::AssetUnavailable(
                super::super::assets::AssetDiagnostic::NotFound,
            ),
            super::super::diagnostics::RadialDiagnosticSource::Asset {
                menu_id: MenuId::new("preview-test"),
                identity: identity.to_owned(),
            },
            identity,
            format!("preview asset unavailable: {identity}"),
        )
    }

    fn snapshot(name: &str, revision: u64) -> AuthoringSnapshot {
        let mut document = RadialDocument::starter();
        document.revision = ConfigRevision(revision);
        document.menus[0].name = name.into();
        AuthoringSnapshot::new(Arc::new(document), format!("sha-{revision}"))
    }

    #[test]
    fn pending_request_classes_and_delivery_reconciliation_are_exact() {
        let disposable = [
            PendingRequestKind::Snapshot,
            PendingRequestKind::FontCatalog,
            PendingRequestKind::PrepareEmbeddedPreview,
            PendingRequestKind::ExportPackage,
            PendingRequestKind::ExportSkin,
            PendingRequestKind::AuditionManagedAsset,
            PendingRequestKind::LivePreview,
            PendingRequestKind::CancelPreview,
        ];
        for kind in disposable {
            assert!(kind.is_disposable());
            assert!(!kind.is_durable());
        }
        for kind in [
            PendingRequestKind::StartNativePreview,
            PendingRequestKind::UpdateNativePreview,
            PendingRequestKind::StopNativePreview,
        ] {
            assert!(kind.is_preview_lifecycle());
            assert!(!kind.is_durable());
        }
        for kind in [
            PendingRequestKind::Commit(CommitDisposition::Apply),
            PendingRequestKind::Commit(CommitDisposition::Save),
            PendingRequestKind::Commit(CommitDisposition::RevertAppliedAndClose),
            PendingRequestKind::ReplacePackage,
        ] {
            assert!(kind.is_durable());
            assert!(!kind.is_disposable());
        }

        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let pending = PendingAuthoringRequest {
            id: AuthoringRequestId(11),
            generation: session.generation,
            editor_session: session.editor_session,
            kind: PendingRequestKind::FontCatalog,
        };
        session.pending_request = Some(pending);
        let stale = PendingAuthoringRequest {
            id: AuthoringRequestId(12),
            ..pending
        };
        assert!(!session.reconcile_request_delivery_failure(stale, "stale send"));
        assert_eq!(session.pending_request, Some(pending));
        assert!(session.reconcile_request_delivery_failure(pending, "send failed"));
        assert!(session.pending_request.is_none());
        assert_eq!(session.last_error.as_deref(), Some("send failed"));

        let menu = session.draft.default_menu_id.clone();
        let start = session
            .request_start_native_preview(menu, false, None)
            .unwrap();
        let stop = session.request_stop_native_preview().unwrap();
        assert!(matches!(stop, AuthoringRequest::StopNativePreview { .. }));
        assert_ne!(start.id(), stop.id());
        assert_eq!(
            session.pending_native_preview.map(|pending| pending.kind),
            Some(PendingRequestKind::StopNativePreview)
        );
    }

    #[test]
    fn rename_uses_stable_id_and_undo_redo() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let id = session.draft.menus[0].id.clone();
        session.select(Some(StableSelection::Menu(id.clone())));
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: id.clone(),
                    name: "Work".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert_eq!(session.draft.menus[0].id, id);
        assert_eq!(session.selection, Some(StableSelection::Menu(id.clone())));
        assert_eq!(session.draft.menus[0].name, "Work");
        assert!(session.undo());
        assert_eq!(session.draft.menus[0].name, "Starter");
        assert!(session.redo());
        assert_eq!(session.draft.menus[0].name, "Work");
    }

    #[test]
    fn coalesced_field_edits_are_one_undo_step() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let id = session.draft.menus[0].id.clone();
        let key = EditKey {
            entity: id.to_string(),
            field: "name".into(),
        };
        for (index, name) in ["S", "St", "Styl"].into_iter().enumerate() {
            session
                .mutate(
                    DocumentMutation::RenameMenu {
                        id: id.clone(),
                        name: name.into(),
                    },
                    Some(key.clone()),
                    if index == 0 {
                        EditPhase::Begin
                    } else {
                        EditPhase::Update
                    },
                )
                .unwrap();
        }
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "Style".into(),
                },
                Some(key),
                EditPhase::End,
            )
            .unwrap();
        assert!(session.undo());
        assert_eq!(session.draft.menus[0].name, "Starter");
        assert!(!session.undo());
    }

    #[test]
    fn font_catalog_request_is_session_correlated_and_cached_in_editor_state() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let request = session.request_font_catalog().unwrap();
        let (id, generation, editor_session) =
            (request.id(), request.generation(), request.editor_session());
        assert!(matches!(request, AuthoringRequest::FontCatalog { .. }));
        assert!(!session.font_catalog_loaded);
        assert!(session.accept_reply(AuthoringReply::FontCatalog {
            id,
            generation,
            editor_session,
            families: Arc::from(["Consolas".to_owned(), "Segoe UI".to_owned()]),
        }));
        assert!(session.font_catalog_loaded);
        assert_eq!(&*session.font_families, &["Consolas", "Segoe UI"]);
    }

    #[test]
    fn embedded_preview_frame_reply_is_generation_and_token_correlated() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let document = Arc::clone(&session.draft);
        let menu_id = document.default_menu_id.clone();
        let anchor = PhysicalPoint { x: 200.0, y: 200.0 };
        let work_area = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 400.0, y: 400.0 },
        };
        let scale = ScaleFactor::new(1.0).unwrap();
        let request = session
            .request_embedded_preview(
                Arc::clone(&document),
                menu_id.clone(),
                None,
                anchor,
                work_area,
                scale,
                "frame-token".into(),
                0,
                None,
            )
            .unwrap();
        let mut preparer = crate::radial::preparation::PreviewFramePreparer::new(PathBuf::new());
        let input = Arc::new(
            preparer
                .prepare(
                    &document,
                    &menu_id,
                    anchor,
                    work_area,
                    scale,
                    request.generation().0,
                    None,
                    &PreviewProjection::default(),
                )
                .unwrap(),
        );
        assert!(
            !session.accept_reply(AuthoringReply::EmbeddedPreviewPrepared {
                id: AuthoringRequestId(request.id().0 + 1),
                generation: request.generation(),
                editor_session: request.editor_session(),
                token: "stale".into(),
                input: Arc::clone(&input),
            })
        );
        assert!(session.embedded_preview.is_none());
        assert!(
            session.accept_reply(AuthoringReply::EmbeddedPreviewPrepared {
                id: request.id(),
                generation: request.generation(),
                editor_session: request.editor_session(),
                token: "frame-token".into(),
                input: Arc::clone(&input),
            })
        );
        assert_eq!(
            session.embedded_preview.as_ref(),
            Some(&("frame-token".into(), input))
        );
    }

    #[test]
    fn draft_edit_retires_stale_embedded_preview_and_allows_fresh_work() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let old_document = Arc::clone(&session.draft);
        let menu_id = old_document.default_menu_id.clone();
        let old_request = session
            .request_embedded_preview(
                old_document.clone(),
                menu_id.clone(),
                None,
                PhysicalPoint { x: 200.0, y: 200.0 },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint { x: 400.0, y: 400.0 },
                },
                ScaleFactor::new(1.0).unwrap(),
                "old-frame".into(),
                0,
                None,
            )
            .unwrap();
        let old_generation = old_request.generation();

        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id.clone(),
                    name: "Edited".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();

        assert_eq!(session.generation, DraftGeneration(old_generation.0 + 1));
        assert!(session.pending_request.is_none());

        let mut preparer = crate::radial::preparation::PreviewFramePreparer::new(PathBuf::new());
        let old_input = Arc::new(
            preparer
                .prepare(
                    &old_document,
                    &menu_id,
                    PhysicalPoint { x: 200.0, y: 200.0 },
                    PhysicalRect {
                        min: PhysicalPoint { x: 0.0, y: 0.0 },
                        max: PhysicalPoint { x: 400.0, y: 400.0 },
                    },
                    ScaleFactor::new(1.0).unwrap(),
                    old_generation.0,
                    None,
                    &PreviewProjection::default(),
                )
                .unwrap(),
        );
        assert!(
            !session.accept_reply(AuthoringReply::EmbeddedPreviewPrepared {
                id: old_request.id(),
                generation: old_generation,
                editor_session: old_request.editor_session(),
                token: "old-frame".into(),
                input: old_input,
            })
        );
        assert!(session.embedded_preview.is_none());

        let fresh_request = session
            .request_embedded_preview(
                Arc::clone(&session.draft),
                menu_id,
                None,
                PhysicalPoint { x: 200.0, y: 200.0 },
                PhysicalRect {
                    min: PhysicalPoint { x: 0.0, y: 0.0 },
                    max: PhysicalPoint { x: 400.0, y: 400.0 },
                },
                ScaleFactor::new(1.0).unwrap(),
                "fresh-frame".into(),
                0,
                None,
            )
            .unwrap();
        assert_eq!(fresh_request.generation(), session.generation);
        assert_ne!(fresh_request.id(), old_request.id());
        let AuthoringRequest::PrepareEmbeddedPreview {
            candidate,
            menu_id,
            selected,
            anchor,
            work_area,
            scale,
            token,
            projection,
            ..
        } = fresh_request
        else {
            panic!("expected a fresh embedded preview request")
        };
        let fresh_input = Arc::new(
            preparer
                .prepare(
                    &candidate,
                    &menu_id,
                    anchor,
                    work_area,
                    scale,
                    session.generation.0,
                    selected.as_ref(),
                    &projection,
                )
                .unwrap(),
        );
        let fresh_pending = session.pending_request.expect("fresh pending");
        assert!(
            session.accept_reply(AuthoringReply::EmbeddedPreviewPrepared {
                id: fresh_pending.id,
                generation: session.generation,
                editor_session: session.editor_session,
                token,
                input: fresh_input,
            })
        );
        assert!(session.request_commit(CommitDisposition::Save).is_ok());
    }

    #[test]
    fn two_rapid_edits_ignore_out_of_order_disposable_replies() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let menu_id = session.draft.default_menu_id.clone();
        let first = session.request_live_preview().unwrap();

        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id.clone(),
                    name: "First".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert!(session.pending_request.is_none());

        let second = session.request_live_preview().unwrap();
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: first.id(),
            generation: first.generation(),
            editor_session: first.editor_session(),
        }));
        assert_eq!(
            session.pending_request,
            Some(PendingAuthoringRequest {
                id: second.id(),
                generation: second.generation(),
                editor_session: second.editor_session(),
                kind: PendingRequestKind::LivePreview,
            })
        );

        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Second".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: second.id(),
            generation: second.generation(),
            editor_session: second.editor_session(),
        }));

        let fresh = session.request_live_preview().unwrap();
        assert_eq!(fresh.generation(), session.generation);
    }

    #[test]
    fn undo_and_redo_terminalize_disposable_work() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let menu_id = session.draft.default_menu_id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id.clone(),
                    name: "Edited".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();

        let undo_request = session.request_live_preview().unwrap();
        assert!(session.undo());
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: undo_request.id(),
            generation: undo_request.generation(),
            editor_session: undo_request.editor_session(),
        }));
        assert_eq!(session.draft.menus[0].name, "Starter");

        let redo_request = session.request_live_preview().unwrap();
        assert!(session.redo());
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: redo_request.id(),
            generation: redo_request.generation(),
            editor_session: redo_request.editor_session(),
        }));
        assert_eq!(session.draft.menus[0].name, "Edited");
    }

    #[test]
    fn font_catalog_reply_survives_a_draft_generation_change() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let request = session.request_font_catalog().unwrap();
        let menu_id = session.draft.default_menu_id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Edited".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert_eq!(
            session.pending_request.map(|pending| pending.kind),
            Some(PendingRequestKind::FontCatalog)
        );
        assert!(session.accept_reply(AuthoringReply::FontCatalog {
            id: request.id(),
            generation: request.generation(),
            editor_session: request.editor_session(),
            families: Arc::from(["Segoe UI".to_owned()]),
        }));
        assert!(session.pending_request.is_none());
        assert!(session.font_catalog_loaded);
        assert_eq!(session.draft.menus[0].name, "Edited");
    }

    #[test]
    fn durable_work_rejects_all_draft_generation_changes_and_stays_correlated() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let menu_id = session.draft.default_menu_id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id.clone(),
                    name: "Before save".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        let before_draft = Arc::clone(&session.draft);
        let before_generation = session.generation;
        let request = session.request_commit(CommitDisposition::Apply).unwrap();
        let pending = session.pending_request;

        assert_eq!(
            session.mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Must not stick".into(),
                },
                None,
                EditPhase::Atomic,
            ),
            Err(AuthoringError::RequestPending)
        );
        assert_eq!(session.draft, before_draft);
        assert_eq!(session.generation, before_generation);
        assert_eq!(session.pending_request, pending);
        assert!(!session.undo());
        assert_eq!(session.pending_request, pending);

        session.pending_request = Some(PendingAuthoringRequest {
            id: AuthoringRequestId(request.id().0 + 1),
            generation: request.generation(),
            editor_session: request.editor_session(),
            kind: PendingRequestKind::ReplacePackage,
        });
        let replacement = (*session.draft).clone();
        assert_eq!(
            session.replace_document_atomic(replacement),
            Err(AuthoringError::RequestPending)
        );
        assert_eq!(session.generation, before_generation);
        assert_eq!(
            session.pending_request.map(|pending| pending.kind),
            Some(PendingRequestKind::ReplacePackage)
        );
    }

    #[test]
    fn asset_stage_generation_changes_invalidate_disposable_work() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let addition = ManagedAssetAddition {
            record: AssetRecord {
                id: AssetId::new("draft-asset"),
                kind: super::super::model::MediaKind::Image,
                relative_path: "draft.png".into(),
                content_sha256: "a".repeat(64),
                byte_len: 3,
            },
            bytes: Arc::from([1_u8, 2, 3]),
        };

        let first = session.request_live_preview().unwrap();
        assert!(session.stage_asset_addition(addition).is_ok());
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: first.id(),
            generation: first.generation(),
            editor_session: first.editor_session(),
        }));

        let second = session.request_live_preview().unwrap();
        session.stage_asset_delete(AssetId::new("draft-asset"));
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: second.id(),
            generation: second.generation(),
            editor_session: second.editor_session(),
        }));

        let third = session.request_live_preview().unwrap();
        let mut replacement = (*session.draft).clone();
        replacement
            .metadata
            .insert("replacement".into(), "yes".into());
        assert!(
            session
                .replace_document_and_assets_atomic(replacement, session.pending_assets.clone())
                .is_ok()
        );
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: third.id(),
            generation: third.generation(),
            editor_session: third.editor_session(),
        }));
    }

    #[test]
    fn conflict_resolution_uses_the_same_generation_transition_policy() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let menu_id = session.draft.default_menu_id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Local".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        let stale = session.request_live_preview().unwrap();
        session.observe_external(snapshot("External", 2));
        let before_resolution = session.generation;
        assert!(session.conflict.is_some());
        assert!(session.pending_request.is_none());

        session
            .resolve_conflict(ConflictResolution::Reload)
            .unwrap();
        assert!(session.generation.0 > before_resolution.0);
        assert!(session.pending_request.is_none());
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: stale.id(),
            generation: stale.generation(),
            editor_session: stale.editor_session(),
        }));
        assert_eq!(session.draft.menus[0].name, "External");
    }

    #[test]
    fn mismatched_disposable_replies_preserve_the_newer_pending_request() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let newer = session.request_live_preview().unwrap();
        let pending = session.pending_request;

        assert!(!session.accept_reply(AuthoringReply::PreviewCancelled {
            id: newer.id(),
            generation: newer.generation(),
            editor_session: newer.editor_session(),
        }));
        assert_eq!(session.pending_request, pending);
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: AuthoringRequestId(newer.id().0 + 1),
            generation: newer.generation(),
            editor_session: newer.editor_session(),
        }));
        assert_eq!(session.pending_request, pending);
    }

    #[test]
    fn exact_terminal_stale_disposable_reply_retires_only_its_slot() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let stale = session.request_live_preview().unwrap();
        session.generation.0 += 1;
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: stale.id(),
            generation: stale.generation(),
            editor_session: stale.editor_session(),
        }));
        assert!(session.pending_request.is_none());

        let newer = session.request_live_preview().unwrap();
        assert!(!session.accept_reply(AuthoringReply::PreviewAccepted {
            id: stale.id(),
            generation: stale.generation(),
            editor_session: stale.editor_session(),
        }));
        assert_eq!(
            session.pending_request.map(|pending| pending.id),
            Some(newer.id())
        );
    }

    #[test]
    fn stale_reply_from_a_closed_session_cannot_complete_a_reopened_editor() {
        let mut closed = RadialAuthoringSession::new(snapshot("Closed", 1));
        let stale = closed.request_live_preview().unwrap();
        let mut reopened = RadialAuthoringSession::new(snapshot("Reopened", 1));
        let current = reopened.request_live_preview().unwrap();
        assert_eq!(stale.id(), current.id());
        assert_ne!(stale.editor_session(), current.editor_session());

        assert!(!reopened.accept_reply(AuthoringReply::PreviewAccepted {
            id: stale.id(),
            generation: stale.generation(),
            editor_session: stale.editor_session(),
        }));
        assert_eq!(
            reopened.pending_request.map(|pending| pending.id),
            Some(current.id())
        );
    }

    #[test]
    fn ring_resize_is_atomic_and_does_not_partially_discard() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let menu = session.draft.menus[0].clone();
        let before = menu.rings[0].cells.clone();
        let retained = before[..2].to_vec();
        session
            .mutate(
                DocumentMutation::ReplaceRingCells {
                    menu_id: menu.id,
                    ring_id: menu.rings[0].id.clone(),
                    cells: retained,
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert_eq!(session.draft.menus[0].rings[0].cells.len(), 2);
        assert!(session.undo());
        assert_eq!(session.draft.menus[0].rings[0].cells, before);
    }

    #[test]
    fn duplication_graph_is_one_atomic_stable_id_edit() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let original_menu_count = session.draft.menus.len();
        let mut document = (*session.draft).clone();
        let mut duplicate = document.menus[0].clone();
        duplicate.id = MenuId::new("duplicate");
        duplicate.name = "Duplicate".into();
        duplicate.rings[0].id = RingId::new("duplicate-ring");
        for (index, cell) in duplicate.rings[0].cells.iter_mut().enumerate() {
            cell.id = CellId::new(format!("duplicate-cell-{index}"));
        }
        document.menus.push(duplicate);
        session.replace_document_atomic(document).unwrap();
        assert_eq!(session.draft.menus.len(), original_menu_count + 1);
        assert_eq!(session.draft.menus.last().unwrap().id.as_str(), "duplicate");
        assert!(session.undo());
        assert_eq!(session.draft.menus.len(), original_menu_count);
        assert!(!session.undo());
    }

    #[test]
    fn dirty_external_publish_conflicts_clean_publish_refreshes() {
        let mut clean = RadialAuthoringSession::new(snapshot("A", 1));
        clean.observe_external(snapshot("B", 2));
        assert_eq!(clean.draft.menus[0].name, "B");
        assert!(clean.conflict.is_none());

        let mut dirty = RadialAuthoringSession::new(snapshot("A", 1));
        let id = dirty.draft.menus[0].id.clone();
        dirty
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "Local".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        dirty.observe_external(snapshot("External", 2));
        assert_eq!(dirty.draft.menus[0].name, "Local");
        assert!(dirty.conflict.is_some());
    }

    #[test]
    fn nonoverlapping_conflict_rebases_and_same_field_conflict_stays_explicit() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let original_metadata_count = session.draft.metadata.len();
        let mut local = (*session.draft).clone();
        local.metadata.insert("local".into(), "yes".into());
        session
            .mutate(
                DocumentMutation::ReplaceDocument { document: local },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        let mut external = snapshot("A", 2);
        Arc::make_mut(&mut external.document)
            .metadata
            .insert("external".into(), "yes".into());
        session.observe_external(external);
        session
            .resolve_conflict(ConflictResolution::Rebase)
            .unwrap();
        assert_eq!(session.draft.metadata.len(), original_metadata_count + 2);

        let mut conflict = RadialAuthoringSession::new(snapshot("A", 1));
        let id = conflict.draft.menus[0].id.clone();
        conflict
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "Local".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        conflict.observe_external(snapshot("External", 2));
        assert_eq!(
            conflict.resolve_conflict(ConflictResolution::Rebase),
            Err(AuthoringError::RebaseConflict)
        );
        assert!(conflict.conflict.is_some());
    }

    #[test]
    fn stale_async_reply_is_ignored() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let request = session.request_commit(CommitDisposition::Apply).unwrap();
        let stale = AuthoringReply::Failed {
            id: request.id(),
            generation: DraftGeneration(request.generation().0 + 1),
            editor_session: session.editor_session,
            message: "stale".into(),
        };
        assert!(!session.accept_reply(stale));
        assert!(session.pending_request.is_some());
    }

    #[test]
    fn package_requests_require_confirmation_and_reject_stale_results() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let source = RadialDocument::starter();
        let export = super::super::package::plan_export(
            &source,
            &[source.default_menu_id.clone()],
            &Default::default(),
            Vec::new(),
        )
        .unwrap();
        let files = super::super::package::decode_mlradial(
            &super::super::package::encode_mlradial(&export).unwrap(),
        )
        .unwrap();
        let plan = super::super::package::plan_import(files, &session.draft).unwrap();
        let revision = session.baseline.revision;
        let disk_sha256 = session.baseline.disk_sha256.0.clone();
        let generation = session.generation;
        assert_eq!(
            session.request_replace_package(
                plan.clone(),
                revision,
                &disk_sha256,
                generation,
                PathBuf::from("backup.json"),
                false,
            ),
            Err(AuthoringError::ConfirmationRequired)
        );
        assert!(session.pending_request.is_none());
        assert_eq!(
            session.request_replace_package(
                plan.clone(),
                revision,
                &disk_sha256,
                DraftGeneration(generation.0 + 1),
                PathBuf::from("backup.json"),
                true,
            ),
            Err(AuthoringError::StalePreview)
        );

        let request = session
            .request_replace_package(
                plan,
                revision,
                &disk_sha256,
                generation,
                PathBuf::from("backup.json"),
                true,
            )
            .unwrap();
        assert!(!session.accept_reply(AuthoringReply::PackageReplaced {
            id: request.id(),
            generation: DraftGeneration(request.generation().0 + 1),
            editor_session: session.editor_session,
            snapshot: snapshot("replacement", 2),
            backup_path: PathBuf::from("backup.json"),
        }));
        assert_eq!(session.draft.menus[0].name, "A");
        assert!(session.accept_reply(AuthoringReply::Failed {
            id: request.id(),
            generation: request.generation(),
            editor_session: session.editor_session,
            message: "replacement rejected after stale reply".into(),
        }));

        let export = session
            .request_export_package(vec![session.draft.default_menu_id.clone()])
            .unwrap();
        assert!(session.accept_reply(AuthoringReply::PackageExported {
            id: export.id(),
            generation: export.generation(),
            editor_session: session.editor_session,
            bytes: Arc::from(&b"package"[..]),
        }));
        assert_eq!(session.exported_package.as_deref(), Some(&b"package"[..]));
    }

    #[test]
    fn native_preview_lease_cannot_survive_generation_save_or_conflict() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let menu = session.draft.default_menu_id.clone();
        let start = session
            .request_start_native_preview(menu.clone(), false, None)
            .unwrap();
        let stale_lease = NativePreviewLease {
            editor_session: session.editor_session,
            generation: start.generation(),
            request_id: start.id(),
        };
        let stop = session.request_stop_native_preview().unwrap();
        assert!(matches!(stop, AuthoringRequest::StopNativePreview { .. }));
        session.pending_native_preview = Some(PendingAuthoringRequest {
            id: start.id(),
            generation: start.generation(),
            editor_session: session.editor_session,
            kind: PendingRequestKind::StartNativePreview,
        });
        session.generation.0 += 1;
        assert!(!session.accept_reply(AuthoringReply::NativePreviewStarted {
            id: start.id(),
            generation: start.generation(),
            editor_session: session.editor_session,
            lease: stale_lease,
            sampled_context: super::super::context::InvocationContext::empty(1),
            diagnostics: Vec::new(),
        }));
        assert!(session.native_preview_lease.is_none());
        // The test advanced generation by direct field access, bypassing the
        // production mutation boundary that invalidates pending preview work.
        session.pending_native_preview = None;

        let start = session
            .request_start_native_preview(menu, false, None)
            .unwrap();
        let lease = NativePreviewLease {
            editor_session: session.editor_session,
            generation: start.generation(),
            request_id: start.id(),
        };
        assert!(session.accept_reply(AuthoringReply::NativePreviewStarted {
            id: start.id(),
            generation: start.generation(),
            editor_session: session.editor_session,
            lease: lease.clone(),
            sampled_context: super::super::context::InvocationContext::empty(2),
            diagnostics: vec![preview_resource_diagnostic("draft image fallback")],
        }));
        assert_eq!(session.native_preview_lease, Some(lease));
        assert_eq!(
            session.native_preview_diagnostics,
            [preview_resource_diagnostic("draft image fallback")]
        );
        let active_lease = session.native_preview_lease.clone().unwrap();
        assert!(
            session.accept_reply(AuthoringReply::NativePreviewDiagnostics {
                editor_session: session.editor_session,
                lease: active_lease.clone(),
                diagnostics: vec![preview_resource_diagnostic("child image missing")],
            })
        );
        assert_eq!(
            session.native_preview_diagnostics,
            [preview_resource_diagnostic("child image missing")]
        );
        let mut stale = active_lease;
        stale.generation.0 += 1;
        assert!(
            !session.accept_reply(AuthoringReply::NativePreviewDiagnostics {
                editor_session: session.editor_session,
                lease: stale,
                diagnostics: Vec::new(),
            })
        );
        assert_eq!(
            session.native_preview_diagnostics,
            [preview_resource_diagnostic("child image missing")]
        );
        let menu_id = session.draft.default_menu_id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Local".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        session.observe_external(snapshot("External", 2));
        assert!(session.native_preview_lease.is_none());
        assert!(session.conflict.is_some());

        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let start = session
            .request_start_native_preview(session.draft.default_menu_id.clone(), false, None)
            .unwrap();
        let lease = NativePreviewLease {
            editor_session: session.editor_session,
            generation: start.generation(),
            request_id: start.id(),
        };
        assert!(session.accept_reply(AuthoringReply::NativePreviewStarted {
            id: start.id(),
            generation: start.generation(),
            editor_session: session.editor_session,
            lease,
            sampled_context: super::super::context::InvocationContext::empty(3),
            diagnostics: Vec::new(),
        }));
        let save = session.request_commit(CommitDisposition::Save).unwrap();
        assert!(session.accept_reply(AuthoringReply::Published {
            id: save.id(),
            generation: save.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::Save,
            snapshot: snapshot("Saved", 3),
            rollback_assets: AssetMutations::default(),
        }));
        assert!(session.native_preview_lease.is_none());
    }

    #[test]
    fn only_explicit_context_samples_replace_the_authoring_picker_context() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let menu = session.draft.default_menu_id.clone();
        let start = session
            .request_start_native_preview(menu.clone(), true, None)
            .unwrap();
        let first = super::super::context::InvocationContext::empty(11);
        let lease = NativePreviewLease {
            editor_session: session.editor_session,
            generation: start.generation(),
            request_id: start.id(),
        };
        assert!(session.accept_reply(AuthoringReply::NativePreviewStarted {
            id: start.id(),
            generation: start.generation(),
            editor_session: session.editor_session,
            lease: lease.clone(),
            sampled_context: first.clone(),
            diagnostics: Vec::new(),
        }));
        assert_eq!(session.sampled_preview_context.as_ref(), Some(&first));

        let update = session
            .request_update_native_preview(menu.clone(), false, None)
            .unwrap();
        let unsampled_lease = NativePreviewLease {
            editor_session: session.editor_session,
            generation: update.generation(),
            request_id: update.id(),
        };
        assert!(session.accept_reply(AuthoringReply::NativePreviewUpdated {
            id: update.id(),
            generation: update.generation(),
            editor_session: session.editor_session,
            lease: unsampled_lease,
            sampled_context: super::super::context::InvocationContext::empty(12),
            diagnostics: Vec::new(),
        }));
        assert_eq!(session.sampled_preview_context.as_ref(), Some(&first));

        let update = session
            .request_update_native_preview(menu, true, None)
            .unwrap();
        let latest = super::super::context::InvocationContext::empty(13);
        assert!(session.accept_reply(AuthoringReply::NativePreviewUpdated {
            id: update.id(),
            generation: update.generation(),
            editor_session: session.editor_session,
            lease: NativePreviewLease {
                editor_session: session.editor_session,
                generation: update.generation(),
                request_id: update.id(),
            },
            sampled_context: latest.clone(),
            diagnostics: Vec::new(),
        }));
        assert_eq!(session.sampled_preview_context.as_ref(), Some(&latest));
    }

    #[test]
    fn asset_staging_participates_in_bounded_undo() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let record = AssetRecord {
            id: AssetId::new("draft-asset"),
            kind: super::super::model::MediaKind::Image,
            relative_path: "draft.png".into(),
            content_sha256: "a".repeat(64),
            byte_len: 3,
        };
        session
            .stage_asset_addition(ManagedAssetAddition {
                record: record.clone(),
                bytes: Arc::from([1_u8, 2, 3]),
            })
            .unwrap();
        assert_eq!(session.pending_assets.additions.len(), 1);
        assert!(session.undo());
        assert!(session.pending_assets.is_empty());
        assert!(session.redo());
        assert_eq!(session.pending_assets.additions[0].record, record);
    }

    #[test]
    fn preview_request_rejects_corrupt_pending_overlay_before_main_or_disk_access() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let record = AssetRecord {
            id: AssetId::new("corrupt-preview"),
            kind: super::super::model::MediaKind::Image,
            relative_path: "corrupt.png".into(),
            content_sha256: "0".repeat(64),
            byte_len: 3,
        };
        session
            .stage_asset_addition(ManagedAssetAddition {
                record,
                bytes: Arc::from([1_u8, 2, 3]),
            })
            .unwrap();
        let menu = session.draft.default_menu_id.clone();
        let result = session.request_embedded_preview(
            Arc::clone(&session.draft),
            menu,
            None,
            PhysicalPoint { x: 100.0, y: 100.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 200.0, y: 200.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            "corrupt-overlay".into(),
            0,
            None,
        );
        assert!(matches!(
            result,
            Err(AuthoringError::AssetOverlayInvalid(_))
        ));
        assert!(session.pending_request.is_none());
    }

    #[test]
    fn undo_count_is_strictly_bounded() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let id = session.draft.menus[0].id.clone();
        for index in 0..(MAX_UNDO_ENTRIES + 20) {
            session
                .mutate(
                    DocumentMutation::RenameMenu {
                        id: id.clone(),
                        name: format!("menu-{index}"),
                    },
                    None,
                    EditPhase::Atomic,
                )
                .unwrap();
        }
        assert!(session.history.undo.len() <= MAX_UNDO_ENTRIES);
        assert!(session.history.bytes <= MAX_UNDO_BYTES);
    }

    #[test]
    fn apply_resets_checkpoint_save_closes_and_dirty_close_prompts() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let id = session.draft.menus[0].id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "B".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        assert_eq!(session.close_decision(), CloseDecision::PromptDirty);
        let request = session.request_commit(CommitDisposition::Apply).unwrap();
        let published = snapshot("B", 2);
        assert!(session.accept_reply(AuthoringReply::Published {
            id: request.id(),
            generation: request.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::Apply,
            snapshot: published,
            rollback_assets: AssetMutations::default(),
        }));
        assert!(!session.is_dirty());
        assert!(!session.is_closed());
        let save = session.request_commit(CommitDisposition::Save).unwrap();
        assert!(session.accept_reply(AuthoringReply::Published {
            id: save.id(),
            generation: save.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::Save,
            snapshot: snapshot("B", 3),
            rollback_assets: AssetMutations::default(),
        }));
        assert!(session.is_closed());
    }

    #[test]
    fn cancel_after_apply_requests_checked_revert_of_last_apply() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        let id = session.draft.menus[0].id.clone();
        session
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "B".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        let apply = session.request_commit(CommitDisposition::Apply).unwrap();
        assert!(session.accept_reply(AuthoringReply::Published {
            id: apply.id(),
            generation: apply.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::Apply,
            snapshot: snapshot("B", 2),
            rollback_assets: AssetMutations::default(),
        }));
        let revert = session
            .request_commit(CommitDisposition::RevertAppliedAndClose)
            .unwrap();
        let AuthoringRequest::Commit {
            candidate,
            expected_revision,
            ..
        } = &revert
        else {
            panic!("cancel-after-apply must be a checked commit")
        };
        assert_eq!(candidate.menus[0].name, "A");
        assert_eq!(*expected_revision, ConfigRevision(2));
        assert!(session.accept_reply(AuthoringReply::Published {
            id: revert.id(),
            generation: revert.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::RevertAppliedAndClose,
            snapshot: snapshot("A", 3),
            rollback_assets: AssetMutations::default(),
        }));
        assert!(session.is_closed());
    }

    fn accept_apply(session: &mut RadialAuthoringSession, name: &str, revision: u64) {
        let request = session.request_commit(CommitDisposition::Apply).unwrap();
        assert!(session.accept_reply(AuthoringReply::Published {
            id: request.id(),
            generation: request.generation(),
            editor_session: session.editor_session,
            disposition: CommitDisposition::Apply,
            snapshot: snapshot(name, revision),
            rollback_assets: AssetMutations::default(),
        }));
    }

    #[test]
    fn external_resolution_invalidates_apply_cancel_checkpoint() {
        let mut reloaded = RadialAuthoringSession::new(snapshot("A", 1));
        accept_apply(&mut reloaded, "B", 2);
        let id = reloaded.draft.menus[0].id.clone();
        reloaded
            .mutate(
                DocumentMutation::RenameMenu {
                    id,
                    name: "Local".into(),
                },
                None,
                EditPhase::Atomic,
            )
            .unwrap();
        reloaded.observe_external(snapshot("External", 3));
        reloaded
            .resolve_conflict(ConflictResolution::Reload)
            .unwrap();
        assert_eq!(
            reloaded.request_commit(CommitDisposition::RevertAppliedAndClose),
            Err(AuthoringError::NothingToRevert)
        );
        assert_eq!(reloaded.draft.menus[0].name, "External");

        let mut rebased = RadialAuthoringSession::new(snapshot("A", 1));
        accept_apply(&mut rebased, "B", 2);
        let mut local = (*rebased.draft).clone();
        local.metadata.insert("local".into(), "yes".into());
        rebased.replace_document_atomic(local).unwrap();
        let mut external = snapshot("B", 3);
        Arc::make_mut(&mut external.document)
            .metadata
            .insert("external".into(), "yes".into());
        rebased.observe_external(external);
        rebased
            .resolve_conflict(ConflictResolution::Rebase)
            .unwrap();
        assert_eq!(
            rebased.request_commit(CommitDisposition::RevertAppliedAndClose),
            Err(AuthoringError::NothingToRevert)
        );
        assert_eq!(
            rebased.draft.metadata.get("local").map(String::as_str),
            Some("yes")
        );
        assert_eq!(
            rebased.draft.metadata.get("external").map(String::as_str),
            Some("yes")
        );
    }

    #[test]
    fn package_replacement_invalidates_apply_cancel_checkpoint() {
        let mut session = RadialAuthoringSession::new(snapshot("A", 1));
        accept_apply(&mut session, "B", 2);
        let id = AuthoringRequestId(91);
        session.pending_request = Some(PendingAuthoringRequest {
            id,
            generation: session.generation,
            editor_session: session.editor_session,
            kind: PendingRequestKind::ReplacePackage,
        });
        assert!(session.accept_reply(AuthoringReply::PackageReplaced {
            id,
            generation: session.generation,
            editor_session: session.editor_session,
            snapshot: snapshot("Imported", 3),
            backup_path: PathBuf::from("backup.json"),
        }));
        assert_eq!(
            session.request_commit(CommitDisposition::RevertAppliedAndClose),
            Err(AuthoringError::NothingToRevert)
        );
        assert_eq!(session.draft.menus[0].name, "Imported");
    }

    #[test]
    fn session_identity_and_reply_kind_guard_reused_request_ids() {
        let mut old = RadialAuthoringSession::new(snapshot("Old", 1));
        let old_request = old.request_snapshot().unwrap();
        let mut current = RadialAuthoringSession::new(snapshot("Starter", 1));
        let current_request = current.request_snapshot().unwrap();
        assert_eq!(old_request.id(), current_request.id());
        assert_eq!(old_request.generation(), current_request.generation());
        assert_ne!(old.editor_session, current.editor_session);

        assert!(!current.accept_reply(AuthoringReply::Snapshot {
            id: old_request.id(),
            generation: old_request.generation(),
            editor_session: old.editor_session,
            snapshot: snapshot("Late old session", 2),
        }));
        assert!(current.is_initial_snapshot_pending());
        assert_eq!(current.draft.menus[0].name, "Starter");

        assert!(!current.accept_reply(AuthoringReply::PreviewCancelled {
            id: current_request.id(),
            generation: current_request.generation(),
            editor_session: current.editor_session,
        }));
        assert!(current.is_initial_snapshot_pending());
        assert!(current.accept_reply(AuthoringReply::Snapshot {
            id: current_request.id(),
            generation: current_request.generation(),
            editor_session: current.editor_session,
            snapshot: snapshot("Authoritative", 2),
        }));
        assert_eq!(current.draft.menus[0].name, "Authoritative");
    }

    #[test]
    fn initial_snapshot_pending_blocks_starter_mutation_and_generation_changes() {
        let mut session = RadialAuthoringSession::new(snapshot("Starter", 1));
        let request = session.request_snapshot().unwrap();
        let generation = session.generation;
        let menu_id = session.draft.menus[0].id.clone();
        assert_eq!(
            session.mutate(
                DocumentMutation::RenameMenu {
                    id: menu_id,
                    name: "Must not stick".into(),
                },
                None,
                EditPhase::Atomic,
            ),
            Err(AuthoringError::RequestPending)
        );
        assert_eq!(session.generation, generation);
        assert!(session.accept_reply(AuthoringReply::Snapshot {
            id: request.id(),
            generation: request.generation(),
            editor_session: session.editor_session,
            snapshot: snapshot("Disk", 2),
        }));
        assert_eq!(session.draft.menus[0].name, "Disk");
    }

    #[test]
    fn service_is_typed_and_bidirectional() {
        let (client, endpoint) = authoring_control_service();
        let request = AuthoringRequest::Snapshot {
            id: AuthoringRequestId(7),
            generation: DraftGeneration(3),
            editor_session: AuthoringSessionId(11),
        };
        client.send(request.clone()).unwrap();
        assert_eq!(endpoint.request_rx.try_recv().unwrap(), request);
        client.acquire_resources(AuthoringSessionId(11));
        client.release_resources(AuthoringSessionId(11));
        assert_eq!(
            endpoint.resource_rx.try_recv().unwrap(),
            AuthoringResourceDemand::Acquire(AuthoringSessionId(11))
        );
        assert_eq!(
            endpoint.resource_rx.try_recv().unwrap(),
            AuthoringResourceDemand::Release(AuthoringSessionId(11))
        );
        endpoint
            .reply_tx
            .send(AuthoringReply::PreviewCancelled {
                id: AuthoringRequestId(7),
                generation: DraftGeneration(3),
                editor_session: AuthoringSessionId(11),
            })
            .unwrap();
        assert_eq!(client.try_recv().unwrap().id(), AuthoringRequestId(7));
    }

    #[test]
    fn authoring_replies_wake_the_registered_designer_owner() {
        let ticks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (client, endpoint) = authoring_control_service_with_wake(None);
        let callback_ticks = std::sync::Arc::clone(&ticks);
        client.set_reply_wake(Some(std::sync::Arc::new(move || {
            callback_ticks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })));
        endpoint
            .reply_tx
            .send(AuthoringReply::PreviewCancelled {
                id: AuthoringRequestId(9),
                generation: DraftGeneration(1),
                editor_session: AuthoringSessionId(2),
            })
            .unwrap();
        assert_eq!(ticks.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(client.try_recv().unwrap().id(), AuthoringRequestId(9));
    }
}
