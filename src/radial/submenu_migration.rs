//! Checked, reversible conversion of persisted submenu presentation defaults.
//!
//! The receipt belongs to top-level Settings rather than RadialDocument so a
//! package import or authoring undo cannot erase recovery state. The durable
//! order is settings(prepared), radial commit, settings(applied).

use super::authoring::AssetMutations;
use super::model::{
    CURRENT_SCHEMA_VERSION, ConfigRevision, MenuDefinition, RadialDocument, SubmenuPresentation,
};
use super::package::sha256_hex;
use super::store::{RadialStore, StoreError};
use crate::common::atomic_file::save_atomic;
use crate::settings::{
    Settings, SubmenuMigrationMenuChange, SubmenuMigrationState,
    SubmenuPresentationMigrationReceipt,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MIGRATION_ID: &str = "radial-submenu-same-center-v1";
pub const MIGRATION_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct MigrationOutcome {
    pub settings: Settings,
    pub document: Arc<RadialDocument>,
    pub notice: Option<String>,
    pub changed: bool,
}

/// Convert old persisted presentations once, or finish a durable migration
/// receipt left by a prior interrupted startup. New migration is allowed only
/// when both primary stores loaded normally; callers must pass false when
/// settings were recovered temporarily or the radial store used its starter.
pub fn startup_migrate(
    store: &RadialStore,
    settings_path: impl AsRef<Path>,
    allow_new_migration: bool,
) -> Result<MigrationOutcome, String> {
    let settings_path = settings_path.as_ref();
    if !allow_new_migration {
        let settings = load_settings(settings_path)?;
        let document = store.snapshot().map_err(store_error)?;
        return Ok(MigrationOutcome {
            settings,
            document,
            notice: None,
            changed: false,
        });
    }

    let settings = load_settings(settings_path)?;
    if let Some(receipt) = settings.radial_submenu_migration.clone() {
        return match receipt.state {
            SubmenuMigrationState::Prepared => {
                recover_prepared(store, settings_path, receipt, settings.clone())
            }
            SubmenuMigrationState::UndoPrepared => recover_undo(store, settings_path, receipt),
            SubmenuMigrationState::Applied
            | SubmenuMigrationState::Undone
            | SubmenuMigrationState::Failed => {
                let document = store.snapshot().map_err(store_error)?;
                Ok(MigrationOutcome {
                    settings,
                    document,
                    notice: None,
                    changed: false,
                })
            }
        };
    }

    begin_migration(store, settings_path)
}

/// User-requested field-level undo. The migration marker is retained as
/// Undone, preventing a later startup from reapplying the conversion.
pub fn restore(
    store: &RadialStore,
    settings_path: impl AsRef<Path>,
) -> Result<MigrationOutcome, String> {
    let settings_path = settings_path.as_ref();
    let settings = load_settings(settings_path)?;
    let mut receipt = settings
        .radial_submenu_migration
        .clone()
        .ok_or_else(|| "there is no radial submenu migration to restore".to_owned())?;
    if receipt.state != SubmenuMigrationState::Applied {
        return Err(format!(
            "radial submenu migration cannot be restored while {:?}",
            receipt.state
        ));
    }
    let applied_receipt = receipt.clone();

    let snapshot = store.authoring_snapshot().map_err(store_error)?;
    let mut candidate = (*snapshot.document).clone();
    let mut restored = Vec::new();
    let mut skipped = 0usize;
    for change in &receipt.changed_menus {
        let Some(menu) = candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id == change.menu_id)
        else {
            skipped += 1;
            continue;
        };
        if menu_fingerprint(menu) != change.entity_fingerprint
            || menu.submenu_presentation != receipt.settings_default_target
        {
            skipped += 1;
            continue;
        }
        menu.submenu_presentation = change.previous;
        restored.push(change.menu_id.clone());
    }
    let restore_settings_default =
        settings.radial.default_submenu_presentation == receipt.settings_default_target;
    let source_settings_content_sha256 = settings_projection_sha256(&settings, None)?;
    let target_settings_content_sha256 = settings_projection_sha256(
        &settings,
        restore_settings_default.then_some(receipt.settings_default_before),
    )?;
    set_next_revision(&mut candidate, snapshot.revision)?;
    let target_bytes = serde_json::to_vec_pretty(&candidate)
        .map_err(|error| format!("failed to serialize radial restore target: {error}"))?;

    receipt.state = SubmenuMigrationState::UndoPrepared;
    receipt.undo_source_radial_sha256 = Some(snapshot.disk_sha256.0.clone());
    receipt.undo_target_radial_revision = Some(candidate.revision.0);
    receipt.undo_target_radial_sha256 = Some(sha256_hex(&target_bytes));
    receipt.undo_source_settings_content_sha256 = Some(source_settings_content_sha256);
    receipt.undo_target_settings_content_sha256 = Some(target_settings_content_sha256);
    receipt.undo_restored_menu_ids = restored;
    receipt.undo_settings_default_source = Some(settings.radial.default_submenu_presentation);
    receipt.undo_restores_settings_default = restore_settings_default;
    receipt.failure = None;
    let receipt_for_write = receipt.clone();
    let expected_source_settings = receipt
        .undo_source_settings_content_sha256
        .clone()
        .expect("restore source settings hash was assigned");
    let expected_settings = settings_file_sha256(settings_path)?;
    Settings::update_checked(settings_path, &expected_settings, move |latest| {
        let current = latest
            .radial_submenu_migration
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("radial migration receipt was removed"))?;
        anyhow::ensure!(
            current == &applied_receipt,
            "radial migration receipt changed before restore"
        );
        anyhow::ensure!(
            settings_projection_sha256(latest, None).map_err(|error| anyhow::anyhow!(error))?
                == expected_source_settings,
            "settings changed while preparing radial migration restore"
        );
        latest.radial_submenu_migration = Some(receipt_for_write);
        Ok(())
    })
    .map_err(|error| format!("failed to prepare radial migration restore: {error}"))?;

    recover_undo(store, settings_path, receipt).map(|mut outcome| {
        if outcome
            .settings
            .radial_submenu_migration
            .as_ref()
            .is_some_and(|receipt| receipt.state == SubmenuMigrationState::Undone)
        {
            outcome.notice = Some(format!(
                "Radial submenu restore completed; {skipped} changed or missing menu entries were left untouched."
            ));
        }
        outcome.changed = true;
        outcome
    })
}

