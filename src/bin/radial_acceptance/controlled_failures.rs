//! Opt-in R03/R05 proof. Inner runs fail; only verified persisted failures pass outside.
use super::*;
use serde::{Deserialize, Serialize};

pub(super) const PROBE_MODE: &str = "native_windows_controlled_probe";
pub(super) const AGGREGATE_MODE: &str = "native_windows_controlled_failures";
pub(super) const CASE_IDS: [&str; 8] = ["N01", "N02", "N03", "N04", "N05", "N06", "CLEANUP", "R0"];
pub(super) const MAX_PROBE_BYTES: usize = 48 * 1024;
const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_CONTROLLED_BYTES: usize = 6 * MAX_PROBE_BYTES + 32 * 1024;
const MAX_ATTEMPT_CLEANUP_ERRORS: usize = 4;

// Internally tagged Serde receipts buffer their fields in a deserializer that
// supports u64, but not u128. Keep the clocks wide in memory and check the
// numeric wire boundary for every controlled receipt timestamp.
mod unix_ms_wire {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        let milliseconds = u64::try_from(*value).map_err(serde::ser::Error::custom)?;
        serializer.serialize_u64(milliseconds)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        u64::deserialize(deserializer).map(u128::from)
    }

    pub mod optional {
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        pub fn serialize<S: Serializer>(
            value: &Option<u128>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            value
                .map(u64::try_from)
                .transpose()
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer)
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<u128>, D::Error> {
            Option::<u64>::deserialize(deserializer).map(|value| value.map(u128::from))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Operation {
    #[default]
    Ordinary,
    Aggregate,
    Probe(ProbeKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProbeKind {
    Startup,
    InputTimeout,
    Query,
    Ui,
    ChildExit,
    DesktopMismatch,
}

impl ProbeKind {
    pub(super) const ALL: [Self; 6] = [
        Self::Startup,
        Self::InputTimeout,
        Self::Query,
        Self::Ui,
        Self::ChildExit,
        Self::DesktopMismatch,
    ];
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "startup" => Ok(Self::Startup), "input-timeout" => Ok(Self::InputTimeout),
            "query" => Ok(Self::Query), "ui" => Ok(Self::Ui), "child-exit" => Ok(Self::ChildExit),
            "desktop-mismatch" => Ok(Self::DesktopMismatch),
            _ => Err("controlled probe must be startup, input-timeout, query, ui, child-exit, or desktop-mismatch".into()),
        }
    }
    pub(super) fn argument(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::InputTimeout => "input-timeout",
            Self::Query => "query",
            Self::Ui => "ui",
            Self::ChildExit => "child-exit",
            Self::DesktopMismatch => "desktop-mismatch",
        }
    }
    pub(super) fn id(self) -> &'static str {
        self.inventory()[0]
    }
    pub(super) fn inventory(self) -> &'static [&'static str; 3] {
        match self {
            Self::Startup => &["N01", "CLEANUP", "R0"],
            Self::InputTimeout => &["N02", "CLEANUP", "R0"],
            Self::Query => &["N03", "CLEANUP", "R0"],
            Self::Ui => &["N04", "CLEANUP", "R0"],
            Self::ChildExit => &["N05", "CLEANUP", "R0"],
            Self::DesktopMismatch => &["N06", "CLEANUP", "R0"],
        }
    }
    pub(super) fn failure_stage(self) -> FailureStage {
        match self {
            Self::Startup => FailureStage::CandidateStartup,
            Self::InputTimeout => FailureStage::InputInjection,
            Self::Query => FailureStage::NativeRootState,
            Self::Ui => FailureStage::DesignerWidget,
            Self::ChildExit => FailureStage::NativeRootState,
            Self::DesktopMismatch => FailureStage::InputInjection,
        }
    }
}

