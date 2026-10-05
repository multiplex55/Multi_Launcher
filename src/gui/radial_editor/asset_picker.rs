//! Media reference authoring and reference-aware managed asset deletion.

use crate::radial::authoring::{AssetMutations, ManagedAssetAddition, RadialAuthoringSession};
use crate::radial::model::{
    AssetId, AssetRecord, MediaKind, MediaReference, RADIAL_ASSETS_DIRECTORY,
};
use crate::radial::package::sha256_hex;
use crate::radial::store::{ReferenceImpact, references_to_asset};
use std::sync::Arc;

pub(super) fn read_bounded(path: &std::path::Path, max: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if reparse || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("resource selector requires a regular non-link file".into());
    }
    if metadata.len() > max {
        return Err(format!("resource exceeds the {max}-byte source budget"));
    }
    std::fs::read(path).map_err(|error| error.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ResourceChoice {
    Managed {
        record: AssetRecord,
        bytes: Arc<[u8]>,
    },
    External {
        path: String,
    },
    SearchPath {
        file_name: String,
    },
    IconResource {
        path: String,
        index: u32,
    },
}

impl ResourceChoice {
    pub(super) fn from_selected_file(
        path: &std::path::Path,
        bytes: Vec<u8>,
        kind: MediaKind,
        managed: bool,
    ) -> Result<Self, String> {
        crate::radial::assets::validate_packaged_media(&bytes, kind)
            .map_err(|error| error.to_string())?;
        if !managed {
            return Ok(Self::External {
                path: path.to_string_lossy().to_string(),
            });
        }
        let digest = sha256_hex(&bytes);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| {
                value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
            })
            .map(|value| format!(".{}", value.to_ascii_lowercase()))
            .unwrap_or_default();
        let prefix = match kind {
            MediaKind::Image => "image",
            MediaKind::Sound => "sound",
        };
        let id = AssetId::new(format!("{prefix}-{}", &digest[..16]));
        Ok(Self::Managed {
            record: AssetRecord {
                id,
                kind,
                relative_path: format!("{digest}{extension}"),
                content_sha256: digest,
                byte_len: bytes.len() as u64,
            },
            bytes: bytes.into(),
        })
    }

    pub(super) fn media_reference(&self) -> MediaReference {
        match self {
            Self::Managed { record, .. } => MediaReference::Managed {
                asset_id: record.id.clone(),
            },
            Self::External { path } => MediaReference::ExternalFile { path: path.clone() },
            Self::SearchPath { file_name } => MediaReference::SearchPath {
                file_name: file_name.clone(),
            },
            Self::IconResource { path, index } => MediaReference::IconResource {
                path: path.clone(),
                index: *index,
            },
        }
    }

    pub(super) fn portability_diagnostic(&self) -> &'static str {
        match self {
            Self::Managed { .. } => "Managed and portable",
            Self::External { .. } => "External path; excluded from portable packages",
            Self::SearchPath { .. } => "Search-path reference; excluded from portable packages",
            Self::IconResource { .. } => "System icon resource; excluded from portable packages",
        }
    }

    pub(super) fn stage(&self, mutations: &mut AssetMutations) {
        if let Self::Managed { record, bytes } = self {
            mutations
                .additions
                .retain(|addition| addition.record.id != record.id);
            mutations.deletions.retain(|id| id != &record.id);
            mutations.additions.push(ManagedAssetAddition {
                record: record.clone(),
                bytes: Arc::clone(bytes),
            });
        }
    }
}

pub(super) fn add_resource(
    session: &mut RadialAuthoringSession,
    choice: &ResourceChoice,
) -> Result<MediaReference, String> {
    let reference = choice.media_reference();
    let mut document = (*session.draft).clone();
    let mut mutations = session.pending_assets.clone();
    if let ResourceChoice::Managed { record, .. } = choice
        && !document.assets.iter().any(|asset| asset.id == record.id)
    {
        document.assets.push(record.clone());
    }
    choice.stage(&mut mutations);
    session
        .replace_document_and_assets_atomic(document, mutations)
        .map_err(|error| format!("{error:?}"))?;
    Ok(reference)
}

pub(super) fn delete_impact(session: &RadialAuthoringSession, id: &AssetId) -> ReferenceImpact {
    references_to_asset(&session.draft, id)
}

pub(super) fn delete_managed_asset(
    session: &mut RadialAuthoringSession,
    id: &AssetId,
) -> Result<(), ReferenceImpact> {
    let impact = delete_impact(session, id);
    if !impact.paths.is_empty() {
        return Err(impact);
    }
    let mut document = (*session.draft).clone();
    if !document.assets.iter().any(|record| &record.id == id) {
        return Ok(());
    }
    document.assets.retain(|record| &record.id != id);
    let mut mutations = session.pending_assets.clone();
    mutations
        .additions
        .retain(|addition| &addition.record.id != id);
    if !mutations.deletions.contains(id) {
        mutations.deletions.push(id.clone());
    }
    session
        .replace_document_and_assets_atomic(document, mutations)
        .map_err(|_| ReferenceImpact {
            paths: vec![format!("{RADIAL_ASSETS_DIRECTORY}/{}", id.as_str())],
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::authoring::{AuthoringSnapshot, DiskSha256};
    use crate::radial::model::{Override, RadialDocument};

    #[test]
    fn usage_query_blocks_delete_and_lists_references() {
        let mut document = RadialDocument::starter();
        let id = AssetId::new("used");
        document.assets.push(AssetRecord {
            id: id.clone(),
            kind: MediaKind::Image,
            relative_path: "used.png".into(),
            content_sha256: "0".repeat(64),
            byte_len: 1,
        });
        document.menus[0].style.values.images.center_image =
            Override::Value(MediaReference::Managed {
                asset_id: id.clone(),
            });
        let revision = document.revision;
        let mut session = RadialAuthoringSession::new(AuthoringSnapshot {
            document: Arc::new(document),
            revision,
            disk_sha256: DiskSha256("test".into()),
        });
        let impact = delete_managed_asset(&mut session, &id).unwrap_err();
        assert!(
            impact
                .paths
                .iter()
                .any(|path| path.contains("center_image"))
        );
        assert!(session.draft.assets.iter().any(|asset| asset.id == id));
    }

    #[test]
    fn external_search_and_icon_references_are_explicitly_nonportable() {
        for choice in [
            ResourceChoice::External {
                path: "C:/image.png".into(),
            },
            ResourceChoice::SearchPath {
                file_name: "image.png".into(),
            },
            ResourceChoice::IconResource {
                path: "shell32.dll".into(),
                index: 4,
            },
        ] {
            assert!(choice.portability_diagnostic().contains("excluded"));
            assert!(!matches!(
                choice.media_reference(),
                MediaReference::Managed { .. }
            ));
        }
    }
}