fn begin_migration(store: &RadialStore, settings_path: &Path) -> Result<MigrationOutcome, String> {
    let snapshot = store.authoring_snapshot().map_err(store_error)?;
    let radial_path = store.path();
    let radial_bytes = std::fs::read(radial_path)
        .map_err(|error| format!("failed to read radial source before migration: {error}"))?;
    let radial_sha256 = sha256_hex(&radial_bytes);
    if radial_sha256 != snapshot.disk_sha256.0 {
        return Err("radial source changed while preparing migration backups".into());
    }
    let settings_bytes = read_source_bytes(settings_path)?;
    let settings_sha256 = sha256_hex(&settings_bytes);
    let mut settings = parse_settings(&settings_bytes)?;

    let backup_root = radial_path
        .parent()
        .or_else(|| settings_path.parent())
        .unwrap_or_else(|| Path::new("."));
    let settings_backup = backup_exact(backup_root, "settings", &settings_bytes, &settings_sha256)?;
    let radial_backup = backup_exact(backup_root, "radial", &radial_bytes, &radial_sha256)?;

    let mut candidate = (*snapshot.document).clone();
    let mut changed_menus = Vec::new();
    for menu in &mut candidate.menus {
        if menu.submenu_presentation == SubmenuPresentation::Cascade {
            changed_menus.push(SubmenuMigrationMenuChange {
                menu_id: menu.id.clone(),
                previous: menu.submenu_presentation,
                entity_fingerprint: menu_fingerprint(menu),
            });
            menu.submenu_presentation = SubmenuPresentation::SameCenter;
        }
    }
    let settings_default_before = settings.radial.default_submenu_presentation;
    settings.radial.default_submenu_presentation = SubmenuPresentation::SameCenter;
    set_next_revision(&mut candidate, snapshot.revision)?;
    let target_radial_bytes = serde_json::to_vec_pretty(&candidate)
        .map_err(|error| format!("failed to serialize radial migration target: {error}"))?;
    let target_settings_content_sha256 =
        settings_projection_sha256(&settings, Some(SubmenuPresentation::SameCenter))?;
    let receipt = SubmenuPresentationMigrationReceipt {
        migration_id: MIGRATION_ID.into(),
        version: MIGRATION_VERSION,
        state: SubmenuMigrationState::Prepared,
        source_settings_sha256: settings_sha256.clone(),
        source_radial_sha256: radial_sha256,
        settings_backup_path: settings_backup.to_string_lossy().into_owned(),
        settings_backup_sha256: settings_sha256,
        settings_source_existed: settings_path.exists(),
        radial_backup_path: radial_backup.to_string_lossy().into_owned(),
        radial_backup_sha256: sha256_hex(&radial_bytes),
        settings_default_before,
        settings_default_target: SubmenuPresentation::SameCenter,
        changed_menus,
        target_radial_revision: candidate.revision.0,
        target_radial_sha256: sha256_hex(&target_radial_bytes),
        target_settings_content_sha256,
        undo_restored_menu_ids: Vec::new(),
        undo_source_radial_sha256: None,
        undo_target_radial_revision: None,
        undo_target_radial_sha256: None,
        undo_source_settings_content_sha256: None,
        undo_target_settings_content_sha256: None,
        undo_settings_default_source: None,
        undo_restores_settings_default: false,
        failure: None,
    };
    let receipt_for_write = receipt.clone();
    let (prepared, _) = Settings::update_checked(
        settings_path,
        &receipt.source_settings_sha256,
        move |latest| {
            anyhow::ensure!(
                latest.radial_submenu_migration.is_none(),
                "radial submenu migration already has a receipt"
            );
            latest.radial.default_submenu_presentation = SubmenuPresentation::SameCenter;
            latest.radial_submenu_migration = Some(receipt_for_write);
            Ok(())
        },
    )
    .map_err(|error| format!("failed to persist prepared radial migration: {error}"))?;

    recover_prepared(
        store,
        settings_path,
        prepared
            .radial_submenu_migration
            .clone()
            .expect("prepared migration receipt is persisted"),
        prepared,
    )
}

fn recover_prepared(
    store: &RadialStore,
    settings_path: &Path,
    receipt: SubmenuPresentationMigrationReceipt,
    fallback_settings: Settings,
) -> Result<MigrationOutcome, String> {
    recover_prepared_with_finalizer(
        store,
        settings_path,
        receipt,
        fallback_settings,
        |settings_path, receipt| {
            update_receipt_state(
                settings_path,
                receipt,
                SubmenuMigrationState::Applied,
                None,
                false,
            )
        },
    )
}