pub(super) fn validate_nonce(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Err("controlled probe nonce must be a bounded lowercase token".into())
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeOwner {
    pub child_pid: u32,
    #[serde(with = "unix_ms_wire")]
    pub child_started_unix_ms: u128,
    pub process_created_filetime: u64,
    pub root_hwnd: u64,
    pub foreground_hwnd: u64,
    pub foreground_pid: u32,
    pub foreground_thread_id: u32,
    pub input_desktop: String,
    #[serde(with = "unix_ms_wire")]
    pub observed_unix_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyEdge {
    pub inserted: usize,
    #[serde(with = "unix_ms_wire")]
    pub at_unix_ms: u128,
    pub foreground_hwnd: u64,
    pub foreground_pid: u32,
    pub input_desktop: String,
    pub vk: u16,
    pub scan: u16,
    pub flags: u32,
    pub cookie: u64,
    pub async_before: u16,
    pub async_after: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObservedKeyEdge {
    pub vk: u32,
    pub down: bool,
    pub injected: bool,
    pub cookie: u64,
    pub relative_us: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeldKeyProof {
    pub down: KeyEdge,
    pub up: Option<KeyEdge>,
    #[serde(with = "unix_ms_wire")]
    pub timeout_at_unix_ms: u128,
    pub timeout_async_state: u16,
    pub timeout_elapsed_ms: u64,
    pub async_after_cleanup: u16,
    pub observer_desktop: String,
    pub observed_edges: Vec<ObservedKeyEdge>,
    pub outstanding_keys: usize,
    pub observer_stopped: bool,
    pub primary_error: String,
    pub release_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnvironmentRefusal {
    pub actual: NativeOwner,
    pub expected_desktop: String,
    #[serde(with = "unix_ms_wire")]
    pub checked_at_unix_ms: u128,
    pub input_inserted: usize,
    pub refused_before_send_input: bool,
    pub owned_state_unchanged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChildExitWait {
    #[serde(with = "unix_ms_wire")]
    pub wait_entered_unix_ms: u128,
    #[serde(with = "unix_ms_wire")]
    pub pending_observed_unix_ms: u128,
    #[serde(with = "unix_ms_wire")]
    pub termination_injected_unix_ms: u128,
    #[serde(with = "unix_ms_wire")]
    pub exit_observed_unix_ms: u128,
    pub pending_after_entry_us: u64,
    pub termination_after_entry_us: u64,
    pub exit_after_entry_us: u64,
    pub pending_poll_index: usize,
    pub exit_poll_index: usize,
    pub observed_exit_code: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum StageProof {
    Startup {
        root_hwnd: u64,
        #[serde(with = "unix_ms_wire")]
        wait_started_unix_ms: u128,
        #[serde(with = "unix_ms_wire")]
        wait_finished_unix_ms: u128,
        elapsed_ms: u64,
        withheld_observations: usize,
    },
    InputTimeout {
        key: HeldKeyProof,
    },
    Query {
        invocation: Box<QueryInvocationEvidence>,
        #[serde(with = "unix_ms_wire")]
        wait_started_unix_ms: u128,
        #[serde(with = "unix_ms_wire")]
        wait_finished_unix_ms: u128,
        elapsed_ms: u64,
        expected_missing_query_digest: u64,
        wait_error: String,
    },
    Ui {
        designer_hwnd: u64,
        child_pid: u32,
        session_id: u64,
        snapshot_trace_seq: u64,
        #[serde(with = "unix_ms_wire")]
        wait_started_unix_ms: u128,
        #[serde(with = "unix_ms_wire")]
        wait_finished_unix_ms: u128,
        elapsed_ms: u64,
        missing_control_digest: u64,
        wait_error: String,
    },
    ChildExit {
        wait: ChildExitWait,
        input_refused_after_exit: bool,
        input_inserted_after_exit: usize,
    },
    DesktopMismatch {
        admission: EnvironmentRefusal,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileHashes {
    pub settings: String,
    pub settings_full: String,
    pub radial: String,
    pub actions: String,
    pub settings_without_designer_geometry: String,
}

impl ProfileHashes {
    fn is_valid(&self) -> bool {
        [
            &self.settings,
            &self.settings_full,
            &self.radial,
            &self.actions,
            &self.settings_without_designer_geometry,
        ]
        .into_iter()
        .all(|hash| is_hash(hash))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LiveOwnership {
    pub kind: ProbeKind,
    pub nonce: String,
    pub runner_pid: u32,
    pub source_revision: String,
    pub candidate_sha256: String,
    pub profile_root: String,
    pub owner: NativeOwner,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreatedOwnership {
    pub kind: ProbeKind,
    pub nonce: String,
    pub runner_pid: u32,
    pub source_revision: String,
    pub candidate_sha256: String,
    pub profile_root: String,
    pub child_pid: u32,
    #[serde(with = "unix_ms_wire")]
    pub child_started_unix_ms: u128,
    pub process_created_filetime: u64,
    pub profile_hashes: ProfileHashes,
}

#[cfg(windows)]
pub(super) struct LaunchOwnership {
    pub output: PathBuf,
    pub created: CreatedOwnership,
}

#[cfg(windows)]
pub(super) fn launch_ownership(
    output: &Path,
    report: &AcceptanceReport,
) -> Result<LaunchOwnership, String> {
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("probe owner missing".into());
    };
    Ok(LaunchOwnership {
        output: output.to_path_buf(),
        created: CreatedOwnership {
            kind: receipt.kind,
            nonce: receipt.nonce.clone(),
            runner_pid: std::process::id(),
            source_revision: report
                .environment
                .source_revision
                .clone()
                .ok_or("source revision missing")?,
            candidate_sha256: report.candidate.sha256.clone(),
            profile_root: report.profile.temporary_data_root.clone(),
            child_pid: 0,
            child_started_unix_ms: 0,
            process_created_filetime: 0,
            profile_hashes: read_profile_hashes(Path::new(&report.profile.temporary_data_root))?,
        },
    })
}

#[cfg(windows)]
impl LaunchOwnership {
    pub(super) fn publish_created(
        &self,
        child_pid: u32,
        started: SystemTime,
        filetime: u64,
    ) -> Result<(), String> {
        let mut created = self.created.clone();
        created.child_pid = child_pid;
        created.child_started_unix_ms = started
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        created.process_created_filetime = filetime;
        publish_new(
            &self
                .output
                .join(format!("case-{}-created.json", created.kind.id())),
            &serde_json::to_vec(&created).map_err(|e| e.to_string())?,
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OwnedKeyLedger {
    pub nonce: String,
    pub edge: KeyEdge,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessLeaseAck {
    nonce: String,
    subprocess_pid: u32,
    child_pid: u32,
    process_created_filetime: u64,
}

fn process_lease_ack_ready(
    ack: Option<&ProcessLeaseAck>,
    nonce: &str,
    subprocess_pid: u32,
    owner: &NativeOwner,
    elapsed: Duration,
    timeout: Duration,
) -> Result<bool, String> {
    require(
        elapsed < timeout,
        "controlled outer owner did not acquire the actual application process lease",
    )?;
    let Some(ack) = ack else { return Ok(false) };
    require(
        ack.nonce == nonce
            && ack.subprocess_pid == subprocess_pid
            && ack.child_pid == owner.child_pid
            && ack.process_created_filetime == owner.process_created_filetime,
        "controlled outer process lease acknowledgement has wrong ownership",
    )?;
    Ok(true)
}

#[cfg(windows)]
pub(super) fn await_process_lease(output: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("probe missing".into());
    };
    let owner = receipt.owner.as_ref().ok_or("owned child missing")?;
    let path = output.join(format!("case-{}-lease-ack.json", receipt.kind.id()));
    // The peer verifies the actual executable hash and creation identity before
    // ACK. That transfer belongs to the same bounded startup budget as ROOT.
    let started = Instant::now();
    loop {
        let ack = if path.exists() {
            let bytes = read_regular_bounded(&path, 4096)?;
            let ack: ProcessLeaseAck = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            Some(ack)
        } else {
            None
        };
        if process_lease_ack_ready(
            ack.as_ref(),
            &receipt.nonce,
            std::process::id(),
            owner,
            started.elapsed(),
            native::STARTUP_TIMEOUT,
        )? {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(windows)]
pub(super) fn prepare_fresh_controlled_fixture(
    profile: &Path,
    fixture: &mut DeterministicFixture,
) -> Result<(), String> {
    use multi_launcher::radial::{store::RadialStore, submenu_migration};
    let settings_path = profile.join("settings.json");
    let radial_path = profile.join("radial.json");
    for (name, expected) in [
        ("settings.json", &fixture.settings_json),
        ("radial.json", &fixture.radial_json),
        ("actions.json", &fixture.actions_json),
    ] {
        require(
            read_regular_bounded(&profile.join(name), MAX_JSON_REPORT_BYTES)? == *expected,
            "fresh controlled fixture differs from its authored bytes before migration",
        )?;
    }
    let authored_settings = canonical_settings(&fixture.settings_json)?;
    let settings: Settings =
        serde_json::from_value(authored_settings.clone()).map_err(|e| e.to_string())?;
    require(
        authored_settings
            == canonical_settings(&serde_json::to_vec(&settings).map_err(|e| e.to_string())?)?,
        "fresh controlled settings must contain their complete authored fields without unknown loss",
    )?;
    let _: Vec<multi_launcher::actions::Action> =
        serde_json::from_slice(&fixture.actions_json).map_err(|e| e.to_string())?;
    let document: RadialDocument =
        serde_json::from_slice(&fixture.radial_json).map_err(|e| e.to_string())?;
    require(
        settings.window_size == Some((900, 650))
            && settings.pinned_panels.is_empty()
            && settings.radial_submenu_migration.is_none()
            && settings.radial.default_submenu_presentation
                == multi_launcher::radial::model::SubmenuPresentation::SameCenter
            && document.menus.iter().all(|menu| {
                menu.submenu_presentation
                    == multi_launcher::radial::model::SubmenuPresentation::SameCenter
            }),
        "controlled migration preparation requires its fresh authored SameCenter fixture",
    )?;
    // Independently admit only the documented settings startup migration. The
    // normal close owner writes the authored ROOT size/no pins and these same
    // Clipboard Modify dimensions; it must not create protected defaults later.
    let mut expected_settings = settings;
    if !expected_settings
        .plugin_settings
        .contains_key("clipboard_modify")
    {
        expected_settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::to_value(
                multi_launcher::settings::ClipboardModifyPluginSettings::default(),
            )
            .map_err(|e| e.to_string())?,
        );
        if let Some(enabled) = expected_settings.enabled_plugins.as_mut() {
            enabled.insert("clipboard_modify".into());
        }
    }
    let clipboard_value = &expected_settings.plugin_settings["clipboard_modify"];
    let clipboard: multi_launcher::settings::ClipboardModifyPluginSettings =
        serde_json::from_value(clipboard_value.clone()).map_err(|e| e.to_string())?;
    require(
        clipboard.dialog_width == 900.0
            && clipboard.dialog_height == 640.0
            && *clipboard_value == serde_json::to_value(&clipboard).map_err(|e| e.to_string())?,
        "fresh controlled Clipboard Modify preferences differ from their complete normal close defaults",
    )?;
    let startup = multi_launcher::startup::load_startup_settings(&settings_path);
    require(
        startup.diagnostic.is_none(),
        "controlled settings startup reported a diagnostic",
    )?;
    let startup_bytes = read_regular_bounded(&settings_path, MAX_JSON_REPORT_BYTES)?;
    let expected_startup =
        canonical_settings(&serde_json::to_vec(&expected_settings).map_err(|e| e.to_string())?)?;
    require(
        canonical_settings(&startup_bytes)? == expected_startup
            && canonical_settings(
                &serde_json::to_vec(&startup.settings).map_err(|e| e.to_string())?,
            )? == expected_startup
            && read_regular_bounded(&radial_path, MAX_JSON_REPORT_BYTES)? == fixture.radial_json
            && read_regular_bounded(&profile.join("actions.json"), MAX_JSON_REPORT_BYTES)?
                == fixture.actions_json,
        "controlled settings startup changed more than its documented fresh migration",
    )?;
    let store =
        RadialStore::at_path(&radial_path, RadialDocument::starter()).map_err(|e| e.to_string())?;
    store.reload().map_err(|e| e.to_string())?;
    let outcome = submenu_migration::startup_migrate(&store, &settings_path, true)?;
    let receipt = outcome
        .settings
        .radial_submenu_migration
        .as_ref()
        .ok_or("controlled fixture migration omitted its durable receipt")?;
    require(
        outcome.changed
            && receipt.migration_id == submenu_migration::MIGRATION_ID
            && receipt.version == submenu_migration::MIGRATION_VERSION
            && receipt.state == multi_launcher::settings::SubmenuMigrationState::Applied
            && receipt.source_settings_sha256 == sha256_bytes(&startup_bytes)
            && receipt.source_radial_sha256 == sha256_bytes(&fixture.radial_json)
            && receipt.failure.is_none(),
        "controlled fixture migration did not complete before its baseline",
    )?;
    let mut expected_document = document;
    expected_document.revision.0 = expected_document
        .revision
        .0
        .checked_add(1)
        .ok_or("controlled fixture revision overflow")?;
    expected_settings.radial_submenu_migration = outcome.settings.radial_submenu_migration.clone();
    let settings_bytes = read_regular_bounded(&settings_path, MAX_JSON_REPORT_BYTES)?;
    let radial_bytes = read_regular_bounded(&radial_path, MAX_JSON_REPORT_BYTES)?;
    let expected_final =
        canonical_settings(&serde_json::to_vec(&expected_settings).map_err(|e| e.to_string())?)?;
    require(
        *outcome.document == expected_document
            && serde_json::from_slice::<RadialDocument>(&radial_bytes)
                .map_err(|e| e.to_string())?
                == expected_document
            && canonical_settings(
                &serde_json::to_vec(&outcome.settings).map_err(|e| e.to_string())?,
            )? == expected_final
            && canonical_settings(&settings_bytes)? == expected_final
            && read_regular_bounded(&profile.join("actions.json"), MAX_JSON_REPORT_BYTES)?
                == fixture.actions_json,
        "controlled fixture migration changed protected authored settings, queries or actions",
    )?;
    fixture.settings_json = settings_bytes;
    fixture.radial_json = radial_bytes;
    Ok(())
}

fn read_regular_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let parent = path.parent().ok_or("controlled artifact has no parent")?;
    let name = Path::new(path.file_name().ok_or("controlled artifact has no name")?);
    let mut input =
        copied_profile::open_regular_profile_file(path, parent, name).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut input)
        .take((maximum + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    require(
        !bytes.is_empty() && bytes.len() <= maximum,
        "controlled artifact is empty or exceeds its cap",
    )?;
    Ok(bytes)
}

// Publish a complete immutable receipt before a different process can observe
// its name. Both peers use fresh directories and fixed, operation-owned names.
fn publish_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    require(
        !bytes.is_empty() && bytes.len() <= 8 * 1024,
        "live ownership receipt exceeds its bound",
    )?;
    require(!path.exists(), "live ownership receipt already exists")?;
    let temporary = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
pub(super) fn publish_live_owner(output: &Path, report: &AcceptanceReport) -> Result<(), String> {
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("probe owner missing".into());
    };
    let live = LiveOwnership {
        kind: receipt.kind,
        nonce: receipt.nonce.clone(),
        runner_pid: std::process::id(),
        source_revision: report
            .environment
            .source_revision
            .clone()
            .ok_or("source revision missing")?,
        candidate_sha256: report.candidate.sha256.clone(),
        profile_root: report.profile.temporary_data_root.clone(),
        owner: receipt.owner.clone().ok_or("native child owner missing")?,
    };
    publish_new(
        &output.join(format!("case-{}-owner.json", receipt.kind.id())),
        &serde_json::to_vec(&live).map_err(|e| e.to_string())?,
    )
}

#[cfg(windows)]
pub(super) fn publish_owned_key(
    output: &Path,
    nonce: &str,
    edge: &KeyEdge,
    down: bool,
) -> Result<(), String> {
    let ledger = OwnedKeyLedger {
        nonce: nonce.into(),
        edge: edge.clone(),
    };
    publish_new(
        &output.join(if down {
            "case-N02-key-down.json"
        } else {
            "case-N02-key-up.json"
        }),
        &serde_json::to_vec(&ledger).map_err(|e| e.to_string())?,
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProbeReceipt {
    pub kind: ProbeKind,
    pub nonce: String,
    pub owner: Option<NativeOwner>,
    pub proof: Option<StageProof>,
    pub profile_before: ProfileHashes,
    pub profile_after: Option<ProfileHashes>,
    pub child_exit_code: Option<u32>,
    pub owned_keys_after: Option<usize>,
    pub marker_count_before: Option<usize>,
    pub marker_count_after: Option<usize>,
    pub execution_count_before: Option<usize>,
    pub execution_count_after: Option<usize>,
    pub cleanup_errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VerifiedProbe {
    pub kind: ProbeKind,
    pub nonce: String,
    pub subprocess_pid: u32,
    pub actual_exit_code: i32,
    #[serde(with = "unix_ms_wire")]
    pub started_unix_ms: u128,
    #[serde(with = "unix_ms_wire")]
    pub finished_unix_ms: u128,
    pub report_path: String,
    pub report_sha256: String,
    pub text_sha256: String,
    pub receipt: ProbeReceipt,
    pub cleanup: CleanupResult,
    pub private_artifacts: private_artifacts::PrivateArtifactSummary,
    pub command: Vec<String>,
    pub created_owner: CreatedOwnership,
    pub leased_owner: NativeOwner,
    pub observed_child_exit_code: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SubprocessExitOrigin {
    RunnerCompletion,
    ObservedBeforeTermination,
    OuterTermination,
}

// Unavailable observations are explicit nulls, never an omitted fact that a
// readback silently supplies. A custom field deserializer keeps Option fields
// required while retaining their truthful nullable representation.
fn required_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubprocessStatus {
    #[serde(deserialize_with = "required_nullable")]
    code: Option<i32>,
    success: bool,
    #[serde(deserialize_with = "required_nullable")]
    windows_status: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
enum SubprocessState {
    NotStarted,
    Running {
        process_id: u32,
    },
    ExitObserved {
        process_id: u32,
        #[serde(with = "unix_ms_wire::optional")]
        observed_unix_ms: Option<u128>,
        status: SubprocessStatus,
        origin: SubprocessExitOrigin,
    },
}

// An attempt records outer-owner observations, including failed admission or
// readback. It certifies neither a native stage nor application/key cleanup.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubprocessAttempt {
    kind: ProbeKind,
    nonce: String,
    #[serde(deserialize_with = "required_nullable")]
    source_revision: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    runner_sha256: Option<String>,
    candidate_executable: String,
    candidate_sha256: String,
    #[serde(deserialize_with = "required_nullable")]
    command: Option<Vec<String>>,
    #[serde(with = "unix_ms_wire::optional")]
    started_unix_ms: Option<u128>,
    #[serde(with = "unix_ms_wire::optional")]
    finished_unix_ms: Option<u128>,
    process: SubprocessState,
    #[serde(deserialize_with = "required_nullable")]
    created_owner: Option<CreatedOwnership>,
    #[serde(deserialize_with = "required_nullable")]
    live_owner: Option<LiveOwnership>,
    #[serde(deserialize_with = "required_nullable")]
    primary_error: Option<String>,
    cleanup_errors: Vec<String>,
}

struct SubprocessRun {
    attempt: SubprocessAttempt,
    result: Result<VerifiedProbe, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Evidence {
    Probe {
        receipt: Box<ProbeReceipt>,
    },
    Aggregate {
        probes: Vec<VerifiedProbe>,
        attempts: Vec<SubprocessAttempt>,
    },
}

pub(super) fn receipt_mut(report: &mut AcceptanceReport) -> Option<&mut ProbeReceipt> {
    match report.controlled_failures.as_mut()? {
        Evidence::Probe { receipt } => Some(receipt),
        _ => None,
    }
}

pub(super) fn primary_message(proof: &StageProof) -> String {
    bounded_text(&match proof {
        StageProof::InputTimeout { key } => key.primary_error.clone(),
        StageProof::Startup { .. } => "controlled startup ROOT readiness acknowledgement withheld until its deadline".into(),
        StageProof::Query { wait_error, .. } => format!("controlled query wait after owned handoff: {wait_error}"),
        StageProof::Ui { wait_error, .. } => format!("controlled UI wait after owned InitialSnapshot: {wait_error}"),
        StageProof::ChildExit { .. } => "controlled owned child exited nonzero during an outstanding operation; subsequent input refused".into(),
        StageProof::DesktopMismatch { .. } => "controlled expected desktop differs from actual measured native environment; refused before SendInput".into(),
    }, MAX_RESULT_BYTES)
}

pub(super) fn record_environment_failure(
    kind: ProbeKind,
    error: &str,
    report: &mut AcceptanceReport,
) {
    report.push_case(AcceptanceCaseResult {
        id: kind.id().into(),
        status: CaseStatus::Failed,
        elapsed_ms: 0,
        expected: "reach the actual controlled native stage on the supported input desktop".into(),
        observed: bounded_text(
            &format!("controlled native stage not reached: {error}"),
            MAX_RESULT_BYTES,
        ),
        failure_stage: Some(FailureStage::Environment),
        artifacts: Vec::new(),
    });
    report.push_case(AcceptanceCaseResult { id: "CLEANUP".into(), status: CaseStatus::Failed, elapsed_ms: 0,
        expected: "retain actual started-child/input cleanup proof".into(),
        observed: "no native child or inserted input was allocated; the intended stage has no successful cleanup proof".into(),
        failure_stage: Some(FailureStage::Cleanup), artifacts: Vec::new() });
}

pub(super) fn case_inventory(report: &AcceptanceReport) -> Option<&'static [&'static str]> {
    match (&report.mode, &report.controlled_failures) {
        (&PROBE_MODE, Some(Evidence::Probe { receipt })) => Some(receipt.kind.inventory()),
        (&AGGREGATE_MODE, Some(Evidence::Aggregate { .. })) => Some(&CASE_IDS),
        _ => None,
    }
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn bounded_wait(start: u128, end: u128, elapsed: u64, minimum_ms: u64, maximum_ms: u64) -> bool {
    start > 0
        && end >= start
        && elapsed >= minimum_ms
        && elapsed <= maximum_ms
        && end - start <= u128::from(maximum_ms)
        && (end - start).abs_diff(u128::from(elapsed)) <= 50
}

fn owner_is_valid(owner: &NativeOwner, report: &AcceptanceReport) -> bool {
    owner.child_pid > 0
        && Some(owner.child_pid) == report.environment.child_process_id
        && owner.child_pid != report.environment.runner_process_id
        && owner.child_started_unix_ms > 0
        && Some(owner.child_started_unix_ms) == report.environment.child_started_unix_ms
        && owner.process_created_filetime > 0
        && owner.root_hwnd > 0
        && owner.foreground_hwnd == owner.root_hwnd
        && owner.foreground_pid == owner.child_pid
        && owner.foreground_thread_id > 0
        && owner.input_desktop == "thread=Default;active=Default"
        && owner.observed_unix_ms >= owner.child_started_unix_ms
        && owner.observed_unix_ms >= report.started_unix_ms
        && owner.observed_unix_ms <= report.finished_unix_ms
}

fn same_native_identity(a: &NativeOwner, b: &NativeOwner) -> bool {
    a.child_pid == b.child_pid
        && a.child_started_unix_ms == b.child_started_unix_ms
        && a.process_created_filetime == b.process_created_filetime
        && a.root_hwnd == b.root_hwnd
        && a.foreground_hwnd == b.foreground_hwnd
        && a.foreground_pid == b.foreground_pid
        && a.foreground_thread_id == b.foreground_thread_id
        && a.input_desktop == b.input_desktop
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn metadata_is_bounded(report: &AcceptanceReport) -> bool {
    report.schema_version == 9
        && report.started_unix_ms > 0
        && report.finished_unix_ms >= report.started_unix_ms
        && !report.run_id.is_empty()
        && report.run_id.len() <= 64
        && report
            .run_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && report.environment.runner_process_id > 0
        && report
            .environment
            .runner_sha256
            .as_deref()
            .is_some_and(is_hash)
        && report
            .environment
            .source_revision
            .as_deref()
            .is_some_and(|s| {
                !s.is_empty() && s.len() <= 160 && s.bytes().all(|b| b.is_ascii_graphic())
            })
        && is_hash(&report.candidate.sha256)
        && !report.candidate.executable.is_empty()
        && report.candidate.executable.len() <= MAX_PATH_BYTES
        && report.profile.temporary_data_root.len() <= MAX_PATH_BYTES
        && !report.profile.temporary_data_root.is_empty()
        && [
            &report.profile.settings_sha256,
            &report.profile.radial_sha256,
            &report.profile.actions_sha256,
        ]
        .iter()
        .all(|s| is_hash(s))
        && report.hotkey == AcceptanceHotkey::F11
        && report.profile.configured_hotkey == ACCEPTANCE_HOTKEY
        && report.h6_repeat_mode == H6RepeatMode::Quiescent
        && report.mouse_gesture_mode == MouseGestureMode::Enabled
        && report.hotkey_evidence.is_empty()
        && report.query_evidence.is_empty()
        && report.gate_c_evidence.is_empty()
        && report.gate_d_evidence.is_empty()
        && report.gate_s_evidence.is_empty()
        && !report.capacity_saturated
        && report.report_overflow.is_none()
        && report.environment.monitors.len() <= 16
        && report.environment.os_version.len() <= MAX_RESULT_BYTES
        && report.environment.architecture.len() <= 128
        && report.artifacts.len() <= MAX_ARTIFACTS
        && report
            .artifacts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= MAX_PATH_BYTES)
}

fn stage_wait_is_owned(
    start: u128,
    end: u128,
    owner: &NativeOwner,
    report: &AcceptanceReport,
) -> bool {
    start >= owner.observed_unix_ms && end <= report.finished_unix_ms
}

pub(super) fn owned_shift_down(edge: &KeyEdge) -> bool {
    edge.inserted == 1
        && edge.vk == 0xa0
        && edge.scan == 0
        && edge.flags == 0
        && edge.cookie == 0x5241_4449_414c_0001
        && edge.async_before & 0x8000 == 0
        && [edge.async_before, edge.async_after]
            .iter()
            .all(|state| state & 0x7ffe == 0)
}

pub(super) fn finalize_probe_cleanup(report: &mut AcceptanceReport) {
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return;
    };
    let mut errors = receipt.cleanup_errors.clone();
    if receipt.owned_keys_after != Some(0) {
        errors.push("owned key cleanup was not confirmed clear".into());
    }
    if !report.cleanup.child_owned_windows_closed {
        errors.push("owned application windows remained after shutdown".into());
    }
    if let Some(StageProof::InputTimeout { key }) = &receipt.proof {
        if let Some(error) = &key.release_error {
            errors.push(format!("owned input cleanup: {error}"));
        }
        if !key.observer_stopped || key.outstanding_keys != 0 {
            errors.push("owned input observer or key ownership remained active".into());
        }
    }
    if errors.is_empty() {
        return;
    }
    if let Some(cleanup) = report.cases.iter_mut().find(|case| case.id == "CLEANUP") {
        cleanup.status = CaseStatus::Failed;
        cleanup.failure_stage = Some(FailureStage::Cleanup);
        cleanup.observed = bounded_text(
            &format!(
                "{}; controlled cleanup: {}",
                cleanup.observed,
                errors.join("; ")
            ),
            MAX_RESULT_BYTES,
        );
    }
}

fn owned_key_pair_is_correlated(down: &KeyEdge, up: &KeyEdge, owner: &NativeOwner) -> bool {
    owned_shift_down(down)
        && up.inserted == 1
        && up.vk == down.vk
        && up.scan == down.scan
        && up.flags == 2
        && up.cookie == down.cookie
        && down.foreground_hwnd == owner.root_hwnd
        && down.foreground_pid == owner.child_pid
        && up.foreground_hwnd == owner.root_hwnd
        && up.foreground_pid == owner.child_pid
        && down.input_desktop == owner.input_desktop
        && up.input_desktop == owner.input_desktop
        && [up.async_before, up.async_after]
            .iter()
            .all(|s| s & 0x7ffe == 0)
        && up.async_before & 0x8000 != 0
        && down.at_unix_ms >= owner.observed_unix_ms
        && up.at_unix_ms >= down.at_unix_ms
}

fn held_key_is_valid(key: &HeldKeyProof, owner: &NativeOwner) -> bool {
    let Some(up) = &key.up else { return false };
    let down = &key.down;
    owned_key_pair_is_correlated(down, up, owner)
        && [key.timeout_async_state, key.async_after_cleanup]
            .iter()
            .all(|s| s & 0x7ffe == 0)
        && key.timeout_async_state & 0x8000 != 0
        && key.async_after_cleanup & 0x8000 == 0
        && key.timeout_at_unix_ms >= down.at_unix_ms
        && up.at_unix_ms >= key.timeout_at_unix_ms
        && key.timeout_elapsed_ms >= 250
        && key.timeout_elapsed_ms <= 1500
        && key.timeout_at_unix_ms - down.at_unix_ms >= 250
        && key.observer_desktop == "Default"
        && key.observed_edges.len() == 2
        && key
            .observed_edges
            .iter()
            .all(|e| e.vk == 0xa0 && e.injected && e.cookie == down.cookie)
        && key.observed_edges[0].down
        && !key.observed_edges[1].down
        && key.observed_edges[1].relative_us > key.observed_edges[0].relative_us
        && key.observed_edges[1].relative_us - key.observed_edges[0].relative_us >= 250_000
        && key.outstanding_keys == 0
        && key.observer_stopped
        && key.release_error.is_none()
        && key.primary_error
            == "controlled input acknowledgement deadline expired while owned LeftShift was held"
}

pub(super) fn validate_probe(
    report: &AcceptanceReport,
    require_disposed: bool,
) -> Result<(), String> {
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("missing typed controlled probe".into());
    };
    validate_nonce(&receipt.nonce)?;
    require(
        metadata_is_bounded(report)
            && report.mode == PROBE_MODE
            && report.profile.mode == "deterministic_fixture"
            && report.suite == AcceptanceSuite::All
            && report.hotkey == AcceptanceHotkey::F11
            && matches!(report.copied_profile_status, CopiedProfileStatus::NotRun)
            && report.copied_profile.is_none(),
        "probe mode/ordinary suite isolation violated",
    )?;
    require(
        serde_json::to_vec(receipt)
            .map_err(|e| e.to_string())?
            .len()
            <= MAX_PROBE_BYTES,
        "controlled receipt exceeds its bound",
    )?;
    let owner = receipt
        .owner
        .as_ref()
        .ok_or("probe did not reach a measured owned child")?;
    require(
        owner_is_valid(owner, report),
        "probe native owner is absent, stale or foreign",
    )?;
    let before = &receipt.profile_before;
    let after = receipt
        .profile_after
        .as_ref()
        .ok_or("probe final file hashes were not measured")?;
    require(
        before.is_valid()
            && after.is_valid()
            && before.settings == report.profile.settings_sha256
            && before.radial == report.profile.radial_sha256
            && before.actions == report.profile.actions_sha256
            && before.radial == after.radial
            && before.actions == after.actions
            && before.settings_without_designer_geometry
                == after.settings_without_designer_geometry
            && (receipt.kind == ProbeKind::Ui || before.settings_full == after.settings_full)
            && (before.settings != after.settings || before.settings_full == after.settings_full),
        "probe protected settings data changed beyond the UI owner's measured Designer geometry preferences",
    )?;
    require(
        receipt.cleanup_errors.is_empty()
            && receipt.owned_keys_after == Some(0)
            && receipt.marker_count_before == Some(0)
            && receipt.marker_count_after == Some(0)
            && receipt.execution_count_before
                == Some(if receipt.kind == ProbeKind::Query {
                    2
                } else {
                    0
                })
            && receipt.execution_count_after == receipt.execution_count_before,
        "probe cleanup/input/fixture effects were not preserved",
    )?;
    let proof = receipt
        .proof
        .as_ref()
        .ok_or("probe omitted its actual stage precondition")?;
    let valid = match (receipt.kind, proof) {
        (
            ProbeKind::Startup,
            StageProof::Startup {
                root_hwnd,
                wait_started_unix_ms,
                wait_finished_unix_ms,
                elapsed_ms,
                withheld_observations,
            },
        ) => {
            *root_hwnd == owner.root_hwnd
                && *withheld_observations > 1
                && *withheld_observations <= 256
                && stage_wait_is_owned(*wait_started_unix_ms, *wait_finished_unix_ms, owner, report)
                && bounded_wait(
                    *wait_started_unix_ms,
                    *wait_finished_unix_ms,
                    *elapsed_ms,
                    250,
                    1500,
                )
        }
        (ProbeKind::InputTimeout, StageProof::InputTimeout { key }) => {
            held_key_is_valid(key, owner)
                && key
                    .up
                    .as_ref()
                    .is_some_and(|up| up.at_unix_ms <= report.finished_unix_ms)
                && key.timeout_at_unix_ms - key.down.at_unix_ms <= 1500
        }
        (
            ProbeKind::Query,
            StageProof::Query {
                invocation,
                wait_started_unix_ms,
                wait_finished_unix_ms,
                elapsed_ms,
                expected_missing_query_digest,
                wait_error,
            },
        ) => {
            invocation.mode == QueryEvidenceMode::OpenLauncher
                && invocation.cell_id == "qa-open"
                && invocation.ui_ack == QueryEvidenceUiAck::LauncherQuery
                && invocation.effect_count == 0
                && invocation.cancelled_confirmation_count == 0
                && invocation.resolution_state == QueryEvidenceState::ManualUi
                && invocation.query_digest
                    == query_cell_digest("app QMarker").wrapping_mul(0x100000001b3)
                && invocation.requirement == QueryEvidenceRequirement::LauncherUi
                && invocation.dispatch_outcome == QueryEvidenceOutcome::Executed
                && invocation.root_policy == QueryEvidenceRootPolicy::Legacy
                && invocation.selected_digest.is_none()
                && invocation.provider_revision.is_none()
                && invocation
                    .root_observations
                    .iter()
                    .all(|o| o.hwnd == owner.root_hwnd)
                && query_invocation_evidence_is_valid(invocation, report)
                && *expected_missing_query_digest
                    == query_cell_digest("qa-controlled-missing-query-ack")
                && wait_error
                    == "focused launcher query editor did not contain the expected exact query \"qa-controlled-missing-query-ack\""
                && wait_error.len() <= MAX_RESULT_BYTES
                && stage_wait_is_owned(*wait_started_unix_ms, *wait_finished_unix_ms, owner, report)
                && bounded_wait(
                    *wait_started_unix_ms,
                    *wait_finished_unix_ms,
                    *elapsed_ms,
                    5000,
                    6500,
                )
        }
        (
            ProbeKind::Ui,
            StageProof::Ui {
                designer_hwnd,
                child_pid,
                session_id,
                snapshot_trace_seq,
                wait_started_unix_ms,
                wait_finished_unix_ms,
                elapsed_ms,
                missing_control_digest,
                wait_error,
            },
        ) => {
            *designer_hwnd > 0
                && *designer_hwnd != owner.root_hwnd
                && *child_pid == owner.child_pid
                && *session_id > 0
                && *snapshot_trace_seq > 0
                && *missing_control_digest == query_cell_digest("qa-controlled-missing-ui-control")
                && wait_error.starts_with(
                    "UIA control 'qa-controlled-missing-ui-control' did not publish fresh bounds",
                )
                && wait_error.len() <= MAX_RESULT_BYTES
                && stage_wait_is_owned(*wait_started_unix_ms, *wait_finished_unix_ms, owner, report)
                && bounded_wait(
                    *wait_started_unix_ms,
                    *wait_finished_unix_ms,
                    *elapsed_ms,
                    5000,
                    6500,
                )
        }
        (
            ProbeKind::ChildExit,
            StageProof::ChildExit {
                wait,
                input_refused_after_exit,
                input_inserted_after_exit,
            },
        ) => {
            stage_wait_is_owned(
                wait.wait_entered_unix_ms,
                wait.exit_observed_unix_ms,
                owner,
                report,
            ) && wait.pending_observed_unix_ms >= wait.wait_entered_unix_ms
                && wait.termination_injected_unix_ms >= wait.pending_observed_unix_ms
                && wait.exit_observed_unix_ms >= wait.termination_injected_unix_ms
                && wait.pending_after_entry_us <= wait.termination_after_entry_us
                && wait.termination_after_entry_us < wait.exit_after_entry_us
                && wait.pending_poll_index == 1
                && wait.exit_poll_index > wait.pending_poll_index
                && wait.exit_poll_index <= 256
                && wait.exit_after_entry_us <= 5_500_000
                && bounded_wait(
                    wait.wait_entered_unix_ms,
                    wait.pending_observed_unix_ms,
                    wait.pending_after_entry_us / 1000,
                    0,
                    5500,
                )
                && bounded_wait(
                    wait.wait_entered_unix_ms,
                    wait.termination_injected_unix_ms,
                    wait.termination_after_entry_us / 1000,
                    0,
                    5500,
                )
                && bounded_wait(
                    wait.wait_entered_unix_ms,
                    wait.exit_observed_unix_ms,
                    wait.exit_after_entry_us / 1000,
                    0,
                    5500,
                )
                && wait.observed_exit_code == 1
                && receipt.child_exit_code == Some(wait.observed_exit_code)
                && *input_refused_after_exit
                && *input_inserted_after_exit == 0
        }
        (ProbeKind::DesktopMismatch, StageProof::DesktopMismatch { admission }) => {
            admission.actual == *owner
                && admission.expected_desktop == "controlled_expected_nondefault"
                && admission.checked_at_unix_ms >= owner.observed_unix_ms
                && admission.checked_at_unix_ms <= report.finished_unix_ms
                && admission.input_inserted == 0
                && admission.refused_before_send_input
                && admission.owned_state_unchanged
        }
        _ => false,
    };
    require(
        valid,
        "controlled stage does not prove the expected actual native failure",
    )?;
    require(
        receipt.child_exit_code
            == Some(if receipt.kind == ProbeKind::ChildExit {
                1
            } else {
                0
            })
            && report.cleanup.child_owned_windows_closed
            && !report.cleanup.child_terminated_after_timeout
            && report.cleanup.child_closed_normally == (receipt.kind != ProbeKind::ChildExit),
        "controlled child disposition is incomplete or contradictory",
    )?;
    if require_disposed {
        require(
            report.cleanup.profile_removed
                && report.cleanup.input_desktop_released
                && report.cleanup.cursor_restored
                && (!report.cleanup.foreground_restore_captured
                    || (report.cleanup.foreground_restore_attempted
                        && report.cleanup.foreground_restored)),
            "controlled final cleanup is incomplete",
        )?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedProfile {
    mode: String,
    temporary_data_root: String,
    settings_sha256: String,
    radial_sha256: String,
    actions_sha256: String,
    configured_hotkey: String,
    hold_threshold_ms: u64,
}

// The controlled operation has no H/Q/D/S packets. Keep the full normal report
// envelope and reject unknown fields and any unowned evidence in persisted bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedReport {
    schema_version: u16,
    run_id: String,
    mode: String,
    #[serde(with = "unix_ms_wire")]
    started_unix_ms: u128,
    #[serde(with = "unix_ms_wire")]
    finished_unix_ms: u128,
    copied_profile_status: CopiedProfileStatus,
    copied_profile: Option<serde_json::Value>,
    private_artifacts: Option<private_artifacts::PrivateArtifactSummary>,
    h6_repeat_mode: H6RepeatMode,
    mouse_gesture_mode: MouseGestureMode,
    suite: AcceptanceSuite,
    hotkey: AcceptanceHotkey,
    outcome: String,
    candidate: CandidateIdentity,
    environment: EnvironmentIdentity,
    profile: PersistedProfile,
    cases: Vec<AcceptanceCaseResult>,
    hotkey_evidence: Vec<serde_json::Value>,
    query_evidence: Vec<serde_json::Value>,
    gate_c_evidence: Vec<serde_json::Value>,
    gate_d_evidence: Vec<serde_json::Value>,
    gate_s_evidence: Vec<serde_json::Value>,
    controlled_failures: Evidence,
    artifacts: Vec<String>,
    cleanup: CleanupResult,
    capacity_saturated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report_overflow: Option<serde_json::Value>,
}

impl PersistedReport {
    fn into_model(self) -> Result<AcceptanceReport, String> {
        require(
            self.mode == PROBE_MODE
                && self.schema_version == 9
                && self.outcome == "failed"
                && self.profile.mode == "deterministic_fixture"
                && self.profile.configured_hotkey == ACCEPTANCE_HOTKEY
                && self.copied_profile.is_none()
                && self.hotkey_evidence.is_empty()
                && self.query_evidence.is_empty()
                && self.gate_c_evidence.is_empty()
                && self.gate_d_evidence.is_empty()
                && self.gate_s_evidence.is_empty()
                && !self.capacity_saturated
                && self.report_overflow.is_none(),
            "persisted controlled report mode, outcome or capacity is invalid",
        )?;
        Ok(AcceptanceReport {
            schema_version: self.schema_version,
            run_id: self.run_id,
            mode: PROBE_MODE,
            started_unix_ms: self.started_unix_ms,
            finished_unix_ms: self.finished_unix_ms,
            copied_profile_status: self.copied_profile_status,
            copied_profile: None,
            private_artifacts: self.private_artifacts,
            h6_repeat_mode: self.h6_repeat_mode,
            mouse_gesture_mode: self.mouse_gesture_mode,
            suite: self.suite,
            hotkey: self.hotkey,
            outcome: "failed",
            candidate: self.candidate,
            environment: self.environment,
            profile: ProfileIdentity {
                mode: "deterministic_fixture",
                temporary_data_root: self.profile.temporary_data_root,
                settings_sha256: self.profile.settings_sha256,
                radial_sha256: self.profile.radial_sha256,
                actions_sha256: self.profile.actions_sha256,
                configured_hotkey: ACCEPTANCE_HOTKEY,
                hold_threshold_ms: self.profile.hold_threshold_ms,
            },
            cases: self.cases,
            hotkey_evidence: Vec::new(),
            query_evidence: Vec::new(),
            gate_c_evidence: Vec::new(),
            gate_d_evidence: Vec::new(),
            gate_s_evidence: Vec::new(),
            controlled_failures: Some(self.controlled_failures),
            artifacts: self.artifacts,
            cleanup: self.cleanup,
            capacity_saturated: false,
            report_overflow: None,
        })
    }
}

pub(super) struct ExpectedSubprocess<'a> {
    pub kind: ProbeKind,
    pub nonce: &'a str,
    pub pid: u32,
    pub exit_code: Option<i32>,
    pub started_unix_ms: u128,
    pub finished_unix_ms: u128,
    pub candidate: &'a CandidateIdentity,
    pub runner_sha256: &'a str,
    pub source_revision: &'a str,
    pub command: &'a [String],
    pub created_owner: &'a CreatedOwnership,
    pub leased_owner: &'a NativeOwner,
    pub observed_child_exit_code: u32,
}

fn parse_persisted(
    bytes: &[u8],
    expected: &ExpectedSubprocess<'_>,
) -> Result<AcceptanceReport, String> {
    require(
        !bytes.is_empty()
            && bytes.len() <= MAX_JSON_REPORT_BYTES
            && expected.exit_code.is_some_and(|code| code != 0),
        "inner report is missing, oversized or subprocess did not fail",
    )?;
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("parse failed report: {e}"))?;
    let wire: PersistedReport =
        serde_json::from_slice(bytes).map_err(|e| format!("parse typed failed report: {e}"))?;
    require(
        serde_json::to_value(&wire).map_err(|e| e.to_string())? == value,
        "failed report contains unknown or omitted producer fields",
    )?;
    let report = wire.into_model()?;
    require(
        expected.command.len() == 11,
        "actual self-subprocess command is missing",
    )?;
    validate_command(
        expected.command,
        &Path::new(&expected.command[8]).join("report.json"),
        expected,
    )?;
    check_created_ownership(
        expected.created_owner,
        expected.kind,
        expected.nonce,
        expected.pid,
        expected.started_unix_ms,
        expected.candidate,
        expected.source_revision,
    )?;
    require(
        report.environment.runner_process_id == expected.pid
            && report.candidate.executable == expected.candidate.executable
            && report.candidate.sha256 == expected.candidate.sha256
            && report.environment.runner_sha256.as_deref() == Some(expected.runner_sha256)
            && report.environment.source_revision.as_deref() == Some(expected.source_revision)
            && report.started_unix_ms >= expected.started_unix_ms
            && report.finished_unix_ms <= expected.finished_unix_ms
            && report.finished_unix_ms >= report.started_unix_ms,
        "failed report source, process, binary or lifetime identity differs from the actual subprocess",
    )?;
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("inner operation is not a probe".into());
    };
    require(
        receipt.kind == expected.kind && receipt.nonce == expected.nonce,
        "stale or wrong probe receipt",
    )?;
    let owner = receipt
        .owner
        .as_ref()
        .ok_or("inner stage omitted its native owner")?;
    require(
        same_native_identity(owner, expected.leased_owner)
            && owner.observed_unix_ms >= expected.leased_owner.observed_unix_ms
            && report.profile.temporary_data_root == expected.created_owner.profile_root
            && receipt.profile_before == expected.created_owner.profile_hashes
            && receipt.child_exit_code == Some(expected.observed_child_exit_code),
        "inner owner/profile/exit differs from the actual leased process",
    )?;
    require(
        report.cases.len() == 3
            && report.cases.iter().map(|c| c.id.as_str()).eq(expected
                .kind
                .inventory()
                .iter()
                .copied()),
        "failed report inventory differs from the exact probe operation",
    )?;
    let primary = &report.cases[0];
    require(
        primary.status == CaseStatus::Failed
            && primary.failure_stage == Some(expected.kind.failure_stage())
            && receipt
                .proof
                .as_ref()
                .is_some_and(|p| primary.observed == primary_message(p))
            && !primary.observed.is_empty()
            && primary.observed.len() <= MAX_RESULT_BYTES,
        "inner failure did not reach the expected actual stage",
    )?;
    require(
        report.cases.iter().all(|case| {
            !case.expected.is_empty()
                && case.expected.len() <= MAX_RESULT_BYTES
                && !case.observed.is_empty()
                && case.observed.len() <= MAX_RESULT_BYTES
                && case.elapsed_ms <= SUBPROCESS_TIMEOUT.as_millis() as u64
                && case.artifacts.len() <= MAX_ARTIFACTS
                && case
                    .artifacts
                    .iter()
                    .all(|p| !p.is_empty() && p.len() <= MAX_PATH_BYTES)
        }),
        "failed report case fields exceeded their bounds",
    )?;
    require(
        report.cases[1].status
            == (if expected.kind == ProbeKind::ChildExit {
                CaseStatus::Failed
            } else {
                CaseStatus::Passed
            })
            && report.cases[1].failure_stage
                == (expected.kind == ProbeKind::ChildExit).then_some(FailureStage::Cleanup)
            && report.cases[2].status == CaseStatus::Passed
            && report.cases[2].failure_stage.is_none(),
        "inner cleanup/R0 inventory is not truthful or valid",
    )?;
    validate_probe(&report, true)?;
    Ok(report)
}

fn attempt_fields_are_bounded(attempt: &SubprocessAttempt) -> bool {
    let bounded_error = |error: &String| !error.is_empty() && error.len() <= MAX_RESULT_BYTES;
    validate_nonce(&attempt.nonce).is_ok()
        && attempt.source_revision.as_ref().is_none_or(|source| {
            !source.is_empty()
                && source.len() <= 160
                && source.bytes().all(|b| b.is_ascii_graphic())
        })
        && attempt.runner_sha256.as_deref().is_none_or(is_hash)
        && is_hash(&attempt.candidate_sha256)
        && !attempt.candidate_executable.is_empty()
        && attempt.candidate_executable.len() <= MAX_PATH_BYTES
        && attempt.command.as_ref().is_none_or(|command| {
            command.len() == 11
                && command
                    .iter()
                    .all(|part| !part.is_empty() && part.len() <= MAX_PATH_BYTES)
        })
        && attempt.primary_error.as_ref().is_none_or(bounded_error)
        && attempt.cleanup_errors.len() <= MAX_ATTEMPT_CLEANUP_ERRORS
        && attempt.cleanup_errors.iter().all(bounded_error)
        && serde_json::to_vec(attempt).is_ok_and(|bytes| bytes.len() <= MAX_PROBE_BYTES)
}

// A completed attempt is diagnostic evidence. Only its separately verified
// proof can qualify a case, and an incomplete last attempt ends admission.
fn validate_attempt_inventory(
    report: &AcceptanceReport,
    attempts: &[SubprocessAttempt],
    probes: &[VerifiedProbe],
) -> Result<(), String> {
    require(
        attempts.len() <= ProbeKind::ALL.len() && probes.len() <= attempts.len(),
        "controlled attempt inventory exceeds its six actual subprocess owners",
    )?;
    let mut nonces = std::collections::BTreeSet::new();
    let mut process_lifetimes = std::collections::BTreeSet::new();
    let mut profiles = std::collections::BTreeSet::new();
    let mut previous_finished = report.started_unix_ms;
    for (index, attempt) in attempts.iter().enumerate() {
        require(
            attempt_fields_are_bounded(attempt)
                && attempt.kind == ProbeKind::ALL[index]
                && nonces.insert(&attempt.nonce)
                && attempt.source_revision == report.environment.source_revision
                && attempt.runner_sha256 == report.environment.runner_sha256
                && attempt.candidate_executable == report.candidate.executable
                && attempt.candidate_sha256 == report.candidate.sha256,
            "controlled attempt has stale, duplicate, foreign or unbounded source ownership",
        )?;
        for time in [attempt.started_unix_ms, attempt.finished_unix_ms]
            .into_iter()
            .flatten()
        {
            require(
                time > 0 && time <= report.finished_unix_ms,
                "controlled attempt clock lies outside its outer owner",
            )?;
        }
        if let Some(started) = attempt.started_unix_ms {
            require(
                started >= previous_finished,
                "controlled attempt starts before its preceding actual completion",
            )?;
            if let Some(finished) = attempt.finished_unix_ms {
                require(
                    finished >= started
                        && finished - started <= SUBPROCESS_TIMEOUT.as_millis() + 15_000,
                    "controlled attempt has contradictory or unbounded completion clocks",
                )?;
            }
        }
        if let Some(command) = &attempt.command {
            validate_invocation_command(
                command,
                attempt.kind,
                &attempt.nonce,
                &attempt.candidate_executable,
                attempt
                    .source_revision
                    .as_deref()
                    .ok_or("invoked controlled subprocess has no source revision")?,
            )?;
            require(
                Path::new(&command[0])
                    == std::env::current_exe().map_err(|error| error.to_string())?
                    && Path::new(&command[8]).is_absolute()
                    && Path::new(&command[8])
                        .file_name()
                        .and_then(|name| name.to_str())
                        == Some(attempt.kind.id()),
                "controlled attempt command has no actual isolated output owner",
            )?;
        }
        let (pid, observed) = match &attempt.process {
            SubprocessState::NotStarted => {
                require(
                    attempt.created_owner.is_none() && attempt.live_owner.is_none(),
                    "unstarted controlled attempt claims native application ownership",
                )?;
                (None, None)
            }
            SubprocessState::Running { process_id } => (Some(*process_id), None),
            SubprocessState::ExitObserved {
                process_id,
                observed_unix_ms,
                status,
                ..
            } => {
                let coherent_status = match (status.code, status.windows_status) {
                    (Some(code), Some(raw)) => code == raw as i32 && status.success == (raw == 0),
                    (None, None) => !status.success,
                    _ => false,
                };
                require(
                    coherent_status,
                    "controlled attempt has contradictory observed OS status",
                )?;
                if let Some(observed) = observed_unix_ms {
                    require(
                        attempt
                            .started_unix_ms
                            .is_some_and(|started| *observed >= started)
                            && *observed <= report.finished_unix_ms
                            && attempt
                                .finished_unix_ms
                                .is_none_or(|finished| *observed <= finished),
                        "controlled exit observation is outside its actual subprocess lifetime",
                    )?;
                }
                (Some(*process_id), *observed_unix_ms)
            }
        };
        if let Some(pid) = pid {
            require(
                pid > 0
                    && pid != report.environment.runner_process_id
                    && attempt.started_unix_ms.is_some()
                    && attempt.command.is_some()
                    && process_lifetimes.insert((pid, attempt.started_unix_ms)),
                "controlled attempt omitted or changed its actual started runner identity",
            )?;
        }
        if let Some(created) = &attempt.created_owner {
            check_created_ownership(
                created,
                attempt.kind,
                &attempt.nonce,
                pid.ok_or("created child has no started runner")?,
                attempt
                    .started_unix_ms
                    .ok_or("created child has no runner start observation")?,
                &report.candidate,
                attempt
                    .source_revision
                    .as_deref()
                    .ok_or("created child has no source revision")?,
            )?;
            require(
                profiles.insert(&created.profile_root)
                    && created.child_pid != report.environment.runner_process_id
                    && observed
                        .or(attempt.finished_unix_ms)
                        .is_none_or(|end| created.child_started_unix_ms <= end)
                    && created.profile_hashes.is_valid(),
                "controlled attempt child/profile identity is contradictory",
            )?;
        }
        if let Some(live) = &attempt.live_owner {
            check_live_ownership(
                live,
                attempt
                    .created_owner
                    .as_ref()
                    .ok_or("live child has no created owner")?,
            )?;
            require(
                observed
                    .or(attempt.finished_unix_ms)
                    .is_none_or(|end| live.owner.observed_unix_ms <= end),
                "controlled live application observation postdates its runner completion",
            )?;
        }
        if let Some(proof) = probes.get(index) {
            let SubprocessState::ExitObserved {
                process_id,
                observed_unix_ms: Some(observed),
                status,
                origin: SubprocessExitOrigin::RunnerCompletion,
            } = &attempt.process
            else {
                return Err(
                    "verified controlled proof has no naturally observed runner completion".into(),
                );
            };
            require(
                proof.kind == attempt.kind
                    && proof.nonce == attempt.nonce
                    && proof.subprocess_pid == *process_id
                    && status.code == Some(proof.actual_exit_code)
                    && status.windows_status.is_some()
                    && !status.success
                    && proof.actual_exit_code != 0
                    && attempt.started_unix_ms == Some(proof.started_unix_ms)
                    && *observed <= proof.finished_unix_ms
                    && attempt
                        .finished_unix_ms
                        .is_some_and(|finished| proof.finished_unix_ms <= finished)
                    && attempt.created_owner.as_ref() == Some(&proof.created_owner)
                    && attempt
                        .live_owner
                        .as_ref()
                        .is_some_and(|live| live.owner == proof.leased_owner)
                    && attempt.command.as_ref() == Some(&proof.command)
                    && attempt.primary_error.is_none()
                    && attempt.cleanup_errors.is_empty(),
                "verified controlled proof differs from its actual subprocess attempt/completion",
            )?;
        } else {
            require(
                attempt.primary_error.is_some() && index + 1 == attempts.len(),
                "incomplete controlled attempt claims success or admits a later subprocess",
            )?;
        }
        if let Some(case) = report
            .cases
            .iter()
            .find(|case| case.id == attempt.kind.id())
        {
            require(
                case.status
                    == if index < probes.len() {
                        CaseStatus::Passed
                    } else {
                        CaseStatus::Failed
                    },
                "controlled attempt diagnostic was relabeled as a qualified case",
            )?;
        }
        previous_finished = attempt.finished_unix_ms.unwrap_or(report.finished_unix_ms);
    }
    Ok(())
}

pub(super) fn validate_report_evidence(report: &AcceptanceReport) -> Result<(), String> {
    match report.controlled_failures.as_ref() {
        None if report.mode != PROBE_MODE && report.mode != AGGREGATE_MODE => Ok(()),
        Some(Evidence::Probe { receipt }) if report.mode == PROBE_MODE => {
            validate_probe(report, true)?;
            verify_private_bundle(report, receipt.kind)
        }
        Some(Evidence::Aggregate { probes, attempts }) if report.mode == AGGREGATE_MODE => {
            validate_attempt_inventory(report, attempts, probes)?;
            require(
                metadata_is_bounded(report)
                    && report.suite == AcceptanceSuite::All
                    && report.profile.mode == "controlled_failure_aggregate"
                    && report.environment.child_process_id.is_none()
                    && report.environment.child_started_unix_ms.is_none()
                    && report.copied_profile.is_none()
                    && matches!(report.copied_profile_status, CopiedProfileStatus::NotRun)
                    && !report.cleanup.child_closed_normally
                    && !report.cleanup.child_terminated_after_timeout
                    && report.cleanup.profile_removed
                    && report.cleanup.child_owned_windows_closed
                    && report.cleanup.cursor_restored
                    && report.cleanup.input_desktop_released
                    && !report.cleanup.foreground_restore_captured
                    && !report.cleanup.foreground_restore_attempted
                    && !report.cleanup.foreground_restored
                    && probes.len() == ProbeKind::ALL.len()
                    && attempts.len() == ProbeKind::ALL.len(),
                "controlled aggregate metadata or inventory is invalid",
            )?;
            let mut nonces = std::collections::BTreeSet::new();
            let mut pids = std::collections::BTreeSet::new();
            let mut profiles = std::collections::BTreeSet::new();
            let mut previous_finished = report.started_unix_ms;
            for (proof, kind) in probes.iter().zip(ProbeKind::ALL) {
                validate_nonce(&proof.nonce)?;
                require(
                    proof.kind == kind
                        && proof.receipt.kind == kind
                        && proof.receipt.nonce == proof.nonce
                        && proof.subprocess_pid > 0
                        && proof.subprocess_pid != report.environment.runner_process_id
                        && proof.actual_exit_code != 0
                        && nonces.insert(&proof.nonce)
                        && pids.insert((proof.subprocess_pid, proof.started_unix_ms))
                        && profiles.insert(&proof.created_owner.profile_root)
                        && proof.started_unix_ms >= previous_finished
                        && proof.finished_unix_ms >= proof.started_unix_ms
                        && proof.finished_unix_ms <= report.finished_unix_ms
                        && proof.finished_unix_ms - proof.started_unix_ms
                            <= SUBPROCESS_TIMEOUT.as_millis() + 15_000
                        && is_hash(&proof.report_sha256)
                        && is_hash(&proof.text_sha256)
                        && proof.report_path.len() <= MAX_PATH_BYTES,
                    "controlled aggregate contains stale, duplicate or unbounded subprocess ownership",
                )?;
                let expected = ExpectedSubprocess {
                    kind,
                    nonce: &proof.nonce,
                    pid: proof.subprocess_pid,
                    exit_code: Some(proof.actual_exit_code),
                    started_unix_ms: proof.started_unix_ms,
                    finished_unix_ms: proof.finished_unix_ms,
                    candidate: &report.candidate,
                    runner_sha256: report
                        .environment
                        .runner_sha256
                        .as_deref()
                        .ok_or("runner hash missing")?,
                    source_revision: report
                        .environment
                        .source_revision
                        .as_deref()
                        .ok_or("source revision missing")?,
                    command: &proof.command,
                    created_owner: &proof.created_owner,
                    leased_owner: &proof.leased_owner,
                    observed_child_exit_code: proof.observed_child_exit_code,
                };
                let actual = verify_report_pair(Path::new(&proof.report_path), &expected)?;
                require(
                    serde_json::to_value(&actual).map_err(|e| e.to_string())?
                        == serde_json::to_value(proof).map_err(|e| e.to_string())?,
                    "controlled aggregate no longer matches its actual persisted failed report/readbacks",
                )?;
                previous_finished = proof.finished_unix_ms;
            }
            Ok(())
        }
        _ => Err("controlled evidence is inconsistent with the report operation".into()),
    }
}

#[cfg(windows)]
#[derive(Debug, PartialEq, Eq)]
struct ExactSettingsDecimal {
    negative: bool,
    digits: Vec<u8>,
    exponent: i64,
}

#[cfg(windows)]
impl ExactSettingsDecimal {
    fn parse(token: &str) -> Result<Self, String> {
        let bytes = token.as_bytes();
        require(
            !bytes.is_empty() && bytes.len() <= MAX_JSON_REPORT_BYTES,
            "protected JSON number exceeds its raw bound",
        )?;
        let negative = bytes[0] == b'-';
        let mut at = usize::from(negative);
        let mut digits = Vec::with_capacity(bytes.len());
        match bytes.get(at) {
            Some(b'0') => {
                digits.push(b'0');
                at += 1;
            }
            Some(b'1'..=b'9') => {
                while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                    digits.push(bytes[at]);
                    at += 1;
                }
            }
            _ => return Err("invalid protected JSON number grammar".into()),
        }
        let mut fractional_digits = 0;
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                digits.push(bytes[at]);
                at += 1;
            }
            require(at > start, "invalid protected JSON number grammar")?;
            fractional_digits = i64::try_from(at - start)
                .map_err(|_| "unsupported protected JSON decimal exponent")?;
        }
        let mut exponent = 0_i64;
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            let exponent_negative = bytes.get(at) == Some(&b'-');
            if exponent_negative || bytes.get(at) == Some(&b'+') {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                exponent = exponent
                    .checked_mul(10)
                    .and_then(|value| value.checked_add(i64::from(bytes[at] - b'0')))
                    .ok_or("unsupported protected JSON decimal exponent")?;
                at += 1;
            }
            require(at > start, "invalid protected JSON number grammar")?;
            if exponent_negative {
                exponent = -exponent;
            }
        }
        require(at == bytes.len(), "invalid protected JSON number grammar")?;
        exponent = exponent
            .checked_sub(fractional_digits)
            .ok_or("unsupported protected JSON decimal exponent")?;
        let Some(first) = digits.iter().position(|digit| *digit != b'0') else {
            return Ok(Self {
                negative,
                digits: vec![b'0'],
                exponent: 0,
            });
        };
        let trailing = digits
            .iter()
            .rev()
            .take_while(|digit| **digit == b'0')
            .count();
        exponent = exponent
            .checked_add(
                i64::try_from(trailing)
                    .map_err(|_| "unsupported protected JSON decimal exponent")?,
            )
            .ok_or("unsupported protected JSON decimal exponent")?;
        // Keep significant digits, not expanded powers of ten. This compares
        // exact decimal values within the existing input bound and retains -0.
        let last = digits.len() - trailing;
        digits.copy_within(first..last, 0);
        digits.truncate(last - first);
        Ok(Self {
            negative,
            digits,
            exponent,
        })
    }
}

#[cfg(windows)]
fn canonical_settings(bytes: &[u8]) -> Result<serde_json::Value, String> {
    use serde::de::DeserializeSeed;

    require(
        !bytes.is_empty() && bytes.len() <= MAX_JSON_REPORT_BYTES,
        "controlled settings exceed their raw JSON bound",
    )?;
    // The locked JSON parser can truncate overflowing integers/long decimals
    // before a visitor sees them. Admit exact integers and normal writer float
    // tokens only, using std's correctly rounded float parser and requiring its
    // serialized token to retain the exact decimal value. f32/f64 writers may
    // choose different notation; neither can discard significant digits here.
    let mut shape = bytes.to_vec();
    let mut numbers = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                at += 1;
                while at < bytes.len() {
                    match bytes[at] {
                        b'\\' => at += 2,
                        b'"' => {
                            at += 1;
                            break;
                        }
                        _ => at += 1,
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = at;
                at += 1;
                while at < bytes.len()
                    && matches!(bytes[at], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    at += 1;
                }
                let token = std::str::from_utf8(&bytes[start..at]).map_err(|e| e.to_string())?;
                let exact = ExactSettingsDecimal::parse(token)?;
                let number = if token.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
                    let value = token
                        .parse::<f64>()
                        .map_err(|_| "unsupported protected JSON float")?;
                    serde_json::Number::from_f64(value).ok_or("non-finite protected JSON float")?
                } else if token.starts_with('-') {
                    token
                        .parse::<i64>()
                        .map(serde_json::Number::from)
                        .map_err(|_| "unsupported protected JSON integer precision")?
                } else {
                    token
                        .parse::<u64>()
                        .map(serde_json::Number::from)
                        .map_err(|_| "unsupported protected JSON integer precision")?
                };
                require(
                    exact == ExactSettingsDecimal::parse(&number.to_string())?,
                    "unsupported or lossy protected JSON number representation",
                )?;
                numbers.push(number);
                // Parse the original structural bytes/strings with safe zero
                // placeholders, then consume the checked numbers in that same
                // visitation order. The JSON parser never owns numeric rounding.
                shape[start] = b'0';
                shape[start + 1..at].fill(b' ');
            }
            _ => at += 1,
        }
    }
    let mut numbers = numbers.into_iter();
    // Value's normal parser silently replaces duplicate keys. A protected-data
    // proof must retain every producer key, including unknown keys, or refuse it.
    struct UniqueValue(serde_json::Value);
    struct UniqueValueSeed<'a> {
        numbers: &'a mut std::vec::IntoIter<serde_json::Number>,
    }
    impl<'de> DeserializeSeed<'de> for UniqueValueSeed<'_> {
        type Value = UniqueValue;

        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            struct Visitor<'a> {
                numbers: &'a mut std::vec::IntoIter<serde_json::Number>,
            }
            impl Visitor<'_> {
                fn number<E: serde::de::Error>(self) -> Result<UniqueValue, E> {
                    self.numbers
                        .next()
                        .map(|number| UniqueValue(number.into()))
                        .ok_or_else(|| E::custom("JSON numeric owner has no matching raw token"))
                }
            }
            impl<'de> serde::de::Visitor<'de> for Visitor<'_> {
                type Value = UniqueValue;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("JSON with unique object keys")
                }
                fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                    Ok(UniqueValue(serde_json::Value::Null))
                }
                fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                    Ok(UniqueValue(value.into()))
                }
                fn visit_i64<E: serde::de::Error>(self, _value: i64) -> Result<Self::Value, E> {
                    self.number()
                }
                fn visit_u64<E: serde::de::Error>(self, _value: u64) -> Result<Self::Value, E> {
                    self.number()
                }
                fn visit_f64<E: serde::de::Error>(self, _value: f64) -> Result<Self::Value, E> {
                    self.number()
                }
                fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                    Ok(UniqueValue(value.into()))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut values = Vec::new();
                    while let Some(UniqueValue(value)) = seq.next_element_seed(UniqueValueSeed {
                        numbers: &mut *self.numbers,
                    })? {
                        values.push(value);
                    }
                    Ok(UniqueValue(serde_json::Value::Array(values)))
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut values = std::collections::BTreeMap::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if values.contains_key(&key) {
                            return Err(serde::de::Error::custom(
                                "duplicate protected settings key",
                            ));
                        }
                        let UniqueValue(value) = map.next_value_seed(UniqueValueSeed {
                            numbers: &mut *self.numbers,
                        })?;
                        values.insert(key, value);
                    }
                    Ok(UniqueValue(serde_json::Value::Object(
                        values.into_iter().collect(),
                    )))
                }
            }
            deserializer.deserialize_any(Visitor {
                numbers: self.numbers,
            })
        }
    }
    let mut parser = serde_json::Deserializer::from_slice(&shape);
    let UniqueValue(mut value) = UniqueValueSeed {
        numbers: &mut numbers,
    }
    .deserialize(&mut parser)
    .map_err(|e| e.to_string())?;
    parser.end().map_err(|e| e.to_string())?;
    require(
        numbers.next().is_none(),
        "JSON numeric owner has an unmatched raw token",
    )?;
    // Validate the actual settings schema without reserializing it: typed
    // defaulting or ignored unknown fields cannot erase protected raw values.
    let _: Settings = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    if let Some(serde_json::Value::Array(plugins)) = value.get_mut("enabled_plugins") {
        let mut names = std::collections::BTreeSet::new();
        for plugin in plugins.iter() {
            let name = plugin
                .as_str()
                .ok_or("enabled_plugins contains a non-string")?;
            require(
                names.insert(name.to_owned()),
                "enabled_plugins contains a duplicate member",
            )?;
        }
        *plugins = names.into_iter().map(serde_json::Value::String).collect();
    }
    Ok(value)
}

