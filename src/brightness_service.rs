//! Per-session physical-monitor brightness inventory and bounded helper service.
//!
//! This module deliberately does not change the legacy `bright <number>` path.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const MAX_LOGICAL_MONITORS: usize = 128;
const MAX_PHYSICAL_MONITORS: usize = 256;
const MAX_DEVICE_NAME_BYTES: usize = 256;
const MAX_DESCRIPTION_BYTES: usize = 512;
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const HELPER_RESPONSE_DEADLINE: Duration = Duration::from_secs(2);
const HELPER_START_DEADLINE: Duration = Duration::from_secs(1);
const HELPER_CLEANUP_DEADLINE: Duration = Duration::from_secs(1);
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrightnessSessionId(u128);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BrightnessRequestId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MonitorTargetId(u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhysicalMonitorIdentity {
    pub logical_device: String,
    pub logical_index: u32,
    pub physical_index: u32,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorTarget {
    pub session: BrightnessSessionId,
    pub generation: u64,
    pub id: MonitorTargetId,
    pub identity: PhysicalMonitorIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBrightnessRange {
    pub minimum: u32,
    pub current: u32,
    pub maximum: u32,
}

impl NativeBrightnessRange {
    pub fn validate(self) -> Result<Self, MonitorUnavailable> {
        if self.minimum >= self.maximum
            || self.current < self.minimum
            || self.current > self.maximum
        {
            return Err(MonitorUnavailable::InvalidNativeRange);
        }
        Ok(self)
    }

    pub fn percent(self) -> Result<u8, MonitorUnavailable> {
        self.validate()?;
        let span = u64::from(self.maximum - self.minimum);
        let offset = u64::from(self.current - self.minimum);
        Ok(((offset * 100 + span / 2) / span).min(100) as u8)
    }

    pub fn native_for_percent(self, percent: u8) -> Result<u32, MonitorUnavailable> {
        self.validate()?;
        if percent > 100 {
            return Err(MonitorUnavailable::InvalidNativeRange);
        }
        let span = u64::from(self.maximum - self.minimum);
        let scaled = (u64::from(percent) * span + 50) / 100;
        let native = u64::from(self.minimum) + scaled;
        u32::try_from(native).map_err(|_| MonitorUnavailable::InvalidNativeRange)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorUnavailable {
    NoPhysicalMonitors,
    EnumerationFailed { code: Option<u32> },
    PhysicalEnumerationFailed { code: Option<u32> },
    ReadFailed { code: Option<u32> },
    InvalidNativeRange,
    IdentityUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInventoryEntry {
    pub id: Option<MonitorTargetId>,
    pub identity: Option<PhysicalMonitorIdentity>,
    pub target: Option<MonitorTarget>,
    pub display_name: String,
    pub native_range: Option<NativeBrightnessRange>,
    pub percent: Option<u8>,
    pub unavailable: Option<MonitorUnavailable>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInventory {
    pub session: BrightnessSessionId,
    pub generation: u64,
    pub entries: Vec<MonitorInventoryEntry>,
    /// False means one or more monitor identities could not be enumerated, so
    /// no per-device Set can be safely authorized from this inventory.
    pub topology_complete: bool,
    topology: Vec<PhysicalMonitorIdentity>,
}

impl MonitorInventory {
    pub fn targets(&self) -> impl Iterator<Item = &MonitorTarget> {
        self.entries
            .iter()
            .filter_map(|entry| entry.target.as_ref())
    }

    pub fn target(&self, id: MonitorTargetId) -> Option<&MonitorTarget> {
        self.targets().find(|target| target.id == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrightnessFailureKind {
    Unsupported,
    NativeFailure,
    HandleCleanupFailure,
    InvalidRange,
    StaleTarget,
    SetOutcomeUnknown,
    ProtocolFailure,
    HelperStartFailure,
    HelperTimeout,
    HelperCleanupFailure,
    Superseded,
    Cancelled,
    ServiceShutdown,
}

/// Describes whether a failed Set request was definitely rejected before it
/// could reach the native brightness API, or whether its effect is unknown.
/// Successful Set requests are represented by [`BrightnessSetAcknowledgement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrightnessSetDisposition {
    NotAttempted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessSetFailureCause {
    pub kind: BrightnessFailureKind,
    pub operation: Option<BrightnessOperation>,
    pub native_code: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrightnessOperation {
    EnumerateLogical,
    GetPhysicalCount,
    GetPhysicalMonitors,
    ReadBrightness,
    SetBrightness,
    DestroyPhysicalMonitors,
    VerifyHelperExit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessFailure {
    pub kind: BrightnessFailureKind,
    pub operation: Option<BrightnessOperation>,
    pub native_code: Option<u32>,
    pub process_cleanup: Option<HelperCleanupDisposition>,
    #[serde(default)]
    pub set_disposition: Option<BrightnessSetDisposition>,
    #[serde(default)]
    pub set_cause: Option<BrightnessSetFailureCause>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelperCleanupDisposition {
    NormalExitVerified,
    ProcessExitVerified,
    ForcedTerminationExitVerified,
    ExitUnverified,
}

impl BrightnessFailure {
    fn new(kind: BrightnessFailureKind) -> Self {
        Self {
            kind,
            operation: None,
            native_code: None,
            process_cleanup: None,
            set_disposition: None,
            set_cause: None,
        }
    }

    fn with_set_disposition(mut self, disposition: BrightnessSetDisposition) -> Self {
        self.set_disposition = Some(disposition);
        if disposition == BrightnessSetDisposition::Unknown && self.set_cause.is_none() {
            self.set_cause = Some(BrightnessSetFailureCause {
                kind: self.kind,
                operation: self.operation,
                native_code: self.native_code,
            });
        }
        self
    }

    fn carrying_set_context_from(mut self, previous: &Self) -> Self {
        self.set_disposition = previous.set_disposition;
        self.set_cause = previous.set_cause.clone().or_else(|| {
            (previous.set_disposition == Some(BrightnessSetDisposition::Unknown)).then(|| {
                BrightnessSetFailureCause {
                    kind: previous.kind,
                    operation: previous.operation,
                    native_code: previous.native_code,
                }
            })
        });
        self
    }

    fn native(operation: BrightnessOperation, native_code: Option<u32>) -> Self {
        Self {
            kind: BrightnessFailureKind::NativeFailure,
            operation: Some(operation),
            native_code,
            process_cleanup: None,
            set_disposition: None,
            set_cause: None,
        }
    }

    fn handle_cleanup(native_code: Option<u32>) -> Self {
        Self {
            kind: BrightnessFailureKind::HandleCleanupFailure,
            operation: Some(BrightnessOperation::DestroyPhysicalMonitors),
            native_code,
            process_cleanup: None,
            set_disposition: None,
            set_cause: None,
        }
    }
}

/// Failure metadata for the parent/helper exchange. A write error can happen
/// after a prefix of the request reached the pipe, so all failures after the
/// write begins conservatively retain the possibility of native execution.
pub(super) struct HelperExchangeFailure {
    pub failure: BrightnessFailure,
    pub request_may_have_been_dispatched: bool,
}

impl HelperExchangeFailure {
    pub fn dispatched(failure: BrightnessFailure) -> Self {
        Self {
            failure,
            request_may_have_been_dispatched: true,
        }
    }
}

impl From<BrightnessFailure> for HelperExchangeFailure {
    fn from(failure: BrightnessFailure) -> Self {
        Self {
            failure,
            request_may_have_been_dispatched: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessSetAcknowledgement {
    pub target: MonitorTarget,
    pub requested_percent: u8,
    pub native_range_before: NativeBrightnessRange,
    pub applied_native: u32,
    pub readback: NativeBrightnessRange,
    pub readback_percent: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrightnessCompletion {
    Refreshed(Result<MonitorInventory, BrightnessFailure>),
    Set(Result<BrightnessSetAcknowledgement, BrightnessFailure>),
    Superseded { by: BrightnessRequestId },
    Cancelled,
}

pub struct BrightnessRequestTicket {
    id: BrightnessRequestId,
    receiver: mpsc::Receiver<BrightnessCompletion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrightnessShutdownReport {
    pub helper_cleanup: Option<HelperCleanupDisposition>,
}

impl BrightnessRequestTicket {
    pub fn id(&self) -> BrightnessRequestId {
        self.id
    }

    pub fn try_recv(&self) -> Option<BrightnessCompletion> {
        self.receiver.try_recv().ok()
    }
}

#[derive(Clone, Debug)]
pub struct LogicalMonitor {
    pub index: u32,
    pub device_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalMonitor {
    pub physical_index: u32,
    pub description: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeFailure {
    pub operation: BrightnessOperation,
    pub code: Option<u32>,
}

pub trait PhysicalMonitorLease {
    fn monitors(&self) -> &[PhysicalMonitor];
    fn read_brightness(&mut self, index: usize) -> Result<NativeBrightnessRange, NativeFailure>;
    fn set_brightness(&mut self, index: usize, value: u32) -> Result<(), NativeFailure>;
    fn destroy(self: Box<Self>) -> Result<(), NativeFailure>;
}

pub trait NativeMonitorApi {
    type Lease: PhysicalMonitorLease;

    fn enumerate_logical(&mut self) -> Result<Vec<LogicalMonitor>, NativeFailure>;
    fn acquire_physical(&mut self, logical: &LogicalMonitor) -> Result<Self::Lease, NativeFailure>;
}

struct AcquiredGroup<L> {
    logical: LogicalMonitor,
    lease: L,
}

struct Scan<L> {
    inventory: MonitorInventory,
    groups: Vec<AcquiredGroup<L>>,
}

#[derive(Clone)]
pub struct BrightnessCancellation(Arc<AtomicBool>);

impl BrightnessCancellation {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for BrightnessCancellation {
    fn default() -> Self {
        Self::new()
    }
}

fn safe_name(value: &str, maximum: usize) -> String {
    let mut output = String::new();
    let mut characters = 0usize;
    for character in value.chars().filter(|character| !character.is_control()) {
        if characters >= maximum || output.len().saturating_add(character.len_utf8()) > maximum {
            break;
        }
        output.push(character);
        characters += 1;
    }
    output
}

fn identity(
    logical: &LogicalMonitor,
    physical: &PhysicalMonitor,
) -> Result<PhysicalMonitorIdentity, MonitorUnavailable> {
    let logical_device = logical
        .device_name
        .as_deref()
        .ok_or(MonitorUnavailable::IdentityUnavailable)?;
    if logical_device.is_empty()
        || logical_device.len() > MAX_DEVICE_NAME_BYTES
        || physical.description.len() > MAX_DESCRIPTION_BYTES
        || logical_device.chars().any(char::is_control)
        || physical.description.chars().any(char::is_control)
    {
        return Err(MonitorUnavailable::IdentityUnavailable);
    }
    Ok(PhysicalMonitorIdentity {
        logical_device: logical_device.to_owned(),
        logical_index: logical.index,
        physical_index: physical.physical_index,
        description: physical.description.clone(),
    })
}

fn scan_inventory<B: NativeMonitorApi>(
    api: &mut B,
    session: BrightnessSessionId,
    generation: u64,
    cancellation: &BrightnessCancellation,
) -> Result<Scan<B::Lease>, BrightnessFailure> {
    let logicals = api
        .enumerate_logical()
        .map_err(|failure| BrightnessFailure::native(failure.operation, failure.code))?;
    if logicals.len() > MAX_LOGICAL_MONITORS {
        return Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported));
    }
    if cancellation.is_cancelled() {
        return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
    }

    let mut entries = Vec::new();
    let mut topology = Vec::new();
    let mut groups = Vec::new();
    let mut observed_fingerprints = HashSet::new();
    let mut topology_complete = true;
    let mut next_id = 0u32;
    let mut total_physical = 0usize;

    for logical in logicals {
        if cancellation.is_cancelled() {
            close_groups(&mut groups)?;
            return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
        }
        if logical
            .device_name
            .as_ref()
            .is_some_and(|name| name.len() > MAX_DEVICE_NAME_BYTES)
        {
            topology_complete = false;
        }
        let lease = match api.acquire_physical(&logical) {
            Ok(lease) => lease,
            Err(failure) => {
                topology_complete = false;
                entries.push(logical_issue(
                    &logical,
                    MonitorUnavailable::PhysicalEnumerationFailed { code: failure.code },
                ));
                continue;
            }
        };

        let device_count = lease.monitors().len();
        if device_count == 0 {
            entries.push(logical_issue(
                &logical,
                MonitorUnavailable::NoPhysicalMonitors,
            ));
            groups.push(AcquiredGroup { logical, lease });
            continue;
        }
        let Some(next_total) = total_physical.checked_add(device_count) else {
            groups.push(AcquiredGroup { logical, lease });
            close_groups(&mut groups)?;
            return Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported));
        };
        if next_total > MAX_PHYSICAL_MONITORS {
            groups.push(AcquiredGroup { logical, lease });
            close_groups(&mut groups)?;
            return Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported));
        }
        total_physical = next_total;
        let devices = lease.monitors().to_vec();

        let mut group = AcquiredGroup {
            logical: logical.clone(),
            lease,
        };
        for (physical_index, device) in devices.iter().enumerate() {
            if cancellation.is_cancelled() {
                if let Err(failure) = Box::new(group.lease).destroy() {
                    let _ = close_groups(&mut groups);
                    return Err(BrightnessFailure::handle_cleanup(failure.code));
                }
                close_groups(&mut groups)?;
                return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
            }
            let target_identity = match identity(&logical, device) {
                Ok(identity) => identity,
                Err(unavailable) => {
                    topology_complete = false;
                    entries.push(logical_issue(&logical, unavailable));
                    continue;
                }
            };
            // The API gives us only a logical device name and the monitor's
            // description, not a durable physical-device identifier. Two
            // identical descriptions on one logical monitor therefore cannot
            // safely be distinguished by their enumeration ordinal: that
            // ordinal may point at the sibling after a fresh acquisition.
            if !observed_fingerprints.insert((
                target_identity.logical_device.clone(),
                target_identity.description.clone(),
            )) {
                topology_complete = false;
            }
            let id = MonitorTargetId(next_id);
            let Some(following_id) = next_id.checked_add(1) else {
                if let Err(failure) = Box::new(group.lease).destroy() {
                    let _ = close_groups(&mut groups);
                    return Err(BrightnessFailure::handle_cleanup(failure.code));
                }
                close_groups(&mut groups)?;
                return Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported));
            };
            next_id = following_id;
            topology.push(target_identity.clone());
            let display_name = if device.description.trim().is_empty() {
                format!(
                    "{} · physical {}",
                    target_identity.logical_device,
                    physical_index + 1
                )
            } else {
                format!(
                    "{} · {} · physical {}",
                    target_identity.logical_device,
                    safe_name(&device.description, MAX_DESCRIPTION_BYTES),
                    physical_index + 1
                )
            };
            let mut entry = MonitorInventoryEntry {
                id: Some(id),
                identity: Some(target_identity.clone()),
                target: Some(MonitorTarget {
                    session,
                    generation,
                    id,
                    identity: target_identity,
                }),
                display_name,
                native_range: None,
                percent: None,
                unavailable: None,
            };
            match group.lease.read_brightness(physical_index) {
                Ok(range) => match range.validate() {
                    Ok(range) => match range.percent() {
                        Ok(percent) => {
                            entry.native_range = Some(range);
                            entry.percent = Some(percent);
                        }
                        Err(_) => entry.unavailable = Some(MonitorUnavailable::InvalidNativeRange),
                    },
                    Err(unavailable) => entry.unavailable = Some(unavailable),
                },
                Err(failure) => {
                    entry.unavailable = Some(MonitorUnavailable::ReadFailed { code: failure.code });
                }
            }
            entries.push(entry);
        }
        groups.push(group);
    }

    if cancellation.is_cancelled() {
        close_groups(&mut groups)?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
    }
    for index in 0..topology.len() {
        if topology[index + 1..].contains(&topology[index]) {
            topology_complete = false;
            break;
        }
    }

    Ok(Scan {
        inventory: MonitorInventory {
            session,
            generation,
            entries,
            topology_complete,
            topology,
        },
        groups,
    })
}

fn logical_issue(
    logical: &LogicalMonitor,
    unavailable: MonitorUnavailable,
) -> MonitorInventoryEntry {
    let name = logical
        .device_name
        .as_deref()
        .map(|name| safe_name(name, 64))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("Display {}", logical.index.saturating_add(1)));
    MonitorInventoryEntry {
        id: None,
        identity: None,
        target: None,
        display_name: name,
        native_range: None,
        percent: None,
        unavailable: Some(unavailable),
    }
}

fn close_groups<L: PhysicalMonitorLease>(
    groups: &mut Vec<AcquiredGroup<L>>,
) -> Result<(), BrightnessFailure> {
    let mut first_error = None;
    for group in groups.drain(..) {
        if let Err(failure) = Box::new(group.lease).destroy() {
            first_error.get_or_insert_with(|| BrightnessFailure::handle_cleanup(failure.code));
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn refresh_with_backend<B: NativeMonitorApi>(
    api: &mut B,
    session: BrightnessSessionId,
    generation: u64,
    cancellation: &BrightnessCancellation,
) -> Result<MonitorInventory, BrightnessFailure> {
    let mut scan = scan_inventory(api, session, generation, cancellation)?;
    close_groups(&mut scan.groups)?;
    Ok(scan.inventory)
}

fn set_with_backend<B: NativeMonitorApi>(
    api: &mut B,
    expected: &MonitorInventory,
    requested: &MonitorTarget,
    percent: u8,
    cancellation: &BrightnessCancellation,
) -> Result<BrightnessSetAcknowledgement, BrightnessFailure> {
    set_with_backend_inner(api, expected, requested, percent, cancellation).map_err(|failure| {
        let disposition = failure
            .set_disposition
            .unwrap_or(BrightnessSetDisposition::NotAttempted);
        failure.with_set_disposition(disposition)
    })
}

fn set_with_backend_inner<B: NativeMonitorApi>(
    api: &mut B,
    expected: &MonitorInventory,
    requested: &MonitorTarget,
    percent: u8,
    cancellation: &BrightnessCancellation,
) -> Result<BrightnessSetAcknowledgement, BrightnessFailure> {
    if percent > 100 {
        return Err(BrightnessFailure::new(BrightnessFailureKind::InvalidRange));
    }
    if requested.session != expected.session || requested.generation != expected.generation {
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    }
    let Some(expected_target) = expected.target(requested.id) else {
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    };
    if expected_target != requested || !expected.topology_complete {
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    }
    let mut scan = scan_inventory(api, expected.session, expected.generation, cancellation)?;
    if !scan.inventory.topology_complete || scan.inventory.topology != expected.topology {
        let cleanup = close_groups(&mut scan.groups);
        cleanup?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    }
    let target_entry_index = scan
        .inventory
        .entries
        .iter()
        .position(|entry| entry.target.as_ref() == Some(requested));
    let Some(target_entry_index) = target_entry_index else {
        close_groups(&mut scan.groups)?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    };
    let Some(range) = scan.inventory.entries[target_entry_index].native_range else {
        close_groups(&mut scan.groups)?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported));
    };
    if cancellation.is_cancelled() {
        close_groups(&mut scan.groups)?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
    }
    let native_value = match range.native_for_percent(percent) {
        Ok(value) => value,
        Err(_) => {
            close_groups(&mut scan.groups)?;
            return Err(BrightnessFailure::new(BrightnessFailureKind::InvalidRange));
        }
    };
    let identity = &requested.identity;
    let mut target_result = None;
    for group in &mut scan.groups {
        if group.logical.index != identity.logical_index
            || group.logical.device_name.as_deref() != Some(identity.logical_device.as_str())
        {
            continue;
        }
        let Some(index) = group.lease.monitors().iter().position(|monitor| {
            monitor.physical_index == identity.physical_index
                && monitor.description == identity.description
        }) else {
            continue;
        };
        let set_result = group.lease.set_brightness(index, native_value);
        let readback = group.lease.read_brightness(index);
        target_result = Some((set_result, readback));
        break;
    }
    let Some((set_result, readback)) = target_result else {
        close_groups(&mut scan.groups)?;
        return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
    };
    let cleanup_result = close_groups(&mut scan.groups);
    if let Err(mut cleanup) = cleanup_result {
        cleanup.set_disposition = Some(BrightnessSetDisposition::Unknown);
        cleanup.set_cause = set_result
            .as_ref()
            .err()
            .or_else(|| readback.as_ref().err())
            .map(|failure| BrightnessSetFailureCause {
                kind: BrightnessFailureKind::NativeFailure,
                operation: Some(failure.operation),
                native_code: failure.code,
            });
        return Err(cleanup.with_set_disposition(BrightnessSetDisposition::Unknown));
    }
    match (set_result, readback) {
        (Ok(()), Ok(readback)) => {
            let readback = readback.validate().map_err(|_| BrightnessFailure {
                kind: BrightnessFailureKind::InvalidRange,
                operation: Some(BrightnessOperation::ReadBrightness),
                native_code: None,
                process_cleanup: None,
                set_disposition: Some(BrightnessSetDisposition::Unknown),
                set_cause: Some(BrightnessSetFailureCause {
                    kind: BrightnessFailureKind::InvalidRange,
                    operation: Some(BrightnessOperation::ReadBrightness),
                    native_code: None,
                }),
            })?;
            let readback_percent = readback.percent().map_err(|_| BrightnessFailure {
                kind: BrightnessFailureKind::InvalidRange,
                operation: Some(BrightnessOperation::ReadBrightness),
                native_code: None,
                process_cleanup: None,
                set_disposition: Some(BrightnessSetDisposition::Unknown),
                set_cause: Some(BrightnessSetFailureCause {
                    kind: BrightnessFailureKind::InvalidRange,
                    operation: Some(BrightnessOperation::ReadBrightness),
                    native_code: None,
                }),
            })?;
            Ok(BrightnessSetAcknowledgement {
                target: requested.clone(),
                requested_percent: percent,
                native_range_before: range,
                applied_native: native_value,
                readback,
                readback_percent,
            })
        }
        (Err(failure), _) | (_, Err(failure)) => Err(BrightnessFailure {
            kind: BrightnessFailureKind::NativeFailure,
            operation: Some(failure.operation),
            native_code: failure.code,
            process_cleanup: None,
            set_disposition: Some(BrightnessSetDisposition::Unknown),
            set_cause: Some(BrightnessSetFailureCause {
                kind: BrightnessFailureKind::NativeFailure,
                operation: Some(failure.operation),
                native_code: failure.code,
            }),
        }),
    }
}

trait HelperSession: Send {
    fn session_id(&self) -> BrightnessSessionId;
    fn refresh(
        &mut self,
        request_id: BrightnessRequestId,
        generation: u64,
        cancellation: &BrightnessCancellation,
        deadline: Instant,
    ) -> Result<MonitorInventory, BrightnessFailure>;
    fn set(
        &mut self,
        request_id: BrightnessRequestId,
        target: &MonitorTarget,
        percent: u8,
        cancellation: &BrightnessCancellation,
        deadline: Instant,
    ) -> Result<BrightnessSetAcknowledgement, BrightnessFailure>;
    fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> Result<HelperCleanupDisposition, BrightnessFailure>;
}

trait HelperFactory: Send + 'static {
    fn start(
        &mut self,
        session: BrightnessSessionId,
        token: String,
        deadline: Instant,
    ) -> Result<Box<dyn HelperSession>, BrightnessFailure>;
}

#[derive(Clone, Copy)]
struct ServiceTiming {
    startup: Duration,
    operation: Duration,
    cleanup: Duration,
}

impl ServiceTiming {
    fn production() -> Self {
        Self {
            startup: HELPER_START_DEADLINE,
            operation: HELPER_RESPONSE_DEADLINE,
            cleanup: HELPER_CLEANUP_DEADLINE,
        }
    }
}

enum ServiceRequest {
    Refresh,
    Set { target: MonitorTarget, percent: u8 },
}

struct QueuedRequest {
    id: BrightnessRequestId,
    request: ServiceRequest,
    reply: mpsc::Sender<BrightnessCompletion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Envelope {
    version: u16,
    token: String,
    session: BrightnessSessionId,
    request_id: BrightnessRequestId,
    generation: u64,
    body: Message,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Message {
    Hello { request_pipe_capacity: u32 },
    Refresh,
    Set { target: MonitorTarget, percent: u8 },
    Shutdown,
    Refreshed(Result<MonitorInventory, BrightnessFailure>),
    SetResult(Result<BrightnessSetAcknowledgement, BrightnessFailure>),
    ShutdownAck,
}

#[derive(Default)]
struct MailboxState {
    pending: Option<QueuedRequest>,
    active_cancel: Option<BrightnessCancellation>,
    shutting_down: bool,
    shutdown_failure: Option<BrightnessFailure>,
    shutdown_disposition: Option<HelperCleanupDisposition>,
}

struct Mailbox {
    state: Mutex<MailboxState>,
    ready: Condvar,
}

pub struct BrightnessService {
    mailbox: Arc<Mailbox>,
    next_request: AtomicU64,
    worker: Option<JoinHandle<()>>,
}

impl BrightnessService {
    pub fn new() -> Self {
        Self::with_factory(default_helper_factory(), ServiceTiming::production())
    }

    fn with_factory(factory: Box<dyn HelperFactory>, timing: ServiceTiming) -> Self {
        let mailbox = Arc::new(Mailbox {
            state: Mutex::new(MailboxState::default()),
            ready: Condvar::new(),
        });
        let worker_mailbox = Arc::clone(&mailbox);
        let worker = thread::Builder::new()
            .name("brightness-service".into())
            .spawn(move || service_worker(worker_mailbox, factory, timing))
            .ok();
        Self {
            mailbox,
            next_request: AtomicU64::new(1),
            worker,
        }
    }

    pub fn refresh(&self) -> Result<BrightnessRequestTicket, BrightnessFailure> {
        self.enqueue(ServiceRequest::Refresh)
    }

    pub fn set(
        &self,
        target: &MonitorTarget,
        percent: u8,
    ) -> Result<BrightnessRequestTicket, BrightnessFailure> {
        self.enqueue(ServiceRequest::Set {
            target: target.clone(),
            percent,
        })
    }

    fn enqueue(
        &self,
        request: ServiceRequest,
    ) -> Result<BrightnessRequestTicket, BrightnessFailure> {
        let raw_id = self
            .next_request
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::Unsupported))?;
        let id = BrightnessRequestId(raw_id);
        let (reply, receiver) = mpsc::channel();
        let mut state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.shutting_down || self.worker.is_none() {
            return Err(BrightnessFailure::new(
                BrightnessFailureKind::ServiceShutdown,
            ));
        }
        if let Some(superseded) = state.pending.take() {
            let _ = superseded
                .reply
                .send(BrightnessCompletion::Superseded { by: id });
        }
        state.pending = Some(QueuedRequest { id, request, reply });
        self.mailbox.ready.notify_one();
        Ok(BrightnessRequestTicket { id, receiver })
    }

    /// Stops accepting requests, cancels the active operation, and cancels
    /// any coalesced request without waiting for native cleanup. Owners that
    /// close asynchronously should retain this service and call
    /// [`Self::try_join_shutdown`] until it returns a report.
    pub fn request_shutdown(&self) {
        let mut state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.shutting_down {
            state.shutting_down = true;
            if let Some(active) = state.active_cancel.as_ref() {
                active.cancel();
            }
            if let Some(pending) = state.pending.take() {
                let _ = pending.reply.send(BrightnessCompletion::Cancelled);
            }
            self.mailbox.ready.notify_all();
        }
    }

    /// After [`Self::request_shutdown`], joins the worker only after it has
    /// actually finished. Returns `None` before shutdown is requested or while
    /// cleanup remains in progress; the JoinHandle stays owned by this service
    /// until it can be joined.
    pub fn try_join_shutdown(
        &mut self,
    ) -> Result<Option<BrightnessShutdownReport>, BrightnessFailure> {
        let is_shutting_down = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .shutting_down;
        if !is_shutting_down
            || self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
        {
            return Ok(None);
        }
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            let failure = BrightnessFailure::new(BrightnessFailureKind::HelperCleanupFailure);
            let mut state = self
                .mailbox
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state
                .shutdown_failure
                .get_or_insert_with(|| failure.clone());
            return Err(failure);
        }
        self.shutdown_report().map(Some)
    }

    /// Stops accepting work and waits for the bounded child cleanup. UI owners
    /// should use `request_shutdown`/`try_join_shutdown` instead of blocking a
    /// frame; this method is retained for final teardown and Drop.
    pub fn shutdown(&mut self) -> Result<BrightnessShutdownReport, BrightnessFailure> {
        self.request_shutdown();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            let failure = BrightnessFailure::new(BrightnessFailureKind::HelperCleanupFailure);
            let mut state = self
                .mailbox
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state
                .shutdown_failure
                .get_or_insert_with(|| failure.clone());
            return Err(failure);
        }
        self.shutdown_report()
    }

    fn shutdown_report(&self) -> Result<BrightnessShutdownReport, BrightnessFailure> {
        let state = self
            .mailbox
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match state.shutdown_failure.clone() {
            Some(failure) => Err(failure),
            None => Ok(BrightnessShutdownReport {
                helper_cleanup: state.shutdown_disposition,
            }),
        }
    }
}

impl Default for BrightnessService {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for BrightnessService {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn service_worker(
    mailbox: Arc<Mailbox>,
    mut factory: Box<dyn HelperFactory>,
    timing: ServiceTiming,
) {
    let mut helper: Option<Box<dyn HelperSession>> = None;
    let mut last_inventory: Option<MonitorInventory> = None;
    loop {
        let queued = {
            let mut state = mailbox
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            while state.pending.is_none() && !state.shutting_down {
                state = mailbox
                    .ready
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if state.shutting_down {
                None
            } else {
                let request = state.pending.take();
                if let Some(queued) = request.as_ref() {
                    let _ = queued.id;
                    state.active_cancel = Some(BrightnessCancellation::new());
                }
                request.map(|queued| {
                    let cancellation = state.active_cancel.as_ref().cloned().unwrap_or_default();
                    (queued, cancellation)
                })
            }
        };
        let Some((queued, cancellation)) = queued else {
            break;
        };
        let completion = match queued.request {
            ServiceRequest::Refresh => {
                if helper.is_none() {
                    let session = BrightnessSessionId(rand::random());
                    let token = format!("{:032x}", rand::random::<u128>());
                    let startup_deadline = Instant::now() + timing.startup;
                    match factory.start(session, token, startup_deadline) {
                        Ok(started)
                            if started.session_id() == session
                                && Instant::now() < startup_deadline =>
                        {
                            helper = Some(started)
                        }
                        Ok(mut started) => {
                            let mut failure = if started.session_id() != session {
                                BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure)
                            } else {
                                BrightnessFailure::new(BrightnessFailureKind::HelperTimeout)
                            };
                            match started.shutdown(Instant::now() + timing.cleanup) {
                                Ok(disposition) => {
                                    failure.process_cleanup = Some(disposition);
                                }
                                Err(cleanup_failure) => failure = cleanup_failure,
                            }
                            publish_completion(
                                &mailbox,
                                queued.reply,
                                &cancellation,
                                BrightnessCompletion::Refreshed(Err(failure)),
                            );
                            continue;
                        }
                        Err(failure) => {
                            publish_completion(
                                &mailbox,
                                queued.reply,
                                &cancellation,
                                BrightnessCompletion::Refreshed(Err(failure)),
                            );
                            continue;
                        }
                    }
                }
                let generation = last_inventory
                    .as_ref()
                    .map(|inventory| inventory.generation.checked_add(1))
                    .unwrap_or(Some(1));
                if generation.is_none() {
                    last_inventory = None;
                    let mut failure = BrightnessFailure::new(BrightnessFailureKind::Unsupported);
                    if let Err(cleanup_failure) =
                        retire_helper(&mut helper, Instant::now() + timing.cleanup, &mut failure)
                    {
                        failure = cleanup_failure;
                    }
                    BrightnessCompletion::Refreshed(Err(failure))
                } else {
                    let generation = generation.unwrap_or(1);
                    let operation_deadline = Instant::now() + timing.operation;
                    let result = helper.as_mut().map_or_else(
                        || {
                            Err(BrightnessFailure::new(
                                BrightnessFailureKind::HelperStartFailure,
                            ))
                        },
                        |session| {
                            session.refresh(
                                queued.id,
                                generation,
                                &cancellation,
                                operation_deadline,
                            )
                        },
                    );
                    let result = guard_helper_result(result, &cancellation, operation_deadline);
                    match result {
                        Ok(inventory)
                            if inventory.generation == generation
                                && helper.as_ref().is_some_and(|session| {
                                    inventory.session == session.session_id()
                                })
                                && validate_inventory_shape(&inventory).is_ok() =>
                        {
                            last_inventory = Some(inventory.clone());
                            BrightnessCompletion::Refreshed(Ok(inventory))
                        }
                        Ok(_) => {
                            let mut failure =
                                BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure);
                            if let Err(cleanup_failure) = retire_helper(
                                &mut helper,
                                Instant::now() + timing.cleanup,
                                &mut failure,
                            ) {
                                failure = cleanup_failure;
                            }
                            last_inventory = None;
                            BrightnessCompletion::Refreshed(Err(failure))
                        }
                        Err(mut failure) => {
                            if helper_failure_loses_session(failure.kind) {
                                if let Err(cleanup_failure) = retire_helper(
                                    &mut helper,
                                    Instant::now() + timing.cleanup,
                                    &mut failure,
                                ) {
                                    failure = cleanup_failure;
                                }
                                last_inventory = None;
                            }
                            BrightnessCompletion::Refreshed(Err(failure))
                        }
                    }
                }
            }
            ServiceRequest::Set { target, percent } => {
                let Some(inventory) = last_inventory.as_ref() else {
                    publish_completion(
                        &mailbox,
                        queued.reply,
                        &cancellation,
                        BrightnessCompletion::Set(Err(BrightnessFailure::new(
                            BrightnessFailureKind::StaleTarget,
                        )
                        .with_set_disposition(BrightnessSetDisposition::NotAttempted))),
                    );
                    continue;
                };
                if target.session != inventory.session
                    || target.generation != inventory.generation
                    || !inventory.topology_complete
                    || inventory.target(target.id) != Some(&target)
                {
                    BrightnessCompletion::Set(Err(BrightnessFailure::new(
                        BrightnessFailureKind::StaleTarget,
                    )
                    .with_set_disposition(BrightnessSetDisposition::NotAttempted)))
                } else if inventory
                    .entries
                    .iter()
                    .find(|entry| entry.target.as_ref() == Some(&target))
                    .and_then(|entry| entry.native_range)
                    .is_none()
                {
                    BrightnessCompletion::Set(Err(BrightnessFailure::new(
                        BrightnessFailureKind::Unsupported,
                    )
                    .with_set_disposition(BrightnessSetDisposition::NotAttempted)))
                } else if let Some(session) = helper.as_mut() {
                    let operation_deadline = Instant::now() + timing.operation;
                    let result = session.set(
                        queued.id,
                        &target,
                        percent,
                        &cancellation,
                        operation_deadline,
                    );
                    match guard_set_helper_result(result, &cancellation, operation_deadline) {
                        Ok(ack)
                            if ack.target == target
                                && ack.requested_percent == percent
                                && ack.native_range_before.validate().is_ok()
                                && ack.readback.validate().is_ok()
                                && ack.readback.percent().ok() == Some(ack.readback_percent)
                                && ack.native_range_before.native_for_percent(percent).ok()
                                    == Some(ack.applied_native) =>
                        {
                            if let Some(updated) = last_inventory.as_mut()
                                && let Some(entry) = updated
                                    .entries
                                    .iter_mut()
                                    .find(|entry| entry.target.as_ref() == Some(&target))
                            {
                                entry.native_range = Some(ack.readback);
                                entry.percent = Some(ack.readback_percent);
                            }
                            BrightnessCompletion::Set(Ok(ack))
                        }
                        Ok(_) => {
                            let mut failure =
                                BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure)
                                    .with_set_disposition(BrightnessSetDisposition::Unknown);
                            if let Err(cleanup_failure) = retire_helper(
                                &mut helper,
                                Instant::now() + timing.cleanup,
                                &mut failure,
                            ) {
                                failure = cleanup_failure.carrying_set_context_from(&failure);
                            }
                            last_inventory = None;
                            BrightnessCompletion::Set(Err(failure))
                        }
                        Err(mut failure) => {
                            let invalidates_inventory = matches!(
                                failure.kind,
                                BrightnessFailureKind::StaleTarget
                                    | BrightnessFailureKind::SetOutcomeUnknown
                                    | BrightnessFailureKind::HandleCleanupFailure
                            ) || failure.set_disposition
                                == Some(BrightnessSetDisposition::Unknown)
                                || helper_failure_loses_session(failure.kind);
                            if invalidates_inventory {
                                last_inventory = None;
                                if let Err(cleanup_failure) = retire_helper(
                                    &mut helper,
                                    Instant::now() + timing.cleanup,
                                    &mut failure,
                                ) {
                                    failure = cleanup_failure.carrying_set_context_from(&failure);
                                }
                            }
                            BrightnessCompletion::Set(Err(failure))
                        }
                    }
                } else {
                    BrightnessCompletion::Set(Err(BrightnessFailure::new(
                        BrightnessFailureKind::StaleTarget,
                    )
                    .with_set_disposition(BrightnessSetDisposition::NotAttempted)))
                }
            }
        };
        publish_completion(&mailbox, queued.reply, &cancellation, completion);
    }

    if let Some(mut current) = helper {
        let outcome = current.shutdown(Instant::now() + timing.cleanup);
        let mut state = mailbox
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match outcome {
            Ok(disposition) => state.shutdown_disposition = Some(disposition),
            Err(failure) => state.shutdown_failure = Some(failure),
        }
    }
}

fn publish_completion(
    mailbox: &Mailbox,
    reply: mpsc::Sender<BrightnessCompletion>,
    cancellation: &BrightnessCancellation,
    mut completion: BrightnessCompletion,
) {
    let mut state = mailbox
        .state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if (state.shutting_down || cancellation.is_cancelled()) && completion_is_success(&completion) {
        completion = match completion {
            BrightnessCompletion::Refreshed(Ok(_)) => BrightnessCompletion::Refreshed(Err(
                BrightnessFailure::new(BrightnessFailureKind::Cancelled),
            )),
            BrightnessCompletion::Set(Ok(_)) => BrightnessCompletion::Set(Err(
                BrightnessFailure::new(BrightnessFailureKind::Cancelled)
                    .with_set_disposition(BrightnessSetDisposition::Unknown),
            )),
            completion => completion,
        };
    }
    let cleanup_disposition = match &completion {
        BrightnessCompletion::Refreshed(Err(failure)) | BrightnessCompletion::Set(Err(failure)) => {
            failure.process_cleanup
        }
        _ => None,
    };
    if cleanup_disposition.is_some() {
        state.shutdown_disposition = cleanup_disposition;
    }
    let _ = reply.send(completion);
    state.active_cancel = None;
    mailbox.ready.notify_all();
}

fn completion_is_success(completion: &BrightnessCompletion) -> bool {
    matches!(
        completion,
        BrightnessCompletion::Refreshed(Ok(_)) | BrightnessCompletion::Set(Ok(_))
    )
}

fn helper_failure_loses_session(kind: BrightnessFailureKind) -> bool {
    matches!(
        kind,
        BrightnessFailureKind::HelperTimeout
            | BrightnessFailureKind::HelperCleanupFailure
            | BrightnessFailureKind::HandleCleanupFailure
            | BrightnessFailureKind::StaleTarget
            | BrightnessFailureKind::SetOutcomeUnknown
            | BrightnessFailureKind::ProtocolFailure
            | BrightnessFailureKind::Cancelled
    )
}

fn guard_helper_result<T>(
    result: Result<T, BrightnessFailure>,
    cancellation: &BrightnessCancellation,
    deadline: Instant,
) -> Result<T, BrightnessFailure> {
    match result {
        Ok(_) if cancellation.is_cancelled() => {
            Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled))
        }
        Ok(_) if Instant::now() >= deadline => {
            Err(BrightnessFailure::new(BrightnessFailureKind::HelperTimeout))
        }
        result => result,
    }
}

fn guard_set_helper_result<T>(
    result: Result<T, BrightnessFailure>,
    cancellation: &BrightnessCancellation,
    deadline: Instant,
) -> Result<T, BrightnessFailure> {
    match result {
        Ok(_) if cancellation.is_cancelled() => {
            Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled)
                .with_set_disposition(BrightnessSetDisposition::Unknown))
        }
        Ok(_) if Instant::now() >= deadline => {
            Err(BrightnessFailure::new(BrightnessFailureKind::HelperTimeout)
                .with_set_disposition(BrightnessSetDisposition::Unknown))
        }
        Err(failure) if failure.set_disposition.is_none() => {
            Err(failure.with_set_disposition(BrightnessSetDisposition::Unknown))
        }
        result => result,
    }
}

fn retire_helper(
    helper: &mut Option<Box<dyn HelperSession>>,
    cleanup_deadline: Instant,
    failure: &mut BrightnessFailure,
) -> Result<(), BrightnessFailure> {
    if let Some(mut current) = helper.take() {
        match current.shutdown(cleanup_deadline) {
            Ok(disposition) => failure.process_cleanup = Some(disposition),
            Err(cleanup_failure) => return Err(cleanup_failure),
        }
    }
    Ok(())
}

fn validate_inventory_shape(inventory: &MonitorInventory) -> Result<(), BrightnessFailure> {
    if inventory.entries.len() > MAX_PHYSICAL_MONITORS + MAX_LOGICAL_MONITORS
        || inventory.topology.len() > MAX_PHYSICAL_MONITORS
        || inventory
            .entries
            .iter()
            .filter_map(|entry| entry.target.as_ref())
            .any(|target| {
                target.session != inventory.session
                    || target.generation != inventory.generation
                    || target.identity.logical_device.len() > MAX_DEVICE_NAME_BYTES
                    || target.identity.description.len() > MAX_DESCRIPTION_BYTES
            })
    {
        return Err(BrightnessFailure::new(
            BrightnessFailureKind::ProtocolFailure,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn default_helper_factory() -> Box<dyn HelperFactory> {
    Box::new(windows_process::WindowsHelperFactory)
}

#[cfg(not(windows))]
fn default_helper_factory() -> Box<dyn HelperFactory> {
    Box::new(UnsupportedHelperFactory)
}

struct UnsupportedHelperFactory;

impl HelperFactory for UnsupportedHelperFactory {
    fn start(
        &mut self,
        _session: BrightnessSessionId,
        _token: String,
        _deadline: Instant,
    ) -> Result<Box<dyn HelperSession>, BrightnessFailure> {
        Err(BrightnessFailure::new(BrightnessFailureKind::Unsupported))
    }
}

/// Runs only when the exact private helper switch is present. The caller must
/// invoke this before normal single-instance, settings, logging, and plugin startup.
pub fn run_internal_helper_if_requested() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    let first = args.next()?;
    if first != std::ffi::OsStr::new("--internal-brightness-helper") {
        return None;
    }
    let Some(token) = args.next().and_then(|value| value.into_string().ok()) else {
        return Some(2);
    };
    let Some(session_hex) = args.next().and_then(|value| value.into_string().ok()) else {
        return Some(2);
    };
    let Ok(session_value) = u128::from_str_radix(&session_hex, 16) else {
        return Some(2);
    };
    if args.next().is_some() || !valid_token(&token) || !valid_token(&session_hex) {
        return Some(2);
    }
    #[cfg(windows)]
    {
        Some(windows_process::run_helper(
            token,
            BrightnessSessionId(session_value),
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = (token, session_value);
        Some(2)
    }
}

fn valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_protocol_frame(bytes: &[u8]) -> Result<Envelope, BrightnessFailure> {
    if bytes.len() < 4 {
        return Err(BrightnessFailure::new(
            BrightnessFailureKind::ProtocolFailure,
        ));
    }
    let length = u32::from_le_bytes(
        bytes[..4]
            .try_into()
            .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?,
    ) as usize;
    if length == 0 || length > MAX_FRAME_BYTES || bytes.len() != length.saturating_add(4) {
        return Err(BrightnessFailure::new(
            BrightnessFailureKind::ProtocolFailure,
        ));
    }
    serde_json::from_slice(&bytes[4..])
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))
}

fn encode_protocol_frame(
    envelope: &Envelope,
    maximum: usize,
) -> Result<Vec<u8>, BrightnessFailure> {
    let payload = serde_json::to_vec(envelope)
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?;
    if payload.is_empty() || payload.len() > maximum {
        return Err(BrightnessFailure::new(
            BrightnessFailureKind::ProtocolFailure,
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?;
    let mut frame = Vec::with_capacity(payload.len().saturating_add(4));
    frame.extend_from_slice(&length.to_le_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

#[derive(Default)]
struct ProtocolFrameAccumulator {
    frame: Vec<u8>,
    expected_total: Option<usize>,
}

impl ProtocolFrameAccumulator {
    fn push(&mut self, chunk: &[u8]) -> Result<Option<Envelope>, BrightnessFailure> {
        let max_total = MAX_FRAME_BYTES.saturating_add(4);
        if self.frame.len().saturating_add(chunk.len()) > max_total {
            return Err(BrightnessFailure::new(
                BrightnessFailureKind::ProtocolFailure,
            ));
        }
        self.frame.extend_from_slice(chunk);
        if self.frame.len() >= 4 && self.expected_total.is_none() {
            let length = u32::from_le_bytes(
                self.frame[..4]
                    .try_into()
                    .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::ProtocolFailure))?,
            ) as usize;
            if length == 0 || length > MAX_FRAME_BYTES {
                return Err(BrightnessFailure::new(
                    BrightnessFailureKind::ProtocolFailure,
                ));
            }
            self.expected_total = Some(length + 4);
        }
        if self
            .expected_total
            .is_some_and(|total| self.frame.len() > total)
        {
            return Err(BrightnessFailure::new(
                BrightnessFailureKind::ProtocolFailure,
            ));
        }
        if self.expected_total == Some(self.frame.len()) {
            decode_protocol_frame(&self.frame).map(Some)
        } else {
            Ok(None)
        }
    }
}

fn envelope_matches(
    envelope: &Envelope,
    token: &str,
    session: BrightnessSessionId,
    request_id: BrightnessRequestId,
    generation: u64,
) -> bool {
    envelope.version == 1
        && envelope.token == token
        && envelope.session == session
        && envelope.request_id == request_id
        && envelope.generation == generation
}

#[cfg(windows)]
mod windows_process;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Condvar, MutexGuard};

    #[derive(Clone)]
    struct FakeGroupSpec {
        acquisition_error: Option<NativeFailure>,
        monitors: Vec<PhysicalMonitor>,
        readings: Vec<Result<NativeBrightnessRange, NativeFailure>>,
        set_failure: Option<NativeFailure>,
        destroy_failure: Option<NativeFailure>,
        destroy_failure_after_set: Option<NativeFailure>,
    }

    impl FakeGroupSpec {
        fn ready(monitors: Vec<PhysicalMonitor>, readings: Vec<NativeBrightnessRange>) -> Self {
            Self {
                acquisition_error: None,
                monitors,
                readings: readings.into_iter().map(Ok).collect(),
                set_failure: None,
                destroy_failure: None,
                destroy_failure_after_set: None,
            }
        }
    }

    #[derive(Default)]
    struct FakeNativeLog {
        destroyed_logicals: Vec<u32>,
        sets: Vec<(u32, usize, u32)>,
        reads: usize,
    }

    struct FakeNativeApi {
        logicals: Vec<LogicalMonitor>,
        groups: HashMap<u32, FakeGroupSpec>,
        log: Arc<Mutex<FakeNativeLog>>,
        cancel_on_read: Option<BrightnessCancellation>,
    }

    impl FakeNativeApi {
        fn new(logicals: Vec<LogicalMonitor>, groups: HashMap<u32, FakeGroupSpec>) -> Self {
            Self {
                logicals,
                groups,
                log: Arc::new(Mutex::new(FakeNativeLog::default())),
                cancel_on_read: None,
            }
        }
    }

    struct FakeNativeLease {
        logical_index: u32,
        spec: FakeGroupSpec,
        log: Arc<Mutex<FakeNativeLog>>,
        cancel_on_read: Option<BrightnessCancellation>,
    }

    impl PhysicalMonitorLease for FakeNativeLease {
        fn monitors(&self) -> &[PhysicalMonitor] {
            &self.spec.monitors
        }

        fn read_brightness(
            &mut self,
            index: usize,
        ) -> Result<NativeBrightnessRange, NativeFailure> {
            self.log.lock().unwrap().reads += 1;
            let result = self.spec.readings.get(index).cloned().unwrap_or_else(|| {
                Err(NativeFailure {
                    operation: BrightnessOperation::ReadBrightness,
                    code: None,
                })
            });
            if let Some(cancellation) = &self.cancel_on_read {
                cancellation.cancel();
            }
            result
        }

        fn set_brightness(&mut self, index: usize, value: u32) -> Result<(), NativeFailure> {
            self.log
                .lock()
                .unwrap()
                .sets
                .push((self.logical_index, index, value));
            if let Some(failure) = self.spec.set_failure {
                return Err(failure);
            }
            let Some(Ok(range)) = self.spec.readings.get_mut(index) else {
                return Err(NativeFailure {
                    operation: BrightnessOperation::SetBrightness,
                    code: None,
                });
            };
            range.current = value;
            self.spec.destroy_failure = self.spec.destroy_failure_after_set;
            Ok(())
        }

        fn destroy(self: Box<Self>) -> Result<(), NativeFailure> {
            self.log
                .lock()
                .unwrap()
                .destroyed_logicals
                .push(self.logical_index);
            self.spec.destroy_failure.map_or(Ok(()), Err)
        }
    }

    impl NativeMonitorApi for FakeNativeApi {
        type Lease = FakeNativeLease;

        fn enumerate_logical(&mut self) -> Result<Vec<LogicalMonitor>, NativeFailure> {
            Ok(self.logicals.clone())
        }

        fn acquire_physical(
            &mut self,
            logical: &LogicalMonitor,
        ) -> Result<Self::Lease, NativeFailure> {
            let Some(spec) = self.groups.get(&logical.index).cloned() else {
                return Err(NativeFailure {
                    operation: BrightnessOperation::GetPhysicalMonitors,
                    code: None,
                });
            };
            if let Some(failure) = spec.acquisition_error {
                return Err(failure);
            }
            Ok(FakeNativeLease {
                logical_index: logical.index,
                spec,
                log: Arc::clone(&self.log),
                cancel_on_read: self.cancel_on_read.clone(),
            })
        }
    }

    fn logical(index: u32) -> LogicalMonitor {
        LogicalMonitor {
            index,
            device_name: Some(format!("\\\\.\\DISPLAY{}", index + 1)),
        }
    }

    fn physical(index: u32, description: &str) -> PhysicalMonitor {
        PhysicalMonitor {
            physical_index: index,
            description: description.to_owned(),
        }
    }

    fn range(minimum: u32, current: u32, maximum: u32) -> NativeBrightnessRange {
        NativeBrightnessRange {
            minimum,
            current,
            maximum,
        }
    }

    #[test]
    fn brightness_inventory_enumerates_all_logical_and_physical_monitors() {
        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(
                vec![physical(0, "Primary panel"), physical(1, "Secondary panel")],
                vec![range(10, 60, 110), range(20, 70, 120)],
            ),
        );
        groups.insert(
            1,
            FakeGroupSpec::ready(vec![physical(0, "Office display")], vec![range(0, 25, 100)]),
        );
        groups.insert(2, FakeGroupSpec::ready(Vec::new(), Vec::new()));
        let mut api = FakeNativeApi::new(vec![logical(0), logical(1), logical(2)], groups);

        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(1),
            7,
            &BrightnessCancellation::new(),
        )
        .unwrap();

        assert_eq!(inventory.targets().count(), 3);
        assert_eq!(inventory.entries.len(), 4);
        assert_eq!(inventory.entries[0].percent, Some(50));
        assert_eq!(inventory.entries[1].percent, Some(50));
        assert_eq!(inventory.entries[2].percent, Some(25));
        assert!(inventory.entries[0].display_name.contains("physical 1"));
        assert!(inventory.entries[1].display_name.contains("physical 2"));
        assert_eq!(
            inventory.entries[0]
                .identity
                .as_ref()
                .unwrap()
                .logical_index,
            0
        );
        assert_eq!(
            inventory.entries[2]
                .identity
                .as_ref()
                .unwrap()
                .logical_index,
            1
        );
        assert_eq!(
            inventory.entries[3].unavailable,
            Some(MonitorUnavailable::NoPhysicalMonitors)
        );
        assert!(inventory.topology_complete);
        let destroyed = &api.log.lock().unwrap().destroyed_logicals;
        assert_eq!(destroyed.len(), 3);
        assert!(destroyed.contains(&0) && destroyed.contains(&1) && destroyed.contains(&2));
    }

    #[test]
    fn brightness_inventory_reports_unsupported_and_read_failures_truthfully() {
        let mut failed_acquire = FakeGroupSpec::ready(Vec::new(), Vec::new());
        failed_acquire.acquisition_error = Some(NativeFailure {
            operation: BrightnessOperation::GetPhysicalMonitors,
            code: Some(17),
        });
        let mut failed_read = FakeGroupSpec::ready(vec![physical(0, "Panel")], Vec::new());
        failed_read.readings = vec![Err(NativeFailure {
            operation: BrightnessOperation::ReadBrightness,
            code: Some(23),
        })];
        let mut groups = HashMap::new();
        groups.insert(0, failed_acquire);
        groups.insert(1, failed_read);
        let mut api = FakeNativeApi::new(vec![logical(0), logical(1)], groups);

        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(2),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();

        assert!(!inventory.topology_complete);
        assert_eq!(
            inventory.entries[0].unavailable,
            Some(MonitorUnavailable::PhysicalEnumerationFailed { code: Some(17) })
        );
        assert_eq!(
            inventory.entries[1].unavailable,
            Some(MonitorUnavailable::ReadFailed { code: Some(23) })
        );
        assert_eq!(
            inventory.entries[1].target.as_ref().unwrap().id,
            MonitorTargetId(0)
        );
        assert_eq!(api.log.lock().unwrap().destroyed_logicals, vec![1]);
    }

    #[test]
    fn brightness_inventory_rejects_overflowing_monitor_counts_and_closes_leases() {
        let logicals = (0..=MAX_LOGICAL_MONITORS)
            .map(|index| logical(index as u32))
            .collect();
        let mut logical_api = FakeNativeApi::new(logicals, HashMap::new());
        assert_eq!(
            refresh_with_backend(
                &mut logical_api,
                BrightnessSessionId(20),
                1,
                &BrightnessCancellation::new()
            )
            .unwrap_err()
            .kind,
            BrightnessFailureKind::Unsupported
        );
        assert!(
            logical_api
                .log
                .lock()
                .unwrap()
                .destroyed_logicals
                .is_empty()
        );

        let physicals = (0..=MAX_PHYSICAL_MONITORS)
            .map(|index| physical(index as u32, "Panel"))
            .collect();
        let mut groups = HashMap::new();
        groups.insert(0, FakeGroupSpec::ready(physicals, Vec::new()));
        let mut physical_api = FakeNativeApi::new(vec![logical(0)], groups);
        assert_eq!(
            refresh_with_backend(
                &mut physical_api,
                BrightnessSessionId(21),
                1,
                &BrightnessCancellation::new()
            )
            .unwrap_err()
            .kind,
            BrightnessFailureKind::Unsupported
        );
        assert_eq!(physical_api.log.lock().unwrap().destroyed_logicals, vec![0]);
    }

    #[test]
    fn brightness_inventory_refuses_ambiguous_duplicate_native_identities() {
        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(
                vec![physical(0, "Same panel"), physical(1, "Same panel")],
                vec![range(0, 40, 100), range(0, 50, 100)],
            ),
        );
        let mut api = FakeNativeApi::new(vec![logical(0)], groups);
        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(22),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        assert_eq!(inventory.targets().count(), 2);
        assert!(!inventory.topology_complete);
        let target = inventory.targets().next().unwrap().clone();
        assert_eq!(
            set_with_backend(
                &mut api,
                &inventory,
                &target,
                75,
                &BrightnessCancellation::new()
            )
            .unwrap_err()
            .kind,
            BrightnessFailureKind::StaleTarget
        );
        let state = api.log.lock().unwrap();
        assert!(state.sets.is_empty());
        assert_eq!(state.destroyed_logicals, vec![0]);

        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(vec![physical(0, "Same panel")], vec![range(0, 40, 100)]),
        );
        groups.insert(
            1,
            FakeGroupSpec::ready(vec![physical(0, "Same panel")], vec![range(0, 50, 100)]),
        );
        let mut separate_logicals = FakeNativeApi::new(vec![logical(0), logical(1)], groups);
        let inventory = refresh_with_backend(
            &mut separate_logicals,
            BrightnessSessionId(23),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        assert!(inventory.topology_complete);
        let target = inventory
            .targets()
            .find(|target| target.identity.logical_index == 1)
            .unwrap()
            .clone();
        assert!(
            set_with_backend(
                &mut separate_logicals,
                &inventory,
                &target,
                75,
                &BrightnessCancellation::new(),
            )
            .is_ok()
        );
        assert_eq!(separate_logicals.log.lock().unwrap().sets.len(), 1);
    }

    #[test]
    fn brightness_inventory_closes_leases_on_cancel_and_cleanup_failure() {
        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(vec![physical(0, "Panel")], vec![range(0, 50, 100)]),
        );
        let mut cancel_api = FakeNativeApi::new(vec![logical(0)], groups.clone());
        let cancellation = BrightnessCancellation::new();
        cancel_api.cancel_on_read = Some(cancellation.clone());
        let cancelled =
            refresh_with_backend(&mut cancel_api, BrightnessSessionId(3), 1, &cancellation);
        assert_eq!(
            cancelled.unwrap_err().kind,
            BrightnessFailureKind::Cancelled
        );
        assert_eq!(cancel_api.log.lock().unwrap().destroyed_logicals, vec![0]);

        let mut first = groups.remove(&0).unwrap();
        first.destroy_failure = Some(NativeFailure {
            operation: BrightnessOperation::DestroyPhysicalMonitors,
            code: Some(31),
        });
        let mut cleanup_groups = HashMap::new();
        cleanup_groups.insert(0, first);
        cleanup_groups.insert(
            1,
            FakeGroupSpec::ready(vec![physical(0, "Second")], vec![range(1, 10, 20)]),
        );
        let mut cleanup_api = FakeNativeApi::new(vec![logical(0), logical(1)], cleanup_groups);
        let cleanup = refresh_with_backend(
            &mut cleanup_api,
            BrightnessSessionId(4),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap_err();
        assert_eq!(cleanup.kind, BrightnessFailureKind::HandleCleanupFailure);
        assert_eq!(
            cleanup.operation,
            Some(BrightnessOperation::DestroyPhysicalMonitors)
        );
        assert_eq!(cleanup.native_code, Some(31));
        assert_eq!(cleanup_api.log.lock().unwrap().destroyed_logicals.len(), 2);
    }

    #[test]
    fn brightness_inventory_converts_native_ranges_and_refuses_stale_sets() {
        assert_eq!(range(10, 10, 110).percent(), Ok(0));
        assert_eq!(range(10, 60, 110).percent(), Ok(50));
        assert_eq!(range(10, 110, 110).percent(), Ok(100));
        assert_eq!(
            range(10, 0, 110).native_for_percent(50),
            Err(MonitorUnavailable::InvalidNativeRange)
        );
        assert_eq!(
            range(0, 0, 0).percent(),
            Err(MonitorUnavailable::InvalidNativeRange)
        );
        assert_eq!(range(0, 0, u32::MAX).native_for_percent(100), Ok(u32::MAX));
        assert_eq!(
            range(0, 0, 100).native_for_percent(101),
            Err(MonitorUnavailable::InvalidNativeRange)
        );

        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(vec![physical(0, "Panel")], vec![range(10, 60, 110)]),
        );
        let mut api = FakeNativeApi::new(vec![logical(0)], groups);
        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(5),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        let target = inventory.targets().next().unwrap().clone();
        let ack = set_with_backend(
            &mut api,
            &inventory,
            &target,
            90,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        assert_eq!(ack.native_range_before, range(10, 60, 110));
        assert_eq!(ack.applied_native, 100);
        assert_eq!(ack.readback, range(10, 100, 110));
        assert_eq!(ack.readback_percent, 90);
        assert_eq!(api.log.lock().unwrap().sets, vec![(0, 0, 100)]);

        let before_stale = api.log.lock().unwrap().sets.len();
        let mut stale = target.clone();
        stale.generation += 1;
        assert_eq!(
            set_with_backend(
                &mut api,
                &inventory,
                &stale,
                40,
                &BrightnessCancellation::new()
            )
            .unwrap_err()
            .kind,
            BrightnessFailureKind::StaleTarget
        );
        assert_eq!(api.log.lock().unwrap().sets.len(), before_stale);

        api.groups.get_mut(&0).unwrap().monitors[0].description = "Replaced".to_owned();
        assert_eq!(
            set_with_backend(
                &mut api,
                &inventory,
                &target,
                40,
                &BrightnessCancellation::new()
            )
            .unwrap_err()
            .kind,
            BrightnessFailureKind::StaleTarget
        );
        assert_eq!(api.log.lock().unwrap().sets.len(), before_stale);
    }

    #[test]
    fn brightness_inventory_reports_uncertain_set_and_closes_all_leases() {
        let mut groups = HashMap::new();
        groups.insert(
            0,
            FakeGroupSpec::ready(vec![physical(0, "First")], vec![range(0, 40, 100)]),
        );
        let mut failed_set =
            FakeGroupSpec::ready(vec![physical(0, "Second")], vec![range(10, 50, 110)]);
        failed_set.set_failure = Some(NativeFailure {
            operation: BrightnessOperation::SetBrightness,
            code: Some(55),
        });
        groups.insert(1, failed_set);
        let mut api = FakeNativeApi::new(vec![logical(0), logical(1)], groups);
        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(6),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        let target = inventory
            .targets()
            .find(|target| target.identity.logical_index == 1)
            .unwrap()
            .clone();

        let failure = set_with_backend(
            &mut api,
            &inventory,
            &target,
            70,
            &BrightnessCancellation::new(),
        )
        .unwrap_err();

        assert_eq!(failure.kind, BrightnessFailureKind::NativeFailure);
        assert_eq!(failure.operation, Some(BrightnessOperation::SetBrightness));
        assert_eq!(failure.native_code, Some(55));
        assert_eq!(
            failure.set_disposition,
            Some(BrightnessSetDisposition::Unknown)
        );
        assert_eq!(
            failure.set_cause,
            Some(BrightnessSetFailureCause {
                kind: BrightnessFailureKind::NativeFailure,
                operation: Some(BrightnessOperation::SetBrightness),
                native_code: Some(55),
            })
        );
        let state = api.log.lock().unwrap();
        assert_eq!(state.sets, vec![(1, 0, 80)]);
        assert_eq!(state.destroyed_logicals, vec![0, 1, 0, 1]);
    }

    #[test]
    fn brightness_service_late_or_cancelled_set_acknowledgements_are_unknown() {
        let cancellation = BrightnessCancellation::new();
        cancellation.cancel();
        let cancelled = guard_set_helper_result(
            Ok(()),
            &cancellation,
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(cancelled.kind, BrightnessFailureKind::Cancelled);
        assert_eq!(
            cancelled.set_disposition,
            Some(BrightnessSetDisposition::Unknown)
        );

        let late = guard_set_helper_result(
            Ok(()),
            &BrightnessCancellation::new(),
            Instant::now() - Duration::from_millis(1),
        )
        .unwrap_err();
        assert_eq!(late.kind, BrightnessFailureKind::HelperTimeout);
        assert_eq!(
            late.set_disposition,
            Some(BrightnessSetDisposition::Unknown)
        );
    }

    #[test]
    fn brightness_inventory_marks_set_unknown_when_post_set_lease_cleanup_fails() {
        let mut group = FakeGroupSpec::ready(vec![physical(0, "Panel")], vec![range(0, 50, 100)]);
        group.destroy_failure_after_set = Some(NativeFailure {
            operation: BrightnessOperation::DestroyPhysicalMonitors,
            code: Some(77),
        });
        let mut groups = HashMap::new();
        groups.insert(0, group);
        let mut api = FakeNativeApi::new(vec![logical(0)], groups);
        let inventory = refresh_with_backend(
            &mut api,
            BrightnessSessionId(24),
            1,
            &BrightnessCancellation::new(),
        )
        .unwrap();
        let target = inventory.targets().next().unwrap().clone();

        let failure = set_with_backend(
            &mut api,
            &inventory,
            &target,
            80,
            &BrightnessCancellation::new(),
        )
        .unwrap_err();

        assert_eq!(failure.kind, BrightnessFailureKind::HandleCleanupFailure);
        assert_eq!(
            failure.operation,
            Some(BrightnessOperation::DestroyPhysicalMonitors)
        );
        assert_eq!(failure.native_code, Some(77));
        assert_eq!(
            failure.set_disposition,
            Some(BrightnessSetDisposition::Unknown)
        );
        assert_eq!(
            failure.set_cause,
            Some(BrightnessSetFailureCause {
                kind: BrightnessFailureKind::HandleCleanupFailure,
                operation: Some(BrightnessOperation::DestroyPhysicalMonitors),
                native_code: Some(77),
            })
        );
        let state = api.log.lock().unwrap();
        assert_eq!(state.sets, vec![(0, 0, 80)]);
        assert_eq!(state.destroyed_logicals, vec![0, 0]);
    }

    #[derive(Clone, Copy)]
    enum FakeServiceMode {
        Normal,
        GateFirstRefresh,
        GateShutdown,
        LateFirstRefresh,
        FailFirstStart,
        FailFirstSetAfterDispatch,
    }

    #[derive(Default)]
    struct FakeServiceState {
        starts: usize,
        shutdowns: usize,
        live_sessions: usize,
        gate_entered: bool,
        release_gate: bool,
        cleanup_entered: bool,
        release_cleanup: bool,
        operations: Vec<(usize, BrightnessRequestId, &'static str, u64)>,
        set_count: usize,
    }

    struct FakeServiceHarness {
        state: Mutex<FakeServiceState>,
        changed: Condvar,
        mode: FakeServiceMode,
    }

    impl FakeServiceHarness {
        fn lock(&self) -> MutexGuard<'_, FakeServiceState> {
            self.state.lock().unwrap()
        }

        fn wait_for(&self, predicate: impl Fn(&FakeServiceState) -> bool) -> bool {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut state = self.lock();
            while !predicate(&state) {
                let now = Instant::now();
                if now >= deadline {
                    return false;
                }
                let (next, result) = self
                    .changed
                    .wait_timeout(state, deadline.saturating_duration_since(now))
                    .unwrap();
                state = next;
                if result.timed_out() && !predicate(&state) {
                    return false;
                }
            }
            true
        }
    }

    struct FakeHelperFactory {
        harness: Arc<FakeServiceHarness>,
    }

    struct FakeHelperSession {
        harness: Arc<FakeServiceHarness>,
        session: BrightnessSessionId,
        ordinal: usize,
        inventory: Option<MonitorInventory>,
        closed: bool,
    }

    impl HelperFactory for FakeHelperFactory {
        fn start(
            &mut self,
            session: BrightnessSessionId,
            _token: String,
            _deadline: Instant,
        ) -> Result<Box<dyn HelperSession>, BrightnessFailure> {
            let mut state = self.harness.lock();
            state.starts += 1;
            let ordinal = state.starts;
            if ordinal == 1 && matches!(self.harness.mode, FakeServiceMode::FailFirstStart) {
                self.harness.changed.notify_all();
                return Err(BrightnessFailure::new(
                    BrightnessFailureKind::HelperStartFailure,
                ));
            }
            state.live_sessions += 1;
            drop(state);
            Ok(Box::new(FakeHelperSession {
                harness: Arc::clone(&self.harness),
                session,
                ordinal,
                inventory: None,
                closed: false,
            }))
        }
    }

    impl HelperSession for FakeHelperSession {
        fn session_id(&self) -> BrightnessSessionId {
            self.session
        }

        fn refresh(
            &mut self,
            request_id: BrightnessRequestId,
            generation: u64,
            cancellation: &BrightnessCancellation,
            deadline: Instant,
        ) -> Result<MonitorInventory, BrightnessFailure> {
            {
                let mut state = self.harness.lock();
                state
                    .operations
                    .push((self.ordinal, request_id, "refresh", generation));
                if self.ordinal == 1
                    && matches!(
                        self.harness.mode,
                        FakeServiceMode::GateFirstRefresh | FakeServiceMode::GateShutdown
                    )
                {
                    state.gate_entered = true;
                    self.harness.changed.notify_all();
                    while !state.release_gate && !cancellation.is_cancelled() {
                        let (next, _) = self
                            .harness
                            .changed
                            .wait_timeout(state, Duration::from_millis(2))
                            .unwrap();
                        state = next;
                    }
                }
            }
            if self.ordinal == 1 && matches!(self.harness.mode, FakeServiceMode::LateFirstRefresh) {
                let target = deadline + Duration::from_millis(2);
                let mut state = self.harness.lock();
                while Instant::now() < target {
                    let (next, _) = self
                        .harness
                        .changed
                        .wait_timeout(state, target.saturating_duration_since(Instant::now()))
                        .unwrap();
                    state = next;
                }
            }
            if cancellation.is_cancelled() {
                return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
            }
            let inventory = test_inventory(self.session, generation);
            self.inventory = Some(inventory.clone());
            Ok(inventory)
        }

        fn set(
            &mut self,
            request_id: BrightnessRequestId,
            target: &MonitorTarget,
            percent: u8,
            cancellation: &BrightnessCancellation,
            _deadline: Instant,
        ) -> Result<BrightnessSetAcknowledgement, BrightnessFailure> {
            if cancellation.is_cancelled() {
                return Err(BrightnessFailure::new(BrightnessFailureKind::Cancelled));
            }
            let Some(inventory) = &self.inventory else {
                return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
            };
            if inventory.target(target.id) != Some(target) {
                return Err(BrightnessFailure::new(BrightnessFailureKind::StaleTarget));
            }
            let before = range(10, 60, 110);
            let applied_native = before
                .native_for_percent(percent)
                .map_err(|_| BrightnessFailure::new(BrightnessFailureKind::InvalidRange))?;
            let readback = NativeBrightnessRange {
                current: applied_native,
                ..before
            };
            let mut state = self.harness.lock();
            state
                .operations
                .push((self.ordinal, request_id, "set", target.generation));
            state.set_count += 1;
            let should_fail = self.ordinal == 1
                && matches!(
                    self.harness.mode,
                    FakeServiceMode::FailFirstSetAfterDispatch
                );
            self.harness.changed.notify_all();
            if should_fail {
                return Err(BrightnessFailure::native(
                    BrightnessOperation::SetBrightness,
                    Some(55),
                )
                .with_set_disposition(BrightnessSetDisposition::Unknown));
            }
            Ok(BrightnessSetAcknowledgement {
                target: target.clone(),
                requested_percent: percent,
                native_range_before: before,
                applied_native,
                readback,
                readback_percent: readback.percent().unwrap(),
            })
        }

        fn shutdown(
            &mut self,
            deadline: Instant,
        ) -> Result<HelperCleanupDisposition, BrightnessFailure> {
            if !self.closed {
                let mut state = self.harness.lock();
                state.shutdowns += 1;
                if self.ordinal == 1 && matches!(self.harness.mode, FakeServiceMode::GateShutdown) {
                    state.cleanup_entered = true;
                    self.harness.changed.notify_all();
                    while !state.release_cleanup && Instant::now() < deadline {
                        let (next, _) = self
                            .harness
                            .changed
                            .wait_timeout(
                                state,
                                Duration::from_millis(2)
                                    .min(deadline.saturating_duration_since(Instant::now())),
                            )
                            .unwrap();
                        state = next;
                    }
                    if !state.release_cleanup {
                        return Err(BrightnessFailure {
                            kind: BrightnessFailureKind::HelperCleanupFailure,
                            operation: Some(BrightnessOperation::VerifyHelperExit),
                            native_code: None,
                            process_cleanup: Some(HelperCleanupDisposition::ExitUnverified),
                            set_disposition: None,
                            set_cause: None,
                        });
                    }
                }
                self.closed = true;
                state.live_sessions = state.live_sessions.saturating_sub(1);
                self.harness.changed.notify_all();
            }
            Ok(
                if self.ordinal == 1
                    && matches!(self.harness.mode, FakeServiceMode::LateFirstRefresh)
                {
                    HelperCleanupDisposition::ForcedTerminationExitVerified
                } else {
                    HelperCleanupDisposition::NormalExitVerified
                },
            )
        }
    }

    fn test_inventory(session: BrightnessSessionId, generation: u64) -> MonitorInventory {
        let monitor_identity = PhysicalMonitorIdentity {
            logical_device: "\\\\.\\DISPLAY1".to_owned(),
            logical_index: 0,
            physical_index: 0,
            description: "Panel".to_owned(),
        };
        let target = MonitorTarget {
            session,
            generation,
            id: MonitorTargetId(0),
            identity: monitor_identity.clone(),
        };
        MonitorInventory {
            session,
            generation,
            entries: vec![MonitorInventoryEntry {
                id: Some(target.id),
                identity: Some(monitor_identity.clone()),
                target: Some(target),
                display_name: "Display 1 · Panel".to_owned(),
                native_range: Some(range(10, 60, 110)),
                percent: Some(50),
                unavailable: None,
            }],
            topology_complete: true,
            topology: vec![monitor_identity],
        }
    }

    fn fake_service(
        mode: FakeServiceMode,
        timing: ServiceTiming,
    ) -> (BrightnessService, Arc<FakeServiceHarness>) {
        let harness = Arc::new(FakeServiceHarness {
            state: Mutex::new(FakeServiceState::default()),
            changed: Condvar::new(),
            mode,
        });
        let service = BrightnessService::with_factory(
            Box::new(FakeHelperFactory {
                harness: Arc::clone(&harness),
            }),
            timing,
        );
        (service, harness)
    }

    fn quick_timing() -> ServiceTiming {
        ServiceTiming {
            startup: Duration::from_millis(100),
            operation: Duration::from_millis(100),
            cleanup: Duration::from_millis(100),
        }
    }

    fn receive(ticket: &BrightnessRequestTicket) -> BrightnessCompletion {
        ticket
            .receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("bounded fake helper should complete")
    }

    #[test]
    fn brightness_service_protocol_accumulates_partial_frames_and_rejects_stale_or_oversize() {
        let envelope = Envelope {
            version: 1,
            token: "0123456789abcdef0123456789abcdef".to_owned(),
            session: BrightnessSessionId(9),
            request_id: BrightnessRequestId(4),
            generation: 3,
            body: Message::Refresh,
        };
        let frame = encode_protocol_frame(&envelope, MAX_FRAME_BYTES).unwrap();
        let mut reader = ProtocolFrameAccumulator::default();
        assert!(reader.push(&frame[..3]).unwrap().is_none());
        let decoded = reader.push(&frame[3..]).unwrap().unwrap();
        assert_eq!(decoded.token, envelope.token);
        assert_eq!(decoded.session, envelope.session);
        assert_eq!(decoded.request_id, envelope.request_id);
        assert_eq!(decoded.generation, envelope.generation);
        assert!(envelope_matches(
            &decoded,
            &envelope.token,
            envelope.session,
            envelope.request_id,
            3
        ));
        assert!(!envelope_matches(
            &decoded,
            "ffffffffffffffffffffffffffffffff",
            envelope.session,
            envelope.request_id,
            3
        ));
        assert!(!envelope_matches(
            &decoded,
            &envelope.token,
            BrightnessSessionId(8),
            envelope.request_id,
            3
        ));
        assert!(!envelope_matches(
            &decoded,
            &envelope.token,
            envelope.session,
            BrightnessRequestId(5),
            3
        ));
        assert!(!envelope_matches(
            &decoded,
            &envelope.token,
            envelope.session,
            envelope.request_id,
            4
        ));
        assert_eq!(
            decode_protocol_frame(&frame[..frame.len() - 1])
                .unwrap_err()
                .kind,
            BrightnessFailureKind::ProtocolFailure
        );
        let mut extra = frame.clone();
        extra.push(0);
        assert_eq!(
            decode_protocol_frame(&extra).unwrap_err().kind,
            BrightnessFailureKind::ProtocolFailure
        );
        let mut too_large = ProtocolFrameAccumulator::default();
        assert_eq!(
            too_large
                .push(&((MAX_FRAME_BYTES as u32 + 1).to_le_bytes()))
                .unwrap_err()
                .kind,
            BrightnessFailureKind::ProtocolFailure
        );
        assert_eq!(
            encode_protocol_frame(&envelope, 1).unwrap_err().kind,
            BrightnessFailureKind::ProtocolFailure
        );
        let hello = Envelope {
            version: 1,
            token: envelope.token.clone(),
            session: envelope.session,
            request_id: BrightnessRequestId(0),
            generation: 0,
            body: Message::Hello {
                request_pipe_capacity: 64 * 1024,
            },
        };
        let decoded_hello =
            decode_protocol_frame(&encode_protocol_frame(&hello, MAX_FRAME_BYTES).unwrap())
                .unwrap();
        let Message::Hello {
            request_pipe_capacity,
        } = decoded_hello.body
        else {
            panic!("expected the helper hello message");
        };
        assert_eq!(request_pipe_capacity, 64 * 1024);
    }

    #[test]
    fn brightness_service_coalesces_one_pending_request_and_preserves_active_order() {
        let (mut service, harness) =
            fake_service(FakeServiceMode::GateFirstRefresh, quick_timing());
        let active = service.refresh().unwrap();
        assert!(harness.wait_for(|state| state.gate_entered));
        let displaced = service.refresh().unwrap();
        let latest = service.refresh().unwrap();
        assert_eq!(
            displaced.try_recv(),
            Some(BrightnessCompletion::Superseded { by: latest.id() })
        );
        {
            let mut state = harness.lock();
            state.release_gate = true;
            harness.changed.notify_all();
        }
        assert!(matches!(
            receive(&active),
            BrightnessCompletion::Refreshed(Ok(_))
        ));
        assert!(matches!(
            receive(&latest),
            BrightnessCompletion::Refreshed(Ok(_))
        ));
        let operations = harness.lock().operations.clone();
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0].1, active.id());
        assert_eq!(operations[1].1, latest.id());
        assert_eq!(operations[0].2, "refresh");
        assert_eq!(operations[1].2, "refresh");
        assert_eq!(
            service.shutdown().unwrap().helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
        let state = harness.lock();
        assert_eq!(state.live_sessions, 0);
        assert_eq!(state.shutdowns, 1);
    }

    #[test]
    fn brightness_service_rejects_stale_targets_and_applies_current_target_once() {
        let (mut service, harness) = fake_service(FakeServiceMode::Normal, quick_timing());
        let refreshed = service.refresh().unwrap();
        let BrightnessCompletion::Refreshed(Ok(inventory)) = receive(&refreshed) else {
            panic!("fake refresh should produce the current inventory")
        };
        let target = inventory.targets().next().unwrap().clone();
        let mut stale = target.clone();
        stale.generation += 1;
        let refused = service.set(&stale, 80).unwrap();
        assert!(matches!(
            receive(&refused),
            BrightnessCompletion::Set(Err(BrightnessFailure {
                kind: BrightnessFailureKind::StaleTarget,
                ..
            }))
        ));
        assert_eq!(harness.lock().set_count, 0);

        let accepted = service.set(&target, 80).unwrap();
        let BrightnessCompletion::Set(Ok(ack)) = receive(&accepted) else {
            panic!("current target should be acknowledged by the fake session")
        };
        assert_eq!(ack.target, target);
        assert_eq!(ack.readback_percent, 80);
        assert_eq!(harness.lock().set_count, 1);
        assert_eq!(
            service.shutdown().unwrap().helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
    }

    #[test]
    fn brightness_service_does_not_reuse_inventory_after_unknown_set_outcome() {
        let (mut service, harness) =
            fake_service(FakeServiceMode::FailFirstSetAfterDispatch, quick_timing());
        let refreshed = service.refresh().unwrap();
        let BrightnessCompletion::Refreshed(Ok(inventory)) = receive(&refreshed) else {
            panic!("the fake helper should provide an inventory before Set")
        };
        let target = inventory.targets().next().unwrap().clone();

        let first = service.set(&target, 75).unwrap();
        let BrightnessCompletion::Set(Err(failure)) = receive(&first) else {
            panic!("the simulated dispatched Set should return an uncertain failure")
        };
        assert_eq!(failure.kind, BrightnessFailureKind::NativeFailure);
        assert_eq!(
            failure.set_disposition,
            Some(BrightnessSetDisposition::Unknown)
        );
        assert_eq!(failure.native_code, Some(55));
        assert_eq!(
            failure.process_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );

        let repeated = service.set(&target, 75).unwrap();
        let BrightnessCompletion::Set(Err(failure)) = receive(&repeated) else {
            panic!("an old target cannot be retried after an uncertain Set")
        };
        assert_eq!(failure.kind, BrightnessFailureKind::StaleTarget);
        assert_eq!(
            failure.set_disposition,
            Some(BrightnessSetDisposition::NotAttempted)
        );
        assert_eq!(harness.lock().set_count, 1);

        let refreshed = service.refresh().unwrap();
        let BrightnessCompletion::Refreshed(Ok(fresh_inventory)) = receive(&refreshed) else {
            panic!("refresh should start a new helper after an unknown Set outcome")
        };
        assert_ne!(fresh_inventory.session, inventory.session);
        assert_eq!(fresh_inventory.generation, 1);
        let state = harness.lock();
        assert_eq!(state.starts, 2);
        assert_eq!(state.set_count, 1);
        drop(state);
        assert_eq!(
            service.shutdown().unwrap().helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
    }

    #[test]
    fn brightness_service_rejects_late_reply_and_retires_the_owned_helper() {
        let mut timing = quick_timing();
        timing.operation = Duration::from_millis(20);
        let (mut service, harness) = fake_service(FakeServiceMode::LateFirstRefresh, timing);
        let late = service.refresh().unwrap();
        let completion = receive(&late);
        let BrightnessCompletion::Refreshed(Err(failure)) = completion else {
            panic!("a fake response beyond the operation deadline must not publish")
        };
        assert_eq!(failure.kind, BrightnessFailureKind::HelperTimeout);
        assert_eq!(
            failure.process_cleanup,
            Some(HelperCleanupDisposition::ForcedTerminationExitVerified)
        );
        assert!(harness.wait_for(|state| state.live_sessions == 0));

        let next = service.refresh().unwrap();
        let BrightnessCompletion::Refreshed(Ok(inventory)) = receive(&next) else {
            panic!("a new helper session should refresh after the timed-out one was retired")
        };
        assert_eq!(inventory.generation, 1);
        assert_eq!(harness.lock().starts, 2);
        assert_eq!(
            service.shutdown().unwrap().helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
        let state = harness.lock();
        assert_eq!(state.live_sessions, 0);
        assert_eq!(state.shutdowns, 2);
    }

    #[test]
    fn brightness_service_retries_a_failed_helper_start_without_claiming_an_inventory() {
        let (mut service, harness) = fake_service(FakeServiceMode::FailFirstStart, quick_timing());
        let first = service.refresh().unwrap();
        assert!(matches!(
            receive(&first),
            BrightnessCompletion::Refreshed(Err(BrightnessFailure {
                kind: BrightnessFailureKind::HelperStartFailure,
                ..
            }))
        ));
        let second = service.refresh().unwrap();
        assert!(matches!(
            receive(&second),
            BrightnessCompletion::Refreshed(Ok(_))
        ));
        assert_eq!(harness.lock().starts, 2);
        assert_eq!(
            service.shutdown().unwrap().helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
        assert_eq!(harness.lock().live_sessions, 0);
    }

    #[test]
    fn brightness_service_shutdown_cancels_active_work_and_verifies_cleanup() {
        let (mut service, harness) =
            fake_service(FakeServiceMode::GateFirstRefresh, quick_timing());
        let active = service.refresh().unwrap();
        assert!(harness.wait_for(|state| state.gate_entered));
        let shutdown_service = std::thread::spawn(move || {
            let result = service.shutdown();
            (service, result)
        });
        let (mut service, shutdown_result) = shutdown_service.join().unwrap();
        let report = shutdown_result.unwrap();
        assert_eq!(
            receive(&active),
            BrightnessCompletion::Refreshed(Err(BrightnessFailure {
                process_cleanup: Some(HelperCleanupDisposition::NormalExitVerified),
                ..BrightnessFailure::new(BrightnessFailureKind::Cancelled)
            }))
        );
        assert_eq!(
            report.helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
        assert_eq!(harness.lock().live_sessions, 0);
        let second_shutdown = service.shutdown().unwrap();
        assert_eq!(
            second_shutdown.helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
    }

    #[test]
    fn brightness_service_shutdown_can_be_requested_and_polled_without_dropping_ownership() {
        let mut timing = quick_timing();
        timing.cleanup = Duration::from_secs(5);
        let (mut service, harness) = fake_service(FakeServiceMode::GateShutdown, timing);
        let active = service.refresh().unwrap();
        assert!(harness.wait_for(|state| state.gate_entered));
        let pending = service.refresh().unwrap();

        service.request_shutdown();
        assert_eq!(receive(&pending), BrightnessCompletion::Cancelled);
        assert!(harness.wait_for(|state| state.cleanup_entered));
        assert_eq!(service.try_join_shutdown().unwrap(), None);
        assert_eq!(harness.lock().live_sessions, 1);

        {
            let mut state = harness.lock();
            state.release_cleanup = true;
            harness.changed.notify_all();
        }
        let report = service.shutdown().unwrap();
        assert_eq!(
            receive(&active),
            BrightnessCompletion::Refreshed(Err(BrightnessFailure {
                process_cleanup: Some(HelperCleanupDisposition::NormalExitVerified),
                ..BrightnessFailure::new(BrightnessFailureKind::Cancelled)
            }))
        );
        assert_eq!(
            report.helper_cleanup,
            Some(HelperCleanupDisposition::NormalExitVerified)
        );
        assert_eq!(harness.lock().live_sessions, 0);
        assert_eq!(service.try_join_shutdown().unwrap(), Some(report));
    }
}