fn recover_prepared_with_finalizer(
    store: &RadialStore,
    settings_path: &Path,
    receipt: SubmenuPresentationMigrationReceipt,
    fallback_settings: Settings,
    finalize: impl FnOnce(&Path, &SubmenuPresentationMigrationReceipt) -> Result<Settings, String>,
) -> Result<MigrationOutcome, String> {
    if receipt.migration_id != MIGRATION_ID || receipt.version != MIGRATION_VERSION {
        return Err("unsupported radial submenu migration receipt".into());
    }
    let snapshot = store.authoring_snapshot().map_err(store_error)?;
    if snapshot.disk_sha256.0 == receipt.target_radial_sha256 {
        return Ok(finalized_migration_outcome(
            settings_path,
            &receipt,
            fallback_settings,
            snapshot.document,
            finalize,
        ));
    }
    if snapshot.disk_sha256.0 != receipt.source_radial_sha256 {
        let message = "radial source matches neither the migration source nor its target";
        let settings = update_receipt_state(
            settings_path,
            &receipt,
            SubmenuMigrationState::Failed,
            Some(message.into()),
            true,
        )?;
        return Ok(MigrationOutcome {
            settings,
            document: snapshot.document,
            notice: Some(format!(
                "Radial submenu migration was not completed: {message}."
            )),
            changed: true,
        });
    }

    let backups_valid = verify_migration_backups(&receipt);
    let settings_target_matches = load_settings(settings_path)
        .and_then(|settings| settings_projection_sha256(&settings, None))
        .is_ok_and(|sha| sha == receipt.target_settings_content_sha256);
    if !backups_valid || !settings_target_matches {
        let message = if !backups_valid {
            "a verified source backup is missing or has changed"
        } else {
            "settings changed after the migration was prepared"
        };
        return fail_prepared(store, settings_path, receipt, snapshot.document, message);
    }

    let mut candidate = (*snapshot.document).clone();
    for change in &receipt.changed_menus {
        let Some(menu) = candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id == change.menu_id)
        else {
            return fail_prepared(
                store,
                settings_path,
                receipt,
                snapshot.document,
                "a migrated menu was deleted",
            );
        };
        if menu_fingerprint(menu) != change.entity_fingerprint
            || menu.submenu_presentation != change.previous
        {
            return fail_prepared(
                store,
                settings_path,
                receipt,
                snapshot.document,
                "a migrated menu changed during recovery",
            );
        }
        menu.submenu_presentation = receipt.settings_default_target;
    }
    set_next_revision(&mut candidate, snapshot.revision)?;
    let expected_target = serde_json::to_vec_pretty(&candidate)
        .map_err(|error| format!("failed to serialize migration recovery target: {error}"))?;
    if sha256_hex(&expected_target) != receipt.target_radial_sha256
        || candidate.revision.0 != receipt.target_radial_revision
    {
        return fail_prepared(
            store,
            settings_path,
            receipt,
            snapshot.document,
            "migration recovery target did not match its durable receipt",
        );
    }
    match store.commit_authoring(
        snapshot.revision,
        &snapshot.disk_sha256,
        candidate,
        AssetMutations::default(),
    ) {
        Ok(committed) if committed.snapshot.disk_sha256.0 == receipt.target_radial_sha256 => {
            Ok(finalized_migration_outcome(
                settings_path,
                &receipt,
                fallback_settings,
                committed.snapshot.document,
                finalize,
            ))
        }
        Ok(committed) => fail_prepared(
            store,
            settings_path,
            receipt,
            committed.snapshot.document,
            "committed radial bytes did not match the durable migration target",
        ),
        Err(error) => fail_prepared(
            store,
            settings_path,
            receipt,
            snapshot.document,
            &format!("radial migration commit failed: {error}"),
        ),
    }
}

fn finalized_migration_outcome(
    settings_path: &Path,
    receipt: &SubmenuPresentationMigrationReceipt,
    fallback_settings: Settings,
    document: Arc<RadialDocument>,
    finalize: impl FnOnce(&Path, &SubmenuPresentationMigrationReceipt) -> Result<Settings, String>,
) -> MigrationOutcome {
    match finalize(settings_path, receipt) {
        Ok(settings) => MigrationOutcome {
            settings,
            document,
            notice: Some("Radial submenu presentation migration completed.".into()),
            changed: true,
        },
        Err(error) => {
            let settings = load_settings(settings_path).unwrap_or(fallback_settings);
            let finalized = settings
                .radial_submenu_migration
                .as_ref()
                .is_some_and(|current| {
                    current.migration_id == receipt.migration_id
                        && current.state == SubmenuMigrationState::Applied
                });
            let message = if finalized {
                "Radial submenu presentation changes are active; the Applied receipt was saved despite a verification error."
                    .to_owned()
            } else {
                format!(
                    "Radial submenu presentation changes were committed, but migration finalization is pending recovery on the next startup: {error}"
                )
            };
            MigrationOutcome {
                settings,
                document,
                notice: Some(message),
                changed: true,
            }
        }
    }
}

fn fail_prepared(
    _store: &RadialStore,
    settings_path: &Path,
    receipt: SubmenuPresentationMigrationReceipt,
    document: Arc<RadialDocument>,
    message: &str,
) -> Result<MigrationOutcome, String> {
    let settings = update_receipt_state(
        settings_path,
        &receipt,
        SubmenuMigrationState::Failed,
        Some(message.to_owned()),
        true,
    )?;
    Ok(MigrationOutcome {
        settings,
        document,
        notice: Some(format!(
            "Radial submenu migration was not completed: {message}."
        )),
        changed: true,
    })
}