#[cfg(windows)]
pub(super) fn read_profile_hashes(path: &Path) -> Result<ProfileHashes, String> {
    let settings_bytes = read_regular_bounded(&path.join("settings.json"), MAX_JSON_REPORT_BYTES)?;
    let mut canonical = canonical_settings(&settings_bytes)?;
    let settings_full = sha256_bytes(&serde_json::to_vec(&canonical).map_err(|e| e.to_string())?);
    // Normal Designer entry owns exactly these three persisted measurements.
    // The full identity still protects them for every non-UI probe.
    if let Some(serde_json::Value::Object(designer)) = canonical.get_mut("radial_designer") {
        for key in ["window_position", "window_scale_factor", "window_size"] {
            if let Some(value) = designer.get_mut(key) {
                // Exclude only each measured value, retaining key presence so
                // a removed/defaulted field cannot masquerade as UI geometry.
                *value = serde_json::Value::Null;
            }
        }
    }
    Ok(ProfileHashes {
        settings: sha256_bytes(&settings_bytes),
        settings_full,
        radial: sha256_file(&path.join("radial.json")).map_err(|e| e.to_string())?,
        actions: sha256_file(&path.join("actions.json")).map_err(|e| e.to_string())?,
        settings_without_designer_geometry: sha256_bytes(
            &serde_json::to_vec(&canonical).map_err(|e| e.to_string())?,
        ),
    })
}

fn profile_path_is_owned(value: &str) -> bool {
    let path = Path::new(value);
    path.is_absolute()
        && value.len() <= MAX_PATH_BYTES
        && path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("multi-launcher-radial-acceptance-"))
        && path.parent().and_then(|p| p.canonicalize().ok())
            == std::env::temp_dir().canonicalize().ok()
}

fn check_created_ownership(
    created: &CreatedOwnership,
    kind: ProbeKind,
    nonce: &str,
    pid: u32,
    started_unix_ms: u128,
    candidate: &CandidateIdentity,
    source_revision: &str,
) -> Result<(), String> {
    require(
        created.kind == kind
            && created.nonce == nonce
            && created.runner_pid == pid
            && created.source_revision == source_revision
            && created.candidate_sha256 == candidate.sha256
            && created.child_pid > 0
            && created.child_pid != pid
            && created.child_started_unix_ms >= started_unix_ms
            && created.process_created_filetime > 0
            && created.profile_hashes.is_valid()
            && profile_path_is_owned(&created.profile_root),
        "created process ownership is stale, foreign or outside the isolated subprocess",
    )
}

fn check_live_ownership(live: &LiveOwnership, created: &CreatedOwnership) -> Result<(), String> {
    require(
        live.kind == created.kind
            && live.nonce == created.nonce
            && live.runner_pid == created.runner_pid
            && live.source_revision == created.source_revision
            && live.candidate_sha256 == created.candidate_sha256
            && live.profile_root == created.profile_root
            && live.owner.child_pid == created.child_pid
            && live.owner.child_started_unix_ms == created.child_started_unix_ms
            && live.owner.process_created_filetime == created.process_created_filetime
            && live.owner.observed_unix_ms >= created.child_started_unix_ms
            && live.owner.root_hwnd > 0
            && live.owner.foreground_hwnd == live.owner.root_hwnd
            && live.owner.foreground_pid == live.owner.child_pid
            && live.owner.foreground_thread_id > 0
            && live.owner.input_desktop == "thread=Default;active=Default"
            && live.profile_root.len() <= MAX_PATH_BYTES,
        "live probe ownership is stale, foreign or outside the actual subprocess",
    )
}

fn verify_private_trace_boundary(trace: &str) -> Result<(), String> {
    let has_event = |event| {
        trace
            .lines()
            .any(|line| trace_field_value(line, "trace_event") == Some(event))
    };
    require(
        has_event("trace_ready") && !has_event("budget_exhausted"),
        "private native trace has no complete producer boundary",
    )
}

fn verify_private_bundle(report: &AcceptanceReport, kind: ProbeKind) -> Result<(), String> {
    let private = report
        .private_artifacts
        .as_ref()
        .ok_or("controlled failure omitted private diagnostic evidence")?;
    let id = private
        .artifact_id
        .as_deref()
        .ok_or("controlled private bundle omitted its identity")?;
    let root = std::env::temp_dir().join(id);
    private_artifacts::verify_retained_artifacts(&root, private)?;
    let window_bytes = read_regular_bounded(&root.join("case-R1-windows.json"), 64 * 1024)?;
    let windows: serde_json::Value =
        serde_json::from_slice(&window_bytes).map_err(|e| e.to_string())?;
    let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
        return Err("probe missing".into());
    };
    let owner = receipt.owner.as_ref().ok_or("probe native owner missing")?;
    let entries = windows["windows"]
        .as_array()
        .ok_or("private HWND inventory omitted its owned windows")?;
    require(
        windows["child_process_id"].as_u64() == Some(u64::from(owner.child_pid))
            && !entries.is_empty()
            && windows["runner_process_id"].as_u64()
                == Some(u64::from(report.environment.runner_process_id))
            && entries.len() <= 256
            && entries
                .iter()
                .all(|e| e["process_id"].as_u64() == Some(u64::from(owner.child_pid)))
            && entries
                .iter()
                .any(|e| e["hwnd"].as_u64() == Some(owner.root_hwnd)),
        "private HWND inventory does not match the actual probe owner",
    )?;
    let trace = read_regular_bounded(&root.join("case-R1-trace.log"), 128 * 1024)?;
    let trace = std::str::from_utf8(&trace).map_err(|e| e.to_string())?;
    verify_private_trace_boundary(trace)?;
    if matches!(kind, ProbeKind::Query | ProbeKind::Ui) {
        let stage_trace = read_regular_bounded(
            &root.join(format!("case-{}-stage-trace.log", kind.id())),
            128 * 1024,
        )?;
        verify_stage_trace(
            std::str::from_utf8(&stage_trace).map_err(|e| e.to_string())?,
            receipt,
            entries,
        )?;
    }
    read_regular_bounded(&root.join("case-R1-private.log"), 64 * 1024)?;
    let image_path = root.join("case-R1.png");
    let (width, height) = image::image_dimensions(image_path).map_err(|e| e.to_string())?;
    require(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 16 * 1024 * 1024,
        "private child-owned screenshot has invalid bounds",
    )
}

pub(super) fn persistence_cap_failure(report: &AcceptanceReport) -> Option<String> {
    let evidence = report.controlled_failures.as_ref()?;
    let too_many = matches!(evidence, Evidence::Aggregate { probes, attempts } if probes.len() > ProbeKind::ALL.len() || attempts.len() > ProbeKind::ALL.len());
    let oversized_receipt = match evidence {
        Evidence::Probe { receipt } => {
            serde_json::to_vec(receipt).map_or(true, |v| v.len() > MAX_PROBE_BYTES)
        }
        Evidence::Aggregate { probes, attempts } => {
            probes
                .iter()
                .any(|p| serde_json::to_vec(&p.receipt).map_or(true, |v| v.len() > MAX_PROBE_BYTES))
                || attempts
                    .iter()
                    .any(|attempt| !attempt_fields_are_bounded(attempt))
        }
    };
    if too_many
        || oversized_receipt
        || serde_json::to_vec(evidence).map_or(true, |v| v.len() > MAX_CONTROLLED_BYTES)
    {
        Some("controlled native proof exceeded its fixed six-probe/receipt/aggregate bound".into())
    } else {
        None
    }
}

pub(super) fn fail_evidence_for_persistence(report: &mut AcceptanceReport, reason: &str) {
    let (omitted, ids) = match report.controlled_failures.take() {
        Some(Evidence::Probe { receipt }) => (1, vec![receipt.kind.id().to_owned()]),
        Some(Evidence::Aggregate { probes, attempts }) => (
            probes.len() + attempts.len(),
            ProbeKind::ALL.iter().map(|k| k.id().to_owned()).collect(),
        ),
        None => (0, Vec::new()),
    };
    let omitted_artifacts = report.artifacts.len()
        + report
            .cases
            .iter()
            .map(|c| c.artifacts.len())
            .sum::<usize>();
    report.artifacts.clear();
    for case in &mut report.cases {
        case.artifacts.clear();
    }
    for id in ids.iter().map(String::as_str).chain(["CLEANUP", "R0"]) {
        mark_persistence_overflow_case(report, id);
    }
    report.report_overflow = Some(ReportOverflowReceipt {
        reason: bounded_text(reason, MAX_RESULT_BYTES),
        omitted_case_evidence: omitted,
        omitted_artifact_references: omitted_artifacts,
        affected_case_ids: ids,
    });
    report.mark_capacity_saturated();
    report.outcome = "failed";
}

fn verify_stage_trace(
    trace: &str,
    receipt: &ProbeReceipt,
    windows: &[serde_json::Value],
) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::verify_controlled_stage_trace(trace, receipt, windows)
    }
    #[cfg(not(windows))]
    {
        let _ = (trace, receipt, windows);
        Err("native controlled stage trace requires Windows".into())
    }
}

fn verify_report_pair(
    path: &Path,
    expected: &ExpectedSubprocess<'_>,
) -> Result<VerifiedProbe, String> {
    validate_command(expected.command, path, expected)?;
    check_created_ownership(
        expected.created_owner,
        expected.kind,
        expected.nonce,
        expected.pid,
        expected.started_unix_ms,
        expected.candidate,
        expected.source_revision,
    )?;
    require(
        sha256_file(Path::new(&expected.command[0])).map_err(|e| e.to_string())?
            == expected.runner_sha256
            && sha256_file(Path::new(&expected.candidate.executable)).map_err(|e| e.to_string())?
                == expected.candidate.sha256,
        "controlled source-matched binary changed before persisted failure readback",
    )?;
    let directory = path.parent().ok_or("probe report has no owned directory")?;
    let created: CreatedOwnership = serde_json::from_slice(&read_regular_bounded(
        &directory.join(format!("case-{}-created.json", expected.kind.id())),
        8 * 1024,
    )?)
    .map_err(|e| e.to_string())?;
    require(
        &created == expected.created_owner,
        "persisted child-creation receipt differs from the actual leased process",
    )?;
    let live: LiveOwnership = serde_json::from_slice(&read_regular_bounded(
        &directory.join(format!("case-{}-owner.json", expected.kind.id())),
        8 * 1024,
    )?)
    .map_err(|e| e.to_string())?;
    check_live_ownership(&live, &created)?;
    require(
        &live.owner == expected.leased_owner,
        "persisted native owner differs from the actual process lease acknowledgement",
    )?;
    let bytes = read_regular_bounded(path, MAX_JSON_REPORT_BYTES)?;
    let report = parse_persisted(&bytes, expected)?;
    let text = read_regular_bounded(&path.with_extension("txt"), MAX_TEXT_REPORT_BYTES)?;
    require(
        text == render_text_report(&report).as_bytes(),
        "failed text report differs from the persisted typed JSON report",
    )?;
    let profile = Path::new(&report.profile.temporary_data_root);
    require(
        !profile.exists() && profile_path_is_owned(&report.profile.temporary_data_root),
        "reported isolated profile was not actually disposed inside the temporary owner",
    )?;
    verify_private_bundle(&report, expected.kind)?;
    let private = report
        .private_artifacts
        .clone()
        .ok_or("private bundle missing")?;
    let Some(Evidence::Probe { receipt }) = report.controlled_failures else {
        return Err("probe receipt missing".into());
    };
    if let Some(StageProof::InputTimeout { key }) = &receipt.proof {
        let down: OwnedKeyLedger = serde_json::from_slice(&read_regular_bounded(
            &directory.join("case-N02-key-down.json"),
            4096,
        )?)
        .map_err(|e| e.to_string())?;
        let up: OwnedKeyLedger = serde_json::from_slice(&read_regular_bounded(
            &directory.join("case-N02-key-up.json"),
            4096,
        )?)
        .map_err(|e| e.to_string())?;
        require(
            down.nonce == receipt.nonce
                && up.nonce == receipt.nonce
                && down.edge == key.down
                && Some(&up.edge) == key.up.as_ref(),
            "persisted actual inserted-down/released-up ownership differs from the timeout proof",
        )?;
    }
    Ok(VerifiedProbe {
        kind: expected.kind,
        nonce: expected.nonce.into(),
        subprocess_pid: expected.pid,
        actual_exit_code: expected
            .exit_code
            .ok_or("actual process exit code missing")?,
        started_unix_ms: expected.started_unix_ms,
        finished_unix_ms: expected.finished_unix_ms,
        report_path: path.to_string_lossy().into_owned(),
        report_sha256: sha256_bytes(&bytes),
        text_sha256: sha256_bytes(&text),
        receipt: *receipt,
        cleanup: report.cleanup,
        private_artifacts: private,
        command: expected.command.to_vec(),
        created_owner: expected.created_owner.clone(),
        leased_owner: expected.leased_owner.clone(),
        observed_child_exit_code: expected.observed_child_exit_code,
    })
}

fn validate_command(
    command: &[String],
    path: &Path,
    expected: &ExpectedSubprocess<'_>,
) -> Result<(), String> {
    validate_invocation_command(
        command,
        expected.kind,
        expected.nonce,
        &expected.candidate.executable,
        expected.source_revision,
    )?;
    require(
        Path::new(&command[8]).join("report.json") == path,
        "controlled receipt changed the actual self-subprocess output owner",
    )
}

fn validate_invocation_command(
    command: &[String],
    kind: ProbeKind,
    nonce: &str,
    candidate_executable: &str,
    source_revision: &str,
) -> Result<(), String> {
    require(
        command.len() == 11
            && command
                .iter()
                .all(|s| !s.is_empty() && s.len() <= MAX_PATH_BYTES)
            && command[1] == "--controlled-native-failure-probe"
            && command[2] == kind.argument()
            && command[3] == "--controlled-probe-nonce"
            && command[4] == nonce
            && command[5] == "--launcher"
            && command[6] == candidate_executable
            && command[7] == "--output"
            && command[9] == "--source-revision"
            && command[10] == source_revision,
        "controlled receipt omitted or changed the actual bounded self-subprocess command",
    )
}

#[cfg(windows)]
fn run_subprocess(kind: ProbeKind, output: &Path, outer: &AcceptanceReport) -> SubprocessRun {
    let started_unix_ms = observed_utc_now();
    let nonce = format!(
        "{}-{}-{}",
        kind.argument(),
        std::process::id(),
        started_unix_ms.map_or_else(|| "unavailable-clock".into(), |time| time.to_string())
    );
    let mut attempt = SubprocessAttempt {
        kind,
        nonce,
        source_revision: outer.environment.source_revision.clone(),
        runner_sha256: outer.environment.runner_sha256.clone(),
        candidate_executable: outer.candidate.executable.clone(),
        candidate_sha256: outer.candidate.sha256.clone(),
        command: None,
        started_unix_ms,
        finished_unix_ms: None,
        process: SubprocessState::NotStarted,
        created_owner: None,
        live_owner: None,
        primary_error: None,
        cleanup_errors: Vec::new(),
    };
    let result = run_subprocess_attempt(kind, output, outer, &mut attempt);
    finish_subprocess_run(attempt, result, observed_utc_now())
}

fn observed_utc_now() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|time| time.as_millis())
}

fn finish_subprocess_run(
    mut attempt: SubprocessAttempt,
    mut result: Result<VerifiedProbe, String>,
    finished_unix_ms: Option<u128>,
) -> SubprocessRun {
    attempt.finished_unix_ms = finished_unix_ms;
    if attempt.finished_unix_ms.is_none() {
        attempt
            .cleanup_errors
            .push("outer completion UTC observation unavailable".into());
        if result.is_ok() {
            result = Err("outer completion UTC observation unavailable".into());
        }
    }
    attempt.primary_error = result
        .as_ref()
        .err()
        .map(|error| bounded_text(error, MAX_RESULT_BYTES));
    SubprocessRun { attempt, result }
}

