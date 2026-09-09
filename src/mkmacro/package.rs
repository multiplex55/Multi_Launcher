//! Portable, dependency-aware MkMacro packages.
//!
//! Planning is deliberately side-effect free. Applying a plan is delegated to
//! [`MkMacroStore`], which owns the document/asset lock order and publication.

use super::{
    MkAction, MkImageRef, MkMacro, MkMacroDocument, MkMacroFolder, MkMacroStore, MkSignatureId,
    SCHEMA_VERSION,
    authoring_fields::{rewrite_step_image_refs, step_image_refs},
    call_graph::{CallGraph, DependencyPolicy},
    model::next_unused_id,
};
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;

pub const PACKAGE_FORMAT_VERSION: u32 = 1;
pub const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PACKAGE_MACROS: usize = 1_024;
pub const MAX_PACKAGE_ROOTS: usize = 256;
pub const MAX_PACKAGE_STEPS: usize = 100_000;
pub const MAX_PACKAGE_ASSETS: usize = 4_096;
pub const MAX_PACKAGE_ASSET_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PACKAGE_DECODED_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroPackage {
    pub manifest: MkMacroPackageManifest,
    #[serde(default)]
    pub assets: Vec<MkMacroPackageAsset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroPackageManifest {
    pub format_version: u32,
    pub schema_version: u32,
    pub roots: Vec<u64>,
    pub macros: Vec<MkMacro>,
    #[serde(default)]
    pub folders: Vec<MkMacroFolder>,
    #[serde(default)]
    pub dependencies: Vec<MkMacroPackageDependency>,
    #[serde(default)]
    pub asset_filenames: Vec<MkImageRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroPackageDependency {
    pub caller_id: u64,
    pub step_id: u64,
    pub target_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MkMacroPackageAsset {
    pub filename: MkImageRef,
    pub png_base64: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackageImportSummary {
    pub added_macros: usize,
    pub added_dependencies: usize,
    pub added_folders: usize,
    pub added_images: usize,
    pub reused_images: usize,
    pub renamed_macros: usize,
    pub renamed_folders: usize,
    pub renamed_images: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PlannedAsset {
    pub image: MkImageRef,
    pub bytes: Vec<u8>,
}

/// An immutable import preview. The draft and persisted baselines are both
/// captured so UI cancellation has no persistence side effect and Apply can
/// reject either an authoring edit or an external store edit.
#[derive(Debug, Clone)]
pub struct PackageImportPlan {
    pub summary: PackageImportSummary,
    pub imported_root_ids: Vec<u64>,
    pub macro_id_map: BTreeMap<u64, u64>,
    pub folder_id_map: BTreeMap<u64, u64>,
    expected_draft: MkMacroDocument,
    expected_authoring_revision: u64,
    pub(crate) expected_persisted: MkMacroDocument,
    pub(crate) expected_disk_bytes: Option<Vec<u8>>,
    pub(crate) expected_assets: Vec<(String, Vec<u8>)>,
    pub(crate) candidate: MkMacroDocument,
    pub(crate) assets_to_create: Vec<PlannedAsset>,
}

impl PackageImportPlan {
    pub fn candidate(&self) -> &MkMacroDocument {
        &self.candidate
    }

    pub fn apply(
        &self,
        store: &MkMacroStore,
        current_draft: &MkMacroDocument,
        authoring_revision: u64,
    ) -> Result<std::sync::Arc<MkMacroDocument>> {
        ensure!(
            current_draft == &self.expected_draft
                && authoring_revision == self.expected_authoring_revision,
            "the authoring draft changed; preview the import again"
        );
        store.apply_package_import(self)
    }
}

pub fn export_package(
    store: &MkMacroStore,
    document: &MkMacroDocument,
    roots: &[u64],
) -> Result<Vec<u8>> {
    ensure!(!roots.is_empty(), "select at least one macro to export");
    ensure!(roots.len() <= MAX_PACKAGE_ROOTS, "too many package roots");
    let graph = CallGraph::build(document);
    ensure!(
        graph.identity_diagnostics().is_empty(),
        "document has ambiguous macro identities"
    );
    let mut root_ids = roots.to_vec();
    root_ids.sort_unstable();
    root_ids.dedup();
    for root in &root_ids {
        ensure!(
            graph.macro_index(*root).is_some(),
            "package root #{root} does not exist"
        );
    }

    let mut closure = BTreeSet::new();
    for root in &root_ids {
        closure.extend(graph.closure(*root, DependencyPolicy::AllAuthoredCalls));
    }
    for id in &closure {
        ensure!(
            graph.macro_index(*id).is_some(),
            "authored Call target #{id} does not exist"
        );
    }
    ensure!(
        closure.len() <= MAX_PACKAGE_MACROS,
        "too many macros in package closure"
    );

    let cycle = graph
        .cycle_diagnostics(DependencyPolicy::AllAuthoredCalls)
        .into_iter()
        .find(|d| closure.contains(&d.macro_id));
    ensure!(
        cycle.is_none(),
        "authored macro dependency closure contains a cycle"
    );

    let mut macros: Vec<_> = document
        .macros
        .iter()
        .filter(|m| closure.contains(&m.id))
        .cloned()
        .collect();
    macros.sort_by_key(|m| m.id);
    let step_count: usize = macros.iter().map(|m| m.steps.len()).sum();
    ensure!(step_count <= MAX_PACKAGE_STEPS, "too many steps in package");

    let mut dependencies = dependencies(&macros);
    dependencies.sort();
    let used_folders: BTreeSet<u64> = macros.iter().filter_map(|m| m.folder_id).collect();
    let mut folders: Vec<_> = document
        .folders
        .iter()
        .filter(|f| used_folders.contains(&f.id))
        .cloned()
        .collect();
    folders.sort_by_key(|f| f.id);
    validate_document_identities(&MkMacroDocument {
        schema_version: SCHEMA_VERSION,
        macros: macros.clone(),
        folders: folders.clone(),
        settings: Default::default(),
    })?;
    validate_bindings(&macros)?;

    let asset_filenames = referenced_images(&macros);
    ensure!(
        asset_filenames.len() <= MAX_PACKAGE_ASSETS,
        "too many assets in package"
    );
    let mut assets = Vec::with_capacity(asset_filenames.len());
    let mut decoded_total = 0usize;
    for image in &asset_filenames {
        let path = store.image_path(image)?;
        let bytes = fs::read(&path)
            .with_context(|| format!("read required package image {}", image.filename()))?;
        store
            .validate_png_bytes(&bytes)
            .with_context(|| format!("validate required package image {}", image.filename()))?;
        ensure!(
            bytes.len() <= MAX_PACKAGE_ASSET_BYTES,
            "package image is too large"
        );
        decoded_total = decoded_total
            .checked_add(bytes.len())
            .context("package decoded size overflow")?;
        ensure!(
            decoded_total <= MAX_PACKAGE_DECODED_BYTES,
            "package images are too large"
        );
        assets.push(MkMacroPackageAsset {
            filename: image.clone(),
            png_base64: BASE64.encode(bytes),
        });
    }
    let package = MkMacroPackage {
        manifest: MkMacroPackageManifest {
            format_version: PACKAGE_FORMAT_VERSION,
            schema_version: SCHEMA_VERSION,
            roots: root_ids,
            macros,
            folders,
            dependencies,
            asset_filenames,
        },
        assets,
    };
    let encoded = serde_json::to_vec_pretty(&package)?;
    ensure!(
        encoded.len() <= MAX_PACKAGE_BYTES,
        "encoded package is too large"
    );
    Ok(encoded)
}

pub fn parse_package(bytes: &[u8]) -> Result<MkMacroPackage> {
    ensure!(
        bytes.len() <= MAX_PACKAGE_BYTES,
        "package exceeds the input size limit"
    );
    let source: serde_json::Value =
        serde_json::from_slice(bytes).context("malformed .mkmacro JSON")?;
    let source_schema = source
        .pointer("/manifest/schema_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|version| u32::try_from(version).ok())
        .context("package manifest has no valid schema version")?;
    ensure!(
        matches!(source_schema, 12 | SCHEMA_VERSION),
        "unsupported macro schema version {source_schema}"
    );
    let mut package: MkMacroPackage =
        serde_json::from_value(source.clone()).context("malformed .mkmacro JSON")?;
    // Canonicality belongs to the package's declared schema. Schema 12 added
    // no fields that require a content rewrite, so deserialize/serialize it
    // with its original version before normalizing the in-memory manifest.
    let canonical = serde_json::to_value(&package)?;
    ensure!(
        source == canonical,
        "package contains unknown fields or a non-canonical nested model shape"
    );
    validate_package(&package)?;
    package.manifest.schema_version = SCHEMA_VERSION;
    Ok(package)
}

pub fn plan_package_import(
    store: &MkMacroStore,
    bytes: &[u8],
    authoring_draft: &MkMacroDocument,
    authoring_revision: u64,
    expected_persisted: &MkMacroDocument,
) -> Result<PackageImportPlan> {
    let package = parse_package(bytes)?;
    let actual_snapshot = store.snapshot();
    ensure!(
        actual_snapshot.as_ref() == expected_persisted,
        "persisted macro state changed"
    );
    let expected_disk_bytes = store.package_document_bytes()?;
    let expected_assets = store.package_asset_inventory()?;

    validate_document_identities(authoring_draft)
        .context("cannot import into a draft with ambiguous identities")?;
    let mut candidate = authoring_draft.clone();
    candidate.schema_version = SCHEMA_VERSION;
    let mut summary = PackageImportSummary::default();
    let mut macro_id_map = BTreeMap::new();
    let mut folder_id_map = BTreeMap::new();

    let mut used_macro_ids: HashSet<u64> = candidate.macros.iter().map(|m| m.id).collect();
    used_macro_ids.extend(package.manifest.macros.iter().map(|m| m.id));
    let mut next_macro = used_macro_ids
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .unwrap_or(1);
    for source in &package.manifest.macros {
        let fresh =
            next_unused_id(&used_macro_ids, &mut next_macro).context("macro ID space exhausted")?;
        used_macro_ids.insert(fresh);
        macro_id_map.insert(source.id, fresh);
    }
    let mut used_folder_ids: HashSet<u64> = candidate.folders.iter().map(|f| f.id).collect();
    used_folder_ids.extend(package.manifest.folders.iter().map(|f| f.id));
    let mut next_folder = used_folder_ids
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .unwrap_or(1);
    let mut folder_names: HashSet<String> = candidate
        .folders
        .iter()
        .map(|folder| windows_fold(&folder.name))
        .collect();
    for source in &package.manifest.folders {
        let fresh = next_unused_id(&used_folder_ids, &mut next_folder)
            .context("folder ID space exhausted")?;
        used_folder_ids.insert(fresh);
        folder_id_map.insert(source.id, fresh);
        let name = unique_name(&source.name, &folder_names);
        if name != source.name {
            summary.renamed_folders += 1;
        }
        folder_names.insert(windows_fold(&name));
        candidate.folders.push(MkMacroFolder { id: fresh, name });
    }

    let decoded_assets = decode_assets(&package)?;
    let existing_fold_count = expected_assets
        .iter()
        .map(|(name, _)| windows_fold(name))
        .collect::<HashSet<_>>()
        .len();
    ensure!(
        existing_fold_count == expected_assets.len(),
        "local assets contain a case-insensitive filename collision"
    );
    let existing_by_fold: HashMap<String, (String, Vec<u8>)> = expected_assets
        .iter()
        .cloned()
        .map(|(name, bytes)| (windows_fold(&name), (name, bytes)))
        .collect();
    let mut reserved: HashSet<String> = existing_by_fold.keys().cloned().collect();
    let mut image_map = HashMap::new();
    let mut assets_to_create = Vec::new();
    for (source, bytes) in decoded_assets {
        let folded = windows_fold(source.filename());
        if let Some((actual, existing)) = existing_by_fold.get(&folded) {
            if existing == &bytes {
                image_map.insert(
                    source.filename().to_owned(),
                    MkImageRef::new(actual.clone()).map_err(anyhow::Error::msg)?,
                );
                summary.reused_images += 1;
                continue;
            }
        }
        let destination = if reserved.contains(&folded) {
            summary.renamed_images += 1;
            unique_image_name(&source, &reserved)?
        } else {
            source.clone()
        };
        reserved.insert(windows_fold(destination.filename()));
        image_map.insert(source.filename().to_owned(), destination.clone());
        assets_to_create.push(PlannedAsset {
            image: destination,
            bytes,
        });
    }

    let mut names: HashSet<String> = candidate
        .macros
        .iter()
        .map(|m| windows_fold(&m.name))
        .collect();
    let mut step_maps = HashMap::<u64, HashMap<u64, u64>>::new();
    let mut signature_maps = HashMap::<u64, HashMap<MkSignatureId, MkSignatureId>>::new();
    for source in &package.manifest.macros {
        let mut steps = HashMap::new();
        let mut next = 1;
        let mut used: HashSet<u64> = source.steps.iter().map(|step| step.id).collect();
        for step in &source.steps {
            let fresh = next_unused_id(&used, &mut next).context("step ID space exhausted")?;
            used.insert(fresh);
            steps.insert(step.id, fresh);
        }
        step_maps.insert(source.id, steps);
        let mut signatures = HashMap::new();
        let mut next = 1;
        let mut used: HashSet<u64> = source
            .signature
            .parameters
            .iter()
            .map(|value| value.id.0)
            .chain(source.signature.outputs.iter().map(|value| value.id.0))
            .collect();
        for id in source
            .signature
            .parameters
            .iter()
            .map(|p| p.id)
            .chain(source.signature.outputs.iter().map(|o| o.id))
        {
            let fresh = MkSignatureId(
                next_unused_id(&used, &mut next).context("signature ID space exhausted")?,
            );
            used.insert(fresh.0);
            signatures.insert(id, fresh);
        }
        signature_maps.insert(source.id, signatures);
    }

    for source in &package.manifest.macros {
        let mut imported = source.clone();
        imported.id = macro_id_map[&source.id];
        imported.folder_id = source.folder_id.map(|id| folder_id_map[&id]);
        let unique = unique_name(&source.name, &names);
        if unique != source.name {
            summary.renamed_macros += 1;
        }
        names.insert(windows_fold(&unique));
        imported.name = unique;
        for parameter in &mut imported.signature.parameters {
            parameter.id = signature_maps[&source.id][&parameter.id];
        }
        for output in &mut imported.signature.outputs {
            output.id = signature_maps[&source.id][&output.id];
        }
        for step in &mut imported.steps {
            step.id = step_maps[&source.id][&step.id];
            rewrite_step_image_refs(step, &image_map).map_err(anyhow::Error::msg)?;
            match &mut step.action {
                MkAction::CallMacro(call) => {
                    let old_target = call.macro_id;
                    call.macro_id = macro_id_map[&old_target];
                    for binding in &mut call.arguments {
                        binding.parameter_id = signature_maps[&old_target][&binding.parameter_id];
                    }
                    for binding in &mut call.outputs {
                        binding.output_id = signature_maps[&old_target][&binding.output_id];
                    }
                }
                MkAction::Return(ret) => {
                    for binding in &mut ret.outputs {
                        binding.output_id = signature_maps[&source.id][&binding.output_id];
                    }
                }
                _ => {}
            }
        }
        candidate.macros.push(imported);
    }
    let mut repaired = candidate.clone();
    ensure!(
        !super::store::repair_ids(&mut repaired) && repaired == candidate,
        "import remapping did not produce stable identities"
    );

    summary.added_macros = package.manifest.macros.len();
    summary.added_dependencies = package.manifest.dependencies.len();
    summary.added_folders = package.manifest.folders.len();
    summary.added_images = assets_to_create.len();
    let imported_root_ids = package
        .manifest
        .roots
        .iter()
        .map(|id| macro_id_map[id])
        .collect();
    Ok(PackageImportPlan {
        summary,
        imported_root_ids,
        macro_id_map,
        folder_id_map,
        expected_draft: authoring_draft.clone(),
        expected_authoring_revision: authoring_revision,
        expected_persisted: expected_persisted.clone(),
        expected_disk_bytes,
        expected_assets,
        candidate,
        assets_to_create,
    })
}

fn validate_package(package: &MkMacroPackage) -> Result<()> {
    let manifest = &package.manifest;
    ensure!(
        manifest.format_version == PACKAGE_FORMAT_VERSION,
        "unsupported .mkmacro format version {}",
        manifest.format_version
    );
    ensure!(
        matches!(manifest.schema_version, 12 | SCHEMA_VERSION),
        "unsupported macro schema version {}",
        manifest.schema_version
    );
    ensure!(
        !manifest.roots.is_empty() && manifest.roots.len() <= MAX_PACKAGE_ROOTS,
        "invalid package root count"
    );
    ensure!(
        !manifest.macros.is_empty() && manifest.macros.len() <= MAX_PACKAGE_MACROS,
        "invalid package macro count"
    );
    ensure!(
        manifest.macros.iter().map(|m| m.steps.len()).sum::<usize>() <= MAX_PACKAGE_STEPS,
        "too many package steps"
    );
    ensure!(
        package.assets.len() <= MAX_PACKAGE_ASSETS,
        "too many package assets"
    );
    validate_document_identities(&MkMacroDocument {
        schema_version: SCHEMA_VERSION,
        macros: manifest.macros.clone(),
        folders: manifest.folders.clone(),
        settings: Default::default(),
    })?;
    let macro_ids: HashSet<_> = manifest.macros.iter().map(|m| m.id).collect();
    ensure!(
        manifest.roots.iter().all(|id| macro_ids.contains(id)),
        "package contains a missing root"
    );
    ensure!(
        manifest.roots.iter().copied().collect::<HashSet<_>>().len() == manifest.roots.len(),
        "package roots contain duplicates"
    );
    ensure!(
        manifest.roots.windows(2).all(|pair| pair[0] < pair[1]),
        "package roots are not in canonical order"
    );
    ensure!(
        manifest
            .macros
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id),
        "package macros are not in canonical order"
    );
    ensure!(
        manifest
            .folders
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id),
        "package folders are not in canonical order"
    );
    let mut actual_dependencies = dependencies(&manifest.macros);
    actual_dependencies.sort();
    ensure!(
        actual_dependencies == manifest.dependencies,
        "package dependency manifest does not match Call steps"
    );
    ensure!(
        actual_dependencies
            .iter()
            .all(|d| macro_ids.contains(&d.target_id)),
        "package contains a missing Call target"
    );
    validate_bindings(&manifest.macros)?;
    let graph = CallGraph::build(&MkMacroDocument {
        schema_version: SCHEMA_VERSION,
        macros: manifest.macros.clone(),
        folders: manifest.folders.clone(),
        settings: Default::default(),
    });
    ensure!(
        graph
            .cycle_diagnostics(DependencyPolicy::AllAuthoredCalls)
            .is_empty(),
        "package contains a recursive dependency cycle"
    );
    let closure: HashSet<u64> = manifest
        .roots
        .iter()
        .flat_map(|root| graph.closure(*root, DependencyPolicy::AllAuthoredCalls))
        .collect();
    ensure!(
        closure == macro_ids,
        "package contains macros outside the declared root dependency closure"
    );
    let used_folders: HashSet<u64> = manifest.macros.iter().filter_map(|m| m.folder_id).collect();
    let packaged_folders: HashSet<u64> = manifest.folders.iter().map(|f| f.id).collect();
    ensure!(
        used_folders == packaged_folders,
        "package folder manifest is not the exact relevant folder set"
    );
    let expected_images = referenced_images(&manifest.macros);
    ensure!(
        expected_images == manifest.asset_filenames,
        "package asset manifest does not match typed image references"
    );
    ensure!(
        package
            .assets
            .iter()
            .map(|a| a.filename.clone())
            .collect::<Vec<_>>()
            == manifest.asset_filenames,
        "package asset payloads do not match the manifest"
    );
    let mut folded = HashSet::new();
    ensure!(
        manifest
            .asset_filenames
            .iter()
            .all(|i| i.is_valid_filename() && folded.insert(windows_fold(i.filename()))),
        "package has invalid or case-colliding asset filenames"
    );
    decode_assets(package)?;
    Ok(())
}

fn validate_document_identities(document: &MkMacroDocument) -> Result<()> {
    let mut macros = HashSet::new();
    for owner in &document.macros {
        ensure!(
            owner.id != 0 && macros.insert(owner.id),
            "macro IDs must be non-zero and unique"
        );
        let mut steps = HashSet::new();
        ensure!(
            owner.steps.iter().all(|s| s.id != 0 && steps.insert(s.id)),
            "step IDs must be non-zero and unique per macro"
        );
        let mut signatures = HashSet::new();
        ensure!(
            owner
                .signature
                .parameters
                .iter()
                .map(|p| p.id.0)
                .chain(owner.signature.outputs.iter().map(|o| o.id.0))
                .all(|id| id != 0 && signatures.insert(id)),
            "signature IDs must be non-zero and unique per macro"
        );
    }
    let mut folders = HashSet::new();
    ensure!(
        document
            .folders
            .iter()
            .all(|f| f.id != 0 && folders.insert(f.id)),
        "folder IDs must be non-zero and unique"
    );
    ensure!(
        document
            .macros
            .iter()
            .all(|m| m.folder_id.is_none_or(|id| folders.contains(&id))),
        "macro references a missing folder"
    );
    Ok(())
}

fn validate_bindings(macros: &[MkMacro]) -> Result<()> {
    let by_id: HashMap<_, _> = macros.iter().map(|m| (m.id, m)).collect();
    for owner in macros {
        let owner_outputs: HashSet<_> = owner.signature.outputs.iter().map(|o| o.id).collect();
        for step in &owner.steps {
            match &step.action {
                MkAction::CallMacro(call) => {
                    let target = by_id.get(&call.macro_id).context("missing Call target")?;
                    let parameters: HashSet<_> =
                        target.signature.parameters.iter().map(|p| p.id).collect();
                    let outputs: HashSet<_> =
                        target.signature.outputs.iter().map(|o| o.id).collect();
                    ensure!(
                        call.arguments
                            .iter()
                            .all(|b| parameters.contains(&b.parameter_id)),
                        "Call has a dangling parameter binding"
                    );
                    ensure!(
                        call.outputs.iter().all(|b| outputs.contains(&b.output_id)),
                        "Call has a dangling output binding"
                    );
                }
                MkAction::Return(ret) => ensure!(
                    ret.outputs
                        .iter()
                        .all(|b| owner_outputs.contains(&b.output_id)),
                    "Return has a dangling output binding"
                ),
                _ => {}
            }
        }
    }
    Ok(())
}

fn dependencies(macros: &[MkMacro]) -> Vec<MkMacroPackageDependency> {
    macros
        .iter()
        .flat_map(|m| {
            m.steps.iter().filter_map(move |s| match &s.action {
                MkAction::CallMacro(call) => Some(MkMacroPackageDependency {
                    caller_id: m.id,
                    step_id: s.id,
                    target_id: call.macro_id,
                }),
                _ => None,
            })
        })
        .collect()
}

fn referenced_images(macros: &[MkMacro]) -> Vec<MkImageRef> {
    macros
        .iter()
        .flat_map(|m| m.steps.iter())
        .flat_map(step_image_refs)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn decode_assets(package: &MkMacroPackage) -> Result<Vec<(MkImageRef, Vec<u8>)>> {
    let mut total = 0usize;
    let mut result = Vec::with_capacity(package.assets.len());
    for asset in &package.assets {
        let estimated = asset
            .png_base64
            .len()
            .checked_mul(3)
            .context("asset size overflow")?
            / 4
            + 3;
        ensure!(
            estimated <= MAX_PACKAGE_ASSET_BYTES + 3,
            "package image is too large"
        );
        let bytes = BASE64
            .decode(&asset.png_base64)
            .context("package asset is not valid base64")?;
        ensure!(
            bytes.len() <= MAX_PACKAGE_ASSET_BYTES,
            "package image is too large"
        );
        super::store::validate_package_png(&bytes)
            .with_context(|| format!("{} is not a valid PNG", asset.filename.filename()))?;
        total = total
            .checked_add(bytes.len())
            .context("package decoded size overflow")?;
        ensure!(
            total <= MAX_PACKAGE_DECODED_BYTES,
            "package decoded assets exceed the limit"
        );
        result.push((asset.filename.clone(), bytes));
    }
    Ok(result)
}

fn windows_fold(value: &str) -> String {
    value.to_lowercase()
}

fn unique_name(source: &str, reserved: &HashSet<String>) -> String {
    if !reserved.contains(&windows_fold(source)) {
        return source.to_owned();
    }
    for suffix in 2u64.. {
        let candidate = format!("{source} ({suffix})");
        if !reserved.contains(&windows_fold(&candidate)) {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_image_name(source: &MkImageRef, reserved: &HashSet<String>) -> Result<MkImageRef> {
    let filename = source.filename();
    let stem = &filename[..filename.len() - 4];
    for suffix in 2u64.. {
        let candidate =
            MkImageRef::new(format!("{stem}_{suffix}.png")).map_err(anyhow::Error::msg)?;
        if !reserved.contains(&windows_fold(candidate.filename())) {
            return Ok(candidate);
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;
    use tempfile::tempdir;

    fn macro_with(id: u64, name: &str, folder_id: Option<u64>, steps: Vec<MkStep>) -> MkMacro {
        MkMacro {
            id,
            name: name.into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id,
            playback: Default::default(),
            signature: Default::default(),
            steps,
        }
    }

    fn step(id: u64, action: MkAction) -> MkStep {
        MkStep {
            id,
            enabled: true,
            breakpoint: false,
            repeat: 1,
            delay_after_ms: 0,
            on_error: Default::default(),
            metadata: Default::default(),
            action,
        }
    }

    fn image_step(id: u64, filename: &str) -> MkStep {
        step(
            id,
            MkAction::ImageFind(MkImagePayload {
                image: MkImageRef::new(filename).unwrap(),
                wait: Default::default(),
                region: Default::default(),
                tolerance: 0,
                alpha: Default::default(),
                return_point: Default::default(),
                not_found_policy: Default::default(),
                outputs: Default::default(),
            }),
        )
    }

    fn write_image(store: &MkMacroStore, filename: &str, color: [u8; 4]) {
        let image = RgbaImage::from_pixel(1, 1, Rgba(color));
        let reference = MkImageRef::new(filename).unwrap();
        assert_eq!(
            store
                .write_captured_png(
                    &image,
                    reference.clone(),
                    ImageImportChoice::SaveAs(reference.clone()),
                )
                .unwrap(),
            ImageImportResult::Imported(reference)
        );
    }

    fn png_bytes(color: [u8; 4]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba(color)))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[cfg(windows)]
    fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[cfg(not(windows))]
    fn create_dir_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[test]
    fn parser_rejects_traversal_before_any_write() {
        let invalid = MkImageRef::from_filename("../x.png");
        let mut owner = macro_with(1, "x", None, vec![image_step(1, "x.png")]);
        let MkAction::ImageFind(image) = &mut owner.steps[0].action else {
            panic!()
        };
        image.image = invalid.clone();
        let package = MkMacroPackage {
            manifest: MkMacroPackageManifest {
                format_version: PACKAGE_FORMAT_VERSION,
                schema_version: SCHEMA_VERSION,
                roots: vec![1],
                macros: vec![owner],
                folders: vec![],
                dependencies: vec![],
                asset_filenames: vec![invalid.clone()],
            },
            assets: vec![MkMacroPackageAsset {
                filename: invalid,
                png_base64: BASE64.encode(png_bytes([1, 2, 3, 255])),
            }],
        };
        let error = parse_package(&serde_json::to_vec(&package).unwrap()).unwrap_err();
        assert!(error.to_string().contains("invalid"));
    }

    #[test]
    fn parser_rejects_unsupported_version_and_input_bound() {
        let json = br#"{"manifest":{"format_version":99,"schema_version":12,"roots":[],"macros":[],"folders":[],"dependencies":[],"asset_filenames":[]},"assets":[]}"#;
        assert!(
            parse_package(json)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
        assert!(
            parse_package(&vec![b' '; MAX_PACKAGE_BYTES + 1])
                .unwrap_err()
                .to_string()
                .contains("size limit")
        );
        let oversized_asset = MkMacroPackage {
            manifest: MkMacroPackageManifest {
                format_version: PACKAGE_FORMAT_VERSION,
                schema_version: SCHEMA_VERSION,
                roots: vec![],
                macros: vec![],
                folders: vec![],
                dependencies: vec![],
                asset_filenames: vec![],
            },
            assets: vec![MkMacroPackageAsset {
                filename: MkImageRef::new("large.png").unwrap(),
                png_base64: "A".repeat(((MAX_PACKAGE_ASSET_BYTES + 3) / 3) * 4),
            }],
        };
        assert!(
            decode_assets(&oversized_asset)
                .unwrap_err()
                .to_string()
                .contains("too large")
        );
    }

    #[test]
    fn parser_normalizes_canonical_schema_twelve_packages_and_rejects_future_schema() {
        let mut package = MkMacroPackage {
            manifest: MkMacroPackageManifest {
                format_version: PACKAGE_FORMAT_VERSION,
                schema_version: 12,
                roots: vec![1],
                macros: vec![macro_with(
                    1,
                    "legacy",
                    None,
                    vec![step(1, MkAction::Delay(Default::default()))],
                )],
                folders: vec![],
                dependencies: vec![],
                asset_filenames: vec![],
            },
            assets: vec![],
        };
        let parsed = parse_package(&serde_json::to_vec(&package).unwrap()).unwrap();
        assert_eq!(parsed.manifest.schema_version, SCHEMA_VERSION);

        package.manifest.schema_version = SCHEMA_VERSION;
        package.manifest.macros[0].steps[0].action = MkAction::OcrFindText(MkOcrFindPayload {
            search: MkOcrSearchSpec {
                text: "Ready".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let parsed = parse_package(&serde_json::to_vec(&package).unwrap()).unwrap();
        assert!(matches!(
            parsed.manifest.macros[0].steps[0].action,
            MkAction::OcrFindText(_)
        ));
        assert!(
            parsed.assets.is_empty(),
            "OCR packages must not synthesize assets"
        );

        package.manifest.schema_version = SCHEMA_VERSION + 1;
        assert!(
            parse_package(&serde_json::to_vec(&package).unwrap())
                .unwrap_err()
                .to_string()
                .contains("unsupported macro schema")
        );
    }

    #[test]
    fn parser_rejects_unknown_nested_model_fields() {
        let directory = tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let document = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "Strict",
                None,
                vec![step(1, MkAction::Delay(Default::default()))],
            )],
            folders: vec![],
            settings: Default::default(),
        };
        let exported = export_package(&store, &document, &[1]).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&exported).unwrap();
        value["manifest"]["macros"][0]["steps"][0]["enabeld"] = serde_json::json!(true);
        let error = parse_package(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert!(error.to_string().contains("unknown fields"));
    }

    #[test]
    fn parser_rejects_ambiguous_identities_noncanonical_manifests_and_count_limits() {
        let valid_macro = macro_with(1, "Root", None, vec![]);
        let package = |roots: Vec<u64>, macros: Vec<MkMacro>| MkMacroPackage {
            manifest: MkMacroPackageManifest {
                format_version: PACKAGE_FORMAT_VERSION,
                schema_version: SCHEMA_VERSION,
                roots,
                macros,
                folders: vec![],
                dependencies: vec![],
                asset_filenames: vec![],
            },
            assets: vec![],
        };
        let parse_error = |package: MkMacroPackage| {
            parse_package(&serde_json::to_vec(&package).unwrap())
                .unwrap_err()
                .to_string()
        };

        let duplicate = package(vec![1], vec![valid_macro.clone(), valid_macro.clone()]);
        assert!(parse_error(duplicate).contains("non-zero and unique"));
        let duplicate_roots = package(vec![1, 1], vec![valid_macro.clone()]);
        assert!(parse_error(duplicate_roots).contains("duplicates"));
        let missing_root = package(vec![2], vec![valid_macro.clone()]);
        assert!(parse_error(missing_root).contains("missing root"));
        let unsorted_roots = package(
            vec![2, 1],
            vec![valid_macro.clone(), macro_with(2, "Second", None, vec![])],
        );
        assert!(parse_error(unsorted_roots).contains("canonical order"));

        let too_many_roots = package(
            (1..=MAX_PACKAGE_ROOTS as u64 + 1).collect(),
            vec![valid_macro.clone()],
        );
        assert!(parse_error(too_many_roots).contains("root count"));

        let mut too_many_macros =
            package(vec![1], vec![valid_macro.clone(); MAX_PACKAGE_MACROS + 1]);
        assert!(
            validate_package(&too_many_macros)
                .unwrap_err()
                .to_string()
                .contains("macro count")
        );
        too_many_macros.manifest.macros = vec![macro_with(
            1,
            "Too many steps",
            None,
            vec![step(1, MkAction::Delay(Default::default())); MAX_PACKAGE_STEPS + 1],
        )];
        assert!(
            validate_package(&too_many_macros)
                .unwrap_err()
                .to_string()
                .contains("too many package steps")
        );
        too_many_macros.manifest.macros = vec![valid_macro];
        too_many_macros.assets = vec![
            MkMacroPackageAsset {
                filename: MkImageRef::new("asset.png").unwrap(),
                png_base64: String::new(),
            };
            MAX_PACKAGE_ASSETS + 1
        ];
        assert!(
            validate_package(&too_many_macros)
                .unwrap_err()
                .to_string()
                .contains("too many package assets")
        );
    }

    #[test]
    fn export_missing_typed_asset_is_read_only_and_reports_the_filename() {
        let directory = tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let document = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "Missing image",
                None,
                vec![image_step(1, "missing.png")],
            )],
            folders: vec![],
            settings: Default::default(),
        };
        let before = document.clone();
        let error = export_package(&store, &document, &[1]).unwrap_err();
        assert!(error.to_string().contains("missing.png"));
        assert_eq!(document, before);
        assert!(store.image_refs().unwrap().is_empty());
        assert_eq!(store.snapshot().as_ref(), &MkMacroDocument::default());
    }

    #[test]
    fn export_is_exact_for_multi_root_transitive_closure_folders_and_images() {
        let directory = tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        write_image(&store, "needed.png", [1, 2, 3, 255]);
        let call = |target| {
            step(
                1,
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: target,
                    arguments: vec![],
                    outputs: vec![],
                }),
            )
        };
        let document = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![
                macro_with(10, "A", Some(1), vec![call(20)]),
                macro_with(20, "B", Some(2), vec![call(30)]),
                macro_with(30, "C", Some(2), vec![image_step(1, "needed.png")]),
                macro_with(
                    40,
                    "Unrelated",
                    Some(3),
                    vec![step(1, MkAction::Delay(Default::default()))],
                ),
            ],
            folders: vec![
                MkMacroFolder {
                    id: 1,
                    name: "One".into(),
                },
                MkMacroFolder {
                    id: 2,
                    name: "Two".into(),
                },
                MkMacroFolder {
                    id: 3,
                    name: "Other".into(),
                },
            ],
            settings: Default::default(),
        };
        let before = document.clone();
        let package = parse_package(&export_package(&store, &document, &[10]).unwrap()).unwrap();
        assert_eq!(
            document, before,
            "export must not mutate the authored source"
        );
        assert_eq!(
            package
                .manifest
                .macros
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
        assert_eq!(
            package
                .manifest
                .folders
                .iter()
                .map(|f| f.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            package.manifest.asset_filenames,
            vec![MkImageRef::new("needed.png").unwrap()]
        );
        assert_eq!(package.assets.len(), 1);
    }

    #[test]
    fn import_remaps_all_relationships_and_resolves_case_insensitive_conflicts() {
        let source_dir = tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        write_image(&source_store, "Icon.png", [1, 2, 3, 255]);
        write_image(&source_store, "Same.png", [4, 5, 6, 255]);
        let parameter = MkMacroParameter {
            id: MkSignatureId(7),
            name: "input".into(),
            value_type: MkValueType::String,
            description: String::new(),
            default_value: None,
        };
        let output = MkMacroOutput {
            id: MkSignatureId(8),
            name: "result".into(),
            value_type: MkValueType::String,
            description: String::new(),
        };
        let mut child = macro_with(
            2,
            "Child",
            Some(4),
            vec![
                image_step(5, "Icon.png"),
                image_step(10, "Same.png"),
                step(
                    6,
                    MkAction::Return(MkReturnPayload {
                        outputs: vec![MkReturnValueBinding {
                            output_id: MkSignatureId(8),
                            source: MkValueSource::Literal(MkValue::String("ok".into())),
                        }],
                    }),
                ),
            ],
        );
        child.signature.parameters.push(parameter);
        child.signature.outputs.push(output);
        let root = macro_with(
            1,
            "Root",
            Some(4),
            vec![step(
                9,
                MkAction::CallMacro(MkCallMacroPayload {
                    macro_id: 2,
                    arguments: vec![MkCallArgumentBinding {
                        parameter_id: MkSignatureId(7),
                        source: MkValueSource::Literal(MkValue::String("value".into())),
                    }],
                    outputs: vec![MkCallOutputBinding {
                        output_id: MkSignatureId(8),
                        caller_variable: "answer".into(),
                    }],
                }),
            )],
        );
        let source = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![root, child],
            folders: vec![MkMacroFolder {
                id: 4,
                name: "Library".into(),
            }],
            settings: Default::default(),
        };
        let bytes = export_package(&source_store, &source, &[1]).unwrap();

        let destination_dir = tempdir().unwrap();
        let (destination, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        write_image(&destination, "icon.PNG", [9, 9, 9, 255]);
        write_image(&destination, "same.PNG", [4, 5, 6, 255]);
        let local = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "root",
                Some(4),
                vec![step(1, MkAction::Delay(Default::default()))],
            )],
            folders: vec![MkMacroFolder {
                id: 4,
                name: "library".into(),
            }],
            settings: Default::default(),
        };
        let persisted = destination.save(local.clone()).unwrap();
        let plan = plan_package_import(&destination, &bytes, &local, 12, &persisted).unwrap();
        assert_eq!(plan.summary.renamed_macros, 1);
        assert_eq!(plan.summary.renamed_folders, 1);
        assert_eq!(plan.summary.renamed_images, 1);
        assert_eq!(plan.summary.reused_images, 1);
        let root_id = plan.macro_id_map[&1];
        let child_id = plan.macro_id_map[&2];
        assert_ne!(root_id, 1);
        assert_ne!(child_id, 2);
        let imported_root = plan
            .candidate()
            .macros
            .iter()
            .find(|m| m.id == root_id)
            .unwrap();
        let imported_child = plan
            .candidate()
            .macros
            .iter()
            .find(|m| m.id == child_id)
            .unwrap();
        let MkAction::CallMacro(call) = &imported_root.steps[0].action else {
            panic!()
        };
        assert_eq!(call.macro_id, child_id);
        assert_eq!(
            call.arguments[0].parameter_id,
            imported_child.signature.parameters[0].id
        );
        assert_eq!(
            call.outputs[0].output_id,
            imported_child.signature.outputs[0].id
        );
        assert_ne!(imported_root.steps[0].id, 9);
        assert_ne!(imported_child.signature.parameters[0].id, MkSignatureId(7));
        let MkAction::Return(ret) = &imported_child.steps[2].action else {
            panic!()
        };
        assert_eq!(
            ret.outputs[0].output_id,
            imported_child.signature.outputs[0].id
        );
        let MkAction::ImageFind(image) = &imported_child.steps[0].action else {
            panic!()
        };
        assert_eq!(image.image.filename(), "Icon_2.png");
        let MkAction::ImageFind(reused_image) = &imported_child.steps[1].action else {
            panic!()
        };
        assert_eq!(reused_image.image.filename(), "same.PNG");
        let applied = plan.apply(&destination, &local, 12).unwrap();
        assert_eq!(applied.as_ref(), plan.candidate());
        assert!(destination.image_path(&image.image).unwrap().is_file());
        assert!(
            destination
                .image_path(&MkImageRef::new("icon.PNG").unwrap())
                .unwrap()
                .is_file()
        );
        assert!(
            destination
                .image_path(&MkImageRef::new("same.PNG").unwrap())
                .unwrap()
                .is_file()
        );
    }

    #[test]
    fn injected_failure_rolls_back_only_new_assets_and_never_publishes_document() {
        let source_dir = tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        write_image(&source_store, "one.png", [1, 1, 1, 255]);
        write_image(&source_store, "two.png", [2, 2, 2, 255]);
        let source = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "Images",
                None,
                vec![image_step(1, "one.png"), image_step(2, "two.png")],
            )],
            folders: vec![],
            settings: Default::default(),
        };
        let bytes = export_package(&source_store, &source, &[1]).unwrap();
        let destination_dir = tempdir().unwrap();
        let (destination, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        write_image(&destination, "keep.png", [7, 7, 7, 255]);
        let baseline = destination.snapshot();
        let plan = plan_package_import(&destination, &bytes, &baseline, 0, &baseline).unwrap();
        assert!(
            destination
                .apply_package_import_with_injected_failure(&plan, 1)
                .is_err()
        );
        assert_eq!(destination.snapshot().as_ref(), baseline.as_ref());
        assert!(destination.package_document_bytes().unwrap().is_none());
        assert_eq!(
            destination.image_refs().unwrap(),
            vec![MkImageRef::new("keep.png").unwrap()]
        );
    }

    #[test]
    fn external_asset_replacement_is_detected_and_never_deleted_by_rollback() {
        let source_dir = tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        write_image(&source_store, "owned.png", [1, 2, 3, 255]);
        let source = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "Image",
                None,
                vec![image_step(1, "owned.png")],
            )],
            folders: vec![],
            settings: Default::default(),
        };
        let bytes = export_package(&source_store, &source, &[1]).unwrap();
        let destination_dir = tempdir().unwrap();
        let (destination, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        let baseline = destination.snapshot();
        let plan = plan_package_import(&destination, &bytes, &baseline, 0, &baseline).unwrap();
        let destination_path = destination
            .image_path(&plan.assets_to_create[0].image)
            .unwrap();
        let replacement = png_bytes([9, 8, 7, 255]);
        let result = destination.apply_package_import_with_test_hook(&plan, |published| {
            if published == 1 {
                fs::write(&destination_path, &replacement)?;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&destination_path).unwrap(), replacement);
        assert!(destination.package_document_bytes().unwrap().is_none());
        assert_eq!(destination.snapshot().as_ref(), baseline.as_ref());
    }

    #[test]
    fn asset_root_symlink_is_rejected_before_package_apply() {
        let source_dir = tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        let source = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(1, "No assets", None, vec![])],
            folders: vec![],
            settings: Default::default(),
        };
        let bytes = export_package(&source_store, &source, &[1]).unwrap();
        let destination_dir = tempdir().unwrap();
        let (destination, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        let baseline = destination.snapshot();
        let plan = plan_package_import(&destination, &bytes, &baseline, 0, &baseline).unwrap();
        let outside = destination_dir.path().join("outside");
        fs::create_dir(&outside).unwrap();
        if create_dir_symlink(&outside, &destination.asset_root()).is_err() {
            return;
        }
        let error = plan.apply(&destination, &baseline, 0).unwrap_err();
        assert!(error.to_string().contains("symlink") || error.to_string().contains("reparse"));
        assert!(destination.package_document_bytes().unwrap().is_none());
    }

    #[test]
    fn apply_rejects_stale_document_state_without_mutation() {
        let source_dir = tempdir().unwrap();
        let (source_store, _) = MkMacroStore::open(source_dir.path()).unwrap();
        let source = MkMacroDocument {
            schema_version: SCHEMA_VERSION,
            macros: vec![macro_with(
                1,
                "Source",
                None,
                vec![step(1, MkAction::Delay(Default::default()))],
            )],
            folders: vec![],
            settings: Default::default(),
        };
        let bytes = export_package(&source_store, &source, &[1]).unwrap();
        let destination_dir = tempdir().unwrap();
        let (destination, _) = MkMacroStore::open(destination_dir.path()).unwrap();
        let baseline = destination.snapshot();
        let plan = plan_package_import(&destination, &bytes, &baseline, 3, &baseline).unwrap();
        let changed = MkMacroDocument {
            macros: vec![macro_with(9, "Concurrent edit", None, vec![])],
            ..MkMacroDocument::default()
        };
        destination.save(changed.clone()).unwrap();
        assert!(plan.apply(&destination, &baseline, 3).is_err());
        assert_eq!(destination.snapshot().as_ref(), &changed);
    }
}