fn fail_undo(
    _store: &RadialStore,
    settings_path: &Path,
    receipt: SubmenuPresentationMigrationReceipt,
    document: Arc<RadialDocument>,
    message: &str,
) -> Result<MigrationOutcome, String> {
    let settings = update_receipt_state(
        settings_path,
        &receipt,
        SubmenuMigrationState::Failed,
        Some(message.to_owned()),
        false,
    )?;
    Ok(MigrationOutcome {
        settings,
        document,
        notice: Some(format!(
            "Radial submenu restore was not completed: {message}."
        )),
        changed: true,
    })
}

fn recover_undo(
    store: &RadialStore,
    settings_path: &Path,
    receipt: SubmenuPresentationMigrationReceipt,
) -> Result<MigrationOutcome, String> {
    if receipt.migration_id != MIGRATION_ID || receipt.version != MIGRATION_VERSION {
        return Err("unsupported radial submenu migration receipt".into());
    }
    let source_sha = receipt
        .undo_source_radial_sha256
        .as_deref()
        .ok_or_else(|| "restore receipt is missing its source radial hash".to_owned())?;
    let target_sha = receipt
        .undo_target_radial_sha256
        .as_deref()
        .ok_or_else(|| "restore receipt is missing its target radial hash".to_owned())?;
    let target_revision = receipt
        .undo_target_radial_revision
        .ok_or_else(|| "restore receipt is missing its target revision".to_owned())?;
    let _source_settings_sha = receipt
        .undo_source_settings_content_sha256
        .as_deref()
        .ok_or_else(|| "restore receipt is missing its source settings hash".to_owned())?;
    let _target_settings_sha = receipt
        .undo_target_settings_content_sha256
        .as_deref()
        .ok_or_else(|| "restore receipt is missing its target settings hash".to_owned())?;
    let snapshot = store.authoring_snapshot().map_err(store_error)?;
    let _latest_settings = load_settings(settings_path)?;
    if snapshot.disk_sha256.0 == target_sha {
        let settings = update_receipt_state(
            settings_path,
            &receipt,
            SubmenuMigrationState::Undone,
            None,
            false,
        )?;
        return Ok(MigrationOutcome {
            settings,
            document: snapshot.document,
            notice: Some(format!(
                "Radial submenu migration restore completed; {} changed or missing menu entries were left untouched.",
                receipt
                    .changed_menus
                    .len()
                    .saturating_sub(receipt.undo_restored_menu_ids.len())
            )),
            changed: true,
        });
    }
    if snapshot.disk_sha256.0 != source_sha {
        let message = "radial source changed after restore was prepared";
        let settings = update_receipt_state(
            settings_path,
            &receipt,
            SubmenuMigrationState::Failed,
            Some(message.into()),
            false,
        )?;
        return Ok(MigrationOutcome {
            settings,
            document: snapshot.document,
            notice: Some(format!(
                "Radial submenu restore was not completed: {message}."
            )),
            changed: true,
        });
    }
    let mut candidate = (*snapshot.document).clone();
    for menu_id in &receipt.undo_restored_menu_ids {
        let Some(change) = receipt
            .changed_menus
            .iter()
            .find(|change| &change.menu_id == menu_id)
        else {
            return Err("restore receipt refers to an unknown migrated menu".into());
        };
        let Some(menu) = candidate.menus.iter_mut().find(|menu| &menu.id == menu_id) else {
            return fail_undo(
                store,
                settings_path,
                receipt,
                snapshot.document,
                "a menu selected for restore was deleted",
            );
        };
        if menu_fingerprint(menu) != change.entity_fingerprint
            || menu.submenu_presentation != receipt.settings_default_target
        {
            return fail_undo(
                store,
                settings_path,
                receipt,
                snapshot.document,
                "a menu selected for restore changed after preparation",
            );
        }
        menu.submenu_presentation = change.previous;
    }
    set_next_revision(&mut candidate, snapshot.revision)?;
    let target_bytes = serde_json::to_vec_pretty(&candidate)
        .map_err(|error| format!("failed to serialize restore recovery target: {error}"))?;
    if candidate.revision.0 != target_revision || sha256_hex(&target_bytes) != target_sha {
        return fail_undo(
            store,
            settings_path,
            receipt,
            snapshot.document,
            "restore recovery target did not match its durable receipt",
        );
    }
    match store.commit_authoring(
        snapshot.revision,
        &snapshot.disk_sha256,
        candidate,
        AssetMutations::default(),
    ) {
        Ok(committed) if committed.snapshot.disk_sha256.0 == target_sha => {
            let settings = update_receipt_state(
                settings_path,
                &receipt,
                SubmenuMigrationState::Undone,
                None,
                false,
            )?;
            Ok(MigrationOutcome {
                settings,
                document: committed.snapshot.document,
                notice: Some(format!(
                    "Radial submenu migration restore completed; {} changed or missing menu entries were left untouched.",
                    receipt
                        .changed_menus
                        .len()
                        .saturating_sub(receipt.undo_restored_menu_ids.len())
                )),
                changed: true,
            })
        }
        Ok(committed) => fail_undo(
            store,
            settings_path,
            receipt,
            committed.snapshot.document,
            "committed radial restore bytes did not match the durable target",
        ),
        Err(error) => fail_undo(
            store,
            settings_path,
            receipt,
            snapshot.document,
            &format!("radial migration restore commit failed: {error}"),
        ),
    }
}

