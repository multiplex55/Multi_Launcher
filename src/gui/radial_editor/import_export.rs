//! Side-effect-free package/legacy review and atomic draft acceptance.

use crate::radial::authoring::{
    AuthoringError, DraftGeneration, ManagedAssetAddition, RadialAuthoringSession,
};
use crate::radial::import::ImportPreview;
use crate::radial::model::{ConfigRevision, RadialDocument};
use crate::radial::package::{
    ImportPlan, PackageError, PackageManifest, PackagePayloadKind, SkinImportPlan, decode_mlradial,
    plan_import, plan_skin_import,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum ReviewedImport {
    Package(ImportPlan),
    Skin(SkinImportPlan),
    Legacy(ImportPreview),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PendingImport {
    pub review: ReviewedImport,
    pub revision: ConfigRevision,
    pub generation: DraftGeneration,
    pub disk_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ImportAcceptError {
    DraftChanged,
    ExternalConflict,
    AssetBudget,
}

impl PendingImport {
    pub(super) fn package(
        bytes: &[u8],
        session: &RadialAuthoringSession,
    ) -> Result<Self, PackageError> {
        let files = decode_mlradial(bytes)?;
        let manifest: PackageManifest = serde_json::from_slice(
            files
                .get(crate::radial::package::MANIFEST_FILE)
                .ok_or_else(|| {
                    PackageError::MissingFile(crate::radial::package::MANIFEST_FILE.into())
                })?,
        )
        .map_err(|error| PackageError::Malformed(error.to_string()))?;
        let review = match manifest.payload {
            PackagePayloadKind::MenuGraph => {
                ReviewedImport::Package(plan_import(files, &session.draft)?)
            }
            PackagePayloadKind::SkinBundle => {
                ReviewedImport::Skin(plan_skin_import(files, &session.draft)?)
            }
        };
        Ok(Self::new(review, session))
    }

    pub(super) fn legacy(preview: ImportPreview, session: &RadialAuthoringSession) -> Self {
        Self::new(ReviewedImport::Legacy(preview), session)
    }

    fn new(review: ReviewedImport, session: &RadialAuthoringSession) -> Self {
        Self {
            review,
            revision: session.baseline.revision,
            generation: session.generation,
            disk_sha256: session.baseline.disk_sha256.0.clone(),
        }
    }

    pub(super) fn plan(&self) -> Option<ImportPlan> {
        match &self.review {
            ReviewedImport::Package(plan) => Some(plan.clone()),
            ReviewedImport::Skin(_) => None,
            ReviewedImport::Legacy(preview) => Some(preview.apply_plan()),
        }
    }

    pub(super) fn warnings(&self) -> Vec<String> {
        match &self.review {
            ReviewedImport::Package(plan) => plan
                .manifest
                .notices
                .iter()
                .map(|notice| notice.message.clone())
                .collect(),
            ReviewedImport::Skin(plan) => plan
                .manifest
                .notices
                .iter()
                .map(|notice| notice.message.clone())
                .collect(),
            ReviewedImport::Legacy(preview) => preview
                .warnings
                .iter()
                .map(|warning| format!("{warning:?}"))
                .collect(),
        }
    }

    pub(super) fn destination(&self) -> String {
        match &self.review {
            ReviewedImport::Package(plan) => format!(
                "new menus: {}",
                plan.manifest
                    .root_menu_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ReviewedImport::Skin(plan) => format!("new skin: {}", plan.skin.name),
            ReviewedImport::Legacy(preview) => format!(
                "menu {}, skin {} ({})",
                preview.destination.menu_id,
                preview.destination.skin_id,
                preview.destination.display_name
            ),
        }
    }

    pub(super) fn mappings(&self) -> Vec<String> {
        match &self.review {
            ReviewedImport::Package(plan) => plan
                .remap
                .menus
                .iter()
                .chain(&plan.remap.skins)
                .map(|(from, to)| format!("{from} → {to}"))
                .collect(),
            ReviewedImport::Skin(plan) => plan
                .remap
                .skins
                .iter()
                .chain(&plan.remap.assets)
                .map(|(from, to)| format!("{from} → {to}"))
                .collect(),
            ReviewedImport::Legacy(preview) => preview
                .mappings
                .iter()
                .map(|mapping| format!("{mapping:?}"))
                .collect(),
        }
    }

    /// Merge the reviewed graph into the current draft as exactly one undo
    /// unit. A changed draft or observed external publication requires a new
    /// preview so stale mappings can never be silently accepted.
    pub(super) fn accept_create_new(
        self,
        session: &mut RadialAuthoringSession,
    ) -> Result<(), ImportAcceptError> {
        if session.conflict.is_some() {
            return Err(ImportAcceptError::ExternalConflict);
        }
        if session.baseline.revision != self.revision
            || session.baseline.disk_sha256.0 != self.disk_sha256
            || session.generation != self.generation
        {
            return Err(ImportAcceptError::DraftChanged);
        }
        let mut candidate = (*session.draft).clone();
        let mut mutations = session.pending_assets.clone();
        let (records, assets) = match &self.review {
            ReviewedImport::Package(plan) => {
                merge_create_new(&mut candidate, &plan.document);
                (&plan.document.assets, &plan.assets)
            }
            ReviewedImport::Skin(plan) => {
                candidate.skins.push(plan.skin.clone());
                candidate.assets.extend(plan.asset_records.clone());
                (&plan.asset_records, &plan.assets)
            }
            ReviewedImport::Legacy(preview) => {
                let plan = preview.apply_plan();
                merge_create_new(&mut candidate, &plan.document);
                return accept_candidate(
                    session,
                    candidate,
                    mutations,
                    &plan.document.assets,
                    &plan.assets,
                );
            }
        };
        for record in records {
            if let Some((_, bytes)) = assets.iter().find(|(path, bytes)| {
                path.starts_with("assets/")
                    && crate::radial::package::sha256_hex(bytes) == record.content_sha256
                    && bytes.len() as u64 == record.byte_len
            }) {
                mutations
                    .additions
                    .retain(|addition| addition.record.id != record.id);
                mutations.additions.push(ManagedAssetAddition {
                    record: record.clone(),
                    bytes: Arc::from(bytes.clone()),
                });
            }
        }
        session
            .replace_document_and_assets_atomic(candidate, mutations)
            .map_err(|error| match error {
                AuthoringError::AssetBudgetExceeded => ImportAcceptError::AssetBudget,
                _ => ImportAcceptError::DraftChanged,
            })
    }
}

fn accept_candidate(
    session: &mut RadialAuthoringSession,
    candidate: RadialDocument,
    mut mutations: crate::radial::authoring::AssetMutations,
    records: &[crate::radial::model::AssetRecord],
    assets: &std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<(), ImportAcceptError> {
    for record in records {
        if let Some((_, bytes)) = assets.iter().find(|(path, bytes)| {
            path.starts_with("assets/")
                && crate::radial::package::sha256_hex(bytes) == record.content_sha256
                && bytes.len() as u64 == record.byte_len
        }) {
            mutations
                .additions
                .retain(|addition| addition.record.id != record.id);
            mutations.additions.push(ManagedAssetAddition {
                record: record.clone(),
                bytes: Arc::from(bytes.clone()),
            });
        }
    }
    session
        .replace_document_and_assets_atomic(candidate, mutations)
        .map_err(|error| match error {
            AuthoringError::AssetBudgetExceeded => ImportAcceptError::AssetBudget,
            _ => ImportAcceptError::DraftChanged,
        })
}

fn merge_create_new(target: &mut RadialDocument, imported: &RadialDocument) {
    target.menus.extend(imported.menus.clone());
    target.skins.extend(imported.skins.clone());
    target.assets.extend(imported.assets.clone());
    target.context_rules.extend(imported.context_rules.clone());
    target
        .custom_triggers
        .extend(imported.custom_triggers.clone());
}

#[cfg(test)]
pub(super) fn export_bytes(
    document: &RadialDocument,
    roots: &[crate::radial::model::MenuId],
    assets: &std::collections::BTreeMap<crate::radial::model::AssetId, Vec<u8>>,
) -> Result<Vec<u8>, PackageError> {
    let plan: crate::radial::package::ExportPlan =
        crate::radial::package::plan_export(document, roots, assets, Vec::new())?;
    crate::radial::package::encode_mlradial(&plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::authoring::{AuthoringSnapshot, DiskSha256};
    use crate::radial::package::{ExportPlan, encode_mlradial};
    use std::collections::BTreeMap;

    fn session() -> RadialAuthoringSession {
        let document = RadialDocument::starter();
        RadialAuthoringSession::new(AuthoringSnapshot {
            revision: document.revision,
            document: Arc::new(document),
            disk_sha256: DiskSha256("disk".into()),
        })
    }

    #[test]
    fn package_preview_does_not_mutate_and_accept_is_one_undo_unit() {
        let source = RadialDocument::starter();
        let bytes =
            export_bytes(&source, &[source.default_menu_id.clone()], &BTreeMap::new()).unwrap();
        let mut target = session();
        let before = target.draft.clone();
        let preview = PendingImport::package(&bytes, &target).unwrap();
        assert_eq!(target.draft, before);
        preview.accept_create_new(&mut target).unwrap();
        assert_eq!(
            target.draft.menus.len(),
            before.menus.len() + source.menus.len()
        );
        assert!(target.undo());
        assert_eq!(target.draft, before);
    }

    #[test]
    fn changed_draft_and_hostile_or_unsupported_package_are_rejected() {
        let mut target = session();
        let source = RadialDocument::starter();
        let bytes =
            export_bytes(&source, &[source.default_menu_id.clone()], &BTreeMap::new()).unwrap();
        let preview = PendingImport::package(&bytes, &target).unwrap();
        target.generation.0 += 1;
        assert_eq!(
            preview.accept_create_new(&mut target),
            Err(ImportAcceptError::DraftChanged)
        );
        assert!(PendingImport::package(b"not a zip", &target).is_err());

        let mut files = decode_mlradial(&bytes).unwrap();
        let mut manifest: crate::radial::package::PackageManifest =
            serde_json::from_slice(files.get(crate::radial::package::MANIFEST_FILE).unwrap())
                .unwrap();
        manifest.package_version += 1;
        files.insert(
            crate::radial::package::MANIFEST_FILE.into(),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        );
        let unsupported = encode_mlradial(&ExportPlan { manifest, files }).unwrap();
        assert!(matches!(
            PendingImport::package(&unsupported, &target),
            Err(PackageError::UnsupportedVersion(_))
        ));
    }

    #[test]
    fn unreferenced_skin_bundle_merges_without_a_menu_and_undoes_atomically() {
        let mut source = RadialDocument::starter();
        let mut unreferenced = source.skins[0].clone();
        unreferenced.id = crate::radial::model::SkinId::new("unreferenced-skin");
        unreferenced.name = "Unreferenced".into();
        source.skins.push(unreferenced);
        let plan = crate::radial::package::plan_skin_export(
            &source,
            &source.skins[1].id,
            &BTreeMap::new(),
            Vec::new(),
        )
        .unwrap();
        let bytes = crate::radial::package::encode_mlradial(&plan).unwrap();
        let mut target = session();
        let before_menus = target.draft.menus.clone();
        let preview = PendingImport::package(&bytes, &target).unwrap();
        assert!(preview.plan().is_none());
        preview.accept_create_new(&mut target).unwrap();
        assert_eq!(target.draft.menus, before_menus);
        assert!(
            target
                .draft
                .skins
                .iter()
                .any(|skin| skin.name == "Unreferenced")
        );
        assert!(target.undo());
        assert!(
            !target
                .draft
                .skins
                .iter()
                .any(|skin| skin.name == "Unreferenced")
        );
    }
}