fn retain_subprocess_run(
    report: &mut AcceptanceReport,
    run: SubprocessRun,
) -> Result<String, String> {
    let Some(Evidence::Aggregate { probes, attempts }) = &mut report.controlled_failures else {
        return Err("controlled aggregate omitted its actual subprocess evidence owner".into());
    };
    attempts.push(run.attempt);
    match run.result {
        Ok(proof) => {
            let summary = format!(
                "actual isolated subprocess PID {} exited {}; failed native {:?} report read back with exact source/binary/profile/stage/cleanup proof",
                proof.subprocess_pid, proof.actual_exit_code, proof.kind
            );
            probes.push(proof);
            Ok(summary)
        }
        Err(primary) => Err(primary),
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnleasedRunnerRetirement {
    AwaitInnerCleanup,
    AfterHeldChildCleanup,
}

// The inner runner still owns NativeChild before lease admission. Its owner
// may be forcibly retired only after a separately held, identity-checked child
// has actually been disposed. Otherwise let its bounded no-ACK/launch cleanup
// finish, keeping that owner alive if cleanup cannot be observed in time.
#[cfg(windows)]
fn cleanup_unleased_runner<C>(
    child_cleanup: Option<&C>,
    retire_child: impl FnOnce(&C) -> Result<(), String>,
    retire_runner: impl FnOnce(UnleasedRunnerRetirement) -> Result<ObservedSubprocessExit, String>,
) -> (
    Result<ObservedSubprocessExit, String>,
    Option<Result<(), String>>,
) {
    let application = child_cleanup.map(retire_child);
    let action = if matches!(&application, Some(Ok(()))) {
        UnleasedRunnerRetirement::AfterHeldChildCleanup
    } else {
        UnleasedRunnerRetirement::AwaitInnerCleanup
    };
    (retire_runner(action), application)
}

#[cfg(windows)]
fn await_inner_runner_cleanup(
    expected_pid: u32,
    known_created_child: bool,
    mut inspect: impl FnMut() -> Result<Option<ObservedSubprocessExit>, String>,
    mut deadline_reached: impl FnMut() -> bool,
    mut poll: impl FnMut(),
) -> Result<ObservedSubprocessExit, String> {
    loop {
        if let Some(exit) = inspect()? {
            require(
                exit.process_id == expected_pid && expected_pid != 0,
                "unleased cleanup observed a foreign runner exit",
            )?;
            return Ok(exit);
        }
        if deadline_reached() {
            return Err(if known_created_child {
                "unleased runner cleanup deadline expired; known created child's inner NativeChild owner retained"
            } else {
                "unleased runner cleanup deadline expired before validated creation; possible inner child owner retained"
            }.into());
        }
        poll();
    }
}

#[cfg(windows)]
fn run_subprocess_attempt(
    kind: ProbeKind,
    output: &Path,
    outer: &AcceptanceReport,
    attempt: &mut SubprocessAttempt,
) -> Result<VerifiedProbe, String> {
    use std::os::windows::process::CommandExt;
    let started_unix_ms = attempt
        .started_unix_ms
        .ok_or("outer attempt UTC observation unavailable")?;
    let nonce = attempt.nonce.clone();
    validate_nonce(&nonce)?;
    let probe_output = output.join(kind.id());
    let report_path = probe_output.join("report.json");
    let runner = std::env::current_exe().map_err(|e| e.to_string())?;
    let stdout_path = output.join(format!("case-{}-subprocess.log", kind.id()));
    let stderr_path = output.join(format!("case-{}-subprocess-error.log", kind.id()));
    let stdout = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout_path)
        .map_err(|e| e.to_string())?;
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
        .map_err(|e| e.to_string())?;
    let source_revision = outer
        .environment
        .source_revision
        .as_deref()
        .ok_or("source revision missing")?;
    let runner_sha256 = outer
        .environment
        .runner_sha256
        .as_deref()
        .ok_or("runner hash missing")?;
    let command = vec![
        runner.to_string_lossy().into_owned(),
        "--controlled-native-failure-probe".into(),
        kind.argument().into(),
        "--controlled-probe-nonce".into(),
        nonce.clone(),
        "--launcher".into(),
        outer.candidate.executable.clone(),
        "--output".into(),
        probe_output.to_string_lossy().into_owned(),
        "--source-revision".into(),
        source_revision.into(),
    ];
    attempt.command = Some(command.clone());
    let mut process = Command::new(&runner)
        .args(&command[1..])
        .creation_flags(0x0800_0000)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| e.to_string())?;
    let pid = process.id();
    attempt.process = SubprocessState::Running { process_id: pid };
    let deadline = Instant::now() + SUBPROCESS_TIMEOUT;
    let mut lease = None;
    let mut rejected_cleanup = None;
    let mut created = None;
    let mut live = None;
    let result = (|| -> Result<ObservedSubprocessExit, String> {
        loop {
            let created_path = probe_output.join(format!("case-{}-created.json", kind.id()));
            if created.is_none() && created_path.exists() {
                let ownership: CreatedOwnership =
                    serde_json::from_slice(&read_regular_bounded(&created_path, 8 * 1024)?)
                        .map_err(|e| e.to_string())?;
                check_created_ownership(
                    &ownership,
                    kind,
                    &nonce,
                    pid,
                    started_unix_ms,
                    &outer.candidate,
                    source_revision,
                )?;
                created = Some(ownership.clone());
                // Keep cleanup ownership even if this real child's measured
                // image hash fails admission. A birth/path mismatch yields no
                // safe outer handle, so the inner NativeChild owner stays alive.
                match native::OwnedProbeProcess::acquire(&ownership, &outer.candidate) {
                    Ok(acquired) => {
                        attempt.created_owner = Some(ownership);
                        lease = Some(acquired);
                    }
                    Err(rejected) => {
                        if rejected.cleanup.is_some() {
                            attempt.created_owner = Some(ownership);
                        }
                        rejected_cleanup = rejected.cleanup;
                        return Err(rejected.error);
                    }
                }
            }
            let owner_path = probe_output.join(format!("case-{}-owner.json", kind.id()));
            if live.is_none() && owner_path.exists() {
                let ownership: LiveOwnership =
                    serde_json::from_slice(&read_regular_bounded(&owner_path, 8 * 1024)?)
                        .map_err(|e| e.to_string())?;
                check_live_ownership(
                    &ownership,
                    created
                        .as_ref()
                        .ok_or("native owner arrived without created-process ownership")?,
                )?;
                require(
                    lease.is_some(),
                    "native owner has no acquired actual application handle",
                )?;
                attempt.live_owner = Some(ownership.clone());
                let ack = ProcessLeaseAck {
                    nonce: nonce.clone(),
                    subprocess_pid: pid,
                    child_pid: ownership.owner.child_pid,
                    process_created_filetime: ownership.owner.process_created_filetime,
                };
                publish_new(
                    &probe_output.join(format!("case-{}-lease-ack.json", kind.id())),
                    &serde_json::to_vec(&ack).map_err(|e| e.to_string())?,
                )?;
                live = Some(ownership);
            }
            for path in [&stdout_path, &stderr_path] {
                if fs::metadata(path).map_err(|e| e.to_string())?.len() > 64 * 1024 {
                    return Err("controlled subprocess diagnostic stream exceeded its bound".into());
                }
            }
            if let Some(status) = process.try_wait().map_err(|e| e.to_string())? {
                return Ok(ObservedSubprocessExit::observed(
                    pid,
                    status,
                    SubprocessExitOrigin::RunnerCompletion,
                ));
            }
            if Instant::now() >= deadline {
                return Err("controlled subprocess exceeded its bounded runtime".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    })();
    let exit = match result {
        Ok(exit) => exit,
        Err(primary) => {
            // Retire the runner before emergency key recovery. Before lease
            // admission, first preserve/dispose its child's cleanup owner.
            // Unobserved termination leaves key ownership with the inner runner.
            let (runner_cleanup, prelease_app_cleanup) = if lease.is_some() {
                (terminate_subprocess(&mut process), None)
            } else {
                cleanup_unleased_runner(
                    rejected_cleanup.as_ref(),
                    |cleanup| cleanup.terminate_owned(),
                    |action| match action {
                        UnleasedRunnerRetirement::AfterHeldChildCleanup => {
                            terminate_subprocess(&mut process)
                        }
                        UnleasedRunnerRetirement::AwaitInnerCleanup => await_inner_runner_cleanup(
                            pid,
                            created.is_some(),
                            || {
                                process.try_wait().map_err(|error| error.to_string()).map(
                                    |status| {
                                        status.map(|status| {
                                            ObservedSubprocessExit::observed(
                                                pid,
                                                status,
                                                SubprocessExitOrigin::ObservedBeforeTermination,
                                            )
                                        })
                                    },
                                )
                            },
                            || Instant::now() >= deadline,
                            || std::thread::sleep(Duration::from_millis(20)),
                        ),
                    },
                )
            };
            if let Ok(exit) = &runner_cleanup {
                if let Err(error) = record_observed_exit(attempt, exit) {
                    attempt.cleanup_errors.push(bounded_text(
                        &format!("runner exit receipt: {error}"),
                        MAX_RESULT_BYTES,
                    ));
                }
            }
            if let Err(error) = &runner_cleanup {
                attempt.cleanup_errors.push(bounded_text(
                    &format!("runner cleanup: {error}"),
                    MAX_RESULT_BYTES,
                ));
            }
            let recovery = recover_after_observed_runner_exit(pid, &runner_cleanup, || {
                recover_subprocess_input(&probe_output, &nonce, lease.as_ref(), live.as_ref())
            });
            if let EmergencyKeyRecovery::Attempted {
                result: Err(error), ..
            } = &recovery
            {
                attempt.cleanup_errors.push(bounded_text(
                    &format!("owned key recovery: {error}"),
                    MAX_RESULT_BYTES,
                ));
            }
            let app_cleanup =
                prelease_app_cleanup.or_else(|| lease.as_ref().map(|l| l.terminate_owned()));
            if let Some(Err(error)) = &app_cleanup {
                attempt.cleanup_errors.push(bounded_text(
                    &format!("application cleanup: {error}"),
                    MAX_RESULT_BYTES,
                ));
            }
            return Err(controlled_subprocess_failure(
                &primary,
                &runner_cleanup,
                &recovery,
                &app_cleanup,
            ));
        }
    };
    let result = complete_observed_subprocess(attempt, &exit, |attempt| {
        // Completion is retained before every fallible post-exit path. Only this
        // positively observed runner exit admits retirement of its owned key.
        let recovery =
            recover_subprocess_input(&probe_output, &nonce, lease.as_ref(), live.as_ref());
        if let Err(error) = &recovery {
            attempt.cleanup_errors.push(bounded_text(
                &format!("owned key recovery: {error}"),
                MAX_RESULT_BYTES,
            ));
        }
        recovery?;
        let lease = lease
            .as_ref()
            .ok_or("actual failed subprocess never transferred verified native child ownership")?;
        require(
            lease.exited_with_closed_windows()?,
            "actual leased application process or owned HWND remains after the failed subprocess",
        )?;
        let created = created
            .as_ref()
            .ok_or("actual failed subprocess omitted created-process ownership")?;
        let live = live
            .as_ref()
            .ok_or("actual failed subprocess omitted measured native ownership")?;
        let expected = ExpectedSubprocess {
            kind,
            nonce: &nonce,
            pid,
            exit_code: exit.status.code(),
            started_unix_ms,
            finished_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis(),
            candidate: &outer.candidate,
            runner_sha256,
            source_revision,
            command: &command,
            created_owner: created,
            leased_owner: &live.owner,
            observed_child_exit_code: lease
                .observed_exit_code()?
                .ok_or("actual application exit was not observed")?,
        };
        verify_report_pair(&report_path, &expected)
    });
    match result {
        Ok(proof) => Ok(proof),
        Err(primary) => {
            let app_cleanup = lease.as_ref().map(|l| l.terminate_owned());
            if let Some(Err(error)) = &app_cleanup {
                attempt.cleanup_errors.push(bounded_text(
                    &format!("application cleanup: {error}"),
                    MAX_RESULT_BYTES,
                ));
            }
            Err(bounded_text(
                &format!("{primary}; application cleanup={app_cleanup:?}"),
                MAX_RESULT_BYTES,
            ))
        }
    }
}

#[cfg(windows)]
fn recover_subprocess_input(
    output: &Path,
    nonce: &str,
    lease: Option<&native::OwnedProbeProcess>,
    live: Option<&LiveOwnership>,
) -> Result<(), String> {
    let down = output.join("case-N02-key-down.json");
    if !down.exists() {
        return Ok(());
    }
    require(
        lease.is_some(),
        "owned key ledger has no verified application process lease",
    )?;
    let live = live.ok_or("owned key ledger has no measured foreground/native owner")?;
    let down: OwnedKeyLedger =
        serde_json::from_slice(&read_regular_bounded(&down, 4096)?).map_err(|e| e.to_string())?;
    require(
        down.nonce == nonce
            && live.nonce == nonce
            && live.kind == ProbeKind::InputTimeout
            && owned_shift_down(&down.edge)
            && down.edge.foreground_hwnd == live.owner.root_hwnd
            && down.edge.foreground_pid == live.owner.child_pid
            && down.edge.at_unix_ms >= live.owner.observed_unix_ms
            && down.edge.input_desktop == live.owner.input_desktop,
        "owned key ledger has stale or foreign insertion ownership",
    )?;
    let up_path = output.join("case-N02-key-up.json");
    if up_path.exists() {
        let up: OwnedKeyLedger = serde_json::from_slice(&read_regular_bounded(&up_path, 4096)?)
            .map_err(|e| e.to_string())?;
        require(
            up.nonce == nonce && owned_key_pair_is_correlated(&down.edge, &up.edge, &live.owner),
            "owned release ledger is incomplete or mismatched",
        )?;
        return native::input_modifiers_clear();
    }
    native::recover_reported_owned_shift(&down.edge)
}

#[cfg(windows)]
#[derive(Debug)]
struct ObservedSubprocessExit {
    process_id: u32,
    status: std::process::ExitStatus,
    observed_unix_ms: Option<u128>,
    origin: SubprocessExitOrigin,
}

#[cfg(windows)]
impl ObservedSubprocessExit {
    fn observed(
        process_id: u32,
        status: std::process::ExitStatus,
        origin: SubprocessExitOrigin,
    ) -> Self {
        Self {
            process_id,
            status,
            observed_unix_ms: observed_utc_now(),
            origin,
        }
    }
}

#[cfg(windows)]
fn record_observed_exit(
    attempt: &mut SubprocessAttempt,
    exit: &ObservedSubprocessExit,
) -> Result<(), String> {
    require(
        matches!(attempt.process, SubprocessState::Running { process_id } if process_id != 0 && process_id == exit.process_id),
        "observed runner exit does not belong to the actual started subprocess",
    )?;
    let code = exit.status.code();
    attempt.process = SubprocessState::ExitObserved {
        process_id: exit.process_id,
        observed_unix_ms: exit.observed_unix_ms,
        status: SubprocessStatus {
            code,
            success: exit.status.success(),
            // Windows exposes the raw DWORD as i32. Reversing that same-width
            // cast preserves all bits, including exception codes with bit31 set.
            windows_status: code.map(|code| code as u32),
        },
        origin: exit.origin,
    };
    Ok(())
}

#[cfg(windows)]
fn complete_observed_subprocess(
    attempt: &mut SubprocessAttempt,
    exit: &ObservedSubprocessExit,
    verify: impl FnOnce(&mut SubprocessAttempt) -> Result<VerifiedProbe, String>,
) -> Result<VerifiedProbe, String> {
    record_observed_exit(attempt, exit)?;
    verify(attempt)
}

#[cfg(windows)]
#[derive(Debug, PartialEq, Eq)]
enum EmergencyKeyRecovery {
    OwnershipRetained,
    Attempted {
        runner_pid: u32,
        actual_exit_code: Option<i32>,
        result: Result<(), String>,
    },
}

#[cfg(windows)]
fn recover_after_observed_runner_exit(
    expected_pid: u32,
    runner_exit: &Result<ObservedSubprocessExit, String>,
    recover: impl FnOnce() -> Result<(), String>,
) -> EmergencyKeyRecovery {
    match runner_exit {
        Ok(exit) if exit.process_id == expected_pid && expected_pid != 0 => {
            EmergencyKeyRecovery::Attempted {
                runner_pid: exit.process_id,
                actual_exit_code: exit.status.code(),
                result: recover(),
            }
        }
        _ => EmergencyKeyRecovery::OwnershipRetained,
    }
}

#[cfg(windows)]
fn controlled_subprocess_failure(
    primary: &str,
    runner_cleanup: &Result<ObservedSubprocessExit, String>,
    recovery: &EmergencyKeyRecovery,
    app_cleanup: &Option<Result<(), String>>,
) -> String {
    bounded_text(
        &format!(
            "{primary}; owned key recovery={recovery:?}; application cleanup={app_cleanup:?}; runner cleanup={runner_cleanup:?}"
        ),
        MAX_RESULT_BYTES,
    )
}

#[cfg(windows)]
fn terminate_subprocess(
    process: &mut std::process::Child,
) -> Result<ObservedSubprocessExit, String> {
    if let Some(status) = process.try_wait().map_err(|e| e.to_string())? {
        return Ok(ObservedSubprocessExit::observed(
            process.id(),
            status,
            SubprocessExitOrigin::ObservedBeforeTermination,
        ));
    }
    process.kill().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(status) = process.try_wait().map_err(|e| e.to_string())? {
            return Ok(ObservedSubprocessExit::observed(
                process.id(),
                status,
                SubprocessExitOrigin::OuterTermination,
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("owned controlled runner did not exit after bounded termination".into())
}

#[cfg(windows)]
pub(super) fn run_aggregate(arguments: Arguments) -> Result<(PathBuf, bool), String> {
    let output = prepare_output_directory(&arguments)?;
    private_artifacts::restrict_directory_acl(&output)?;
    let report_path = proposed_report_path(&arguments, &output)?;
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let candidate = inspect_candidate(arguments.launcher.as_deref())?;
    let profile = tempfile::Builder::new()
        .prefix("multi-launcher-controlled-proof-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let fixture = deterministic_fixture_for_hotkey(
        &profile.path().join("acceptance.log"),
        MouseGestureMode::Enabled,
        AcceptanceHotkey::F11,
    )?;
    for (name, bytes) in [
        ("settings.json", &fixture.settings_json),
        ("radial.json", &fixture.radial_json),
        ("actions.json", &fixture.actions_json),
    ] {
        write_new(&profile.path().join(name), bytes)?;
    }
    let hashes = read_profile_hashes(profile.path())?;
    let mut report = AcceptanceReport {
        schema_version: 9,
        run_id: format!("{}-{started}", std::process::id()),
        mode: AGGREGATE_MODE,
        started_unix_ms: started,
        finished_unix_ms: 0,
        copied_profile_status: CopiedProfileStatus::NotRun,
        copied_profile: None,
        private_artifacts: None,
        h6_repeat_mode: H6RepeatMode::Quiescent,
        mouse_gesture_mode: MouseGestureMode::Enabled,
        suite: AcceptanceSuite::All,
        hotkey: AcceptanceHotkey::F11,
        outcome: "running",
        candidate,
        environment: EnvironmentIdentity {
            os_version: sysinfo::System::long_os_version()
                .unwrap_or_else(|| "unknown Windows version".into()),
            architecture: std::env::consts::ARCH.into(),
            runner_process_id: std::process::id(),
            runner_sha256: Some(
                sha256_file(&std::env::current_exe().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?,
            ),
            child_process_id: None,
            child_started_unix_ms: None,
            source_revision: arguments.source_revision,
            monitors: monitor_inventory(),
        },
        profile: ProfileIdentity {
            mode: "controlled_failure_aggregate",
            temporary_data_root: profile.path().to_string_lossy().into_owned(),
            settings_sha256: hashes.settings,
            radial_sha256: hashes.radial,
            actions_sha256: hashes.actions,
            configured_hotkey: ACCEPTANCE_HOTKEY,
            hold_threshold_ms: fixture.hold_threshold_ms,
        },
        cases: Vec::new(),
        hotkey_evidence: Vec::new(),
        query_evidence: Vec::new(),
        gate_c_evidence: Vec::new(),
        gate_d_evidence: Vec::new(),
        gate_s_evidence: Vec::new(),
        controlled_failures: Some(Evidence::Aggregate {
            probes: Vec::new(),
            attempts: Vec::new(),
        }),
        artifacts: Vec::new(),
        cleanup: CleanupResult::default(),
        capacity_saturated: false,
        report_overflow: None,
    };
    let mut unsafe_cleanup = false;
    for kind in ProbeKind::ALL {
        let at = Instant::now();
        let result = if unsafe_cleanup {
            Err("not run because a prior controlled subprocess failed ownership/cleanup verification".into())
        } else {
            let run = run_subprocess(kind, &output, &report);
            retain_subprocess_run(&mut report, run)
        };
        let (status, observed, stage) = match result {
            Ok(summary) => (CaseStatus::Passed, summary, None),
            Err(error) => {
                unsafe_cleanup = true;
                (
                    CaseStatus::Failed,
                    bounded_text(&error, MAX_RESULT_BYTES),
                    Some(FailureStage::Environment),
                )
            }
        };
        report.push_case(AcceptanceCaseResult { id: kind.id().into(), status,
            elapsed_ms: at.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            expected: "verify an actual isolated failed native stage, persisted failed report/nonzero exit and complete cleanup".into(), observed, failure_stage: stage, artifacts: Vec::new() });
    }
    let profile_hashes_valid = validate_profile_hashes(profile.path(), &report.profile).is_ok();
    report.cleanup.profile_removed = profile.close().is_ok();
    let verified_all = match &report.controlled_failures {
        Some(Evidence::Aggregate { probes, .. }) => {
            probes.len() == 6
                && probes.iter().all(|p| {
                    p.cleanup.profile_removed
                        && p.cleanup.child_owned_windows_closed
                        && p.receipt.owned_keys_after == Some(0)
                })
        }
        _ => false,
    };
    report.cleanup.child_owned_windows_closed = verified_all;
    report.cleanup.cursor_restored = verified_all;
    report.cleanup.input_desktop_released = verified_all;
    report.push_case(AcceptanceCaseResult { id: "CLEANUP".into(), status: if verified_all && report.cleanup.profile_removed { CaseStatus::Passed } else { CaseStatus::Failed }, elapsed_ms: 0,
        expected: "all six inner owned processes/windows/input/profiles disposed; aggregate identity fixture disposed".into(),
        observed: format!("verified_native_cleanup={verified_all}; aggregate_profile_removed={}", report.cleanup.profile_removed),
        failure_stage: (!verified_all || !report.cleanup.profile_removed).then_some(FailureStage::Cleanup), artifacts: Vec::new() });
    report.finished_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let validation = validate_r0_report(&report, Path::new(""), profile_hashes_valid);
    report.push_case(AcceptanceCaseResult { id: "R0".into(), status: if validation.is_ok() { CaseStatus::Passed } else { CaseStatus::Failed }, elapsed_ms: 0,
        expected: "bounded exact operation inventory and actual failed report readbacks remain source matched".into(),
        observed: bounded_text(&validation.clone().unwrap_or_else(|e| e), MAX_RESULT_BYTES), failure_stage: validation.err().map(|_| FailureStage::Environment), artifacts: Vec::new() });
    report.outcome = if report.passed() { "passed" } else { "failed" };
    write_report(&report_path, &mut report)?;
    write_text_report(&report_path.with_extension("txt"), &report)?;
    Ok((report_path, report.passed()))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn probe_report(kind: ProbeKind) -> AcceptanceReport {
        let mut report = super::super::tests::acceptance_report(PROBE_MODE);
        report.started_unix_ms = 1000;
        report.finished_unix_ms = 10_000;
        report.outcome = "failed";
        report.profile.mode = "deterministic_fixture";
        report.profile.hold_threshold_ms = 350;
        report.profile.temporary_data_root = std::env::temp_dir()
            .join("multi-launcher-radial-acceptance-unit-fixture")
            .to_string_lossy()
            .into_owned();
        report.environment.child_process_id = Some(7);
        report.environment.child_started_unix_ms = Some(1100);
        report.environment.monitors = vec![MonitorIdentity {
            id: 0,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
        }];
        let owner = NativeOwner {
            child_pid: 7,
            child_started_unix_ms: 1100,
            process_created_filetime: 11,
            root_hwnd: 44,
            foreground_hwnd: 44,
            foreground_pid: 7,
            foreground_thread_id: 9,
            input_desktop: "thread=Default;active=Default".into(),
            observed_unix_ms: 1200,
        };
        let hashes = ProfileHashes {
            settings: report.profile.settings_sha256.clone(),
            settings_full: "e".repeat(64),
            radial: report.profile.radial_sha256.clone(),
            actions: report.profile.actions_sha256.clone(),
            settings_without_designer_geometry: "f".repeat(64),
        };
        let proof = match kind {
            ProbeKind::Startup => StageProof::Startup { root_hwnd: 44, wait_started_unix_ms: 1300,
                wait_finished_unix_ms: 1550, elapsed_ms: 250, withheld_observations: 10 },
            ProbeKind::InputTimeout => {
                let down = KeyEdge { inserted: 1, at_unix_ms: 1300, foreground_hwnd: 44, foreground_pid: 7,
                    input_desktop: owner.input_desktop.clone(), vk: 0xa0, scan: 0, flags: 0,
                    cookie: 0x5241_4449_414c_0001, async_before: 0, async_after: 0x8000 };
                let up = KeyEdge { at_unix_ms: 1600, flags: 2, async_before: 0x8000, async_after: 0, ..down.clone() };
                StageProof::InputTimeout { key: HeldKeyProof { down, up: Some(up), timeout_at_unix_ms: 1550,
                    timeout_async_state: 0x8000, timeout_elapsed_ms: 250, async_after_cleanup: 0,
                    observer_desktop: "Default".into(), observed_edges: vec![
                        ObservedKeyEdge { vk: 0xa0, down: true, injected: true, cookie: 0x5241_4449_414c_0001, relative_us: 0 },
                        ObservedKeyEdge { vk: 0xa0, down: false, injected: true, cookie: 0x5241_4449_414c_0001, relative_us: 300_000 }],
                    outstanding_keys: 0, observer_stopped: true,
                    primary_error: "controlled input acknowledgement deadline expired while owned LeftShift was held".into(), release_error: None } }
            },
            ProbeKind::Query => {
                let mut invocation = super::super::tests::query_invocation_for_test("qa-open", QueryEvidenceMode::OpenLauncher,
                    QueryEvidenceState::ManualUi, QueryEvidenceRequirement::LauncherUi, QueryEvidenceOutcome::Executed,
                    QueryEvidenceRootPolicy::Legacy, 0, 0, QueryEvidenceUiAck::LauncherQuery, true, 10);
                invocation.query_digest = query_cell_digest("app QMarker").wrapping_mul(0x100000001b3);
                StageProof::Query { invocation: Box::new(invocation), wait_started_unix_ms: 1800,
                    wait_finished_unix_ms: 6800, elapsed_ms: 5000,
                    expected_missing_query_digest: query_cell_digest("qa-controlled-missing-query-ack"),
                    wait_error: "focused launcher query editor did not contain the expected exact query \"qa-controlled-missing-query-ack\"".into() }
            },
            ProbeKind::Ui => StageProof::Ui { designer_hwnd: 55, child_pid: 7, session_id: 13, snapshot_trace_seq: 21,
                wait_started_unix_ms: 1800, wait_finished_unix_ms: 6800, elapsed_ms: 5000,
                missing_control_digest: query_cell_digest("qa-controlled-missing-ui-control"),
                wait_error: "UIA control 'qa-controlled-missing-ui-control' did not publish fresh bounds inside child client [0, 0, 900, 650] before timeout; last bounds=None".into() },
            ProbeKind::ChildExit => StageProof::ChildExit { wait: ChildExitWait {
                wait_entered_unix_ms: 1400, pending_observed_unix_ms: 1420,
                termination_injected_unix_ms: 1450, exit_observed_unix_ms: 1475,
                pending_after_entry_us: 20_000, termination_after_entry_us: 50_000,
                exit_after_entry_us: 75_000, pending_poll_index: 1, exit_poll_index: 2,
                observed_exit_code: 1 }, input_refused_after_exit: true, input_inserted_after_exit: 0 },
            ProbeKind::DesktopMismatch => StageProof::DesktopMismatch { admission: EnvironmentRefusal {
                actual: owner.clone(), expected_desktop: "controlled_expected_nondefault".into(), checked_at_unix_ms: 1300,
                input_inserted: 0, refused_before_send_input: true, owned_state_unchanged: true } },
        };
        let primary = primary_message(&proof);
        report.controlled_failures = Some(Evidence::Probe {
            receipt: Box::new(ProbeReceipt {
                kind,
                nonce: format!("unit-{}", kind.argument()),
                owner: Some(owner),
                proof: Some(proof),
                profile_before: hashes.clone(),
                profile_after: Some(hashes),
                child_exit_code: Some(if kind == ProbeKind::ChildExit { 1 } else { 0 }),
                owned_keys_after: Some(0),
                marker_count_before: Some(0),
                marker_count_after: Some(0),
                execution_count_before: Some(if kind == ProbeKind::Query { 2 } else { 0 }),
                execution_count_after: Some(if kind == ProbeKind::Query { 2 } else { 0 }),
                cleanup_errors: Vec::new(),
            }),
        });
        report.cleanup = CleanupResult {
            child_closed_normally: kind != ProbeKind::ChildExit,
            child_owned_windows_closed: true,
            profile_removed: true,
            foreground_restore_captured: true,
            foreground_restore_attempted: true,
            foreground_restored: true,
            cursor_restored: true,
            input_desktop_released: true,
            ..CleanupResult::default()
        };
        report.cases = vec![
            AcceptanceCaseResult {
                id: kind.id().into(),
                status: CaseStatus::Failed,
                elapsed_ms: 6000,
                expected: "actual bounded controlled native stage".into(),
                observed: primary,
                failure_stage: Some(kind.failure_stage()),
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "CLEANUP".into(),
                status: if kind == ProbeKind::ChildExit {
                    CaseStatus::Failed
                } else {
                    CaseStatus::Passed
                },
                elapsed_ms: 50,
                expected: "dispose only the actual owned child and input".into(),
                observed: if kind == ProbeKind::ChildExit {
                    "candidate exited before the runner requested normal shutdown".into()
                } else {
                    "actual owned child disposed normally".into()
                },
                failure_stage: (kind == ProbeKind::ChildExit).then_some(FailureStage::Cleanup),
                artifacts: Vec::new(),
            },
            AcceptanceCaseResult {
                id: "R0".into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "exact failed report integrity".into(),
                observed: "source and bounded report verified".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            },
        ];
        report
    }

    fn owners(report: &AcceptanceReport) -> (CreatedOwnership, NativeOwner) {
        let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
            panic!("unit probe missing")
        };
        let owner = receipt.owner.clone().unwrap();
        (
            CreatedOwnership {
                kind: receipt.kind,
                nonce: receipt.nonce.clone(),
                runner_pid: report.environment.runner_process_id,
                source_revision: report.environment.source_revision.clone().unwrap(),
                candidate_sha256: report.candidate.sha256.clone(),
                profile_root: report.profile.temporary_data_root.clone(),
                child_pid: owner.child_pid,
                child_started_unix_ms: owner.child_started_unix_ms,
                process_created_filetime: owner.process_created_filetime,
                profile_hashes: receipt.profile_before.clone(),
            },
            owner,
        )
    }

    fn attempt_for_proof(proof: &VerifiedProbe, outer: &AcceptanceReport) -> SubprocessAttempt {
        SubprocessAttempt {
            kind: proof.kind,
            nonce: proof.nonce.clone(),
            source_revision: outer.environment.source_revision.clone(),
            runner_sha256: outer.environment.runner_sha256.clone(),
            candidate_executable: outer.candidate.executable.clone(),
            candidate_sha256: outer.candidate.sha256.clone(),
            command: Some(proof.command.clone()),
            started_unix_ms: Some(proof.started_unix_ms),
            finished_unix_ms: Some(proof.finished_unix_ms),
            process: SubprocessState::ExitObserved {
                process_id: proof.subprocess_pid,
                observed_unix_ms: Some(proof.finished_unix_ms - 1),
                status: SubprocessStatus {
                    code: Some(proof.actual_exit_code),
                    success: proof.actual_exit_code == 0,
                    windows_status: Some(proof.actual_exit_code as u32),
                },
                origin: SubprocessExitOrigin::RunnerCompletion,
            },
            created_owner: Some(proof.created_owner.clone()),
            live_owner: Some(LiveOwnership {
                kind: proof.kind,
                nonce: proof.nonce.clone(),
                runner_pid: proof.subprocess_pid,
                source_revision: proof.created_owner.source_revision.clone(),
                candidate_sha256: proof.created_owner.candidate_sha256.clone(),
                profile_root: proof.created_owner.profile_root.clone(),
                owner: proof.leased_owner.clone(),
            }),
            primary_error: None,
            cleanup_errors: Vec::new(),
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_process_lease_admits_delayed_exact_ack_within_shared_startup_budget() {
        let report = probe_report(ProbeKind::Startup);
        let (_, owner) = owners(&report);
        let ack = ProcessLeaseAck {
            nonce: "unit-startup".into(),
            subprocess_pid: 77,
            child_pid: owner.child_pid,
            process_created_filetime: owner.process_created_filetime,
        };
        let admitted = std::cell::Cell::new(0);
        for elapsed in [
            Duration::ZERO,
            Duration::from_secs(3),
            Duration::from_millis(4321),
        ] {
            if process_lease_ack_ready(
                None,
                &ack.nonce,
                77,
                &owner,
                elapsed,
                native::STARTUP_TIMEOUT,
            )
            .unwrap()
            {
                admitted.set(admitted.get() + 1);
            }
        }
        assert_eq!(
            admitted.get(),
            0,
            "no input/stage is admitted while ownership is pending"
        );
        let actual_delayed_ack = Duration::from_millis(4322);
        assert!(actual_delayed_ack > Duration::from_secs(3));
        if process_lease_ack_ready(
            Some(&ack),
            &ack.nonce,
            77,
            &owner,
            actual_delayed_ack,
            native::STARTUP_TIMEOUT,
        )
        .unwrap()
        {
            admitted.set(admitted.get() + 1);
        }
        assert_eq!(admitted.get(), 1);
        assert_eq!(native::STARTUP_TIMEOUT, Duration::from_secs(20));
        assert!(native::STARTUP_TIMEOUT < SUBPROCESS_TIMEOUT);
    }

    #[cfg(windows)]
    #[test]
    fn controlled_process_lease_refuses_expired_missing_stale_and_foreign_ack_before_admission() {
        let report = probe_report(ProbeKind::Startup);
        let (_, owner) = owners(&report);
        let valid = || ProcessLeaseAck {
            nonce: "unit-startup".into(),
            subprocess_pid: 77,
            child_pid: owner.child_pid,
            process_created_filetime: owner.process_created_filetime,
        };
        let mut wrong = Vec::new();
        let mut stale = valid();
        stale.nonce = "older-startup".into();
        wrong.push(stale);
        let mut foreign = valid();
        foreign.subprocess_pid = 78;
        wrong.push(foreign);
        let mut foreign = valid();
        foreign.child_pid += 1;
        wrong.push(foreign);
        let mut stale = valid();
        stale.process_created_filetime += 1;
        wrong.push(stale);
        let admitted = std::cell::Cell::new(0);
        for ack in &wrong {
            let result = process_lease_ack_ready(
                Some(ack),
                "unit-startup",
                77,
                &owner,
                Duration::from_millis(4322),
                native::STARTUP_TIMEOUT,
            );
            if result == Ok(true) {
                admitted.set(admitted.get() + 1);
            }
            assert!(result.is_err());
        }
        let ack = valid();
        for elapsed in [
            native::STARTUP_TIMEOUT,
            native::STARTUP_TIMEOUT + Duration::from_millis(1),
        ] {
            for receipt in [None, Some(&ack)] {
                let result = process_lease_ack_ready(
                    receipt,
                    &ack.nonce,
                    77,
                    &owner,
                    elapsed,
                    native::STARTUP_TIMEOUT,
                );
                if result == Ok(true) {
                    admitted.set(admitted.get() + 1);
                }
                assert!(
                    result.is_err(),
                    "a correct late ACK must not bypass the real bound"
                );
            }
        }
        assert_eq!(admitted.get(), 0);
    }

    #[cfg(windows)]
    fn fresh_query_profile() -> (tempfile::TempDir, DeterministicFixture) {
        let profile = tempfile::Builder::new()
            .prefix("multi-launcher-radial-acceptance-controlled-unit-")
            .tempdir()
            .unwrap();
        let marker = profile.path().join("query-marker-ledger.txt");
        write_new(&marker, b"radial-acceptance-marker-ledger:v1\n").unwrap();
        let fixture = deterministic_query_fixture(
            &profile.path().join("acceptance.log"),
            MouseGestureMode::Enabled,
            AcceptanceHotkey::F11,
            &marker,
            &std::env::current_exe().unwrap(),
        )
        .unwrap();
        for (name, bytes) in [
            ("settings.json", &fixture.settings_json),
            ("radial.json", &fixture.radial_json),
            ("actions.json", &fixture.actions_json),
        ] {
            write_new(&profile.path().join(name), bytes).unwrap();
        }
        (profile, fixture)
    }

    #[cfg(windows)]
    #[test]
    fn controlled_fresh_query_fixture_migrates_before_baseline_and_normal_startup_is_idempotent() {
        use multi_launcher::radial::{store::RadialStore, submenu_migration};
        let (profile, mut fixture) = fresh_query_profile();
        let original: RadialDocument = serde_json::from_slice(&fixture.radial_json).unwrap();
        let original_settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        assert!(
            !original_settings
                .plugin_settings
                .contains_key("clipboard_modify")
        );
        let original_actions = fixture.actions_json.clone();
        let old_hashes = read_profile_hashes(profile.path()).unwrap();
        let marker = fs::read(profile.path().join("query-marker-ledger.txt")).unwrap();
        let note = fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap();
        prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap();
        let baseline = read_profile_hashes(profile.path()).unwrap();
        assert_ne!(baseline.settings, old_hashes.settings);
        assert_ne!(baseline.radial, old_hashes.radial);
        assert_eq!(baseline.actions, old_hashes.actions);
        assert_eq!(baseline.settings, sha256_bytes(&fixture.settings_json));
        assert_eq!(baseline.radial, sha256_bytes(&fixture.radial_json));
        assert_eq!(fixture.actions_json, original_actions);
        let migrated: RadialDocument = serde_json::from_slice(&fixture.radial_json).unwrap();
        let mut expected = original.clone();
        expected.revision.0 += 1;
        assert_eq!(
            migrated, expected,
            "only the existing migration's revision changes authored radial state"
        );
        let cells = &migrated
            .menus
            .iter()
            .find(|menu| menu.id == migrated.default_menu_id)
            .unwrap()
            .rings[0]
            .cells;
        assert_eq!(
            cells
                .iter()
                .take(9)
                .map(|cell| cell.id.as_str())
                .collect::<Vec<_>>(),
            [
                "qa-open",
                "qa-hidden-first",
                "qa-no-results",
                "qa-visible-first",
                "qa-note-new",
                "qa-launcher-show",
                "qa-note-remove",
                "qa-exact-marker",
                "qa-pinned-second"
            ]
        );
        assert!(
            matches!(&cells[0].content, CellContent::Action { binding: ActionBinding::LauncherQuery { query, mode } }
            if query == "app QMarker" && *mode == multi_launcher::radial::model::QueryRunMode::OpenLauncher)
        );
        assert!(
            matches!(&cells[7].content, CellContent::Action { binding: ActionBinding::ExactCommand { command, args: Some(args) } }
            if command == std::env::current_exe().unwrap().to_string_lossy().as_ref()
                && args.contains("q-exact"))
        );
        assert!(
            matches!(&cells[8].content, CellContent::Action { binding: ActionBinding::Persisted { action } }
            if action.action_id == action_ids::RESULT_EXECUTE)
        );
        validate_radial_document(&migrated).unwrap();
        let settings: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
        let mut expected_settings = original_settings;
        expected_settings.plugin_settings.insert(
            "clipboard_modify".into(),
            serde_json::to_value(
                multi_launcher::settings::ClipboardModifyPluginSettings::default(),
            )
            .unwrap(),
        );
        expected_settings.radial_submenu_migration = settings.radial_submenu_migration.clone();
        assert_eq!(
            canonical_settings(&fixture.settings_json).unwrap(),
            canonical_settings(&serde_json::to_vec(&expected_settings).unwrap()).unwrap(),
            "only the documented Clipboard Modify default and radial receipt are materialized"
        );
        assert_eq!(settings.window_size, Some((900, 650)));
        assert!(settings.pinned_panels.is_empty());
        let receipt = settings.radial_submenu_migration.as_ref().unwrap();
        assert_eq!(
            receipt.state,
            multi_launcher::settings::SubmenuMigrationState::Applied
        );
        assert!(
            receipt.changed_menus.is_empty()
                && receipt.undo_restored_menu_ids.is_empty()
                && receipt.failure.is_none()
        );
        let restarted_settings =
            multi_launcher::startup::load_startup_settings(profile.path().join("settings.json"));
        assert!(restarted_settings.diagnostic.is_none());
        assert_eq!(
            canonical_settings(&serde_json::to_vec(&restarted_settings.settings).unwrap()).unwrap(),
            canonical_settings(&fixture.settings_json).unwrap()
        );
        let store = RadialStore::at_path(
            profile.path().join("radial.json"),
            RadialDocument::starter(),
        )
        .unwrap();
        store.reload().unwrap();
        let restarted =
            submenu_migration::startup_migrate(&store, &profile.path().join("settings.json"), true)
                .unwrap();
        assert!(!restarted.changed);
        assert_eq!(*restarted.document, migrated);
        assert_eq!(read_profile_hashes(profile.path()).unwrap(), baseline);
        for (name, bytes) in [
            ("settings.json", &fixture.settings_json),
            ("radial.json", &fixture.radial_json),
            ("actions.json", &fixture.actions_json),
        ] {
            assert_eq!(
                fs::read(profile.path().join(name)).unwrap(),
                *bytes,
                "startup must preserve the captured raw baseline"
            );
        }
        assert_eq!(
            fs::read(profile.path().join("query-marker-ledger.txt")).unwrap(),
            marker
        );
        assert_eq!(
            fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap(),
            note
        );
    }

    #[cfg(windows)]
    #[test]
    fn controlled_fresh_fixture_preserves_complete_authored_preferences_and_plugin_enablement() {
        for clipboard_present in [false, true] {
            let (profile, mut fixture) = fresh_query_profile();
            let mut authored: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            authored.enabled_plugins = Some(
                ["custom", "mouse_gestures"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            );
            authored.plugin_settings.insert(
                "custom".into(),
                serde_json::json!({
                    "protected": {"flag": false}, "ordered": ["second", "first"]
                }),
            );
            if clipboard_present {
                let mut preferences =
                    multi_launcher::settings::ClipboardModifyPluginSettings::default();
                preferences.hide_launcher_after_apply = false;
                preferences.template_filter = "protected authored filter".into();
                authored.plugin_settings.insert(
                    "clipboard_modify".into(),
                    serde_json::to_value(preferences).unwrap(),
                );
            }
            fixture.settings_json = serde_json::to_vec_pretty(&authored).unwrap();
            fs::write(profile.path().join("settings.json"), &fixture.settings_json).unwrap();
            let radial = fixture.radial_json.clone();
            let actions = fixture.actions_json.clone();
            let marker = fs::read(profile.path().join("query-marker-ledger.txt")).unwrap();
            let note = fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap();
            prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap();
            let mut expected = authored;
            if !clipboard_present {
                expected.plugin_settings.insert(
                    "clipboard_modify".into(),
                    serde_json::to_value(
                        multi_launcher::settings::ClipboardModifyPluginSettings::default(),
                    )
                    .unwrap(),
                );
                expected
                    .enabled_plugins
                    .as_mut()
                    .unwrap()
                    .insert("clipboard_modify".into());
            }
            let actual: Settings = serde_json::from_slice(&fixture.settings_json).unwrap();
            expected.radial_submenu_migration = actual.radial_submenu_migration.clone();
            assert_eq!(
                canonical_settings(&fixture.settings_json).unwrap(),
                canonical_settings(&serde_json::to_vec(&expected).unwrap()).unwrap()
            );
            assert_eq!(
                actual.plugin_settings["custom"],
                expected.plugin_settings["custom"]
            );
            assert_eq!(actual.enabled_plugins, expected.enabled_plugins);
            let mut expected_radial: RadialDocument = serde_json::from_slice(&radial).unwrap();
            expected_radial.revision.0 += 1;
            assert_eq!(
                serde_json::from_slice::<RadialDocument>(&fixture.radial_json).unwrap(),
                expected_radial
            );
            assert_eq!(fixture.actions_json, actions);
            let baseline = read_profile_hashes(profile.path()).unwrap();
            let startup = multi_launcher::startup::load_startup_settings(
                profile.path().join("settings.json"),
            );
            assert!(startup.diagnostic.is_none());
            let store = multi_launcher::radial::store::RadialStore::at_path(
                profile.path().join("radial.json"),
                RadialDocument::starter(),
            )
            .unwrap();
            store.reload().unwrap();
            let restarted = multi_launcher::radial::submenu_migration::startup_migrate(
                &store,
                &profile.path().join("settings.json"),
                true,
            )
            .unwrap();
            assert!(!restarted.changed);
            assert_eq!(read_profile_hashes(profile.path()).unwrap(), baseline);
            assert_eq!(
                fs::read(profile.path().join("query-marker-ledger.txt")).unwrap(),
                marker
            );
            assert_eq!(
                fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap(),
                note
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_fresh_fixture_refuses_incomplete_unknown_or_close_incompatible_authored_settings()
    {
        for mutation in [
            "missing field",
            "unknown field",
            "ROOT size",
            "pinned panels",
            "clipboard dimensions",
            "clipboard unknown",
        ] {
            let (profile, mut fixture) = fresh_query_profile();
            let mut value: serde_json::Value =
                serde_json::from_slice(&fixture.settings_json).unwrap();
            match mutation {
                "missing field" => {
                    value.as_object_mut().unwrap().remove("debug_logging");
                }
                "unknown field" => {
                    value["unknown_protected"] = serde_json::json!(7);
                }
                "ROOT size" => {
                    value["window_size"] = serde_json::json!([901, 650]);
                }
                "pinned panels" => {
                    value["pinned_panels"] = serde_json::json!(["ClipboardDialog"]);
                }
                _ => {
                    value["plugin_settings"]["clipboard_modify"] = serde_json::to_value(
                        multi_launcher::settings::ClipboardModifyPluginSettings::default(),
                    )
                    .unwrap();
                    if mutation == "clipboard dimensions" {
                        value["plugin_settings"]["clipboard_modify"]["dialog_width"] =
                            serde_json::json!(901.0);
                    } else {
                        value["plugin_settings"]["clipboard_modify"]["unknown_protected"] =
                            serde_json::json!(true);
                    }
                }
            }
            fixture.settings_json = serde_json::to_vec(&value).unwrap();
            fs::write(profile.path().join("settings.json"), &fixture.settings_json).unwrap();
            let authored = fixture.settings_json.clone();
            let before = [
                "settings.json",
                "radial.json",
                "actions.json",
                "query-marker-ledger.txt",
                "notes/radial-acceptance-q13.md",
            ]
            .map(|name| fs::read(profile.path().join(name)).unwrap());
            assert!(
                prepare_fresh_controlled_fixture(profile.path(), &mut fixture).is_err(),
                "{mutation}"
            );
            assert_eq!(
                fixture.settings_json, authored,
                "no failed preparation publishes a new baseline"
            );
            assert_eq!(
                [
                    "settings.json",
                    "radial.json",
                    "actions.json",
                    "query-marker-ledger.txt",
                    "notes/radial-acceptance-q13.md"
                ]
                .map(|name| fs::read(profile.path().join(name)).unwrap()),
                before,
                "{mutation}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_fresh_query_fixture_refuses_corruption_and_nonfresh_identity_without_resetting_baseline()
     {
        for file in ["settings.json", "radial.json", "actions.json"] {
            let (profile, mut fixture) = fresh_query_profile();
            fs::write(profile.path().join(file), b"{corrupt-before-migration").unwrap();
            let before = ["settings.json", "radial.json", "actions.json"]
                .map(|name| fs::read(profile.path().join(name)).unwrap());
            assert!(
                prepare_fresh_controlled_fixture(profile.path(), &mut fixture).is_err(),
                "{file}"
            );
            let after = ["settings.json", "radial.json", "actions.json"]
                .map(|name| fs::read(profile.path().join(name)).unwrap());
            assert_eq!(
                before, after,
                "corrupt disk identity cannot be migrated or silently rebaselined"
            );
            assert_eq!(
                fs::read_dir(profile.path())
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_name().to_string_lossy().contains("migration"))
                    .count(),
                0
            );
        }
        for file in ["settings.json", "radial.json"] {
            let (profile, mut fixture) = fresh_query_profile();
            let corrupt = b"{corrupt-authored-source".to_vec();
            if file == "settings.json" {
                fixture.settings_json = corrupt.clone();
            } else {
                fixture.radial_json = corrupt.clone();
            }
            fs::write(profile.path().join(file), &corrupt).unwrap();
            assert!(prepare_fresh_controlled_fixture(profile.path(), &mut fixture).is_err());
            assert_eq!(
                fs::read(profile.path().join(file)).unwrap(),
                corrupt,
                "no starter/default fallback replaces malformed authored input"
            );
        }
        let (profile, mut fixture) = fresh_query_profile();
        prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap();
        let applied = read_profile_hashes(profile.path()).unwrap();
        assert!(
            prepare_fresh_controlled_fixture(profile.path(), &mut fixture).is_err(),
            "only fresh isolated fixtures use this preparation path"
        );
        assert_eq!(read_profile_hashes(profile.path()).unwrap(), applied);
        let (profile, mut fixture) = fresh_query_profile();
        let mut document: RadialDocument = serde_json::from_slice(&fixture.radial_json).unwrap();
        document.menus[0].submenu_presentation =
            multi_launcher::radial::model::SubmenuPresentation::Cascade;
        fixture.radial_json = serde_json::to_vec_pretty(&document).unwrap();
        fs::write(profile.path().join("radial.json"), &fixture.radial_json).unwrap();
        let protected = read_profile_hashes(profile.path()).unwrap();
        assert!(prepare_fresh_controlled_fixture(profile.path(), &mut fixture).is_err());
        assert_eq!(read_profile_hashes(profile.path()).unwrap(), protected);
    }

    fn complete_wire_evidence() -> Vec<Evidence> {
        let mut evidence = Vec::new();
        let mut probes = Vec::new();
        for kind in ProbeKind::ALL {
            let report = probe_report(kind);
            let (created_owner, leased_owner) = owners(&report);
            let proof = report.controlled_failures.clone().unwrap();
            let Evidence::Probe { receipt } = &proof else {
                panic!("complete probe fixture")
            };
            probes.push(VerifiedProbe {
                kind,
                nonce: receipt.nonce.clone(),
                subprocess_pid: report.environment.runner_process_id,
                actual_exit_code: 1,
                started_unix_ms: report.started_unix_ms,
                finished_unix_ms: report.finished_unix_ms,
                report_path: format!("fixture-{}.json", kind.id()),
                report_sha256: "a".repeat(64),
                text_sha256: "b".repeat(64),
                receipt: *receipt.clone(),
                cleanup: report.cleanup.clone(),
                private_artifacts: private_artifacts::PrivateArtifactSummary::not_run(),
                command: command(&report, Path::new("wire-fixture")),
                created_owner,
                leased_owner,
                observed_child_exit_code: if kind == ProbeKind::ChildExit { 1 } else { 0 },
            });
            evidence.push(proof);
        }
        let outer = super::super::tests::acceptance_report(AGGREGATE_MODE);
        let attempts = probes
            .iter()
            .map(|proof| attempt_for_proof(proof, &outer))
            .collect();
        evidence.push(Evidence::Aggregate { probes, attempts });
        evidence
    }

    #[test]
    fn controlled_tagged_receipts_round_trip_all_stages_and_complete_aggregate() {
        let fixtures = complete_wire_evidence();
        assert_eq!(fixtures.len(), ProbeKind::ALL.len() + 1);
        for evidence in fixtures {
            let bytes = serde_json::to_vec(&evidence).unwrap();
            let decoded: Evidence = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                serde_json::to_value(&decoded).unwrap(),
                serde_json::to_value(&evidence).unwrap()
            );
            match decoded {
                Evidence::Probe { receipt } => {
                    assert!(receipt.owner.is_some() && receipt.proof.is_some());
                }
                Evidence::Aggregate { probes, attempts } => {
                    assert_eq!(probes.len(), ProbeKind::ALL.len());
                    assert_eq!(attempts.len(), probes.len());
                    assert!(probes.iter().map(|p| p.kind).eq(ProbeKind::ALL));
                    for probe in probes {
                        assert!(probe.receipt.owner.is_some() && probe.receipt.proof.is_some());
                        assert_eq!(probe.created_owner.child_pid, probe.leased_owner.child_pid);
                    }
                }
            }
        }
    }

    #[test]
    fn controlled_timestamp_wire_rejects_noninteger_negative_and_overflow() {
        fn timestamp_paths(value: &serde_json::Value, prefix: &str, paths: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, child) in object {
                        let path = format!("{prefix}/{key}");
                        if key.ends_with("unix_ms") {
                            assert!(child.is_u64(), "timestamp must remain a JSON number");
                            paths.push(path);
                        } else {
                            timestamp_paths(child, &path, paths);
                        }
                    }
                }
                serde_json::Value::Array(array) => {
                    for (index, child) in array.iter().enumerate() {
                        timestamp_paths(child, &format!("{prefix}/{index}"), paths);
                    }
                }
                _ => {}
            }
        }
        for evidence in complete_wire_evidence() {
            let value = serde_json::to_value(&evidence).unwrap();
            let mut paths = Vec::new();
            timestamp_paths(&value, "", &mut paths);
            assert!(!paths.is_empty());
            for path in paths {
                let mut maximum = value.clone();
                *maximum.pointer_mut(&path).unwrap() = serde_json::json!(u64::MAX);
                let decoded: Evidence = serde_json::from_value(maximum.clone()).unwrap();
                assert_eq!(serde_json::to_value(&decoded).unwrap(), maximum);
                for bad in ["-1", "1.5", "18446744073709551616", "\"1300\""] {
                    let mut changed = value.clone();
                    *changed.pointer_mut(&path).unwrap() = serde_json::from_str(bad).unwrap();
                    assert!(
                        serde_json::from_value::<Evidence>(changed).is_err(),
                        "timestamp {path} accepted {bad}"
                    );
                }
            }
        }
        let mut probes = complete_wire_evidence();
        let Evidence::Probe { receipt } = &mut probes[0] else {
            panic!("probe")
        };
        receipt.owner.as_mut().unwrap().child_started_unix_ms = u128::from(u64::MAX) + 1;
        assert!(serde_json::to_vec(&probes[0]).is_err());
        let Evidence::Aggregate { probes, .. } = probes.last_mut().unwrap() else {
            panic!("aggregate")
        };
        probes[0].started_unix_ms = u128::from(u64::MAX) + 1;
        assert!(serde_json::to_vec(&probes).is_err());
        probes[0].started_unix_ms = 1000;
        probes[0].created_owner.child_started_unix_ms = u128::from(u64::MAX) + 1;
        assert!(serde_json::to_vec(&probes).is_err());
    }

    #[test]
    fn controlled_full_settings_identity_is_required_through_all_receipts_and_aggregate_attempts() {
        fn paths(value: &serde_json::Value, prefix: &str, output: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, child) in object {
                        let path = format!("{prefix}/{key}");
                        if key == "settings_full" {
                            output.push(path);
                        } else {
                            paths(child, &path, output);
                        }
                    }
                }
                serde_json::Value::Array(array) => {
                    for (index, child) in array.iter().enumerate() {
                        paths(child, &format!("{prefix}/{index}"), output);
                    }
                }
                _ => {}
            }
        }
        for evidence in complete_wire_evidence() {
            let bytes = serde_json::to_vec(&evidence).unwrap();
            let cap = if matches!(&evidence, Evidence::Aggregate { .. }) {
                MAX_CONTROLLED_BYTES
            } else {
                MAX_PROBE_BYTES
            };
            assert!(bytes.len() < cap, "existing proof cap must suffice");
            let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let decoded: Evidence = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_value(&decoded).unwrap(), original);
            let mut fields = Vec::new();
            paths(&original, "", &mut fields);
            assert_eq!(
                fields.len(),
                if matches!(evidence, Evidence::Aggregate { .. }) {
                    24
                } else {
                    2
                }
            );
            for path in fields {
                let mut missing = original.clone();
                let (parent, field) = path.rsplit_once('/').unwrap();
                missing
                    .pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    serde_json::from_value::<Evidence>(missing).is_err(),
                    "legacy omitted identity {path}"
                );
                for invalid in [
                    serde_json::Value::Null,
                    serde_json::json!(1),
                    serde_json::json!([]),
                ] {
                    let mut changed = original.clone();
                    *changed.pointer_mut(&path).unwrap() = invalid;
                    assert!(
                        serde_json::from_value::<Evidence>(changed).is_err(),
                        "wrong identity type {path}"
                    );
                }
            }
        }
    }

    #[test]
    fn controlled_full_settings_identity_is_strictly_bound_to_created_owner_and_raw_observations() {
        for kind in ProbeKind::ALL {
            let report = probe_report(kind);
            let (created, owner) = owners(&report);
            let command = command(&report, Path::new("full-identity"));
            let bytes = serde_json::to_vec(&report).unwrap();
            let actual = expected(&report, &created, &owner, &command);
            parse_persisted(&bytes, &actual).unwrap();
            let mut changed_created = created.clone();
            changed_created.profile_hashes.settings_full = "d".repeat(64);
            assert!(
                parse_persisted(
                    &bytes,
                    &expected(&report, &changed_created, &owner, &command)
                )
                .is_err(),
                "created baseline mismatch {kind:?}"
            );
            for bad in ["".into(), "x".repeat(64), "a".repeat(65)] {
                let mut changed = report.clone();
                receipt_mut(&mut changed)
                    .unwrap()
                    .profile_before
                    .settings_full = bad.clone();
                receipt_mut(&mut changed)
                    .unwrap()
                    .profile_after
                    .as_mut()
                    .unwrap()
                    .settings_full = bad.clone();
                assert!(
                    validate_probe(&changed, true).is_err(),
                    "invalid full identity {kind:?}"
                );
                let mut invalid_created = created.clone();
                invalid_created.profile_hashes.settings_full = bad;
                assert!(
                    check_created_ownership(
                        &invalid_created,
                        kind,
                        &created.nonce,
                        created.runner_pid,
                        1000,
                        &report.candidate,
                        "deadbeef"
                    )
                    .is_err()
                );
            }
            let mut contradictory = report.clone();
            receipt_mut(&mut contradictory)
                .unwrap()
                .profile_after
                .as_mut()
                .unwrap()
                .settings_full = "d".repeat(64);
            assert!(
                validate_probe(&contradictory, true).is_err(),
                "unchanged raw bytes cannot have changed full identity, including UI {kind:?}"
            );
            let mut changed = serde_json::to_value(&report).unwrap();
            changed["controlled_failures"]["receipt"]["profile_before"]
                .as_object_mut()
                .unwrap()
                .remove("settings_full");
            assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        }
    }

    fn command(report: &AcceptanceReport, directory: &Path) -> Vec<String> {
        let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
            panic!("unit probe missing")
        };
        vec![
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            "--controlled-native-failure-probe".into(),
            receipt.kind.argument().into(),
            "--controlled-probe-nonce".into(),
            receipt.nonce.clone(),
            "--launcher".into(),
            report.candidate.executable.clone(),
            "--output".into(),
            directory.to_string_lossy().into_owned(),
            "--source-revision".into(),
            report.environment.source_revision.clone().unwrap(),
        ]
    }

    fn expected<'a>(
        report: &'a AcceptanceReport,
        created: &'a CreatedOwnership,
        owner: &'a NativeOwner,
        command: &'a [String],
    ) -> ExpectedSubprocess<'a> {
        let Some(Evidence::Probe { receipt }) = &report.controlled_failures else {
            panic!("unit probe missing")
        };
        ExpectedSubprocess {
            kind: receipt.kind,
            nonce: &receipt.nonce,
            pid: report.environment.runner_process_id,
            exit_code: Some(1),
            started_unix_ms: 900,
            finished_unix_ms: 11_000,
            candidate: &report.candidate,
            runner_sha256: report.environment.runner_sha256.as_deref().unwrap(),
            source_revision: report.environment.source_revision.as_deref().unwrap(),
            command,
            created_owner: created,
            leased_owner: owner,
            observed_child_exit_code: receipt.child_exit_code.unwrap(),
        }
    }

    fn attempt_fixture() -> (
        AcceptanceReport,
        AcceptanceReport,
        VerifiedProbe,
        SubprocessAttempt,
    ) {
        let mut outer = super::super::tests::acceptance_report(AGGREGATE_MODE);
        outer.started_unix_ms = 800;
        outer.finished_unix_ms = 20_000;
        outer.outcome = "failed";
        outer.profile.mode = "controlled_failure_aggregate";
        outer.profile.hold_threshold_ms = 350;
        outer.candidate.executable = std::env::temp_dir()
            .join("controlled-unit-candidate.exe")
            .to_string_lossy()
            .into_owned();
        let mut inner = probe_report(ProbeKind::Startup);
        inner.candidate = outer.candidate.clone();
        inner.environment.runner_process_id = 77;
        let (created_owner, leased_owner) = owners(&inner);
        let output = std::env::temp_dir().join(ProbeKind::Startup.id());
        let command = command(&inner, &output);
        let Some(Evidence::Probe { receipt }) = &inner.controlled_failures else {
            panic!("probe")
        };
        let proof = VerifiedProbe {
            kind: ProbeKind::Startup,
            nonce: receipt.nonce.clone(),
            subprocess_pid: 77,
            actual_exit_code: 1,
            started_unix_ms: 900,
            finished_unix_ms: 11_000,
            report_path: output.join("report.json").to_string_lossy().into_owned(),
            report_sha256: "a".repeat(64),
            text_sha256: "b".repeat(64),
            receipt: *receipt.clone(),
            cleanup: inner.cleanup.clone(),
            private_artifacts: private_artifacts::PrivateArtifactSummary::not_run(),
            command,
            created_owner,
            leased_owner,
            observed_child_exit_code: 0,
        };
        let mut attempt = attempt_for_proof(&proof, &outer);
        attempt.primary_error =
            Some("actual failed readback did not prove the expected stage".into());
        outer.controlled_failures = Some(Evidence::Aggregate {
            probes: Vec::new(),
            attempts: Vec::new(),
        });
        outer.cases = CASE_IDS
            .iter()
            .map(|id| AcceptanceCaseResult {
                id: (*id).into(),
                status: CaseStatus::Failed,
                elapsed_ms: 0,
                expected: "actual isolated failed native stage and complete verified cleanup"
                    .into(),
                observed: "not run after the current failed ownership/readback".into(),
                failure_stage: Some(FailureStage::Environment),
                artifacts: Vec::new(),
            })
            .collect();
        (outer, inner, proof, attempt)
    }

    #[cfg(windows)]
    struct PreleaseChildFixture {
        alive: std::rc::Rc<std::cell::Cell<bool>>,
        cleanup_calls: std::rc::Rc<std::cell::Cell<usize>>,
        events: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
    }

    #[cfg(windows)]
    impl PreleaseChildFixture {
        fn new(events: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>) -> Self {
            Self {
                alive: std::rc::Rc::new(std::cell::Cell::new(true)),
                cleanup_calls: std::rc::Rc::new(std::cell::Cell::new(0)),
                events,
            }
        }
        fn cleanup(&self, fail: bool) -> Result<(), String> {
            self.cleanup_calls.set(self.cleanup_calls.get() + 1);
            self.events.borrow_mut().push("child cleanup attempted");
            if fail {
                return Err("actual child termination rejected".into());
            }
            self.alive.set(false);
            self.events.borrow_mut().push("child disposed");
            Ok(())
        }
    }

    #[cfg(windows)]
    impl Drop for PreleaseChildFixture {
        fn drop(&mut self) {
            if self.alive.get() {
                self.events
                    .borrow_mut()
                    .push("inner NativeChild owner drop");
                let _ = self.cleanup(false);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_rejected_image_retains_safe_child_cleanup_before_forced_runner_retirement() {
        use std::os::windows::process::ExitStatusExt;
        let images = tempfile::tempdir().unwrap();
        let image = images.path().join("owned-probe.exe");
        fs::write(&image, b"real owned image bytes for hash admission").unwrap();
        for read_failure in [false, true] {
            let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let child = PreleaseChildFixture::new(events.clone());
            let alive = child.alive.clone();
            let cleanup_calls = child.cleanup_calls.clone();
            // The production acquisition seam runs only after native birth/path
            // checks. This resource fixture proves its rejection retains the
            // cleanup owner rather than disposing it before the coordinator.
            let acquisition =
                native::qualify_probe_cleanup_owner(child, &"a".repeat(64), |owner| {
                    assert!(owner.alive.get());
                    owner.events.borrow_mut().push("measure image SHA256");
                    let measured_image = if read_failure {
                        images.path().join("unavailable-probe.exe")
                    } else {
                        image.clone()
                    };
                    super::super::sha256_file(&measured_image).map_err(|error| error.to_string())
                });
            let (cleanup, primary) = match acquisition {
                Err(rejected) => rejected,
                Ok(_) => panic!("wrong image must not acquire a lease"),
            };
            assert!(primary.contains(if read_failure {
                "measure owned probe image SHA256"
            } else {
                "different application image SHA256"
            }));
            assert!(
                alive.get() && cleanup_calls.get() == 0,
                "acquisition rejection must retain its cleanup owner"
            );
            let (runner_exit, app_cleanup) = cleanup_unleased_runner(
                Some(&cleanup),
                |child| child.cleanup(false),
                |action| {
                    assert_eq!(action, UnleasedRunnerRetirement::AfterHeldChildCleanup);
                    assert!(!alive.get());
                    assert_eq!(cleanup_calls.get(), 1);
                    events
                        .borrow_mut()
                        .push("terminate only-owned inner runner");
                    Ok(ObservedSubprocessExit {
                        process_id: 77,
                        status: std::process::ExitStatus::from_raw(1),
                        observed_unix_ms: Some(10_900),
                        origin: SubprocessExitOrigin::OuterTermination,
                    })
                },
            );
            assert_eq!(app_cleanup, Some(Ok(())));
            assert_eq!(
                *events.borrow(),
                [
                    "measure image SHA256",
                    "child cleanup attempted",
                    "child disposed",
                    "terminate only-owned inner runner"
                ]
            );
            let (mut report, _, _, mut attempt) = attempt_fixture();
            attempt.process = SubprocessState::Running { process_id: 77 };
            attempt.live_owner = None;
            record_observed_exit(&mut attempt, runner_exit.as_ref().unwrap()).unwrap();
            let recovery = recover_after_observed_runner_exit(77, &runner_exit, || Ok(()));
            let failure =
                controlled_subprocess_failure(&primary, &runner_exit, &recovery, &app_cleanup);
            let retained = retain_subprocess_run(
                &mut report,
                finish_subprocess_run(attempt, Err(failure), Some(12_000)),
            )
            .unwrap_err();
            assert!(retained.contains(&primary));
            parse_attempt_evidence(
                &serde_json::to_vec(report.controlled_failures.as_ref().unwrap()).unwrap(),
                &report,
            )
            .unwrap();
            assert!(!report.passed());
            assert!(validate_report_evidence(&report).is_err());
            let Some(Evidence::Aggregate { probes, attempts }) = &report.controlled_failures else {
                panic!("aggregate")
            };
            assert!(probes.is_empty() && attempts.len() == 1 && attempts[0].live_owner.is_none());
            assert!(matches!(
                attempts[0].process,
                SubprocessState::ExitObserved {
                    origin: SubprocessExitOrigin::OuterTermination,
                    ..
                }
            ));
            drop(cleanup);
            assert_eq!(
                cleanup_calls.get(),
                1,
                "disposed child has no synthetic extra cleanup"
            );
        }
        let child =
            PreleaseChildFixture::new(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
        let alive = child.alive.clone();
        let calls = child.cleanup_calls.clone();
        let expected = super::super::sha256_file(&image).unwrap();
        let admitted = native::qualify_probe_cleanup_owner(child, &expected, |_| {
            super::super::sha256_file(&image).map_err(|error| error.to_string())
        });
        let cleanup = match admitted {
            Ok(cleanup) => cleanup,
            Err(_) => panic!("exact owned image must retain its admitted handle"),
        };
        assert!(alive.get());
        assert_eq!(
            calls.get(),
            0,
            "qualification itself does not dispose the actual child"
        );
        drop(cleanup);
        assert!(!alive.get());
        assert_eq!(calls.get(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn controlled_foreign_birth_or_image_waits_for_inner_cleanup_without_terminating_foreign_child()
    {
        use std::os::windows::process::ExitStatusExt;
        let files = tempfile::tempdir().unwrap();
        let expected = files.path().join("expected.exe");
        let foreign = files.path().join("foreign.exe");
        fs::write(&expected, b"expected image").unwrap();
        fs::write(&foreign, b"foreign image").unwrap();
        native::validate_probe_cleanup_identity(11, 11, &expected, &expected).unwrap();
        for (actual_filetime, path) in [(12, &expected), (11, &foreign), (0, &expected)] {
            assert!(
                native::validate_probe_cleanup_identity(11, actual_filetime, &expected, path)
                    .is_err()
            );
            let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let mut inner_child = Some(PreleaseChildFixture::new(events.clone()));
            let inner_alive = inner_child.as_ref().unwrap().alive.clone();
            let inner_cleanup_calls = inner_child.as_ref().unwrap().cleanup_calls.clone();
            let foreign_child =
                PreleaseChildFixture::new(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
            let inspected = std::cell::Cell::new(0);
            let (runner_exit, app_cleanup) = cleanup_unleased_runner::<PreleaseChildFixture>(
                None,
                |_| panic!("no trusted outer handle may retire a foreign/reused child"),
                |action| {
                    assert_eq!(action, UnleasedRunnerRetirement::AwaitInnerCleanup);
                    await_inner_runner_cleanup(
                        77,
                        true,
                        || {
                            inspected.set(inspected.get() + 1);
                            if inspected.get() == 1 {
                                assert!(inner_alive.get());
                                return Ok(None);
                            }
                            drop(inner_child.take());
                            assert!(!inner_alive.get());
                            assert_eq!(inner_cleanup_calls.get(), 1);
                            events
                                .borrow_mut()
                                .push("observe inner runner exit after NativeChild cleanup");
                            Ok(Some(ObservedSubprocessExit {
                                process_id: 77,
                                status: std::process::ExitStatus::from_raw(1),
                                observed_unix_ms: Some(10_900),
                                origin: SubprocessExitOrigin::ObservedBeforeTermination,
                            }))
                        },
                        || false,
                        || events.borrow_mut().push("bounded inner cleanup poll"),
                    )
                },
            );
            assert!(runner_exit.is_ok());
            assert_eq!(app_cleanup, None);
            assert_eq!(inspected.get(), 2);
            assert_eq!(
                *events.borrow(),
                [
                    "bounded inner cleanup poll",
                    "inner NativeChild owner drop",
                    "child cleanup attempted",
                    "child disposed",
                    "observe inner runner exit after NativeChild cleanup"
                ]
            );
            assert!(foreign_child.alive.get());
            assert_eq!(foreign_child.cleanup_calls.get(), 0);
        }
        assert!(native::validate_probe_cleanup_identity(0, 0, &expected, &expected).is_err());
        assert!(
            native::validate_probe_cleanup_identity(
                11,
                11,
                &expected,
                &files.path().join("unavailable.exe")
            )
            .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    fn controlled_prelease_child_cleanup_error_preserves_inner_owner_until_its_cleanup_completes() {
        use std::os::windows::process::ExitStatusExt;
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let child = PreleaseChildFixture::new(events.clone());
        let alive = child.alive.clone();
        let calls = child.cleanup_calls.clone();
        let (runner_exit, application) = cleanup_unleased_runner(
            Some(&child),
            |child| child.cleanup(true),
            |action| {
                assert_eq!(
                    action,
                    UnleasedRunnerRetirement::AwaitInnerCleanup,
                    "failed actual disposal cannot destroy the inner NativeChild owner"
                );
                assert!(alive.get());
                assert_eq!(calls.get(), 1);
                events.borrow_mut().push("await still-owning inner runner");
                let inspections = std::cell::Cell::new(0);
                await_inner_runner_cleanup(
                    77,
                    true,
                    || {
                        inspections.set(inspections.get() + 1);
                        if inspections.get() == 1 {
                            assert!(alive.get());
                            return Ok(None);
                        }
                        child.cleanup(false)?;
                        Ok(Some(ObservedSubprocessExit {
                            process_id: 77,
                            status: std::process::ExitStatus::from_raw(1),
                            observed_unix_ms: Some(10_900),
                            origin: SubprocessExitOrigin::ObservedBeforeTermination,
                        }))
                    },
                    || false,
                    || events.borrow_mut().push("bounded inner cleanup poll"),
                )
            },
        );
        assert_eq!(
            application,
            Some(Err("actual child termination rejected".into()))
        );
        assert_eq!(
            *events.borrow(),
            [
                "child cleanup attempted",
                "await still-owning inner runner",
                "bounded inner cleanup poll",
                "child cleanup attempted",
                "child disposed"
            ]
        );
        assert_eq!(calls.get(), 2);
        assert!(!alive.get());
        let (mut report, _, _, mut attempt) = attempt_fixture();
        attempt.process = SubprocessState::Running { process_id: 77 };
        attempt.live_owner = None;
        attempt
            .cleanup_errors
            .push("application cleanup: actual child termination rejected".into());
        record_observed_exit(&mut attempt, runner_exit.as_ref().unwrap()).unwrap();
        let recovery = recover_after_observed_runner_exit(77, &runner_exit, || Ok(()));
        let failure = controlled_subprocess_failure(
            "original lease acquisition rejection",
            &runner_exit,
            &recovery,
            &application,
        );
        let primary = retain_subprocess_run(
            &mut report,
            finish_subprocess_run(attempt, Err(failure), Some(12_000)),
        )
        .unwrap_err();
        assert!(
            primary.contains("original lease acquisition rejection")
                && primary.contains("actual child termination rejected")
        );
        assert!(!report.passed());
        parse_attempt_evidence(
            &serde_json::to_vec(report.controlled_failures.as_ref().unwrap()).unwrap(),
            &report,
        )
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn controlled_unleased_cleanup_timeout_retains_owner_without_exit_claim_or_key_release() {
        for (known_created, already_expired) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let inner_child = known_created.then(|| PreleaseChildFixture::new(events.clone()));
            let inspections = std::cell::Cell::new(0);
            let polls = std::cell::Cell::new(0);
            let (runner_exit, application) = cleanup_unleased_runner::<PreleaseChildFixture>(
                None,
                |_| panic!("no acquired outer child owner"),
                |action| {
                    assert_eq!(action, UnleasedRunnerRetirement::AwaitInnerCleanup);
                    await_inner_runner_cleanup(
                        77,
                        known_created,
                        || {
                            inspections.set(inspections.get() + 1);
                            Ok(None)
                        },
                        || already_expired || inspections.get() >= 2,
                        || polls.set(polls.get() + 1),
                    )
                },
            );
            assert_eq!(inspections.get(), if already_expired { 1 } else { 2 });
            assert_eq!(polls.get(), if already_expired { 0 } else { 1 });
            assert_eq!(application, None);
            let error = runner_exit.as_ref().unwrap_err();
            assert!(error.contains(if known_created {
                "known created child"
            } else {
                "before validated creation"
            }));
            if let Some(child) = &inner_child {
                assert!(child.alive.get());
                assert_eq!(child.cleanup_calls.get(), 0);
            }
            let releases = std::cell::Cell::new(0);
            let recovery = recover_after_observed_runner_exit(77, &runner_exit, || {
                releases.set(releases.get() + 1);
                Ok(())
            });
            assert_eq!(recovery, EmergencyKeyRecovery::OwnershipRetained);
            assert_eq!(releases.get(), 0);
            let (mut report, _, _, mut attempt) = attempt_fixture();
            attempt.process = SubprocessState::Running { process_id: 77 };
            attempt.created_owner = None;
            attempt.live_owner = None;
            attempt.cleanup_errors.push(bounded_text(
                &format!("runner cleanup: {error}"),
                MAX_RESULT_BYTES,
            ));
            let failure = controlled_subprocess_failure(
                "original unleased admission failure",
                &runner_exit,
                &recovery,
                &application,
            );
            retain_subprocess_run(
                &mut report,
                finish_subprocess_run(attempt, Err(failure), Some(12_000)),
            )
            .unwrap_err();
            let Some(Evidence::Aggregate { probes, attempts }) = &report.controlled_failures else {
                panic!("aggregate")
            };
            assert!(probes.is_empty());
            assert!(matches!(
                attempts[0].process,
                SubprocessState::Running { process_id: 77 }
            ));
            assert!(!report.passed());
            parse_attempt_evidence(
                &serde_json::to_vec(report.controlled_failures.as_ref().unwrap()).unwrap(),
                &report,
            )
            .unwrap();
            assert!(
                events.borrow().is_empty(),
                "the timeout must not discard the sole child owner"
            );
        }
    }

    fn parse_attempt_evidence(bytes: &[u8], outer: &AcceptanceReport) -> Result<Evidence, String> {
        require(
            !bytes.is_empty() && bytes.len() <= MAX_CONTROLLED_BYTES,
            "attempt evidence exceeds its fixed wire bound",
        )?;
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let evidence: Evidence =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        require(
            serde_json::to_value(&evidence).map_err(|error| error.to_string())? == value,
            "attempt wire omitted typed facts or changed their canonical representation",
        )?;
        let Evidence::Aggregate { probes, attempts } = &evidence else {
            return Err("attempt evidence is not an aggregate".into());
        };
        let mut model = outer.clone();
        model.controlled_failures = Some(evidence.clone());
        require(
            persistence_cap_failure(&model).is_none(),
            "attempt readback exceeds its existing persistence owners",
        )?;
        validate_attempt_inventory(outer, attempts, probes)?;
        Ok(evidence)
    }

    #[cfg(windows)]
    #[test]
    fn controlled_failed_readback_retains_actual_completion_command_and_owners_without_stage_proof()
    {
        use std::os::windows::process::ExitStatusExt;
        for failure in [
            "wrong stage",
            "missing report",
            "malformed report",
            "tampered source",
        ] {
            let (mut outer, inner, proof, mut attempt) = attempt_fixture();
            attempt.process = SubprocessState::Running { process_id: 77 };
            attempt.finished_unix_ms = None;
            attempt.primary_error = None;
            let exit = ObservedSubprocessExit {
                process_id: 77,
                status: std::process::ExitStatus::from_raw(1),
                observed_unix_ms: Some(10_900),
                origin: SubprocessExitOrigin::RunnerCompletion,
            };
            let directory = tempfile::tempdir().unwrap();
            let (created, owner) = owners(&inner);
            let actual = expected(&inner, &created, &owner, &proof.command);
            let result = complete_observed_subprocess(&mut attempt, &exit, |attempt| {
                assert!(
                    matches!(
                        attempt.process,
                        SubprocessState::ExitObserved {
                            process_id: 77,
                            observed_unix_ms: Some(10_900),
                            ..
                        }
                    ),
                    "observation must be retained before verification"
                );
                match failure {
                    "missing report" => read_regular_bounded(
                        &directory.path().join("missing-report.json"),
                        MAX_JSON_REPORT_BYTES,
                    )
                    .map(|_| proof.clone()),
                    "malformed report" => {
                        parse_persisted(b"{malformed", &actual).map(|_| proof.clone())
                    }
                    _ => {
                        let mut changed = inner.clone();
                        if failure == "wrong stage" {
                            changed.cases[0].failure_stage = Some(FailureStage::DesignerWidget);
                        } else {
                            changed.environment.source_revision = Some("foreign-revision".into());
                        }
                        parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual)
                            .map(|_| proof.clone())
                    }
                }
                .map_err(|primary| format!("{failure}: {primary}"))
            });
            assert!(
                result.is_err(),
                "{failure} must exercise a real readback rejection"
            );
            let run = finish_subprocess_run(attempt, result, Some(12_000));
            let primary = retain_subprocess_run(&mut outer, run).unwrap_err();
            outer.cases[0].observed = primary.clone();
            assert!(primary.contains(failure));
            let Evidence::Aggregate { probes, attempts } = parse_attempt_evidence(
                &serde_json::to_vec(outer.controlled_failures.as_ref().unwrap()).unwrap(),
                &outer,
            )
            .unwrap() else {
                panic!("aggregate")
            };
            assert!(probes.is_empty());
            assert_eq!(attempts.len(), 1);
            let retained = &attempts[0];
            assert_eq!(retained.command.as_ref(), Some(&proof.command));
            assert_eq!(retained.started_unix_ms, Some(900));
            assert_eq!(retained.finished_unix_ms, Some(12_000));
            assert_eq!(retained.source_revision, outer.environment.source_revision);
            assert_eq!(retained.runner_sha256, outer.environment.runner_sha256);
            assert_eq!(retained.candidate_sha256, outer.candidate.sha256);
            assert_eq!(retained.created_owner.as_ref(), Some(&created));
            assert_eq!(retained.live_owner.as_ref().unwrap().owner, owner);
            assert_eq!(retained.primary_error.as_deref(), Some(primary.as_str()));
            assert_eq!(
                retained.process,
                SubprocessState::ExitObserved {
                    process_id: 77,
                    observed_unix_ms: Some(10_900),
                    status: SubprocessStatus {
                        code: Some(1),
                        success: false,
                        windows_status: Some(1)
                    },
                    origin: SubprocessExitOrigin::RunnerCompletion
                }
            );
            assert!(!outer.passed());
            assert!(validate_report_evidence(&outer).is_err());
            assert!(
                outer
                    .cases
                    .iter()
                    .all(|case| case.status == CaseStatus::Failed)
            );
            assert!(
                attempts
                    .iter()
                    .all(|attempt| attempt.kind == ProbeKind::Startup),
                "unrun N02-N06 have no fabricated receipts"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_early_post_exit_cleanup_rejection_retains_completion_primary_and_independent_error()
     {
        use std::os::windows::process::ExitStatusExt;
        let (mut outer, _, proof, mut attempt) = attempt_fixture();
        attempt.process = SubprocessState::Running { process_id: 77 };
        attempt.finished_unix_ms = None;
        let exit = ObservedSubprocessExit {
            process_id: 77,
            status: std::process::ExitStatus::from_raw(1),
            observed_unix_ms: Some(10_900),
            origin: SubprocessExitOrigin::RunnerCompletion,
        };
        let result = complete_observed_subprocess(&mut attempt, &exit, |attempt| {
            assert!(matches!(
                attempt.process,
                SubprocessState::ExitObserved { process_id: 77, .. }
            ));
            attempt
                .cleanup_errors
                .push("owned key recovery: actual release acknowledgement absent".into());
            Err("actual release acknowledgement absent; application cleanup=Err(owned HWND remained)".into())
        });
        attempt
            .cleanup_errors
            .push("application cleanup: owned HWND remained".into());
        let primary = retain_subprocess_run(
            &mut outer,
            finish_subprocess_run(attempt, result, Some(12_000)),
        )
        .unwrap_err();
        outer.cases[0].observed = primary;
        let bytes = serde_json::to_vec(outer.controlled_failures.as_ref().unwrap()).unwrap();
        let Evidence::Aggregate { probes, attempts } =
            parse_attempt_evidence(&bytes, &outer).unwrap()
        else {
            panic!("aggregate")
        };
        assert!(probes.is_empty());
        assert!(
            attempts[0]
                .primary_error
                .as_ref()
                .unwrap()
                .contains("actual release acknowledgement absent")
        );
        assert_eq!(
            attempts[0].cleanup_errors,
            [
                "owned key recovery: actual release acknowledgement absent",
                "application cleanup: owned HWND remained"
            ]
        );
        assert_eq!(attempts[0].command.as_ref(), Some(&proof.command));
        assert!(matches!(
            attempts[0].process,
            SubprocessState::ExitObserved {
                observed_unix_ms: Some(10_900),
                status: SubprocessStatus {
                    code: Some(1),
                    windows_status: Some(1),
                    ..
                },
                ..
            }
        ));
        assert!(!outer.passed() && validate_report_evidence(&outer).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn controlled_completion_requires_started_exact_runner_and_records_before_verifier_failure() {
        use std::os::windows::process::ExitStatusExt;
        let (_, _, _, baseline) = attempt_fixture();
        for state in [
            SubprocessState::NotStarted,
            SubprocessState::Running { process_id: 78 },
        ] {
            let mut attempt = baseline.clone();
            attempt.process = state.clone();
            let called = std::cell::Cell::new(0);
            let exit = ObservedSubprocessExit {
                process_id: 77,
                status: std::process::ExitStatus::from_raw(1),
                observed_unix_ms: Some(10_900),
                origin: SubprocessExitOrigin::RunnerCompletion,
            };
            assert!(
                complete_observed_subprocess(&mut attempt, &exit, |_| {
                    called.set(called.get() + 1);
                    Err("must not verify foreign completion".into())
                })
                .is_err()
            );
            assert_eq!(called.get(), 0);
            assert_eq!(attempt.process, state);
        }
        for (raw, origin) in [
            (0, SubprocessExitOrigin::RunnerCompletion),
            (1, SubprocessExitOrigin::ObservedBeforeTermination),
            (0x80000000, SubprocessExitOrigin::RunnerCompletion),
            (0xc000013a, SubprocessExitOrigin::OuterTermination),
            (u32::MAX, SubprocessExitOrigin::OuterTermination),
        ] {
            let mut attempt = baseline.clone();
            attempt.process = SubprocessState::Running { process_id: 77 };
            let exit = ObservedSubprocessExit {
                process_id: 77,
                status: std::process::ExitStatus::from_raw(raw),
                observed_unix_ms: Some(10_900),
                origin,
            };
            let result = complete_observed_subprocess(&mut attempt, &exit, |attempt| {
                assert_eq!(
                    attempt.process,
                    SubprocessState::ExitObserved {
                        process_id: 77,
                        observed_unix_ms: Some(10_900),
                        status: SubprocessStatus {
                            code: Some(raw as i32),
                            success: raw == 0,
                            windows_status: Some(raw)
                        },
                        origin
                    }
                );
                Err("observed status does not certify a verified native stage".into())
            });
            assert!(result.is_err());
        }
    }

    #[test]
    fn controlled_attempt_wire_keeps_not_started_running_unknown_and_zero_exit_unqualified() {
        let (outer, _, proof, baseline) = attempt_fixture();
        let mut unstarted = baseline.clone();
        unstarted.process = SubprocessState::NotStarted;
        unstarted.command = None;
        unstarted.created_owner = None;
        unstarted.live_owner = None;
        unstarted.started_unix_ms = None;
        let mut running = baseline.clone();
        running.process = SubprocessState::Running { process_id: 77 };
        running.cleanup_errors = vec!["runner cleanup: owned kill failed; no observed exit".into()];
        let mut unknown = baseline.clone();
        unknown.process = SubprocessState::ExitObserved {
            process_id: 77,
            observed_unix_ms: None,
            status: SubprocessStatus {
                code: None,
                success: false,
                windows_status: None,
            },
            origin: SubprocessExitOrigin::RunnerCompletion,
        };
        let mut zero = baseline.clone();
        zero.process = SubprocessState::ExitObserved {
            process_id: 77,
            observed_unix_ms: Some(10_999),
            status: SubprocessStatus {
                code: Some(0),
                success: true,
                windows_status: Some(0),
            },
            origin: SubprocessExitOrigin::RunnerCompletion,
        };
        let mut terminated = baseline.clone();
        if let SubprocessState::ExitObserved { origin, .. } = &mut terminated.process {
            *origin = SubprocessExitOrigin::OuterTermination;
        }
        for attempt in [unstarted, running, unknown, zero, terminated] {
            let evidence = Evidence::Aggregate {
                probes: Vec::new(),
                attempts: vec![attempt.clone()],
            };
            let bytes = serde_json::to_vec(&evidence).unwrap();
            let decoded = parse_attempt_evidence(&bytes, &outer).unwrap();
            assert_eq!(
                serde_json::to_value(&decoded).unwrap(),
                serde_json::to_value(&evidence).unwrap()
            );
            let mut report = outer.clone();
            report.controlled_failures = Some(decoded);
            assert!(!report.passed() && validate_report_evidence(&report).is_err());
            assert!(
                validate_attempt_inventory(&outer, &[attempt], &[proof.clone()]).is_err(),
                "diagnostic completion must not be coerced to nonzero verified success"
            );
        }
        let mut unknown_clock = baseline;
        unknown_clock.finished_unix_ms = None;
        let run = finish_subprocess_run(unknown_clock, Ok(proof), None);
        assert!(run.result.is_err());
        assert_eq!(run.attempt.finished_unix_ms, None);
        assert_eq!(
            run.attempt.cleanup_errors,
            ["outer completion UTC observation unavailable"]
        );
        assert!(
            run.attempt
                .primary_error
                .as_ref()
                .unwrap()
                .contains("UTC observation unavailable")
        );
    }

    #[test]
    fn controlled_attempt_readback_rejects_conflicting_source_command_owner_clock_order_and_status()
    {
        let (outer, _, _, attempt) = attempt_fixture();
        let original = Evidence::Aggregate {
            probes: Vec::new(),
            attempts: vec![attempt],
        };
        let value = serde_json::to_value(&original).unwrap();
        parse_attempt_evidence(&serde_json::to_vec(&value).unwrap(), &outer).unwrap();
        let mutations: Vec<(&str, serde_json::Value)> = vec![
            ("/attempts/0/kind", serde_json::json!("query")),
            ("/attempts/0/nonce", serde_json::json!("foreign-startup")),
            (
                "/attempts/0/source_revision",
                serde_json::json!("foreign-revision"),
            ),
            (
                "/attempts/0/runner_sha256",
                serde_json::json!("f".repeat(64)),
            ),
            (
                "/attempts/0/candidate_sha256",
                serde_json::json!("f".repeat(64)),
            ),
            (
                "/attempts/0/candidate_executable",
                serde_json::json!("foreign-candidate.exe"),
            ),
            (
                "/attempts/0/command/0",
                serde_json::json!(std::env::temp_dir().join("foreign-runner.exe")),
            ),
            ("/attempts/0/command/1", serde_json::json!("--run-native")),
            ("/attempts/0/command/2", serde_json::json!("query")),
            (
                "/attempts/0/command/4",
                serde_json::json!("foreign-startup"),
            ),
            (
                "/attempts/0/command/6",
                serde_json::json!("foreign-candidate.exe"),
            ),
            (
                "/attempts/0/command/8",
                serde_json::json!(std::env::temp_dir().join("foreign-output")),
            ),
            (
                "/attempts/0/command/10",
                serde_json::json!("foreign-revision"),
            ),
            ("/attempts/0/process/process_id", serde_json::json!(0)),
            ("/attempts/0/started_unix_ms", serde_json::json!(799)),
            ("/attempts/0/finished_unix_ms", serde_json::json!(800)),
            ("/attempts/0/finished_unix_ms", serde_json::json!(20001)),
            (
                "/attempts/0/process/observed_unix_ms",
                serde_json::json!(899),
            ),
            (
                "/attempts/0/process/observed_unix_ms",
                serde_json::json!(11001),
            ),
            ("/attempts/0/process/status/code", serde_json::json!(0)),
            (
                "/attempts/0/process/status/success",
                serde_json::json!(true),
            ),
            (
                "/attempts/0/process/status/windows_status",
                serde_json::json!(2),
            ),
            (
                "/attempts/0/created_owner/runner_pid",
                serde_json::json!(78),
            ),
            (
                "/attempts/0/created_owner/child_started_unix_ms",
                serde_json::json!(11001),
            ),
            (
                "/attempts/0/created_owner/process_created_filetime",
                serde_json::json!(0),
            ),
            (
                "/attempts/0/live_owner/owner/process_created_filetime",
                serde_json::json!(12),
            ),
            (
                "/attempts/0/live_owner/owner/observed_unix_ms",
                serde_json::json!(11001),
            ),
            (
                "/attempts/0/live_owner/owner/input_desktop",
                serde_json::json!("foreign"),
            ),
            ("/attempts/0/primary_error", serde_json::Value::Null),
        ];
        for (path, changed) in mutations {
            let mut negative = value.clone();
            *negative.pointer_mut(path).unwrap() = changed;
            assert!(
                parse_attempt_evidence(&serde_json::to_vec(&negative).unwrap(), &outer).is_err(),
                "{path}"
            );
        }
        let Evidence::Aggregate { attempts, .. } = original else {
            panic!("aggregate")
        };
        let mut later = attempts[0].clone();
        later.kind = ProbeKind::InputTimeout;
        let admitted_after_failure = Evidence::Aggregate {
            probes: Vec::new(),
            attempts: vec![attempts[0].clone(), later],
        };
        assert!(
            parse_attempt_evidence(
                &serde_json::to_vec(&admitted_after_failure).unwrap(),
                &outer
            )
            .is_err()
        );
        let mut early_owner = attempts[0].clone();
        early_owner.process = SubprocessState::NotStarted;
        assert!(validate_attempt_inventory(&outer, &[early_owner], &[]).is_err());
        let mut unbounded = attempts[0].clone();
        unbounded.finished_unix_ms = Some(106_001);
        let mut later_outer = outer;
        later_outer.finished_unix_ms = 120_000;
        assert!(validate_attempt_inventory(&later_outer, &[unbounded], &[]).is_err());
    }

    #[test]
    fn controlled_attempt_readback_requires_explicit_nullable_fields_and_checked_numeric_wire() {
        let (outer, _, _, attempt) = attempt_fixture();
        let evidence = Evidence::Aggregate {
            probes: Vec::new(),
            attempts: vec![attempt.clone()],
        };
        let value = serde_json::to_value(&evidence).unwrap();
        for path in [
            "/attempts/0/source_revision",
            "/attempts/0/runner_sha256",
            "/attempts/0/command",
            "/attempts/0/created_owner",
            "/attempts/0/created_owner/profile_hashes/settings_full",
            "/attempts/0/live_owner",
            "/attempts/0/primary_error",
            "/attempts/0/started_unix_ms",
            "/attempts/0/finished_unix_ms",
            "/attempts/0/process/observed_unix_ms",
            "/attempts/0/process/status/code",
            "/attempts/0/process/status/windows_status",
        ] {
            let mut missing = value.clone();
            let (parent, field) = path.rsplit_once('/').unwrap();
            missing
                .pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<Evidence>(missing.clone()).is_err(),
                "missing field {path} must not silently default"
            );
            assert!(
                parse_attempt_evidence(&serde_json::to_vec(&missing).unwrap(), &outer).is_err()
            );
        }
        for path in [
            "/attempts/0",
            "/attempts/0/process",
            "/attempts/0/process/status",
        ] {
            let mut unknown = value.clone();
            unknown
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("fabricated_fact".into(), serde_json::json!(true));
            assert!(serde_json::from_value::<Evidence>(unknown).is_err());
        }
        for path in [
            "/attempts/0/started_unix_ms",
            "/attempts/0/finished_unix_ms",
            "/attempts/0/process/observed_unix_ms",
        ] {
            for invalid in ["-1", "1.5", "18446744073709551616", "\"10999\""] {
                let mut changed = value.clone();
                *changed.pointer_mut(path).unwrap() = serde_json::from_str(invalid).unwrap();
                assert!(
                    serde_json::from_value::<Evidence>(changed).is_err(),
                    "{path} accepted {invalid}"
                );
            }
        }
        let mut overflow = attempt;
        overflow.finished_unix_ms = Some(u128::from(u64::MAX) + 1);
        assert!(
            serde_json::to_vec(&Evidence::Aggregate {
                probes: Vec::new(),
                attempts: vec![overflow]
            })
            .is_err()
        );
        let mut unknown = value;
        unknown["attempts"][0]["process"]["origin"] = serde_json::json!("fabricated_exit");
        assert!(serde_json::from_value::<Evidence>(unknown).is_err());
    }

    #[test]
    fn controlled_attempt_proof_binding_requires_the_exact_natural_completion_and_all_six_readbacks()
     {
        let (mut outer, _, proof, attempt) = attempt_fixture();
        let mut qualified = attempt.clone();
        qualified.primary_error = None;
        outer.cases[0].status = CaseStatus::Passed;
        outer.cases[0].failure_stage = None;
        validate_attempt_inventory(&outer, &[qualified.clone()], &[proof.clone()]).unwrap();
        let mutations: Vec<fn(&mut SubprocessAttempt)> = vec![
            |attempt| {
                if let SubprocessState::ExitObserved { origin, .. } = &mut attempt.process {
                    *origin = SubprocessExitOrigin::OuterTermination;
                }
            },
            |attempt| {
                if let SubprocessState::ExitObserved { origin, .. } = &mut attempt.process {
                    *origin = SubprocessExitOrigin::ObservedBeforeTermination;
                }
            },
            |attempt| attempt.primary_error = Some("verification rejected".into()),
            |attempt| {
                attempt
                    .cleanup_errors
                    .push("actual cleanup rejected".into())
            },
            |attempt| attempt.started_unix_ms = Some(901),
            |attempt| attempt.finished_unix_ms = None,
            |attempt| {
                if let SubprocessState::ExitObserved {
                    observed_unix_ms, ..
                } = &mut attempt.process
                {
                    *observed_unix_ms = None;
                }
            },
            |attempt| {
                if let SubprocessState::ExitObserved { status, .. } = &mut attempt.process {
                    *status = SubprocessStatus {
                        code: None,
                        success: false,
                        windows_status: None,
                    };
                }
            },
        ];
        for mutate in mutations {
            let mut changed = qualified.clone();
            mutate(&mut changed);
            assert!(validate_attempt_inventory(&outer, &[changed], &[proof.clone()]).is_err());
        }
        outer.controlled_failures = Some(Evidence::Aggregate {
            probes: vec![proof],
            attempts: vec![qualified],
        });
        assert!(!outer.passed());
        assert!(
            validate_report_evidence(&outer).is_err(),
            "one completed proof cannot qualify an all-six aggregate"
        );
    }

    #[test]
    fn controlled_attempt_inventory_accepts_only_six_chronological_pairs_and_exact_proof_binding() {
        let (mut outer, _, _, _) = attempt_fixture();
        outer.finished_unix_ms = 100_000;
        for case in &mut outer.cases {
            case.status = CaseStatus::Passed;
            case.failure_stage = None;
        }
        let Evidence::Aggregate { mut probes, .. } = complete_wire_evidence().pop().unwrap() else {
            panic!("aggregate")
        };
        for (index, proof) in probes.iter_mut().enumerate() {
            let offset = index as u128 * 12_000;
            proof.subprocess_pid = 77 + index as u32;
            proof.started_unix_ms = 900 + offset;
            proof.finished_unix_ms = 11_000 + offset;
            proof.created_owner.runner_pid = proof.subprocess_pid;
            proof.created_owner.child_pid = 7 + index as u32;
            proof.created_owner.child_started_unix_ms = 1100 + offset;
            proof.created_owner.profile_root = std::env::temp_dir()
                .join(format!(
                    "multi-launcher-radial-acceptance-unit-{}",
                    proof.kind.id()
                ))
                .to_string_lossy()
                .into_owned();
            proof.leased_owner.child_pid = proof.created_owner.child_pid;
            proof.leased_owner.foreground_pid = proof.created_owner.child_pid;
            proof.leased_owner.child_started_unix_ms = proof.created_owner.child_started_unix_ms;
            proof.leased_owner.observed_unix_ms = 1200 + offset;
            proof.receipt.owner = Some(proof.leased_owner.clone());
            proof.command[6] = outer.candidate.executable.clone();
            proof.command[8] = std::env::temp_dir()
                .join(proof.kind.id())
                .to_string_lossy()
                .into_owned();
            proof.report_path = Path::new(&proof.command[8])
                .join("report.json")
                .to_string_lossy()
                .into_owned();
        }
        let attempts = probes
            .iter()
            .map(|proof| attempt_for_proof(proof, &outer))
            .collect::<Vec<_>>();
        validate_attempt_inventory(&outer, &attempts, &probes).unwrap();
        let evidence = Evidence::Aggregate {
            probes: probes.clone(),
            attempts: attempts.clone(),
        };
        parse_attempt_evidence(&serde_json::to_vec(&evidence).unwrap(), &outer).unwrap();
        let mut reordered = attempts.clone();
        reordered.swap(1, 2);
        assert!(validate_attempt_inventory(&outer, &reordered, &probes).is_err());
        assert!(validate_attempt_inventory(&outer, &attempts[..5], &probes).is_err());
        let mut overlapping = attempts.clone();
        overlapping[1].started_unix_ms = Some(10_999);
        assert!(validate_attempt_inventory(&outer, &overlapping, &probes).is_err());
        let mut incomplete = attempts.clone();
        incomplete[0].finished_unix_ms = None;
        assert!(validate_attempt_inventory(&outer, &incomplete, &probes).is_err());
        let mut wrong_baseline = attempts.clone();
        wrong_baseline[0]
            .created_owner
            .as_mut()
            .unwrap()
            .profile_hashes
            .settings_full = "d".repeat(64);
        assert!(
            validate_attempt_inventory(&outer, &wrong_baseline, &probes).is_err(),
            "attempt baseline must exactly equal its verified proof"
        );
        let mut wrong_proof = probes.clone();
        wrong_proof[0].created_owner.profile_hashes.settings_full = "d".repeat(64);
        assert!(
            validate_attempt_inventory(&outer, &attempts, &wrong_proof).is_err(),
            "aggregate full baseline cannot substitute historical proof"
        );
        let mut changed = probes;
        changed[0].command[4] = "foreign-nonce".into();
        assert!(validate_attempt_inventory(&outer, &attempts, &changed).is_err());
        outer.controlled_failures = Some(evidence);
        assert!(validate_report_evidence(&outer).is_err());
        assert!(
            !outer.passed(),
            "typed pairs cannot replace actual persisted stage/cleanup readbacks"
        );
    }

    #[test]
    fn controlled_attempt_inventory_and_command_overflow_remain_bounded_explicit_failures() {
        let (outer, _, _, baseline) = attempt_fixture();
        let mut oversized_command = baseline.clone();
        oversized_command.command.as_mut().unwrap()[0] = "x".repeat(MAX_PATH_BYTES + 1);
        let mut extra_argument = baseline.clone();
        extra_argument
            .command
            .as_mut()
            .unwrap()
            .push("unowned-extra-argument".into());
        let mut oversized_receipt = baseline.clone();
        oversized_receipt.live_owner.as_mut().unwrap().profile_root = "x".repeat(MAX_PROBE_BYTES);
        let mut extra_errors = baseline.clone();
        extra_errors.cleanup_errors =
            vec!["cleanup rejected".into(); MAX_ATTEMPT_CLEANUP_ERRORS + 1];
        for attempts in [
            vec![baseline; ProbeKind::ALL.len() + 1],
            vec![oversized_command],
            vec![extra_argument],
            vec![oversized_receipt],
            vec![extra_errors],
        ] {
            let expected_omitted = attempts.len();
            let mut report = outer.clone();
            report.controlled_failures = Some(Evidence::Aggregate {
                probes: Vec::new(),
                attempts,
            });
            assert!(persistence_cap_failure(&report).is_some());
            bound_report_for_persistence(&mut report).unwrap();
            assert!(
                report.capacity_saturated
                    && report.controlled_failures.is_none()
                    && !report.passed()
            );
            assert_eq!(
                report
                    .report_overflow
                    .as_ref()
                    .unwrap()
                    .omitted_case_evidence,
                expected_omitted
            );
            assert_eq!(
                report
                    .cases
                    .iter()
                    .map(|case| case.id.as_str())
                    .collect::<Vec<_>>(),
                CASE_IDS
            );
            assert!(
                report
                    .cases
                    .iter()
                    .all(|case| case.status == CaseStatus::Failed)
            );
            assert!(serde_json::to_vec(&report).unwrap().len() <= MAX_JSON_REPORT_BYTES);
            assert!(render_text_report(&report).len() <= MAX_TEXT_REPORT_BYTES);
        }
    }

    #[test]
    fn controlled_cli_preserves_ordinary_defaults_and_exact_new_modes() {
        let ordinary =
            parse_arguments([OsString::from("--output"), OsString::from("ordinary")]).unwrap();
        let ParseResult::Run(ordinary) = ordinary else {
            panic!("ordinary run")
        };
        assert_eq!(ordinary.operation, Operation::Ordinary);
        assert_eq!(ordinary.suite, AcceptanceSuite::All);
        assert_eq!(ordinary.hotkey, AcceptanceHotkey::F11);
        assert!(ordinary.probe_nonce.is_none());
        for kind in ProbeKind::ALL {
            let args = [
                "--controlled-native-failure-probe",
                kind.argument(),
                "--controlled-probe-nonce",
                "unit-receipt",
                "--launcher",
                "candidate.exe",
                "--output",
                "probe",
                "--source-revision",
                "deadbeef",
            ];
            let ParseResult::Run(parsed) = parse_arguments(args.map(OsString::from)).unwrap()
            else {
                panic!("probe run")
            };
            assert_eq!(parsed.operation, Operation::Probe(kind));
            assert_eq!(parsed.probe_nonce.as_deref(), Some("unit-receipt"));
            assert_eq!(parsed.hotkey, AcceptanceHotkey::F11);
            assert_eq!(parsed.suite, AcceptanceSuite::All);
        }
        let ParseResult::Run(aggregate) = parse_arguments(
            [
                "--controlled-native-failures",
                "--launcher",
                "candidate.exe",
                "--output",
                "aggregate",
                "--source-revision",
                "deadbeef",
            ]
            .map(OsString::from),
        )
        .unwrap() else {
            panic!("aggregate")
        };
        assert_eq!(aggregate.operation, Operation::Aggregate);
        assert!(aggregate.profile_copy.is_none() && aggregate.probe_nonce.is_none());
        assert!(!aggregate.keep_profile_on_failure);
    }

    #[test]
    fn controlled_cli_rejects_incompatible_options_nonce_and_missing_identity() {
        let base = [
            "--controlled-native-failures",
            "--launcher",
            "candidate.exe",
            "--output",
            "aggregate",
            "--source-revision",
            "deadbeef",
        ];
        for extra in [
            vec!["--suite", "all"],
            vec!["--suite", "hotkey"],
            vec!["--hotkey", "f11"],
            vec!["--hotkey", "shift-alt-win-end"],
            vec!["--h6-repeat", "quiescent"],
            vec!["--mouse-gestures", "enabled"],
            vec!["--profile-copy", "active-profile"],
            vec!["--keep-profile-on-failure"],
            vec!["--controlled-probe-nonce", "unit"],
            vec!["--controlled-native-failures"],
            vec![
                "--controlled-native-failure-probe",
                "query",
                "--controlled-probe-nonce",
                "unit",
            ],
        ] {
            assert!(
                parse_arguments(base.iter().chain(extra.iter()).map(OsString::from)).is_err(),
                "{extra:?}"
            );
        }
        for args in [
            vec!["--controlled-native-failures", "--output", "aggregate"],
            vec![
                "--controlled-native-failures",
                "--launcher",
                "candidate.exe",
                "--output",
                "aggregate",
            ],
            vec![
                "--controlled-native-failure-probe",
                "query",
                "--launcher",
                "candidate.exe",
                "--output",
                "probe",
                "--source-revision",
                "deadbeef",
            ],
            vec![
                "--controlled-native-failure-probe",
                "unknown",
                "--controlled-probe-nonce",
                "unit",
                "--output",
                "probe",
            ],
            vec!["--controlled-probe-nonce", "unit", "--output", "ordinary"],
            vec![
                "--controlled-native-failure-probe",
                "query",
                "--controlled-probe-nonce",
                "UPPERCASE",
                "--output",
                "probe",
            ],
            vec![
                "--controlled-native-failure-probe",
                "query",
                "--controlled-probe-nonce",
                "a/b",
                "--output",
                "probe",
            ],
        ] {
            assert!(
                parse_arguments(args.iter().map(OsString::from)).is_err(),
                "{args:?}"
            );
        }
        assert!(validate_nonce(&"a".repeat(65)).is_err());
        assert!(validate_nonce("").is_err());
    }

    #[test]
    fn controlled_probe_stage_proofs_preserve_inner_failure_and_exact_inventory() {
        let ids = ProbeKind::ALL.map(ProbeKind::id);
        assert_eq!(&CASE_IDS[..6], &ids);
        for kind in ProbeKind::ALL {
            let report = probe_report(kind);
            let (created, owner) = owners(&report);
            let command = command(&report, Path::new("probe"));
            assert_eq!(case_inventory(&report), Some(kind.inventory().as_slice()));
            validate_probe(&report, true).unwrap();
            let readback = parse_persisted(
                &serde_json::to_vec(&report).unwrap(),
                &expected(&report, &created, &owner, &command),
            )
            .unwrap();
            assert_eq!(readback.cases[0].status, CaseStatus::Failed);
            assert_eq!(readback.cases[0].failure_stage, Some(kind.failure_stage()));
            assert!(
                !readback.passed(),
                "an expected failed inner run must still exit nonzero"
            );
            let mut falsely_passed = report.clone();
            for case in &mut falsely_passed.cases {
                case.status = CaseStatus::Passed;
                case.failure_stage = None;
            }
            assert!(!falsely_passed.passed());
            assert!(
                parse_persisted(
                    &serde_json::to_vec(&falsely_passed).unwrap(),
                    &expected(&report, &created, &owner, &command)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn controlled_stage_oracle_rejects_wrong_owner_kind_effects_and_cleanup() {
        let mutations: Vec<(&str, fn(&mut AcceptanceReport))> = vec![
            ("foreign PID", |r| r.environment.child_process_id = Some(8)),
            ("runner owns child PID", |r| {
                r.environment.runner_process_id = 7
            }),
            ("wrong foreground", |r| {
                receipt_mut(r)
                    .unwrap()
                    .owner
                    .as_mut()
                    .unwrap()
                    .foreground_hwnd = 99
            }),
            ("no native thread", |r| {
                receipt_mut(r)
                    .unwrap()
                    .owner
                    .as_mut()
                    .unwrap()
                    .foreground_thread_id = 0
            }),
            ("desktop shorthand", |r| {
                receipt_mut(r)
                    .unwrap()
                    .owner
                    .as_mut()
                    .unwrap()
                    .input_desktop = "Default".into()
            }),
            ("stale owner", |r| {
                receipt_mut(r)
                    .unwrap()
                    .owner
                    .as_mut()
                    .unwrap()
                    .observed_unix_ms = 900
            }),
            ("no owner", |r| receipt_mut(r).unwrap().owner = None),
            ("no native attempt", |r| {
                receipt_mut(r).unwrap().proof = None
            }),
            ("wrong kind", |r| {
                receipt_mut(r).unwrap().kind = ProbeKind::Query
            }),
            ("marker effect", |r| {
                receipt_mut(r).unwrap().marker_count_after = Some(1)
            }),
            ("extra dispatch", |r| {
                receipt_mut(r).unwrap().execution_count_after = Some(1)
            }),
            ("changed fixture", |r| {
                receipt_mut(r)
                    .unwrap()
                    .profile_after
                    .as_mut()
                    .unwrap()
                    .radial = "f".repeat(64)
            }),
            ("live child", |r| {
                receipt_mut(r).unwrap().child_exit_code = None
            }),
            ("owned key remains", |r| {
                receipt_mut(r).unwrap().owned_keys_after = Some(1)
            }),
            ("failed cleanup", |r| {
                receipt_mut(r)
                    .unwrap()
                    .cleanup_errors
                    .push("actual cleanup failed".into())
            }),
            ("live HWND", |r| {
                r.cleanup.child_owned_windows_closed = false
            }),
            ("profile remains", |r| r.cleanup.profile_removed = false),
            ("desktop handle remains", |r| {
                r.cleanup.input_desktop_released = false
            }),
            ("cursor not restored", |r| r.cleanup.cursor_restored = false),
            ("foreground not restored", |r| {
                r.cleanup.foreground_restored = false
            }),
        ];
        for (label, mutate) in mutations {
            let mut report = probe_report(ProbeKind::Startup);
            mutate(&mut report);
            assert!(validate_probe(&report, true).is_err(), "{label}");
        }
        for kind in [
            ProbeKind::Query,
            ProbeKind::Ui,
            ProbeKind::DesktopMismatch,
            ProbeKind::ChildExit,
        ] {
            let mut report = probe_report(kind);
            match receipt_mut(&mut report).unwrap().proof.as_mut().unwrap() {
                StageProof::Query { invocation, .. } => {
                    invocation.mode = QueryEvidenceMode::ExecuteFirst
                }
                StageProof::Ui { designer_hwnd, .. } => *designer_hwnd = 44,
                StageProof::DesktopMismatch { admission } => {
                    admission.refused_before_send_input = false
                }
                StageProof::ChildExit {
                    input_inserted_after_exit,
                    ..
                } => *input_inserted_after_exit = 1,
                _ => unreachable!(),
            }
            assert!(validate_probe(&report, true).is_err(), "{kind:?}");
        }
    }

    #[test]
    fn controlled_held_key_proof_preserves_raw_async_edges_and_requires_retirement() {
        let report = probe_report(ProbeKind::InputTimeout);
        validate_probe(&report, true).unwrap();
        let mut pending_async = report.clone();
        if let StageProof::InputTimeout { key } = receipt_mut(&mut pending_async)
            .unwrap()
            .proof
            .as_mut()
            .unwrap()
        {
            key.down.async_after = 0;
            key.up.as_mut().unwrap().async_after = 0x8000;
            assert!(owned_shift_down(&key.down));
        }
        // SendInput queues an edge. The raw immediate sample is retained; the
        // observed up plus measured final async/ownership retirement is decisive.
        validate_probe(&pending_async, true).unwrap();
        let mutations: Vec<(&str, fn(&mut HeldKeyProof))> = vec![
            ("no insertion", |k| k.down.inserted = 0),
            ("wrong key", |k| k.down.vk = 0xa1),
            ("wrong scan", |k| k.down.scan = 7),
            ("wrong down flags", |k| k.down.flags = 2),
            ("wrong cookie", |k| k.down.cookie = 0),
            ("foreign HWND", |k| k.down.foreground_hwnd = 99),
            ("already held", |k| k.down.async_before = 0x8000),
            ("invalid async bits", |k| k.down.async_after = 2),
            ("timeout was released", |k| k.timeout_async_state = 0),
            ("no real deadline", |k| k.timeout_elapsed_ms = 249),
            ("missing up", |k| k.up = None),
            ("wrong up flags", |k| k.up.as_mut().unwrap().flags = 0),
            ("wrong up scan", |k| k.up.as_mut().unwrap().scan = 7),
            ("foreign release HWND", |k| {
                k.up.as_mut().unwrap().foreground_hwnd = 99
            }),
            ("foreign release PID", |k| {
                k.up.as_mut().unwrap().foreground_pid = 8
            }),
            ("foreign release desktop", |k| {
                k.up.as_mut().unwrap().input_desktop = "thread=Other;active=Other".into()
            }),
            ("foreign release cookie", |k| {
                k.up.as_mut().unwrap().cookie = 0
            }),
            ("up preceded timeout", |k| {
                k.up.as_mut().unwrap().at_unix_ms = 1400
            }),
            ("still held after cleanup", |k| {
                k.async_after_cleanup = 0x8000
            }),
            ("no hook down", |k| {
                k.observed_edges.remove(0);
            }),
            ("unowned observed edge", |k| k.observed_edges[0].cookie = 0),
            ("not injected", |k| k.observed_edges[0].injected = false),
            ("up is another down", |k| k.observed_edges[1].down = true),
            ("released before deadline", |k| {
                k.observed_edges[1].relative_us = 249_000
            }),
            ("observer remains", |k| k.observer_stopped = false),
            ("owned key remains", |k| k.outstanding_keys = 1),
            ("release error hidden", |k| {
                k.release_error = Some("owned key up failed".into())
            }),
        ];
        for (label, mutate) in mutations {
            let mut changed = report.clone();
            if let StageProof::InputTimeout { key } =
                receipt_mut(&mut changed).unwrap().proof.as_mut().unwrap()
            {
                mutate(key);
            }
            assert!(validate_probe(&changed, true).is_err(), "{label}");
        }
    }

    #[test]
    fn controlled_cleanup_failure_keeps_primary_and_marks_secondary_case() {
        let mut report = probe_report(ProbeKind::InputTimeout);
        let primary = serde_json::to_value(&report.cases[0]).unwrap();
        finalize_probe_cleanup(&mut report);
        assert_eq!(report.cases[1].status, CaseStatus::Passed);
        receipt_mut(&mut report)
            .unwrap()
            .cleanup_errors
            .push("actual private-artifact retention failed".into());
        if let StageProof::InputTimeout { key } =
            receipt_mut(&mut report).unwrap().proof.as_mut().unwrap()
        {
            key.release_error = Some("actual owned key-up failed".into());
        }
        finalize_probe_cleanup(&mut report);
        assert_eq!(serde_json::to_value(&report.cases[0]).unwrap(), primary);
        assert_eq!(report.cases[1].status, CaseStatus::Failed);
        assert_eq!(report.cases[1].failure_stage, Some(FailureStage::Cleanup));
        assert!(
            report.cases[1]
                .observed
                .contains("actual private-artifact retention failed")
        );
        assert!(
            report.cases[1]
                .observed
                .contains("actual owned key-up failed")
        );
        assert!(!report.passed());
        assert!(validate_probe(&report, true).is_err());
    }

    #[test]
    fn controlled_failed_report_parser_rejects_zero_exit_unknown_duplicate_fields_and_wrong_stage()
    {
        let report = probe_report(ProbeKind::Startup);
        let (created, owner) = owners(&report);
        let command = command(&report, Path::new("probe"));
        let mut actual = expected(&report, &created, &owner, &command);
        let bytes = serde_json::to_vec(&report).unwrap();
        parse_persisted(&bytes, &actual).unwrap();
        actual.exit_code = Some(0);
        assert!(parse_persisted(&bytes, &actual).is_err());
        actual.exit_code = None;
        assert!(parse_persisted(&bytes, &actual).is_err());
        actual.exit_code = Some(1);
        assert!(parse_persisted(&[], &actual).is_err());
        assert!(parse_persisted(&vec![b' '; MAX_JSON_REPORT_BYTES + 1], &actual).is_err());
        let original = serde_json::to_value(&report).unwrap();
        let mut changed = original.clone();
        changed["extra"] = true.into();
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["cleanup"]["unknown_native_receipt"] = true.into();
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["cases"][0]["failure_stage"] = serde_json::json!("input_injection");
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["cases"][0]["observed"] = serde_json::json!("arbitrary early error");
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["cases"][1]["id"] = serde_json::json!("R1");
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["environment"]["source_revision"] = serde_json::json!("stale-source");
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let mut changed = original.clone();
        changed["controlled_failures"]["receipt"]["owner"]["child_pid"] = serde_json::json!(8);
        assert!(parse_persisted(&serde_json::to_vec(&changed).unwrap(), &actual).is_err());
        let duplicate = String::from_utf8(bytes).unwrap().replacen(
            "\"schema_version\":9",
            "\"schema_version\":9,\"schema_version\":9",
            1,
        );
        assert!(parse_persisted(duplicate.as_bytes(), &actual).is_err());
        let mut changed_command = command.clone();
        changed_command[6] = "different.exe".into();
        let wrong_command = expected(&report, &created, &owner, &changed_command);
        assert!(parse_persisted(&serde_json::to_vec(&report).unwrap(), &wrong_command).is_err());
    }

    #[test]
    fn controlled_primary_timeout_and_cleanup_error_survive_failed_publication() {
        let mut report = probe_report(ProbeKind::InputTimeout);
        if let StageProof::InputTimeout { key } =
            receipt_mut(&mut report).unwrap().proof.as_mut().unwrap()
        {
            key.release_error = Some("actual owned-up cleanup failed".into());
            key.outstanding_keys = 1;
            assert_eq!(
                primary_message(&StageProof::InputTimeout { key: key.clone() }),
                key.primary_error
            );
        }
        report.cases[1].status = CaseStatus::Failed;
        report.cases[1].failure_stage = Some(FailureStage::Cleanup);
        report.cases[1].observed = "actual owned-up cleanup failed".into();
        report.cases[2].status = CaseStatus::Failed;
        report.cases[2].failure_stage = Some(FailureStage::Environment);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("failed.json");
        write_report(&path, &mut report).unwrap();
        write_text_report(&path.with_extension("txt"), &report).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["outcome"], "failed");
        assert_eq!(value["cases"][0]["failure_stage"], "input_injection");
        assert_eq!(
            value["controlled_failures"]["receipt"]["proof"]["key"]["release_error"],
            "actual owned-up cleanup failed"
        );
        assert_eq!(
            value["cases"][0]["observed"],
            value["controlled_failures"]["receipt"]["proof"]["key"]["primary_error"]
        );
        assert!(!report.passed());
    }

    #[test]
    fn controlled_persistence_overflow_fails_primary_cleanup_and_integrity_at_existing_caps() {
        let mut report = probe_report(ProbeKind::InputTimeout);
        if let StageProof::InputTimeout { key } =
            receipt_mut(&mut report).unwrap().proof.as_mut().unwrap()
        {
            key.primary_error = "x".repeat(MAX_JSON_REPORT_BYTES + 1);
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("overflow.json");
        write_report(&path, &mut report).unwrap();
        write_text_report(&path.with_extension("txt"), &report).unwrap();
        assert!(fs::metadata(&path).unwrap().len() <= MAX_JSON_REPORT_BYTES as u64);
        assert!(
            fs::metadata(path.with_extension("txt")).unwrap().len() <= MAX_TEXT_REPORT_BYTES as u64
        );
        assert_eq!(report.cases.len(), 3);
        assert_eq!(
            report
                .cases
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["N02", "CLEANUP", "R0"]
        );
        assert!(
            report
                .cases
                .iter()
                .all(|c| c.status == CaseStatus::Failed && c.failure_stage.is_some())
        );
        assert!(
            report.capacity_saturated && report.controlled_failures.is_none() && !report.passed()
        );
        let overflow = report.report_overflow.as_ref().unwrap();
        assert_eq!(overflow.omitted_case_evidence, 1);
        assert_eq!(overflow.affected_case_ids, ["N02"]);
        assert_eq!(report.schema_version, 9);
        assert_eq!(report.profile.hold_threshold_ms, 350);
    }

    #[test]
    fn controlled_live_ownership_and_atomic_publication_require_exact_scope() {
        let report = probe_report(ProbeKind::Startup);
        let (created, owner) = owners(&report);
        check_created_ownership(
            &created,
            created.kind,
            &created.nonce,
            1,
            900,
            &report.candidate,
            "deadbeef",
        )
        .unwrap();
        let live = LiveOwnership {
            kind: created.kind,
            nonce: created.nonce.clone(),
            runner_pid: 1,
            source_revision: "deadbeef".into(),
            candidate_sha256: report.candidate.sha256.clone(),
            profile_root: created.profile_root.clone(),
            owner: owner.clone(),
        };
        check_live_ownership(&live, &created).unwrap();
        for changed in [
            CreatedOwnership {
                child_pid: 1,
                ..created.clone()
            },
            CreatedOwnership {
                process_created_filetime: 0,
                ..created.clone()
            },
            CreatedOwnership {
                nonce: "stale".into(),
                ..created.clone()
            },
            CreatedOwnership {
                source_revision: "different".into(),
                ..created.clone()
            },
            CreatedOwnership {
                profile_root: "C:\\Users\\Jay\\active-profile".into(),
                ..created.clone()
            },
        ] {
            assert!(
                check_created_ownership(
                    &changed,
                    created.kind,
                    &created.nonce,
                    1,
                    900,
                    &report.candidate,
                    "deadbeef"
                )
                .is_err()
            );
        }
        let mut foreign = live.clone();
        foreign.owner.process_created_filetime += 1;
        assert!(check_live_ownership(&foreign, &created).is_err());
        let mut foreign = live;
        foreign.owner.foreground_pid = 8;
        assert!(check_live_ownership(&foreign, &created).is_err());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owned.json");
        publish_new(&path, b"{\"complete\":true}").unwrap();
        assert_eq!(
            read_regular_bounded(&path, 64).unwrap(),
            b"{\"complete\":true}"
        );
        assert!(!path.with_extension("pending").exists());
        assert!(publish_new(&path, b"overwrite").is_err());
        assert!(read_regular_bounded(&path, 2).is_err());
        assert!(publish_new(&directory.path().join("empty.json"), &[]).is_err());
        assert!(publish_new(&directory.path().join("large.json"), &vec![0; 8 * 1024 + 1]).is_err());
        let invalid = directory.path().join("nonregular.json");
        fs::create_dir(&invalid).unwrap();
        assert!(read_regular_bounded(&invalid, 64).is_err());
    }

    #[test]
    fn controlled_readback_pair_checks_real_hashes_files_disposal_and_private_bundle() {
        // Native receipts are explicit unit fixtures. This test exercises actual
        // persisted bytes/private bundles, without injecting keys or starting GUI.
        let profile = tempfile::Builder::new()
            .prefix("multi-launcher-radial-acceptance-")
            .tempdir()
            .unwrap();
        let output = tempfile::tempdir().unwrap();
        let mut report = probe_report(ProbeKind::Startup);
        report.profile.temporary_data_root = profile.path().to_string_lossy().into_owned();
        for name in ["settings.json", "radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let hashes = ProfileHashes {
            settings: sha256_file(&profile.path().join("settings.json")).unwrap(),
            settings_full: "e".repeat(64),
            radial: sha256_file(&profile.path().join("radial.json")).unwrap(),
            actions: sha256_file(&profile.path().join("actions.json")).unwrap(),
            settings_without_designer_geometry: "f".repeat(64),
        };
        report.profile.settings_sha256 = hashes.settings.clone();
        report.profile.radial_sha256 = hashes.radial.clone();
        report.profile.actions_sha256 = hashes.actions.clone();
        receipt_mut(&mut report).unwrap().profile_before = hashes.clone();
        receipt_mut(&mut report).unwrap().profile_after = Some(hashes);
        let candidate = output.path().join("candidate.exe");
        fs::write(&candidate, b"controlled unit binary identity").unwrap();
        report.candidate.executable = candidate.to_string_lossy().into_owned();
        report.candidate.sha256 = sha256_file(&candidate).unwrap();
        report.environment.runner_sha256 =
            Some(sha256_file(&std::env::current_exe().unwrap()).unwrap());
        let windows = serde_json::json!({"runner_process_id":1,"child_process_id":7,"windows":[{
            "hwnd":44,"process_id":7,"role":"root","visible":true,"minimized":false,"bounds":[0,0,500,500]}]});
        let trace = "trace_event=trace_ready trace_sequence=1".to_owned();
        #[cfg(windows)]
        let trace = {
            let sanitized = native::controlled_stage_trace_fixture(
                "WARN trace_event=\"trace_ready\" trace_sequence=1\n",
            );
            assert_eq!(
                sanitized, trace,
                "private bundle uses canonical trace bytes"
            );
            assert!(!sanitized.contains("trace_event=\"trace_ready\""));
            sanitized
        };
        let files = [
            ("case-R1-trace.log", trace.into_bytes()),
            (
                "case-R1-private.log",
                b"bounded private fixture log\n".to_vec(),
            ),
            (
                "case-R1-windows.json",
                serde_json::to_vec(&windows).unwrap(),
            ),
        ];
        let mut paths = Vec::new();
        for (name, bytes) in files {
            let path = profile.path().join(name);
            fs::write(&path, bytes).unwrap();
            paths.push(path);
        }
        let png = profile.path().join("case-R1.png");
        image::RgbaImage::new(8, 8).save(&png).unwrap();
        paths.push(png);
        let staged = private_artifacts::stage_diagnostics(profile.path(), paths).unwrap();
        report.private_artifacts = Some(staged.summary().clone());
        let (created, owner) = owners(&report);
        let command = command(&report, output.path());
        let live = LiveOwnership {
            kind: created.kind,
            nonce: created.nonce.clone(),
            runner_pid: 1,
            source_revision: "deadbeef".into(),
            candidate_sha256: report.candidate.sha256.clone(),
            profile_root: created.profile_root.clone(),
            owner: owner.clone(),
        };
        publish_new(
            &output.path().join("case-N01-created.json"),
            &serde_json::to_vec(&created).unwrap(),
        )
        .unwrap();
        publish_new(
            &output.path().join("case-N01-owner.json"),
            &serde_json::to_vec(&live).unwrap(),
        )
        .unwrap();
        let path = output.path().join("report.json");
        write_report(&path, &mut report).unwrap();
        write_text_report(&path.with_extension("txt"), &report).unwrap();
        let expectation = expected(&report, &created, &owner, &command);
        assert!(
            verify_report_pair(&path, &expectation).is_err(),
            "live private profile cannot be accepted as disposed"
        );
        profile.close().unwrap();
        let proof = verify_report_pair(&path, &expectation).unwrap();
        assert_eq!(proof.actual_exit_code, 1);
        assert_eq!(proof.observed_child_exit_code, 0);
        assert_eq!(proof.report_sha256, sha256_file(&path).unwrap());
        let original_report = fs::read(&path).unwrap();
        let original_value: serde_json::Value = serde_json::from_slice(&original_report).unwrap();
        let mut legacy = original_value.clone();
        legacy["controlled_failures"]["receipt"]["profile_before"]
            .as_object_mut()
            .unwrap()
            .remove("settings_full");
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(
            verify_report_pair(&path, &expectation).is_err(),
            "persisted pair requires the full canonical baseline"
        );
        let mut conflicting = original_value;
        for scope in ["profile_before", "profile_after"] {
            conflicting["controlled_failures"]["receipt"][scope]["settings_full"] =
                serde_json::json!("d".repeat(64));
        }
        fs::write(&path, serde_json::to_vec(&conflicting).unwrap()).unwrap();
        assert!(
            verify_report_pair(&path, &expectation).is_err(),
            "receipt cannot replace the independently published created baseline"
        );
        fs::write(&path, &original_report).unwrap();
        assert_eq!(
            verify_report_pair(&path, &expectation)
                .unwrap()
                .receipt
                .profile_before
                .settings_full,
            created.profile_hashes.settings_full
        );
        let text_path = path.with_extension("txt");
        let original_text = fs::read(&text_path).unwrap();
        fs::write(&text_path, b"different text report\n").unwrap();
        assert!(verify_report_pair(&path, &expectation).is_err());
        fs::write(&text_path, original_text).unwrap();
        fs::write(&candidate, b"changed candidate").unwrap();
        assert!(verify_report_pair(&path, &expectation).is_err());
        fs::write(&candidate, b"controlled unit binary identity").unwrap();
        let bundle = std::env::temp_dir().join(staged.summary().artifact_id.as_ref().unwrap());
        fs::write(bundle.join("case-R1-private.log"), b"modified bundle\n").unwrap();
        assert!(verify_report_pair(&path, &expectation).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn controlled_private_trace_boundary_accepts_actual_sanitized_ready_fields() {
        for raw in [
            "WARN trace_event=\"trace_ready\" trace_budget_profile=\"default\" event_budget=8192 reserved_event_budget=0 trace_sequence=1\n",
            "WARN trace_event=trace_ready trace_budget_profile=default event_budget=8192 reserved_event_budget=0 trace_sequence=1\n",
        ] {
            let trace = native::controlled_stage_trace_fixture(raw);
            assert_eq!(
                trace,
                "trace_event=trace_ready trace_budget_profile=default event_budget=8192 reserved_event_budget=0 trace_sequence=1"
            );
            verify_private_trace_boundary(&trace).unwrap();
        }

        let trace = native::controlled_stage_trace_fixture(
            "WARN trace_event=\"trace_ready\" trace_sequence=1\nWARN trace_event=\"hook_service_ready\" state=\"budget_exhausted\" trace_sequence=2\n",
        );
        assert!(trace.contains("trace_event=hook_service_ready state=budget_exhausted"));
        verify_private_trace_boundary(&trace).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn controlled_private_trace_boundary_rejects_missing_lookalike_and_malformed_ready() {
        for (reason, raw) in [
            (
                "missing event",
                "WARN trace_sequence=1 state=\"trace_ready\"\n",
            ),
            (
                "event value suffix",
                "WARN trace_event=\"trace_ready_extra\" trace_sequence=1\n",
            ),
            (
                "event value prefix",
                "WARN trace_event=\"not_trace_ready\" trace_sequence=1\n",
            ),
            (
                "wrong field prefix",
                "WARN prefix_trace_event=\"trace_ready\" trace_event=\"hook_service_ready\" trace_sequence=1\n",
            ),
            (
                "ready in another retained field",
                "WARN trace_event=\"hook_service_ready\" state=\"trace_ready\" trace_sequence=1\n",
            ),
            (
                "wrong field suffix",
                "WARN trace_event_suffix=\"trace_ready\" trace_sequence=1\n",
            ),
            (
                "empty event value",
                "WARN trace_event=\"\" trace_sequence=1\n",
            ),
            (
                "split event assignment",
                "WARN trace_event= \"trace_ready\" trace_sequence=1\n",
            ),
            (
                "extra assignment delimiter",
                "WARN trace_event==trace_ready trace_sequence=1\n",
            ),
            (
                "spaces inside event value",
                "WARN trace_event=\"trace_ready extra\" trace_sequence=1\n",
            ),
        ] {
            let trace = native::controlled_stage_trace_fixture(raw);
            assert_eq!(
                verify_private_trace_boundary(&trace),
                Err("private native trace has no complete producer boundary".into()),
                "{reason}: {trace}"
            );
        }
        let trace = native::controlled_stage_trace_fixture(
            "WARN trace_event=\"hook_service_ready\" state=\"trace_ready\" trace_sequence=1\n",
        );
        assert_eq!(
            trace,
            "trace_event=hook_service_ready state=trace_ready trace_sequence=1"
        );
        assert!(verify_private_trace_boundary(&trace).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn controlled_private_trace_boundary_rejects_actual_sanitized_budget_exhaustion() {
        let ready = "WARN trace_event=\"trace_ready\" trace_sequence=1\n";
        for exhausted in [
            "WARN trace_event=\"budget_exhausted\" event_budget=8192 trace_sequence=2",
            "WARN trace_event=budget_exhausted event_budget=8192 trace_sequence=2",
        ] {
            for raw in [
                format!("{ready}{exhausted}\n"),
                format!("{exhausted}\n{ready}"),
            ] {
                let trace = native::controlled_stage_trace_fixture(&raw);
                assert!(trace.contains("trace_event=trace_ready trace_sequence=1"));
                assert!(trace.contains("trace_event=budget_exhausted event_budget=8192"));
                assert!(!trace.contains("trace_event=\""));
                assert_eq!(
                    verify_private_trace_boundary(&trace),
                    Err("private native trace has no complete producer boundary".into()),
                    "{trace}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_query_and_ui_stage_traces_require_actual_correlated_producer_fields() {
        let query = probe_report(ProbeKind::Query);
        let Some(Evidence::Probe { receipt }) = &query.controlled_failures else {
            panic!("query")
        };
        let Some(StageProof::Query { invocation, .. }) = &receipt.proof else {
            panic!("invocation")
        };
        let action = query_cell_digest(
            multi_launcher::universal_actions::action_ids::RESULT_EXECUTE.as_str(),
        )
        .wrapping_mul(0x100000001b3);
        let common = format!(
            "invocation_id={} session_digest={} cell_digest={} session_generation={} config_revision={} query_digest={} mode=\"open_launcher\"",
            invocation.invocation_id,
            invocation.session_digest,
            invocation.cell_digest,
            invocation.session_generation,
            invocation.config_revision,
            invocation.query_digest
        );
        let trace = format!(
            "WARN trace_event=\"trace_ready\" trace_sequence=1\nWARN trace_event=\"radial_query_resolution\" {common} preparation_generation={} state=\"manual_ui\"\nWARN trace_event=\"radial_dispatch_requested\" invocation_id={} session_generation={}\nWARN trace_event=\"universal_action_execution\" action_id_digest={action} action_surface=\"RadialMenu\" activation_source=\"click\" trace_sequence=17\nWARN trace_event=\"radial_query_dispatch\" {common} outcome=\"executed\" interaction_requirement=\"launcher_ui\" root_policy=\"legacy\"\n",
            invocation.preparation_generation,
            invocation.invocation_id,
            invocation.session_generation
        );
        let trace = native::controlled_stage_trace_fixture(&trace);
        for field in [
            "trace_event=trace_ready".to_owned(),
            "mode=open_launcher".to_owned(),
            "state=manual_ui".to_owned(),
            "action_surface=RadialMenu".to_owned(),
            "activation_source=click".to_owned(),
            "outcome=executed".to_owned(),
            "interaction_requirement=launcher_ui".to_owned(),
            "root_policy=legacy".to_owned(),
            format!("invocation_id={}", invocation.invocation_id),
            format!("session_digest={}", invocation.session_digest),
            format!("cell_digest={}", invocation.cell_digest),
            format!("session_generation={}", invocation.session_generation),
            format!("config_revision={}", invocation.config_revision),
            format!("query_digest={}", invocation.query_digest),
            format!(
                "preparation_generation={}",
                invocation.preparation_generation
            ),
            format!("action_id_digest={action}"),
        ] {
            assert!(trace.contains(&field), "sanitizer omitted {field}: {trace}");
        }
        verify_stage_trace(&trace, receipt, &[]).unwrap();
        for changed in [
            trace.replace("open_launcher", "execute_first"),
            trace.replace("manual_ui", "ready"),
            trace.replace("action_surface=RadialMenu", "action_surface=LauncherList"),
            trace.replace(&format!("action_id_digest={action}"), "action_id_digest=99"),
            trace.replace(
                &format!("cell_digest={}", invocation.cell_digest),
                "cell_digest=99",
            ),
            format!("{trace}\ntrace_event=universal_action_execution action_id_digest={action}\n"),
            format!("{trace}\ntrace_event=budget_exhausted\n"),
        ] {
            assert_ne!(
                changed, trace,
                "negative mutation must change sanitized proof"
            );
            assert!(verify_stage_trace(&changed, receipt, &[]).is_err());
        }
        let ui = probe_report(ProbeKind::Ui);
        let Some(Evidence::Probe { receipt }) = &ui.controlled_failures else {
            panic!("UI")
        };
        let trace = "WARN trace_event=\"trace_ready\" trace_sequence=1\nWARN trace_event=\"authoring\" edge=ReplyAccepted request_kind=Snapshot session_id=13 request_id=1 trace_sequence=21\n";
        let trace = native::controlled_stage_trace_fixture(trace);
        for field in [
            "trace_event=trace_ready",
            "trace_event=authoring",
            "edge=ReplyAccepted",
            "request_kind=Snapshot",
            "session_id=13",
            "request_id=1",
            "trace_sequence=21",
        ] {
            assert!(trace.contains(field), "sanitizer omitted {field}: {trace}");
        }
        let windows =
            vec![serde_json::json!({"hwnd":55,"process_id":7,"role":"designer","visible":true})];
        verify_stage_trace(&trace, receipt, &windows).unwrap();
        for changed in [
            trace.replace("session_id=13", "session_id=12"),
            trace.replace("trace_sequence=21", "trace_sequence=20"),
            trace.replace("ReplyAccepted", "ReplyRejected"),
            format!("{trace}\ntrace_event=universal_action_execution action_id_digest=99\n"),
        ] {
            assert_ne!(
                changed, trace,
                "negative mutation must change sanitized proof"
            );
            assert!(verify_stage_trace(&changed, receipt, &windows).is_err());
        }
        let mut foreign = windows;
        foreign[0]["process_id"] = serde_json::json!(8);
        assert!(verify_stage_trace(&trace, receipt, &foreign).is_err());
        assert!(verify_stage_trace(&trace, receipt, &[]).is_err());
    }

    #[test]
    fn controlled_stage_waits_refuse_stale_short_wrong_query_and_preclosed_process_receipts() {
        for kind in ProbeKind::ALL {
            let mut report = probe_report(kind);
            match receipt_mut(&mut report).unwrap().proof.as_mut().unwrap() {
                StageProof::Startup {
                    wait_started_unix_ms,
                    ..
                } => *wait_started_unix_ms = 900,
                StageProof::InputTimeout { key } => {
                    key.timeout_at_unix_ms = key.down.at_unix_ms + 249
                }
                StageProof::Query { invocation, .. } => {
                    invocation.cell_id = "qa-pinned-second".into()
                }
                StageProof::Ui { elapsed_ms, .. } => *elapsed_ms = 4999,
                StageProof::ChildExit { wait, .. } => wait.observed_exit_code = 0,
                StageProof::DesktopMismatch { admission } => {
                    admission.expected_desktop = admission.actual.input_desktop.clone()
                }
            }
            assert!(validate_probe(&report, true).is_err(), "{kind:?}");
        }
        let report = probe_report(ProbeKind::ChildExit);
        let (created, owner) = owners(&report);
        let command = command(&report, Path::new("probe"));
        let mut actual = expected(&report, &created, &owner, &command);
        actual.observed_child_exit_code = 0;
        assert!(parse_persisted(&serde_json::to_vec(&report).unwrap(), &actual).is_err());
    }

    #[test]
    fn controlled_child_exit_readback_requires_pending_entry_injection_and_observed_completion() {
        let original = probe_report(ProbeKind::ChildExit);
        let (created, owner) = owners(&original);
        let command = command(&original, Path::new("probe"));
        let expected = expected(&original, &created, &owner, &command);
        parse_persisted(&serde_json::to_vec(&original).unwrap(), &expected).unwrap();
        let mutations: Vec<(&str, fn(&mut ChildExitWait))> = vec![
            ("wait predates owner", |w| w.wait_entered_unix_ms = 900),
            ("pending before entry", |w| {
                w.pending_observed_unix_ms = 1399
            }),
            ("termination before pending", |w| {
                w.termination_injected_unix_ms = 1419
            }),
            ("exit observed before termination", |w| {
                w.exit_observed_unix_ms = 1449
            }),
            ("monotonic termination before pending", |w| {
                w.termination_after_entry_us = 19_999
            }),
            ("no wait after termination", |w| {
                w.exit_after_entry_us = w.termination_after_entry_us
            }),
            ("no pending operation", |w| w.pending_poll_index = 0),
            ("injection delayed past first pending observation", |w| {
                w.pending_poll_index = 2
            }),
            ("already completed operation", |w| w.exit_poll_index = 1),
            ("unbounded operation", |w| w.exit_after_entry_us = 5_500_001),
            ("wrong observed exit", |w| w.observed_exit_code = 0),
        ];
        for (label, mutate) in mutations {
            let mut changed = original.clone();
            let Some(StageProof::ChildExit { wait, .. }) =
                &mut receipt_mut(&mut changed).unwrap().proof
            else {
                panic!("child exit fixture")
            };
            mutate(wait);
            assert!(validate_probe(&changed, true).is_err(), "{label}");
            assert!(
                parse_persisted(&serde_json::to_vec(&changed).unwrap(), &expected).is_err(),
                "{label}"
            );
        }
        let mut old_kill_then_wait = serde_json::to_value(&original).unwrap();
        old_kill_then_wait["controlled_failures"]["receipt"]["proof"] = serde_json::json!({
            "stage": "child_exit", "outstanding_wait_started_unix_ms": 1400,
            "terminated_unix_ms": 1450, "observed_exit_code": 1,
            "input_refused_after_exit": true, "input_inserted_after_exit": 0,
        });
        assert!(
            parse_persisted(&serde_json::to_vec(&old_kill_then_wait).unwrap(), &expected).is_err()
        );
        let mut missing = serde_json::to_value(&original).unwrap();
        missing["controlled_failures"]["receipt"]["proof"]
            .as_object_mut()
            .unwrap()
            .remove("wait");
        assert!(parse_persisted(&serde_json::to_vec(&missing).unwrap(), &expected).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn controlled_emergency_key_recovery_requires_observed_runner_exit_and_exact_owner() {
        use std::cell::Cell;
        use std::os::windows::process::ExitStatusExt;
        let releases = Cell::new(0);
        for reason in [
            "owned runner kill failed; runner remains live",
            "bounded runner exit wait expired; runner remains live",
        ] {
            let runner_cleanup = Err(reason.into());
            let recovery = recover_after_observed_runner_exit(77, &runner_cleanup, || {
                releases.set(releases.get() + 1);
                Ok(())
            });
            assert_eq!(recovery, EmergencyKeyRecovery::OwnershipRetained);
            assert_eq!(
                releases.get(),
                0,
                "live inner guard must remain the sole release owner"
            );
            let diagnostic = controlled_subprocess_failure(
                "original controlled subprocess deadline failure",
                &runner_cleanup,
                &recovery,
                &Some(Err("owned application cleanup failed".into())),
            );
            assert!(diagnostic.contains("original controlled subprocess deadline failure"));
            assert!(diagnostic.contains(reason));
            assert!(diagnostic.contains("owned application cleanup failed"));
            assert!(diagnostic.contains("OwnershipRetained"));
        }
        for foreign_pid in [0, 78] {
            let exit = Ok(ObservedSubprocessExit {
                process_id: foreign_pid,
                status: std::process::ExitStatus::from_raw(1),
                observed_unix_ms: Some(1600),
                origin: SubprocessExitOrigin::OuterTermination,
            });
            assert_eq!(
                recover_after_observed_runner_exit(77, &exit, || {
                    releases.set(releases.get() + 1);
                    Ok(())
                }),
                EmergencyKeyRecovery::OwnershipRetained
            );
        }
        assert_eq!(releases.get(), 0);
        let observed = Ok(ObservedSubprocessExit {
            process_id: 77,
            status: std::process::ExitStatus::from_raw(1),
            observed_unix_ms: Some(1600),
            origin: SubprocessExitOrigin::OuterTermination,
        });
        let recovery = recover_after_observed_runner_exit(77, &observed, || {
            releases.set(releases.get() + 1);
            Ok(())
        });
        assert_eq!(releases.get(), 1);
        assert_eq!(
            recovery,
            EmergencyKeyRecovery::Attempted {
                runner_pid: 77,
                actual_exit_code: Some(1),
                result: Ok(())
            }
        );
        let failed_release = recover_after_observed_runner_exit(77, &observed, || {
            releases.set(releases.get() + 1);
            Err("actual emergency up failed".into())
        });
        assert_eq!(
            releases.get(),
            2,
            "one attempt per admitted ownership transfer; no hidden retry"
        );
        assert!(matches!(
            failed_release,
            EmergencyKeyRecovery::Attempted { result: Err(_), .. }
        ));
    }

    #[test]
    fn controlled_aggregate_cannot_pass_boolean_cleanup_without_actual_readbacks() {
        let mut aggregate = super::super::tests::acceptance_report(AGGREGATE_MODE);
        aggregate.started_unix_ms = 800;
        aggregate.finished_unix_ms = 80_000;
        aggregate.profile.mode = "controlled_failure_aggregate";
        aggregate.profile.hold_threshold_ms = 350;
        aggregate.cleanup = CleanupResult {
            child_owned_windows_closed: true,
            profile_removed: true,
            cursor_restored: true,
            input_desktop_released: true,
            ..CleanupResult::default()
        };
        aggregate.cases = CASE_IDS
            .iter()
            .map(|id| AcceptanceCaseResult {
                id: (*id).into(),
                status: CaseStatus::Passed,
                elapsed_ms: 0,
                expected: "required native failed-report proof".into(),
                observed: "asserted true".into(),
                failure_stage: None,
                artifacts: Vec::new(),
            })
            .collect();
        aggregate.controlled_failures = Some(Evidence::Aggregate {
            probes: Vec::new(),
            attempts: Vec::new(),
        });
        assert!(!aggregate.passed());
        let mut probes = Vec::new();
        for (index, kind) in ProbeKind::ALL.into_iter().enumerate() {
            let report = probe_report(kind);
            let (created, owner) = owners(&report);
            let Some(Evidence::Probe { receipt }) = report.controlled_failures else {
                panic!("probe")
            };
            probes.push(VerifiedProbe {
                kind,
                nonce: receipt.nonce.clone(),
                subprocess_pid: index as u32 + 20,
                actual_exit_code: 1,
                started_unix_ms: 900 + index as u128 * 10_000,
                finished_unix_ms: 9900 + index as u128 * 10_000,
                report_path: format!("missing-private-readback-{}.json", kind.id()),
                report_sha256: "a".repeat(64),
                text_sha256: "b".repeat(64),
                receipt: *receipt,
                cleanup: report.cleanup,
                private_artifacts: private_artifacts::PrivateArtifactSummary::not_run(),
                command: Vec::new(),
                created_owner: created,
                leased_owner: owner,
                observed_child_exit_code: if kind == ProbeKind::ChildExit { 1 } else { 0 },
            });
        }
        aggregate.controlled_failures = Some(Evidence::Aggregate {
            probes,
            attempts: Vec::new(),
        });
        assert!(validate_report_evidence(&aggregate).is_err());
        assert!(!aggregate.passed());
        if let Some(Evidence::Aggregate { probes, .. }) = &mut aggregate.controlled_failures {
            probes.push(probes[0].clone());
        }
        assert!(persistence_cap_failure(&aggregate).is_some());
        bound_report_for_persistence(&mut aggregate).unwrap();
        assert!(
            aggregate.capacity_saturated
                && aggregate.controlled_failures.is_none()
                && !aggregate.passed()
        );
        assert_eq!(aggregate.cases.len(), CASE_IDS.len());
        assert!(
            aggregate
                .cases
                .iter()
                .all(|case| case.status == CaseStatus::Failed)
        );
    }

    #[cfg(windows)]
    #[test]
    fn controlled_profile_identity_is_canonical_for_maps_and_plugin_sets_but_preserves_ordered_values()
     {
        fn reordered_json(value: &serde_json::Value) -> String {
            match value {
                serde_json::Value::Object(object) => format!(
                    "{{{}}}",
                    object
                        .iter()
                        .rev()
                        .map(|(key, value)| format!(
                            "{}:{}",
                            serde_json::to_string(key).unwrap(),
                            reordered_json(value)
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                serde_json::Value::Array(array) => format!(
                    "[{}]",
                    array
                        .iter()
                        .map(reordered_json)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                _ => serde_json::to_string(value).unwrap(),
            }
        }
        let profile = tempfile::tempdir().unwrap();
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let path = profile.path().join("settings.json");
        let mut settings = Settings::default();
        settings.enabled_plugins = Some(
            ["z-plugin", "a-plugin", "m-plugin"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        );
        settings.enabled_capabilities = Some(std::collections::HashMap::from([
            ("z-plugin".into(), vec!["second".into(), "first".into()]),
            ("a-plugin".into(), vec!["read".into()]),
        ]));
        settings.plugin_settings = std::collections::HashMap::from([
            (
                "z-plugin".into(),
                serde_json::json!({"level":2,"ordered":["first","second"]}),
            ),
            (
                "a-plugin".into(),
                serde_json::json!({"flag":false,"nested":{"b":2,"a":1}}),
            ),
        ]);
        // Capture the actual writer's f32 tokens. Promoting them through Value
        // can change protected decimals and is a rejection case below.
        settings.save(path.to_str().unwrap()).unwrap();
        let authored = fs::read(&path).unwrap();
        let value = canonical_settings(&authored).unwrap();
        let original = read_profile_hashes(profile.path()).unwrap();
        assert_eq!(original.settings, sha256_bytes(&authored));
        for _ in 0..16 {
            assert_eq!(read_profile_hashes(profile.path()).unwrap(), original);
        }
        // Exercise the actual independently reloaded streaming settings writer.
        // Whether randomized maps happen to reorder on this one run is irrelevant.
        let persisted = Settings::update(path.to_str().unwrap(), |_| Ok(())).unwrap();
        let reloaded = read_profile_hashes(profile.path()).unwrap();
        assert_eq!(reloaded.settings_full, original.settings_full);
        assert_eq!(
            reloaded.settings_without_designer_geometry,
            original.settings_without_designer_geometry
        );
        assert_eq!(
            canonical_settings(&serde_json::to_vec(&persisted).unwrap()).unwrap(),
            canonical_settings(&serde_json::to_vec(&value).unwrap()).unwrap()
        );
        assert_eq!(reloaded.radial, original.radial);
        assert_eq!(reloaded.actions, original.actions);
        let mut reordered = value.clone();
        reordered["enabled_plugins"]
            .as_array_mut()
            .unwrap()
            .reverse();
        fs::write(&path, reordered_json(&reordered)).unwrap();
        let reordered_hashes = read_profile_hashes(profile.path()).unwrap();
        assert_ne!(reordered_hashes.settings, original.settings);
        assert_eq!(reordered_hashes.settings_full, original.settings_full);
        assert_eq!(
            reordered_hashes.settings_without_designer_geometry,
            original.settings_without_designer_geometry,
        );
        assert_eq!(reordered_hashes.radial, original.radial);
        assert_eq!(reordered_hashes.actions, original.actions);
        for _ in 0..16 {
            assert_eq!(
                read_profile_hashes(profile.path()).unwrap(),
                reordered_hashes
            );
        }
        let mut changes = Vec::new();
        let mut promoted_f32 = value.clone();
        promoted_f32["note_graph"]["label_zoom_threshold"] =
            serde_json::to_value(settings.note_graph.label_zoom_threshold).unwrap();
        assert_ne!(
            promoted_f32["note_graph"]["label_zoom_threshold"],
            value["note_graph"]["label_zoom_threshold"],
            "Value promotion changes the actual writer's protected f32 decimal"
        );
        changes.push(("f32 scalar promoted through Value", promoted_f32));
        let mut protected_map = value.clone();
        protected_map["plugin_settings"]["a-plugin"]["nested"]["a"] = serde_json::json!(3);
        changes.push(("nested map value", protected_map));
        let mut protected_set = value.clone();
        protected_set["enabled_plugins"]
            .as_array_mut()
            .unwrap()
            .pop();
        changes.push(("plugin set membership", protected_set));
        let mut protected_flag = value.clone();
        protected_flag["plugin_settings"]["a-plugin"]["flag"] = serde_json::json!(true);
        changes.push(("protected flag", protected_flag));
        let mut ordered_capabilities = value.clone();
        ordered_capabilities["enabled_capabilities"]["z-plugin"]
            .as_array_mut()
            .unwrap()
            .reverse();
        changes.push(("capability vector order", ordered_capabilities));
        let mut ordered_plugin_values = value.clone();
        ordered_plugin_values["plugin_settings"]["z-plugin"]["ordered"]
            .as_array_mut()
            .unwrap()
            .reverse();
        changes.push(("plugin array order", ordered_plugin_values));
        let mut root_geometry = value.clone();
        root_geometry["window_size"] = serde_json::json!([901, 650]);
        changes.push(("ROOT geometry", root_geometry));
        let mut clipboard_preferences = value.clone();
        clipboard_preferences["plugin_settings"]["clipboard_modify"] =
            serde_json::json!({"hide_launcher_after_apply":false,"dialog_width":901.0});
        changes.push((
            "Clipboard Modify protected preferences",
            clipboard_preferences,
        ));
        let mut unknown_top = value.clone();
        unknown_top["unknown_protected"] = serde_json::json!({"v":1});
        changes.push(("unknown top-level addition", unknown_top));
        let mut unknown_nested = value.clone();
        unknown_nested["theme"]["unknown_protected"] = serde_json::json!([2, 1]);
        changes.push(("unknown nested addition", unknown_nested));
        let mut missing_default = value.clone();
        missing_default
            .as_object_mut()
            .unwrap()
            .remove("debug_logging");
        changes.push(("missing defaultable field", missing_default));
        let mut pinned = value.clone();
        pinned["pinned_panels"] = serde_json::json!(["NotesDialog", "ClipboardDialog"]);
        changes.push(("pinned-panel values", pinned));
        for (label, changed) in changes {
            fs::write(&path, reordered_json(&changed)).unwrap();
            let changed = read_profile_hashes(profile.path()).unwrap();
            assert_ne!(
                changed.settings_full, original.settings_full,
                "full identity must protect {label}"
            );
            assert_ne!(
                changed.settings_without_designer_geometry,
                original.settings_without_designer_geometry,
                "canonical identity must protect {label}",
            );
            for kind in ProbeKind::ALL {
                let mut report = probe_report(kind);
                report.profile.settings_sha256 = original.settings.clone();
                report.profile.radial_sha256 = original.radial.clone();
                report.profile.actions_sha256 = original.actions.clone();
                receipt_mut(&mut report).unwrap().profile_before = original.clone();
                receipt_mut(&mut report).unwrap().profile_after = Some(changed.clone());
                assert!(
                    validate_probe(&report, true).is_err(),
                    "{kind:?} accepted {label}"
                );
            }
        }
        for kind in ProbeKind::ALL {
            let mut report = probe_report(kind);
            report.profile.settings_sha256 = original.settings.clone();
            report.profile.radial_sha256 = original.radial.clone();
            report.profile.actions_sha256 = original.actions.clone();
            receipt_mut(&mut report).unwrap().profile_before = original.clone();
            receipt_mut(&mut report).unwrap().profile_after = Some(reordered_hashes.clone());
            validate_probe(&report, true).unwrap();
            let receipt = receipt_mut(&mut report).unwrap();
            assert_ne!(
                receipt.profile_before.settings,
                receipt.profile_after.as_ref().unwrap().settings,
                "representation-only equivalence retains both actual raw observations"
            );
            assert_eq!(
                receipt.profile_before.settings_full,
                receipt.profile_after.as_ref().unwrap().settings_full
            );
        }
    }

    #[cfg(windows)]
    fn raw_controlled_settings_number(source: &[u8], placement: &str, number: &str) -> Vec<u8> {
        // Insert raw text after constructing the surrounding valid settings;
        // never round the tested token through Value before reaching its owner.
        let mut value: serde_json::Value = serde_json::from_slice(source).unwrap();
        let placeholder = "__controlled_raw_number__";
        if placement == "plugin" {
            value["plugin_settings"]["custom"] = serde_json::json!({"counter":placeholder});
        } else {
            value["unknown_counter"] = serde_json::json!(placeholder);
        }
        let text = serde_json::to_string(&value).unwrap();
        let needle = serde_json::to_string(placeholder).unwrap();
        assert_eq!(text.matches(&needle).count(), 1);
        text.replace(&needle, number).into_bytes()
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_refuses_lossy_integer_decimal_and_exponent_pairs() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let source = serde_json::to_vec(&Settings::default()).unwrap();
        for placement in ["plugin", "unknown"] {
            for pair in [
                ["18446744073709551616", "18446744073709551617"],
                ["0.123456789012345678901", "0.123456789012345678902"],
                ["1.23456789012345678901e-10", "1.23456789012345678902e-10"],
            ] {
                let before = raw_controlled_settings_number(&source, placement, pair[0]);
                let after = raw_controlled_settings_number(&source, placement, pair[1]);
                assert_ne!(sha256_bytes(&before), sha256_bytes(&after));
                for bytes in [before, after] {
                    // These are otherwise-valid JSON/settings, not a malformed
                    // document being mistaken for the precision regression.
                    assert!(serde_json::from_slice::<Settings>(&bytes).is_ok());
                    let error = canonical_settings(&bytes).unwrap_err();
                    assert!(
                        error.contains("protected JSON integer precision")
                            || error.contains("protected JSON number representation"),
                        "{error}"
                    );
                    fs::write(&path, &bytes).unwrap();
                    assert!(read_profile_hashes(profile.path()).is_err());
                    assert_eq!(
                        fs::read(&path).unwrap(),
                        bytes,
                        "refused numbers are never rewritten or measured as rounded proof"
                    );
                }
            }
        }
        for malformed in [
            "00", "01", "+1", "1.e0", "0x1", "1_000", "1 2", "--1", "1e+", "1e999", "1e-999",
            "NaN", "Infinity",
        ] {
            let bytes = raw_controlled_settings_number(&source, "plugin", malformed);
            assert!(
                canonical_settings(&bytes).is_err(),
                "numeric placeholders must not admit invalid or unsupported raw tokens: {malformed}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_preserves_distinct_safe_scalars_for_every_probe_kind() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let source = serde_json::to_vec(&Settings::default()).unwrap();
        for placement in ["plugin", "unknown"] {
            for pair in [
                ["9007199254740992", "9007199254740993"],
                ["18446744073709551614", "18446744073709551615"],
                ["-9223372036854775808", "-9223372036854775807"],
                ["1.25", "1.5"],
                ["-0.0", "0.0"],
            ] {
                let before = raw_controlled_settings_number(&source, placement, pair[0]);
                let after = raw_controlled_settings_number(&source, placement, pair[1]);
                fs::write(&path, &before).unwrap();
                let baseline = read_profile_hashes(profile.path()).unwrap();
                fs::write(&path, &after).unwrap();
                let changed = read_profile_hashes(profile.path()).unwrap();
                assert_ne!(
                    baseline.settings_full, changed.settings_full,
                    "{placement} {pair:?}"
                );
                assert_ne!(
                    baseline.settings_without_designer_geometry,
                    changed.settings_without_designer_geometry
                );
                assert_eq!(baseline.settings, sha256_bytes(&before));
                assert_eq!(changed.settings, sha256_bytes(&after));
                for kind in ProbeKind::ALL {
                    let mut report = probe_report(kind);
                    report.profile.settings_sha256 = baseline.settings.clone();
                    report.profile.radial_sha256 = baseline.radial.clone();
                    report.profile.actions_sha256 = baseline.actions.clone();
                    receipt_mut(&mut report).unwrap().profile_before = baseline.clone();
                    receipt_mut(&mut report).unwrap().profile_after = Some(changed.clone());
                    assert!(
                        validate_probe(&report, true).is_err(),
                        "{kind:?} cannot qualify distinct protected scalars {placement} {pair:?}"
                    );
                }
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_preserves_product_floats_strings_and_object_set_reordering()
    {
        fn reverse_objects(value: &serde_json::Value) -> String {
            match value {
                serde_json::Value::Object(object) => format!(
                    "{{{}}}",
                    object
                        .iter()
                        .rev()
                        .map(|(key, value)| format!(
                            "{}:{}",
                            serde_json::to_string(key).unwrap(),
                            reverse_objects(value)
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                serde_json::Value::Array(values) => format!(
                    "[{}]",
                    values
                        .iter()
                        .map(reverse_objects)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                _ => serde_json::to_string(value).unwrap(),
            }
        }
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let mut settings = Settings::default();
        settings.query_scale = Some(1.17);
        settings.list_scale = Some(0.5612154);
        settings.enabled_plugins = Some(["alpha", "zeta"].into_iter().map(str::to_owned).collect());
        settings.plugin_settings.insert("protected_text".into(), serde_json::json!("counter=18446744073709551617; quoted=\"1e999\"; slash=\\; negative=-0.123456789012345678901"));
        let source = serde_json::to_vec(&settings).unwrap();
        for placement in ["plugin", "unknown"] {
            for float in [
                0.0_f64,
                -0.0,
                1.25,
                -2.5,
                1.17,
                0.5612154,
                f64::MIN_POSITIVE,
                f64::MAX,
                f64::from_bits(1),
            ] {
                let token = serde_json::to_string(&float).unwrap();
                let raw = raw_controlled_settings_number(&source, placement, &token);
                let value = canonical_settings(&raw).unwrap();
                let pointer = if placement == "plugin" {
                    "/plugin_settings/custom/counter"
                } else {
                    "/unknown_counter"
                };
                assert_eq!(
                    serde_json::to_string(value.pointer(pointer).unwrap()).unwrap(),
                    token,
                    "the actual writer-produced numeric token remains exact"
                );
                assert_eq!(
                    value.pointer(pointer).unwrap().as_f64().unwrap().to_bits(),
                    float.to_bits()
                );
                assert_eq!(
                    value["plugin_settings"]["protected_text"],
                    settings.plugin_settings["protected_text"]
                );
                fs::write(&path, &raw).unwrap();
                let baseline = read_profile_hashes(profile.path()).unwrap();
                let mut reordered = value;
                reordered["enabled_plugins"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
                let reordered = reverse_objects(&reordered);
                fs::write(&path, &reordered).unwrap();
                let changed = read_profile_hashes(profile.path()).unwrap();
                assert_ne!(baseline.settings, changed.settings);
                assert_eq!(
                    baseline.settings_full, changed.settings_full,
                    "raw numeric visitation order must retain each field under reordered objects"
                );
                assert_eq!(
                    baseline.settings_without_designer_geometry,
                    changed.settings_without_designer_geometry
                );
                assert_eq!(baseline.radial, changed.radial);
                assert_eq!(baseline.actions, changed.actions);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_admits_actual_f32_writer_thresholds_and_reload() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        for (weight, writer_token, equivalent_token) in [
            (0.000001_f32, "0.000001", "1e-6"),
            (10_000_000_000_000.0_f32, "1e13", "10000000000000.0"),
        ] {
            let mut settings = Settings::default();
            settings.fuzzy_weight = weight;
            settings.enabled_plugins =
                Some(["zeta", "alpha"].into_iter().map(str::to_owned).collect());
            settings.plugin_settings = std::collections::HashMap::from([
                (
                    "zeta".into(),
                    serde_json::json!({"ordered":[2,1],"flag":false}),
                ),
                ("alpha".into(), serde_json::json!({"value":1.25})),
            ]);
            assert_eq!(serde_json::to_string(&weight).unwrap(), writer_token);
            // The typed f32 writer must be the source; constructing the tested
            // value through Value/f64 would miss both notation boundaries.
            settings.save(path.to_str().unwrap()).unwrap();
            let raw = fs::read(&path).unwrap();
            let needle = format!("\"fuzzy_weight\": {writer_token}");
            assert_eq!(
                std::str::from_utf8(&raw).unwrap().matches(&needle).count(),
                1
            );
            let canonical = canonical_settings(&raw).unwrap();
            assert_eq!(
                serde_json::to_string(&canonical["fuzzy_weight"]).unwrap(),
                equivalent_token
            );
            assert_eq!(
                canonical["fuzzy_weight"].as_f64().unwrap().to_bits(),
                writer_token.parse::<f64>().unwrap().to_bits()
            );
            let before = read_profile_hashes(profile.path()).unwrap();
            assert_eq!(before.settings, sha256_bytes(&raw));
            assert_eq!(
                Settings::load(path.to_str().unwrap())
                    .unwrap()
                    .fuzzy_weight
                    .to_bits(),
                weight.to_bits()
            );
            let persisted = Settings::update(path.to_str().unwrap(), |current| {
                assert_eq!(current.fuzzy_weight.to_bits(), weight.to_bits());
                assert_eq!(current.plugin_settings, settings.plugin_settings);
                assert_eq!(current.enabled_plugins, settings.enabled_plugins);
                Ok(())
            })
            .unwrap();
            assert_eq!(persisted.fuzzy_weight.to_bits(), weight.to_bits());
            let reloaded = read_profile_hashes(profile.path()).unwrap();
            assert_eq!(reloaded.settings_full, before.settings_full);
            assert_eq!(
                reloaded.settings_without_designer_geometry,
                before.settings_without_designer_geometry
            );
            assert_eq!(reloaded.radial, before.radial);
            assert_eq!(reloaded.actions, before.actions);

            let mut reordered = canonical_settings(&fs::read(&path).unwrap()).unwrap();
            reordered["enabled_plugins"]
                .as_array_mut()
                .unwrap()
                .reverse();
            let reordered = format!(
                "{{{}}}",
                reordered
                    .as_object()
                    .unwrap()
                    .iter()
                    .rev()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        serde_json::to_string(value).unwrap()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            assert!(reordered.contains(&format!("\"fuzzy_weight\":{equivalent_token}")));
            assert_ne!(raw.as_slice(), reordered.as_bytes());
            fs::write(&path, &reordered).unwrap();
            let after = read_profile_hashes(profile.path()).unwrap();
            assert_ne!(before.settings, after.settings);
            assert_eq!(before.settings_full, after.settings_full);
            assert_eq!(
                before.settings_without_designer_geometry,
                after.settings_without_designer_geometry
            );
            assert_eq!(
                Settings::load(path.to_str().unwrap())
                    .unwrap()
                    .fuzzy_weight
                    .to_bits(),
                weight.to_bits()
            );
            for kind in ProbeKind::ALL {
                let mut report = probe_report(kind);
                report.profile.settings_sha256 = before.settings.clone();
                report.profile.radial_sha256 = before.radial.clone();
                report.profile.actions_sha256 = before.actions.clone();
                receipt_mut(&mut report).unwrap().profile_before = before.clone();
                receipt_mut(&mut report).unwrap().profile_after = Some(after.clone());
                validate_probe(&report, true).unwrap();
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_rejects_neighboring_actual_f32_settings_for_every_probe_kind()
     {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        for weight in [0.000001_f32, 10_000_000_000_000.0_f32] {
            let neighbor = f32::from_bits(weight.to_bits() + 1);
            assert_ne!(
                serde_json::to_string(&weight).unwrap(),
                serde_json::to_string(&neighbor).unwrap()
            );
            let mut settings = Settings::default();
            settings.fuzzy_weight = weight;
            settings.save(path.to_str().unwrap()).unwrap();
            let before_raw = fs::read(&path).unwrap();
            let before = read_profile_hashes(profile.path()).unwrap();
            let changed = Settings::update(path.to_str().unwrap(), |current| {
                assert_eq!(current.fuzzy_weight.to_bits(), weight.to_bits());
                current.fuzzy_weight = neighbor;
                Ok(())
            })
            .unwrap();
            assert_eq!(changed.fuzzy_weight.to_bits(), neighbor.to_bits());
            assert_eq!(
                Settings::load(path.to_str().unwrap())
                    .unwrap()
                    .fuzzy_weight
                    .to_bits(),
                neighbor.to_bits()
            );
            let after_raw = fs::read(&path).unwrap();
            let after = read_profile_hashes(profile.path()).unwrap();
            assert_eq!(before.settings, sha256_bytes(&before_raw));
            assert_eq!(after.settings, sha256_bytes(&after_raw));
            assert_ne!(before.settings_full, after.settings_full);
            assert_ne!(
                before.settings_without_designer_geometry,
                after.settings_without_designer_geometry
            );
            assert_ne!(
                canonical_settings(&before_raw).unwrap()["fuzzy_weight"],
                canonical_settings(&after_raw).unwrap()["fuzzy_weight"]
            );
            for kind in ProbeKind::ALL {
                let mut report = probe_report(kind);
                report.profile.settings_sha256 = before.settings.clone();
                report.profile.radial_sha256 = before.radial.clone();
                report.profile.actions_sha256 = before.actions.clone();
                receipt_mut(&mut report).unwrap().profile_before = before.clone();
                receipt_mut(&mut report).unwrap().profile_after = Some(after.clone());
                assert!(
                    validate_probe(&report, true).is_err(),
                    "{kind:?} cannot qualify a neighboring protected f32 value"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_raw_numeric_identity_normalizes_exact_decimal_notation_without_rounding() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let source = serde_json::to_vec(&Settings::default()).unwrap();
        for placement in ["plugin", "unknown"] {
            for pair in [
                ["0.000001", "1e-6"],
                ["10000000000000.0", "1e13"],
                ["1.2500", "125e-2"],
                ["-0.0000010", "-10e-7"],
                ["0.0", "0e-9"],
                ["-0.0", "-0e+7"],
            ] {
                let before_raw = raw_controlled_settings_number(&source, placement, pair[0]);
                let after_raw = raw_controlled_settings_number(&source, placement, pair[1]);
                let pointer = if placement == "plugin" {
                    "/plugin_settings/custom/counter"
                } else {
                    "/unknown_counter"
                };
                for raw in [&before_raw, &after_raw] {
                    let value = canonical_settings(raw).unwrap();
                    assert_eq!(
                        value.pointer(pointer).unwrap().as_f64().unwrap().to_bits(),
                        pair[0].parse::<f64>().unwrap().to_bits(),
                        "{placement} {pair:?}"
                    );
                }
                fs::write(&path, &before_raw).unwrap();
                let before = read_profile_hashes(profile.path()).unwrap();
                fs::write(&path, &after_raw).unwrap();
                let after = read_profile_hashes(profile.path()).unwrap();
                assert_ne!(before.settings, after.settings);
                assert_eq!(before.settings_full, after.settings_full);
                assert_eq!(
                    before.settings_without_designer_geometry,
                    after.settings_without_designer_geometry
                );
            }
            for refused in [
                "0.00000100000000000000001",
                "1.00000000000000001e-6",
                "10000000000000.000001",
                "1.0000000000000000001e13",
                "01.0",
                "-01e0",
                "1.e0",
                "1e+-1",
                "1e9223372036854775808",
                "1e-9223372036854775808",
                "1e999",
                "1e-999",
            ] {
                let raw = raw_controlled_settings_number(&source, placement, refused);
                assert!(
                    canonical_settings(&raw).is_err(),
                    "notation equivalence cannot legalize rounding or invalid grammar: {refused}"
                );
                fs::write(&path, &raw).unwrap();
                assert!(read_profile_hashes(profile.path()).is_err());
                assert_eq!(fs::read(&path).unwrap(), raw);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_fresh_fixture_refuses_lossy_numeric_source_before_migration_or_baseline_change() {
        for placement in ["plugin", "unknown"] {
            for token in [
                "18446744073709551616",
                "18446744073709551617",
                "0.123456789012345678901",
                "1.23456789012345678902e-10",
            ] {
                let (profile, mut fixture) = fresh_query_profile();
                fixture.settings_json =
                    raw_controlled_settings_number(&fixture.settings_json, placement, token);
                fs::write(profile.path().join("settings.json"), &fixture.settings_json).unwrap();
                let authored = fixture.settings_json.clone();
                let before = [
                    "settings.json",
                    "radial.json",
                    "actions.json",
                    "query-marker-ledger.txt",
                    "notes/radial-acceptance-q13.md",
                ]
                .map(|name| fs::read(profile.path().join(name)).unwrap());
                let error =
                    prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap_err();
                assert!(
                    error.contains("protected JSON integer precision")
                        || error.contains("protected JSON number representation"),
                    "{error}"
                );
                assert_eq!(fixture.settings_json, authored);
                assert_eq!(
                    [
                        "settings.json",
                        "radial.json",
                        "actions.json",
                        "query-marker-ledger.txt",
                        "notes/radial-acceptance-q13.md"
                    ]
                    .map(|name| fs::read(profile.path().join(name)).unwrap()),
                    before
                );
                assert_eq!(
                    fs::read_dir(profile.path())
                        .unwrap()
                        .filter_map(Result::ok)
                        .filter(|entry| entry.file_name().to_string_lossy().contains("migration"))
                        .count(),
                    0,
                    "no normal migration owner ran on unsupported numeric input"
                );
            }
        }
        // A safe manually authored plugin integer remains exact through both
        // actual startup migrations and the captured complete baseline.
        let (profile, mut fixture) = fresh_query_profile();
        fixture.settings_json = raw_controlled_settings_number(
            &fixture.settings_json,
            "plugin",
            "18446744073709551615",
        );
        fs::write(profile.path().join("settings.json"), &fixture.settings_json).unwrap();
        let actions = fixture.actions_json.clone();
        let marker = fs::read(profile.path().join("query-marker-ledger.txt")).unwrap();
        let note = fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap();
        prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap();
        let prepared = canonical_settings(&fixture.settings_json).unwrap();
        assert_eq!(
            prepared["plugin_settings"]["custom"]["counter"].as_u64(),
            Some(u64::MAX)
        );
        let baseline = read_profile_hashes(profile.path()).unwrap();
        assert_eq!(baseline.settings, sha256_bytes(&fixture.settings_json));
        assert_eq!(fixture.actions_json, actions);
        assert_eq!(
            fs::read(profile.path().join("query-marker-ledger.txt")).unwrap(),
            marker
        );
        assert_eq!(
            fs::read(profile.path().join("notes/radial-acceptance-q13.md")).unwrap(),
            note
        );
    }

    #[cfg(windows)]
    #[test]
    fn controlled_settings_identity_refuses_duplicate_keys_and_invalid_schema_before_hashing() {
        let profile = tempfile::tempdir().unwrap();
        let path = profile.path().join("settings.json");
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let mut settings = Settings::default();
        settings.enabled_plugins = Some(["alpha", "beta"].into_iter().map(str::to_owned).collect());
        settings
            .plugin_settings
            .insert("custom".into(), serde_json::json!({"nested":{"a":1,"b":2}}));
        let value = serde_json::to_value(&settings).unwrap();
        let valid = serde_json::to_string(&value).unwrap();
        fs::write(&path, &valid).unwrap();
        let original = read_profile_hashes(profile.path()).unwrap();
        for duplicate in [
            format!("{{\"debug_logging\":true,{}", &valid[1..]),
            format!("{{\"\\u0064ebug_logging\":true,{}", &valid[1..]),
            format!(
                "{{\"unknown_protected\":1,\"unknown_protected\":2,{}",
                &valid[1..]
            ),
            valid.replace(
                "\"nested\":{\"a\":1,\"b\":2}",
                "\"nested\":{\"a\":9,\"a\":1,\"b\":2}",
            ),
        ] {
            assert_ne!(duplicate, valid);
            let collapsed: serde_json::Value = serde_json::from_str(&duplicate).unwrap();
            assert!(
                serde_json::from_value::<Settings>(collapsed).is_ok(),
                "ordinary Value parsing plus typed validity would erase the duplicate"
            );
            fs::write(&path, &duplicate).unwrap();
            assert!(
                read_profile_hashes(profile.path()).is_err(),
                "a collapsed earlier protected value cannot be hashed"
            );
            assert_eq!(fs::read(&path).unwrap(), duplicate.as_bytes());
        }
        for (field, invalid) in [
            ("window_size", serde_json::json!("900x650")),
            ("debug_logging", serde_json::json!(0)),
            ("enabled_plugins", serde_json::json!(["alpha", 1])),
            ("enabled_plugins", serde_json::json!(["alpha", "alpha"])),
            ("pinned_panels", serde_json::json!(["not_a_panel"])),
            ("history_limit", serde_json::json!(-1)),
        ] {
            let mut changed = value.clone();
            changed[field] = invalid;
            let bytes = serde_json::to_vec(&changed).unwrap();
            fs::write(&path, &bytes).unwrap();
            assert!(
                read_profile_hashes(profile.path()).is_err(),
                "typed invalid or ambiguous set: {field}"
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        fs::write(&path, &valid).unwrap();
        assert_eq!(read_profile_hashes(profile.path()).unwrap(), original);
    }

    #[cfg(windows)]
    #[test]
    fn controlled_full_settings_identity_protects_unknown_field_removals_missing_defaults_and_array_order()
     {
        let profile = tempfile::tempdir().unwrap();
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let path = profile.path().join("settings.json");
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value["unknown_protected"] = serde_json::json!({"nested": [1, 2]});
        value["theme"]["unknown_protected"] = serde_json::json!({"scalar": 7});
        value["plugin_settings"]["custom"] = serde_json::json!({"unknown_protected":{"scalar":9}, "enabled_plugins":["second", "first"]});
        value["pinned_panels"] = serde_json::json!(["NotesDialog", "ClipboardDialog"]);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let original = read_profile_hashes(profile.path()).unwrap();
        for cause in [
            "unknown top removal",
            "unknown nested removal",
            "unknown plugin removal",
            "defaultable field removal",
            "designer owner removal",
            "designer position removal",
            "designer size removal",
            "designer scale removal",
            "pinned order",
            "nested plugins remain ordered",
        ] {
            let mut changed = value.clone();
            match cause {
                "unknown top removal" => {
                    changed.as_object_mut().unwrap().remove("unknown_protected");
                }
                "unknown nested removal" => {
                    changed["theme"]
                        .as_object_mut()
                        .unwrap()
                        .remove("unknown_protected");
                }
                "unknown plugin removal" => {
                    changed["plugin_settings"]["custom"]
                        .as_object_mut()
                        .unwrap()
                        .remove("unknown_protected");
                }
                "defaultable field removal" => {
                    changed.as_object_mut().unwrap().remove("history_limit");
                }
                "designer owner removal" => {
                    changed.as_object_mut().unwrap().remove("radial_designer");
                }
                "designer position removal" => {
                    changed["radial_designer"]
                        .as_object_mut()
                        .unwrap()
                        .remove("window_position");
                }
                "designer size removal" => {
                    changed["radial_designer"]
                        .as_object_mut()
                        .unwrap()
                        .remove("window_size");
                }
                "designer scale removal" => {
                    changed["radial_designer"]
                        .as_object_mut()
                        .unwrap()
                        .remove("window_scale_factor");
                }
                "pinned order" => {
                    changed["pinned_panels"].as_array_mut().unwrap().reverse();
                }
                _ => {
                    changed["plugin_settings"]["custom"]["enabled_plugins"]
                        .as_array_mut()
                        .unwrap()
                        .reverse();
                }
            }
            fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
            let actual = read_profile_hashes(profile.path()).unwrap();
            assert_ne!(actual.settings_full, original.settings_full, "{cause}");
            assert_ne!(
                actual.settings_without_designer_geometry,
                original.settings_without_designer_geometry,
                "{cause}"
            );
            for kind in ProbeKind::ALL {
                let mut report = probe_report(kind);
                report.profile.settings_sha256 = original.settings.clone();
                report.profile.radial_sha256 = original.radial.clone();
                report.profile.actions_sha256 = original.actions.clone();
                receipt_mut(&mut report).unwrap().profile_before = original.clone();
                receipt_mut(&mut report).unwrap().profile_after = Some(actual.clone());
                assert!(
                    validate_probe(&report, true).is_err(),
                    "{kind:?} accepted {cause}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_full_settings_identity_protects_actual_migration_receipt_clipboard_and_launcher_geometry()
     {
        let (profile, mut fixture) = fresh_query_profile();
        prepare_fresh_controlled_fixture(profile.path(), &mut fixture).unwrap();
        let original = read_profile_hashes(profile.path()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fixture.settings_json).unwrap();
        assert!(value["radial_submenu_migration"].is_object());
        for pointer in [
            "/radial_submenu_migration/version",
            "/radial_submenu_migration/target_radial_revision",
            "/plugin_settings/clipboard_modify/dialog_width",
            "/plugin_settings/clipboard_modify/dialog_height",
            "/plugin_settings/clipboard_modify/hide_launcher_after_apply",
            "/window_size/0",
            "/static_size/0",
            "/static_pos/0",
        ] {
            let mut changed = value.clone();
            let field = changed.pointer_mut(pointer).unwrap();
            *field = if let Some(boolean) = field.as_bool() {
                serde_json::json!(!boolean)
            } else if let Some(integer) = field.as_u64() {
                serde_json::json!(integer + 1)
            } else {
                serde_json::json!(field.as_f64().unwrap() + 1.0)
            };
            fs::write(
                profile.path().join("settings.json"),
                serde_json::to_vec(&changed).unwrap(),
            )
            .unwrap();
            let actual = read_profile_hashes(profile.path()).unwrap();
            assert_ne!(actual.settings_full, original.settings_full, "{pointer}");
            for kind in ProbeKind::ALL {
                let mut report = probe_report(kind);
                report.profile.settings_sha256 = original.settings.clone();
                report.profile.radial_sha256 = original.radial.clone();
                report.profile.actions_sha256 = original.actions.clone();
                receipt_mut(&mut report).unwrap().profile_before = original.clone();
                receipt_mut(&mut report).unwrap().profile_after = Some(actual.clone());
                assert!(
                    validate_probe(&report, true).is_err(),
                    "{kind:?} accepted {pointer}"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn controlled_ui_geometry_hash_allows_only_normal_measured_designer_preferences() {
        let profile = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        for name in ["radial.json", "actions.json"] {
            fs::write(profile.path().join(name), b"{}").unwrap();
        }
        let path = profile.path().join("settings.json");
        fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
        let initial = read_profile_hashes(profile.path()).unwrap();
        settings.radial_designer.window_position = Some((120.0, 160.0));
        settings.radial_designer.window_size = (1100.0, 720.0);
        settings.radial_designer.window_scale_factor = Some(1.25);
        fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
        let moved = read_profile_hashes(profile.path()).unwrap();
        assert_ne!(moved.settings, initial.settings);
        assert_ne!(moved.settings_full, initial.settings_full);
        assert_eq!(
            moved.settings_without_designer_geometry,
            initial.settings_without_designer_geometry
        );
        let mut ui = probe_report(ProbeKind::Ui);
        ui.profile.settings_sha256 = initial.settings.clone();
        ui.profile.radial_sha256 = initial.radial.clone();
        ui.profile.actions_sha256 = initial.actions.clone();
        receipt_mut(&mut ui).unwrap().profile_before = initial.clone();
        receipt_mut(&mut ui).unwrap().profile_after = Some(moved.clone());
        validate_probe(&ui, true).unwrap();
        settings.radial_designer.show_skins = !settings.radial_designer.show_skins;
        fs::write(&path, serde_json::to_vec(&settings).unwrap()).unwrap();
        receipt_mut(&mut ui).unwrap().profile_after =
            Some(read_profile_hashes(profile.path()).unwrap());
        assert!(
            validate_probe(&ui, true).is_err(),
            "other Designer preferences remain protected"
        );
        let mut startup = probe_report(ProbeKind::Startup);
        startup.profile.settings_sha256 = initial.settings.clone();
        startup.profile.radial_sha256 = initial.radial.clone();
        startup.profile.actions_sha256 = initial.actions.clone();
        receipt_mut(&mut startup).unwrap().profile_before = initial;
        receipt_mut(&mut startup).unwrap().profile_after = Some(moved);
        assert!(
            validate_probe(&startup, true).is_err(),
            "non-UI full identity still protects all Designer geometry"
        );
    }

    #[test]
    fn controlled_early_environment_failure_keeps_local_inventory_and_cannot_prove_native_stage() {
        let mut report = super::super::tests::acceptance_report(PROBE_MODE);
        report.cases.clear();
        record_environment_failure(ProbeKind::Ui, "actual desktop attach failed", &mut report);
        assert_eq!(
            report
                .cases
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["N04", "CLEANUP"]
        );
        assert_eq!(
            report.cases[0].failure_stage,
            Some(FailureStage::Environment)
        );
        assert!(report.environment.child_process_id.is_none());
        assert!(!report.cleanup.child_closed_normally);
        assert!(!report.passed());
        assert!(validate_probe(&report, true).is_err());
    }
}