fn update_receipt_state(
    path: &Path,
    receipt: &SubmenuPresentationMigrationReceipt,
    state: SubmenuMigrationState,
    failure: Option<String>,
    restore_prior_default: bool,
) -> Result<Settings, String> {
    let expected = settings_file_sha256(path)?;
    let receipt = receipt.clone();
    let (settings, _) = Settings::update_checked(path, &expected, move |latest| {
        let current_matches = latest
            .radial_submenu_migration
            .as_ref()
            .is_some_and(|current| current == &receipt);
        anyhow::ensure!(
            current_matches,
            "radial migration receipt changed during recovery"
        );
        let current_default = latest.radial.default_submenu_presentation;
        let migration_target_is_current = current_default == receipt.settings_default_target;
        let undo_source_is_current = receipt.undo_settings_default_source == Some(current_default);
        if (restore_prior_default && migration_target_is_current)
            || (state == SubmenuMigrationState::Undone
                && receipt.undo_restores_settings_default
                && undo_source_is_current)
        {
            latest.radial.default_submenu_presentation = receipt.settings_default_before;
        }
        let current = latest
            .radial_submenu_migration
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("radial migration receipt was removed"))?;
        current.state = state;
        current.failure = failure;
        Ok(())
    })
    .map_err(|error| format!("failed to update radial migration receipt: {error}"))?;
    Ok(settings)
}

fn settings_projection_sha256(
    settings: &Settings,
    default: Option<SubmenuPresentation>,
) -> Result<String, String> {
    let mut projection = settings.clone();
    projection.radial_submenu_migration = None;
    if let Some(default) = default {
        projection.radial.default_submenu_presentation = default;
    }
    let mut value = serde_json::to_value(&projection)
        .map_err(|error| format!("failed to serialize settings migration target: {error}"))?;
    // Settings contain HashMap and HashSet fields. Hash their logical value,
    // not the randomized iteration order selected by a particular hasher.
    if let Some(enabled_plugins) = value
        .get_mut("enabled_plugins")
        .and_then(serde_json::Value::as_array_mut)
    {
        enabled_plugins.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
    }
    canonicalize_settings_json(&mut value);
    serde_json::to_vec(&value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("failed to serialize settings migration target: {error}"))
}

fn canonicalize_settings_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            let mut fields = std::mem::take(object).into_iter().collect::<Vec<_>>();
            fields.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            for (_, value) in &mut fields {
                canonicalize_settings_json(value);
            }
            for (key, value) in fields {
                object.insert(key, value);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                canonicalize_settings_json(value);
            }
        }
        _ => {}
    }
}

fn set_next_revision(
    candidate: &mut RadialDocument,
    revision: ConfigRevision,
) -> Result<(), String> {
    candidate.schema_version = CURRENT_SCHEMA_VERSION;
    candidate.revision = ConfigRevision(
        revision
            .0
            .checked_add(1)
            .ok_or_else(|| "radial revision overflow during submenu migration".to_owned())?,
    );
    Ok(())
}

fn menu_fingerprint(menu: &MenuDefinition) -> String {
    let mut rings = menu
        .rings
        .iter()
        .map(|ring| {
            let mut cells = ring
                .cells
                .iter()
                .map(|cell| cell.id.as_str().to_owned())
                .collect::<Vec<_>>();
            cells.sort_unstable();
            (ring.id.as_str().to_owned(), cells)
        })
        .collect::<Vec<_>>();
    rings.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    // Definitions have no immutable creation UUID. Stable structural IDs are
    // the bounded identity guard; mutable labels, actions, layout, and style
    // remain independently editable and do not block field-level undo.
    serde_json::to_vec(&(menu.id.as_str(), rings))
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default()
}

fn verify_migration_backups(receipt: &SubmenuPresentationMigrationReceipt) -> bool {
    [
        (
            &receipt.settings_backup_path,
            &receipt.settings_backup_sha256,
        ),
        (&receipt.radial_backup_path, &receipt.radial_backup_sha256),
    ]
    .into_iter()
    .all(|(path, expected)| {
        std::fs::read(path).is_ok_and(|bytes| sha256_hex(&bytes) == expected.as_str())
    })
}

fn settings_file_sha256(path: &Path) -> Result<String, String> {
    crate::settings::store::settings_file_sha256(path)
        .map_err(|error| format!("failed to hash settings source: {error}"))
}

fn load_settings(path: &Path) -> Result<Settings, String> {
    let bytes = read_source_bytes(path)?;
    parse_settings(&bytes)
}

fn parse_settings(bytes: &[u8]) -> Result<Settings, String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        Ok(Settings::default())
    } else {
        serde_json::from_slice(bytes)
            .map_err(|error| format!("settings are malformed; migration was skipped: {error}"))
    }
}

fn read_source_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("failed to read {}: {error}", path.display())),
    }
}

fn backup_exact(root: &Path, label: &str, bytes: &[u8], digest: &str) -> Result<PathBuf, String> {
    for index in 0..1_000u32 {
        let path = root.join(format!(".{MIGRATION_ID}.{label}.{digest}.{index}.bak"));
        if path.exists() {
            if std::fs::read(&path).ok().is_some_and(|existing| {
                sha256_hex(&existing) == digest && existing.as_slice() == bytes
            }) {
                return Ok(path);
            }
            continue;
        }
        save_atomic(&path, bytes)
            .map_err(|error| format!("failed to back up {label} before migration: {error}"))?;
        let verified = std::fs::read(&path)
            .map_err(|error| format!("failed to verify {label} migration backup: {error}"))?;
        if sha256_hex(&verified) != digest || verified != bytes {
            return Err(format!("{label} migration backup failed SHA verification"));
        }
        return Ok(path);
    }
    Err(format!(
        "unable to allocate a verified {label} migration backup"
    ))
}

fn store_error(error: StoreError) -> String {
    format!("radial submenu migration store error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::MenuId;
    use crate::settings::store::settings_file_sha256;

    struct Fixture {
        _directory: tempfile::TempDir,
        radial_path: PathBuf,
        settings_path: PathBuf,
        source_document: RadialDocument,
        source_radial_bytes: Vec<u8>,
        source_settings_bytes: Vec<u8>,
        store: RadialStore,
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let radial_path = directory.path().join("radial.json");
        let settings_path = directory.path().join("settings.json");
        let mut document = RadialDocument::starter();
        document.revision = ConfigRevision(9);
        for menu in &mut document.menus {
            menu.submenu_presentation = SubmenuPresentation::Cascade;
        }
        let source_radial_bytes = serde_json::to_vec_pretty(&document).unwrap();
        std::fs::write(&radial_path, &source_radial_bytes).unwrap();
        let store = RadialStore::at_path(&radial_path, RadialDocument::starter()).unwrap();
        let source_document = (*store.reload().unwrap()).clone();

        let mut settings = Settings::default();
        settings.radial.default_submenu_presentation = SubmenuPresentation::Cascade;
        let source_settings_bytes = serde_json::to_vec_pretty(&settings).unwrap();
        std::fs::write(&settings_path, &source_settings_bytes).unwrap();
        Fixture {
            _directory: directory,
            radial_path,
            settings_path,
            source_document,
            source_radial_bytes,
            source_settings_bytes,
            store,
        }
    }

    fn rewrite_settings(path: &Path, mutate: impl FnOnce(&mut Settings)) {
        let expected = settings_file_sha256(path).unwrap();
        Settings::update_checked(path, &expected, |settings| {
            mutate(settings);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn settings_projection_hash_is_stable_across_unordered_collection_roundtrip() {
        let mut settings = Settings::default();
        settings.enabled_plugins = Some(
            ["zeta", "alpha", "mu", "beta"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
        settings.plugin_settings = [
            (
                "zeta".to_owned(),
                serde_json::json!({"inner": {"z": 1, "a": 2}}),
            ),
            ("alpha".to_owned(), serde_json::json!({"enabled": true})),
            ("mu".to_owned(), serde_json::json!(["ordered", "values"])),
        ]
        .into_iter()
        .collect();

        let original_hash = settings_projection_sha256(&settings, None).unwrap();
        let serialized = serde_json::to_vec_pretty(&settings).unwrap();
        let reloaded = parse_settings(&serialized).unwrap();

        assert_eq!(
            settings_projection_sha256(&reloaded, None).unwrap(),
            original_hash
        );
    }

    #[test]
    fn migration_changes_only_presentations_and_writes_verified_backups_once() {
        let fixture = fixture();
        let outcome = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert!(outcome.changed);
        assert_eq!(
            outcome.settings.radial.default_submenu_presentation,
            SubmenuPresentation::SameCenter
        );
        assert!(
            outcome
                .document
                .menus
                .iter()
                .all(|menu| menu.submenu_presentation == SubmenuPresentation::SameCenter)
        );

        let mut expected = fixture.source_document.clone();
        expected.revision = ConfigRevision(fixture.source_document.revision.0 + 1);
        for menu in &mut expected.menus {
            menu.submenu_presentation = SubmenuPresentation::SameCenter;
        }
        assert_eq!(*outcome.document, expected);

        let receipt = outcome.settings.radial_submenu_migration.as_ref().unwrap();
        assert_eq!(receipt.migration_id, MIGRATION_ID);
        assert_eq!(receipt.version, MIGRATION_VERSION);
        assert_eq!(receipt.state, SubmenuMigrationState::Applied);
        assert_eq!(
            receipt.source_radial_sha256,
            sha256_hex(&fixture.source_radial_bytes)
        );
        assert_eq!(
            receipt.source_settings_sha256,
            sha256_hex(&fixture.source_settings_bytes)
        );
        assert_eq!(
            std::fs::read(&receipt.radial_backup_path).unwrap(),
            fixture.source_radial_bytes
        );
        assert_eq!(
            std::fs::read(&receipt.settings_backup_path).unwrap(),
            fixture.source_settings_bytes
        );
        assert_eq!(
            sha256_hex(&std::fs::read(&receipt.radial_backup_path).unwrap()),
            receipt.radial_backup_sha256
        );
        assert_eq!(
            sha256_hex(&std::fs::read(&receipt.settings_backup_path).unwrap()),
            receipt.settings_backup_sha256
        );

        let radial_bytes = std::fs::read(&fixture.radial_path).unwrap();
        let settings_bytes = std::fs::read(&fixture.settings_path).unwrap();
        let repeated = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert!(!repeated.changed);
        assert_eq!(std::fs::read(&fixture.radial_path).unwrap(), radial_bytes);
        assert_eq!(
            std::fs::read(&fixture.settings_path).unwrap(),
            settings_bytes
        );
    }

    #[test]
    fn migration_publishes_committed_document_when_applied_receipt_write_fails() {
        let fixture = fixture();
        let migrated = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let receipt = migrated.settings.radial_submenu_migration.clone().unwrap();

        std::fs::write(&fixture.radial_path, &fixture.source_radial_bytes).unwrap();
        fixture.store.reload().unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::Prepared;
        });
        let prepared = load_settings(&fixture.settings_path).unwrap();
        let recovered = recover_prepared_with_finalizer(
            &fixture.store,
            &fixture.settings_path,
            receipt.clone(),
            prepared,
            |_, _| Err("injected Applied receipt write failure".into()),
        )
        .unwrap();

        assert_eq!(
            recovered.document.revision.0,
            receipt.target_radial_revision
        );
        assert!(
            recovered
                .document
                .menus
                .iter()
                .all(|menu| menu.submenu_presentation == SubmenuPresentation::SameCenter)
        );
        assert_eq!(
            recovered
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Prepared
        );
        assert!(
            recovered
                .notice
                .as_deref()
                .unwrap()
                .contains("finalization is pending recovery")
        );
        assert_eq!(
            fixture.store.authoring_snapshot().unwrap().disk_sha256.0,
            receipt.target_radial_sha256
        );

        let next_startup = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            next_startup
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Applied
        );
        assert_eq!(
            next_startup.document.revision.0,
            receipt.target_radial_revision
        );
    }

    #[test]
    fn undo_preserves_unrelated_edits_and_never_overwrites_a_later_cascade() {
        let fixture = fixture();
        startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let snapshot = fixture.store.authoring_snapshot().unwrap();
        let mut candidate = (*snapshot.document).clone();
        candidate
            .metadata
            .insert("unrelated-edit".into(), "keep this".into());
        let later_cascade = MenuId::new("starter-favorites");
        candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id == later_cascade)
            .unwrap()
            .submenu_presentation = SubmenuPresentation::Cascade;
        fixture
            .store
            .commit_authoring(
                snapshot.revision,
                &snapshot.disk_sha256,
                candidate,
                AssetMutations::default(),
            )
            .unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.enable_toasts = false;
        });

        let restored = restore(&fixture.store, &fixture.settings_path).unwrap();
        assert_eq!(
            restored.settings.radial.default_submenu_presentation,
            SubmenuPresentation::Cascade
        );
        assert!(!restored.settings.enable_toasts);
        assert_eq!(
            restored
                .document
                .metadata
                .get("unrelated-edit")
                .map(String::as_str),
            Some("keep this")
        );
        assert_eq!(
            restored
                .document
                .menus
                .iter()
                .find(|menu| menu.id == later_cascade)
                .unwrap()
                .submenu_presentation,
            SubmenuPresentation::Cascade
        );
        assert_eq!(
            restored
                .document
                .menus
                .iter()
                .find(|menu| menu.id.as_str() == "starter-applications")
                .unwrap()
                .submenu_presentation,
            SubmenuPresentation::Cascade
        );
        let receipt = restored.settings.radial_submenu_migration.as_ref().unwrap();
        assert_eq!(receipt.state, SubmenuMigrationState::Undone);
        assert!(!receipt.undo_restored_menu_ids.contains(&later_cascade));
        let after_undo = std::fs::read(&fixture.radial_path).unwrap();
        let rerun = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert!(!rerun.changed);
        assert_eq!(std::fs::read(&fixture.radial_path).unwrap(), after_undo);
        assert_eq!(
            rerun
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );
    }

    #[test]
    fn undo_restores_presentation_after_mutable_menu_and_cell_edits() {
        let fixture = fixture();
        startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let snapshot = fixture.store.authoring_snapshot().unwrap();
        let mut candidate = (*snapshot.document).clone();
        let menu = candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id.as_str() == "starter-favorites")
            .unwrap();
        menu.name = "Renamed favorites".into();
        let cell = &mut menu.rings[0].cells[0];
        cell.label = "Updated favorite action".into();
        cell.content = super::super::model::CellContent::Control {
            control: super::super::model::Control::Close,
        };
        fixture
            .store
            .commit_authoring(
                snapshot.revision,
                &snapshot.disk_sha256,
                candidate,
                AssetMutations::default(),
            )
            .unwrap();

        let restored = restore(&fixture.store, &fixture.settings_path).unwrap();
        let menu = restored
            .document
            .menus
            .iter()
            .find(|menu| menu.id.as_str() == "starter-favorites")
            .unwrap();
        assert_eq!(menu.submenu_presentation, SubmenuPresentation::Cascade);
        assert_eq!(menu.name, "Renamed favorites");
        assert_eq!(menu.rings[0].cells[0].label, "Updated favorite action");
        assert!(matches!(
            &menu.rings[0].cells[0].content,
            super::super::model::CellContent::Control {
                control: super::super::model::Control::Close
            }
        ));
    }

    #[test]
    fn undo_skips_deleted_and_reused_menu_ids_but_marks_the_receipt_undone() {
        let fixture = fixture();
        startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let snapshot = fixture.store.authoring_snapshot().unwrap();
        let mut candidate = (*snapshot.document).clone();
        let root_id = candidate.default_menu_id.clone();
        let root = candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id == root_id)
            .unwrap();
        for ring in &mut root.rings {
            ring.cells
                .retain(|cell| cell.id.as_str() != "starter-root-dashboard");
        }
        candidate
            .menus
            .retain(|menu| menu.id.as_str() != "starter-dashboard");
        let replacement = candidate
            .menus
            .iter_mut()
            .find(|menu| menu.id.as_str() == "starter-favorites")
            .unwrap();
        replacement.name = "Replacement with a reused stable ID".into();
        replacement.rings[0].id = crate::radial::model::RingId::new("replacement-ring");
        replacement.rings[0].cells[0].id = crate::radial::model::CellId::new("replacement-cell");
        fixture
            .store
            .commit_authoring(
                snapshot.revision,
                &snapshot.disk_sha256,
                candidate,
                AssetMutations::default(),
            )
            .unwrap();

        let restored = restore(&fixture.store, &fixture.settings_path).unwrap();
        assert_eq!(
            restored
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );
        assert_eq!(
            restored
                .document
                .menus
                .iter()
                .find(|menu| menu.id.as_str() == "starter-favorites")
                .unwrap()
                .submenu_presentation,
            SubmenuPresentation::SameCenter
        );
        assert!(
            !restored
                .document
                .menus
                .iter()
                .any(|menu| menu.id.as_str() == "starter-dashboard")
        );
        assert!(
            restored
                .notice
                .as_deref()
                .unwrap()
                .contains("left untouched")
        );
        assert!(restored.notice.as_deref().unwrap().contains("3"));
    }

    #[test]
    fn prepared_migration_recovers_before_and_after_radial_commit_and_prepared_undo_finishes() {
        let fixture = fixture();
        let migrated = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let original_receipt = migrated.settings.radial_submenu_migration.clone().unwrap();

        std::fs::write(&fixture.radial_path, &fixture.source_radial_bytes).unwrap();
        fixture.store.reload().unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::Prepared;
        });
        let recovered_before_commit =
            startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            recovered_before_commit
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Applied
        );
        assert_eq!(
            fixture.store.authoring_snapshot().unwrap().disk_sha256.0,
            original_receipt.target_radial_sha256
        );

        let radial_target = std::fs::read(&fixture.radial_path).unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::Prepared;
        });
        let recovered_after_commit =
            startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            recovered_after_commit
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Applied
        );
        assert_eq!(std::fs::read(&fixture.radial_path).unwrap(), radial_target);

        let undone = restore(&fixture.store, &fixture.settings_path).unwrap();
        assert_eq!(
            undone
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial.default_submenu_presentation = SubmenuPresentation::SameCenter;
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::UndoPrepared;
        });
        let recovered_undo = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            recovered_undo
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );
        assert_eq!(
            recovered_undo.settings.radial.default_submenu_presentation,
            SubmenuPresentation::Cascade
        );
    }

    #[test]
    fn prepared_undo_recovery_preserves_unrelated_settings_and_a_later_cascade() {
        let fixture = fixture();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial.default_submenu_presentation = SubmenuPresentation::SameCenter;
        });
        startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let migrated_radial_bytes = std::fs::read(&fixture.radial_path).unwrap();
        let undone = restore(&fixture.store, &fixture.settings_path).unwrap();
        assert_eq!(
            undone
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );

        // Recreate a crash after UndoPrepared but before its checked radial
        // commit, then apply unrelated settings plus an explicit later Cascade.
        std::fs::write(&fixture.radial_path, migrated_radial_bytes).unwrap();
        fixture.store.reload().unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial.default_submenu_presentation = SubmenuPresentation::Cascade;
            settings.enable_toasts = false;
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::UndoPrepared;
        });

        let recovered = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            recovered
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Undone
        );
        assert_eq!(
            recovered.settings.radial.default_submenu_presentation,
            SubmenuPresentation::Cascade
        );
        assert!(!recovered.settings.enable_toasts);
        assert!(
            recovered
                .document
                .menus
                .iter()
                .all(|menu| menu.submenu_presentation == SubmenuPresentation::Cascade)
        );
    }

    #[test]
    fn malformed_settings_and_prepared_backup_sha_conflicts_do_not_change_radial_data() {
        let fixture = fixture();
        std::fs::write(&fixture.settings_path, b"{broken").unwrap();
        let radial_before = std::fs::read(&fixture.radial_path).unwrap();
        let settings_before = std::fs::read(&fixture.settings_path).unwrap();
        assert!(startup_migrate(&fixture.store, &fixture.settings_path, true).is_err());
        assert_eq!(std::fs::read(&fixture.radial_path).unwrap(), radial_before);
        assert_eq!(
            std::fs::read(&fixture.settings_path).unwrap(),
            settings_before
        );

        // Prepare through a successful migration, then reproduce a crash before
        // the radial commit and corrupt one verified backup.
        std::fs::write(&fixture.settings_path, &fixture.source_settings_bytes).unwrap();
        let migrated = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        let receipt = migrated.settings.radial_submenu_migration.clone().unwrap();
        std::fs::write(&fixture.radial_path, &fixture.source_radial_bytes).unwrap();
        fixture.store.reload().unwrap();
        rewrite_settings(&fixture.settings_path, |settings| {
            settings.radial_submenu_migration.as_mut().unwrap().state =
                SubmenuMigrationState::Prepared;
        });
        std::fs::write(&receipt.radial_backup_path, b"tampered backup").unwrap();
        let source_sha = sha256_hex(&fixture.source_radial_bytes);
        let failed = startup_migrate(&fixture.store, &fixture.settings_path, true).unwrap();
        assert_eq!(
            failed
                .settings
                .radial_submenu_migration
                .as_ref()
                .unwrap()
                .state,
            SubmenuMigrationState::Failed
        );
        assert_eq!(
            fixture.store.authoring_snapshot().unwrap().disk_sha256.0,
            source_sha
        );
        assert_eq!(
            failed.settings.radial.default_submenu_presentation,
            SubmenuPresentation::Cascade
        );
    }

    #[test]
    fn disk_content_conflict_fails_before_any_migration_write() {
        let fixture = fixture();
        let source_settings = std::fs::read(&fixture.settings_path).unwrap();
        let mut changed_disk = fixture.source_document.clone();
        changed_disk
            .metadata
            .insert("external-conflict".into(), "must not overwrite".into());
        let changed_bytes = serde_json::to_vec_pretty(&changed_disk).unwrap();
        std::fs::write(&fixture.radial_path, &changed_bytes).unwrap();
        let radial_before = std::fs::read(&fixture.radial_path).unwrap();
        assert!(startup_migrate(&fixture.store, &fixture.settings_path, true).is_err());
        assert_eq!(std::fs::read(&fixture.radial_path).unwrap(), radial_before);
        assert_eq!(
            std::fs::read(&fixture.settings_path).unwrap(),
            source_settings
        );
    }
}
