//! Session-only screenshot pin models and capture resource accounting.
//!
//! Native pin windows are owned by the GUI layer. This module owns only the
//! immutable image snapshots and independent per-pin display state.

pub(crate) mod live;
pub(crate) mod source;

use crate::mkmacro::ScreenRect;
use crate::pinboard::source::PinSourceBinding;
use image::RgbaImage;
use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::{Arc, atomic::AtomicUsize};
use std::thread::JoinHandle;

const MIB: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PinId(NonZeroU64);

impl PinId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinCaptureGeneration(NonZeroU64);

impl PinCaptureGeneration {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Strongly typed identity for one shared rectangle selection and capture.
/// Its numeric value is registered with `SharedVisualOverlayController` before
/// that controller receives the rectangle-pick command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinCaptureOperationId(NonZeroU64);

impl PinCaptureOperationId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinCaptureIdentity {
    pub generation: PinCaptureGeneration,
    pub operation_id: PinCaptureOperationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinboardLimits {
    pub max_pins: usize,
    pub max_image_bytes: usize,
    pub max_capture_bytes: usize,
}

impl PinboardLimits {
    pub const fn new(max_pins: usize, max_image_bytes: usize, max_capture_bytes: usize) -> Self {
        Self {
            max_pins,
            max_image_bytes,
            max_capture_bytes,
        }
    }

    fn validate(self) -> Result<Self, PinboardError> {
        if self.max_pins == 0 {
            return Err(PinboardError::InvalidLimits("pin count must be positive"));
        }
        if self.max_image_bytes == 0 || self.max_capture_bytes == 0 {
            return Err(PinboardError::InvalidLimits(
                "image and capture byte limits must be positive",
            ));
        }
        if self.max_capture_bytes > self.max_image_bytes {
            return Err(PinboardError::InvalidLimits(
                "one capture cannot exceed the aggregate image budget",
            ));
        }
        Ok(self)
    }
}

impl Default for PinboardLimits {
    fn default() -> Self {
        Self::new(8, 256 * MIB, 128 * MIB)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinboardError {
    InvalidLimits(&'static str),
    CaptureInProgress,
    PinLimitReached {
        limit: usize,
    },
    LiveSessionLimitReached {
        limit: usize,
    },
    ImageBudgetReached {
        available: usize,
    },
    CaptureTooLarge {
        requested: usize,
        limit: usize,
    },
    InvalidCaptureGeometry(String),
    CaptureDimensionsMismatch {
        expected: (u32, u32),
        actual: (u32, u32),
    },
    CapturePixelBufferMismatch {
        expected: usize,
        actual: usize,
    },
    StaleCapture,
    CaptureNotReady,
    CaptureAlreadyStaged,
    IdentityExhausted,
}

impl fmt::Display for PinboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits(message) => write!(f, "invalid pinboard limits: {message}"),
            Self::CaptureInProgress => f.write_str("a pinboard capture is already in progress"),
            Self::PinLimitReached { limit } => {
                write!(f, "pinboard limit reached ({limit} pins)")
            }
            Self::LiveSessionLimitReached { limit } => {
                write!(
                    f,
                    "live pin capture worker limit reached ({limit} sessions)"
                )
            }
            Self::ImageBudgetReached { available } => write!(
                f,
                "pinboard image memory limit reached ({} bytes available)",
                available
            ),
            Self::CaptureTooLarge { requested, limit } => write!(
                f,
                "selected region needs {requested} RGBA bytes; the pinboard allows {limit}"
            ),
            Self::InvalidCaptureGeometry(message) => {
                write!(f, "invalid pin capture region: {message}")
            }
            Self::CaptureDimensionsMismatch { expected, actual } => write!(
                f,
                "capture returned {}×{} pixels for a {}×{} region",
                actual.0, actual.1, expected.0, expected.1
            ),
            Self::CapturePixelBufferMismatch { expected, actual } => write!(
                f,
                "capture returned {actual} RGBA bytes; expected {expected}"
            ),
            Self::StaleCapture => f.write_str("pinboard capture result is stale"),
            Self::CaptureNotReady => f.write_str("pinboard capture has no completed image"),
            Self::CaptureAlreadyStaged => f.write_str("pinboard capture image was already staged"),
            Self::IdentityExhausted => f.write_str("pinboard identity space was exhausted"),
        }
    }
}

impl std::error::Error for PinboardError {}

pub struct PinSnapshot {
    image: PinImageHandle,
    physical_dimensions: (u32, u32),
    capture_screen_rect: ScreenRect,
}

impl fmt::Debug for PinSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinSnapshot")
            .field("physical_dimensions", &self.physical_dimensions)
            .field("capture_screen_rect", &self.capture_screen_rect)
            .field("pixel_bytes", &self.image.image_bytes())
            .finish()
    }
}

impl PinSnapshot {
    /// Borrows the immutable pixels without letting an untracked `Arc` escape.
    pub fn pixels(&self) -> &RgbaImage {
        self.image.pixels()
    }

    /// Returns an opaque strong reference that keeps the allocation and its
    /// shared image-budget lease alive for native windows or future actions.
    pub fn image_handle(&self) -> PinImageHandle {
        self.image.clone()
    }

    pub const fn physical_dimensions(&self) -> (u32, u32) {
        self.physical_dimensions
    }

    pub const fn capture_screen_rect(&self) -> ScreenRect {
        self.capture_screen_rect
    }

    pub fn image_bytes(&self) -> usize {
        self.image.image_bytes()
    }
}

/// Opaque, clonable ownership of one tracked immutable RGBA allocation.
/// Consumers can retain image pixels without bypassing pinboard accounting.
#[derive(Clone)]
pub struct PinImageHandle(Arc<TrackedRgbaImage>);

impl fmt::Debug for PinImageHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PinImageHandle")
            .field("dimensions", &self.0.image.dimensions())
            .field("image_bytes", &self.0.bytes)
            .finish()
    }
}

impl PinImageHandle {
    pub fn pixels(&self) -> &RgbaImage {
        &self.0.image
    }

    pub fn image_bytes(&self) -> usize {
        self.0.bytes
    }

    pub(crate) fn reserve_related_image_bytes(
        &self,
        bytes: usize,
    ) -> Result<ImageBudgetReservation, PinboardError> {
        self.0._reservation.budget.reserve(bytes)
    }
}

struct TrackedRgbaImage {
    image: RgbaImage,
    bytes: usize,
    _reservation: ImageBudgetReservation,
}

pub(crate) struct SharedImageBudget {
    limit: usize,
    reserved_and_allocated_bytes: AtomicUsize,
}

impl SharedImageBudget {
    fn used_bytes(&self) -> usize {
        self.reserved_and_allocated_bytes
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn reserve(self: &Arc<Self>, bytes: usize) -> Result<ImageBudgetReservation, PinboardError> {
        self.reserved_and_allocated_bytes
            .try_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |allocated| {
                    allocated
                        .checked_add(bytes)
                        .filter(|total| *total <= self.limit)
                },
            )
            .map(|_| ImageBudgetReservation {
                budget: Arc::clone(self),
                bytes,
            })
            .map_err(|allocated| PinboardError::ImageBudgetReached {
                available: self.limit.saturating_sub(allocated),
            })
    }
}

/// One atomic reservation for either an in-flight RGBA conversion or a
/// published image. Moving it into the tracked image prevents a budget gap or
/// a double charge between conversion and publication.
pub(crate) struct ImageBudgetReservation {
    budget: Arc<SharedImageBudget>,
    bytes: usize,
}

impl ImageBudgetReservation {
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    pub(crate) fn shared_budget(&self) -> Arc<SharedImageBudget> {
        Arc::clone(&self.budget)
    }

    pub(crate) fn resize(&mut self, bytes: usize) -> Result<(), PinboardError> {
        if bytes > self.bytes {
            let additional = bytes - self.bytes;
            self.budget
                .reserved_and_allocated_bytes
                .try_update(
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                    |used| {
                        used.checked_add(additional)
                            .filter(|total| *total <= self.budget.limit)
                    },
                )
                .map_err(|used| PinboardError::ImageBudgetReached {
                    available: self.budget.limit.saturating_sub(used),
                })?;
        } else if bytes < self.bytes {
            self.release(self.bytes - bytes);
        }
        self.bytes = bytes;
        Ok(())
    }

    pub(crate) fn into_image(self, image: RgbaImage) -> Result<PinImageHandle, PinboardError> {
        if image.as_raw().len() != self.bytes {
            return Err(PinboardError::CapturePixelBufferMismatch {
                expected: self.bytes,
                actual: image.as_raw().len(),
            });
        }
        Ok(PinImageHandle(Arc::new(TrackedRgbaImage {
            bytes: self.bytes,
            image,
            _reservation: self,
        })))
    }

    fn release(&self, bytes: usize) {
        let released = self.budget.reserved_and_allocated_bytes.try_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |used| used.checked_sub(bytes),
        );
        debug_assert!(released.is_ok(), "image budget reservation released once");
    }
}

impl Drop for ImageBudgetReservation {
    fn drop(&mut self) {
        self.release(self.bytes);
    }
}

#[derive(Debug)]
pub struct Pin {
    id: PinId,
    content: PinContent,
    display_bounds: ScreenRect,
    zoom: f32,
    opacity_percent: u8,
    always_on_top: bool,
    click_through: bool,
}

pub const MIN_PIN_OPACITY_PERCENT: u8 = 10;
pub const MAX_PIN_OPACITY_PERCENT: u8 = 100;

impl Pin {
    pub const fn id(&self) -> PinId {
        self.id
    }

    /// Static pins retain their immutable snapshot API. Live pins publish via
    /// their independent latest-frame slot instead of retaining a seed image.
    pub const fn snapshot(&self) -> Option<&PinSnapshot> {
        match &self.content {
            PinContent::Static(snapshot) => Some(snapshot),
            PinContent::Live(_) => None,
        }
    }

    pub fn current_image_handle(&self) -> Option<PinImageHandle> {
        match &self.content {
            PinContent::Static(snapshot) => Some(snapshot.image_handle()),
            PinContent::Live(session) => session.current_frame().map(|frame| frame.image),
        }
    }

    pub fn live_frame(&self) -> Option<live::PinLiveFrameLease> {
        match &self.content {
            PinContent::Static(_) => None,
            PinContent::Live(session) => session.current_frame(),
        }
    }

    pub fn live_status(&self) -> Option<live::PinLiveStatus> {
        match &self.content {
            PinContent::Static(_) => None,
            PinContent::Live(session) => Some(session.status()),
        }
    }

    pub(crate) fn live_frame_source(&self) -> Option<live::PinLiveFrameSource> {
        match &self.content {
            PinContent::Static(_) => None,
            PinContent::Live(session) => Some(session.frame_source()),
        }
    }

    /// Registers a repaint notifier for this live pin's child viewport.
    /// Static pins have no live publication source and return false.
    #[allow(dead_code)] // The child presenter installs this in M2-C.
    pub(crate) fn set_live_child_wake_notifier(
        &self,
        notifier: Option<live::PinLiveChildWake>,
    ) -> bool {
        match &self.content {
            PinContent::Static(_) => false,
            PinContent::Live(session) => {
                session.set_child_wake_notifier(notifier);
                true
            }
        }
    }

    pub const fn display_bounds(&self) -> ScreenRect {
        self.display_bounds
    }

    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns the pin's desired whole-window opacity. This is presentation
    /// state only; it never modifies the captured image pixels.
    pub const fn opacity_percent(&self) -> u8 {
        self.opacity_percent
    }

    pub const fn always_on_top(&self) -> bool {
        self.always_on_top
    }

    pub const fn click_through(&self) -> bool {
        self.click_through
    }

    pub fn set_display_bounds(&mut self, bounds: ScreenRect) {
        self.display_bounds = bounds;
    }

    pub fn set_zoom(&mut self, zoom: f32) -> Result<(), PinboardError> {
        if !zoom.is_finite() || zoom <= 0.0 {
            return Err(PinboardError::InvalidLimits(
                "pin zoom must be finite and positive",
            ));
        }
        self.zoom = zoom;
        Ok(())
    }

    pub fn set_opacity_percent(&mut self, opacity_percent: u8) -> Result<(), PinboardError> {
        if !(MIN_PIN_OPACITY_PERCENT..=MAX_PIN_OPACITY_PERCENT).contains(&opacity_percent) {
            return Err(PinboardError::InvalidLimits(
                "pin opacity must be between 10 and 100 percent",
            ));
        }
        self.opacity_percent = opacity_percent;
        Ok(())
    }

    pub fn set_always_on_top(&mut self, enabled: bool) {
        self.always_on_top = enabled;
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
    }
}

#[derive(Debug)]
enum PinContent {
    Static(PinSnapshot),
    Live(live::PinLiveCaptureSession),
}

struct PendingCapture {
    identity: PinCaptureIdentity,
    reservation: Option<ImageBudgetReservation>,
    selected_rect: Option<ScreenRect>,
    image: Option<PinImageHandle>,
    live: Option<(PinSourceBinding, live::PinLiveCaptureSession)>,
    live_actor_reserved: bool,
}

/// Owns session pin image references and reserves memory before native work.
/// This manager intentionally has no persistence or serialization path.
pub struct Pinboard {
    limits: PinboardLimits,
    pins: BTreeMap<PinId, Pin>,
    image_budget: Arc<SharedImageBudget>,
    gpu_budget: Arc<live::SharedGpuBudget>,
    pending: Option<PendingCapture>,
    retired_live_workers: Vec<JoinHandle<()>>,
    next_pin_id: u64,
    next_capture_generation: u64,
}

impl Default for Pinboard {
    fn default() -> Self {
        Self::with_limits(PinboardLimits::default()).expect("default pinboard limits are valid")
    }
}

impl Pinboard {
    pub fn with_limits(limits: PinboardLimits) -> Result<Self, PinboardError> {
        Ok(Self {
            limits: limits.validate()?,
            pins: BTreeMap::new(),
            image_budget: Arc::new(SharedImageBudget {
                limit: limits.max_image_bytes,
                reserved_and_allocated_bytes: AtomicUsize::new(0),
            }),
            gpu_budget: Arc::new(live::SharedGpuBudget::default()),
            pending: None,
            retired_live_workers: Vec::new(),
            next_pin_id: 1,
            next_capture_generation: 1,
        })
    }

    pub const fn limits(&self) -> PinboardLimits {
        self.limits
    }

    pub fn len(&self) -> usize {
        self.pins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pins.is_empty()
    }

    pub fn image_bytes(&self) -> usize {
        self.image_budget.used_bytes()
    }

    pub fn pending_bytes(&self) -> usize {
        self.pending
            .as_ref()
            .and_then(|pending| pending.reservation.as_ref())
            .map_or(0, ImageBudgetReservation::bytes)
    }

    pub fn active_capture(&self) -> Option<PinCaptureIdentity> {
        self.pending.as_ref().map(|pending| pending.identity)
    }

    pub fn get(&self, id: PinId) -> Option<&Pin> {
        self.pins.get(&id)
    }

    pub fn get_mut(&mut self, id: PinId) -> Option<&mut Pin> {
        self.pins.get_mut(&id)
    }

    pub fn pins(&self) -> impl Iterator<Item = &Pin> {
        self.pins.values()
    }

    /// Reserves one capture slot and the largest image allocation that can
    /// still fit both configured limits before parking or selection begins.
    pub fn reserve_capture(
        &mut self,
        operation_id: PinCaptureOperationId,
    ) -> Result<PinCaptureIdentity, PinboardError> {
        self.reap_finished_live_workers();
        if self.pending.is_some() {
            return Err(PinboardError::CaptureInProgress);
        }
        if self.pins.len() >= self.limits.max_pins {
            return Err(PinboardError::PinLimitReached {
                limit: self.limits.max_pins,
            });
        }
        let available = self
            .limits
            .max_image_bytes
            .saturating_sub(self.image_budget.used_bytes());
        let reserved_bytes = available.min(self.limits.max_capture_bytes);
        if reserved_bytes == 0 {
            return Err(PinboardError::ImageBudgetReached { available });
        }
        let reservation = self.image_budget.reserve(reserved_bytes)?;
        let generation = PinCaptureGeneration(
            NonZeroU64::new(self.next_capture_generation)
                .ok_or(PinboardError::IdentityExhausted)?,
        );
        self.next_capture_generation = self
            .next_capture_generation
            .checked_add(1)
            .ok_or(PinboardError::IdentityExhausted)?;
        let identity = PinCaptureIdentity {
            generation,
            operation_id,
        };
        self.pending = Some(PendingCapture {
            identity,
            reservation: Some(reservation),
            selected_rect: None,
            image: None,
            live: None,
            live_actor_reserved: false,
        });
        Ok(identity)
    }

    /// Reserves one bounded live-capture actor before parking or starting
    /// native window work. Cancelled actors continue to count until reaped.
    pub(crate) fn reserve_live_capture(
        &mut self,
        operation_id: PinCaptureOperationId,
    ) -> Result<PinCaptureIdentity, PinboardError> {
        self.reap_finished_live_workers();
        let live_actors = self.retired_live_workers.len()
            + self
                .pins
                .values()
                .filter(|pin| matches!(&pin.content, PinContent::Live(_)))
                .count()
            + usize::from(
                self.pending
                    .as_ref()
                    .is_some_and(|pending| pending.live_actor_reserved),
            );
        if live_actors >= self.limits.max_pins {
            return Err(PinboardError::LiveSessionLimitReached {
                limit: self.limits.max_pins,
            });
        }
        let identity = self.reserve_capture(operation_id)?;
        if let Some(pending) = self.pending.as_mut() {
            pending.live_actor_reserved = true;
        }
        Ok(identity)
    }

    /// Narrows the reservation to the selected physical rectangle before the
    /// pixel backend is called.
    pub fn reserve_selected_rect(
        &mut self,
        identity: PinCaptureIdentity,
        rect: ScreenRect,
    ) -> Result<usize, PinboardError> {
        let byte_count = capture_byte_count(rect)?;
        let pending = self
            .pending
            .as_mut()
            .filter(|pending| pending.identity == identity)
            .ok_or(PinboardError::StaleCapture)?;
        let reservation = pending
            .reservation
            .as_mut()
            .ok_or(PinboardError::CaptureNotReady)?;
        if byte_count > reservation.bytes() {
            return Err(PinboardError::CaptureTooLarge {
                requested: byte_count,
                limit: reservation.bytes(),
            });
        }
        reservation.resize(byte_count)?;
        pending.selected_rect = Some(rect);
        Ok(byte_count)
    }

    /// Holds a validated immutable image until all owned native visibility and
    /// launcher restoration work has completed.
    pub fn stage_capture(
        &mut self,
        identity: PinCaptureIdentity,
        rect: ScreenRect,
        image: RgbaImage,
    ) -> Result<(), PinboardError> {
        let pending = self
            .pending
            .as_mut()
            .filter(|pending| pending.identity == identity)
            .ok_or(PinboardError::StaleCapture)?;
        let expected_rect = pending
            .selected_rect
            .ok_or(PinboardError::CaptureNotReady)?;
        if rect != expected_rect {
            return Err(PinboardError::StaleCapture);
        }
        if pending.image.is_some() || pending.live.is_some() {
            return Err(PinboardError::CaptureAlreadyStaged);
        }
        let expected_bytes = capture_byte_count(rect)?;
        let actual_dimensions = image.dimensions();
        let expected_dimensions = (rect.width, rect.height);
        if actual_dimensions != expected_dimensions {
            return Err(PinboardError::CaptureDimensionsMismatch {
                expected: expected_dimensions,
                actual: actual_dimensions,
            });
        }
        let actual_bytes = image.as_raw().len();
        if actual_bytes != expected_bytes {
            return Err(PinboardError::CapturePixelBufferMismatch {
                expected: expected_bytes,
                actual: actual_bytes,
            });
        }
        let reservation = pending
            .reservation
            .as_ref()
            .ok_or(PinboardError::CaptureNotReady)?;
        if actual_bytes > reservation.bytes() {
            return Err(PinboardError::CaptureTooLarge {
                requested: actual_bytes,
                limit: reservation.bytes(),
            });
        }
        let reservation = pending
            .reservation
            .take()
            .ok_or(PinboardError::CaptureNotReady)?;
        pending.image = Some(reservation.into_image(image)?);
        Ok(())
    }

    pub(crate) fn take_live_reservation(
        &mut self,
        identity: PinCaptureIdentity,
    ) -> Result<ImageBudgetReservation, PinboardError> {
        self.pending
            .as_mut()
            .filter(|pending| pending.identity == identity)
            .ok_or(PinboardError::StaleCapture)?
            .reservation
            .take()
            .ok_or(PinboardError::CaptureNotReady)
    }

    pub(crate) fn gpu_budget(&self) -> Arc<live::SharedGpuBudget> {
        Arc::clone(&self.gpu_budget)
    }

    pub(crate) fn stage_live_capture(
        &mut self,
        identity: PinCaptureIdentity,
        binding: PinSourceBinding,
        session: live::PinLiveCaptureSession,
    ) -> Result<(), PinboardError> {
        let validation = match self
            .pending
            .as_ref()
            .filter(|pending| pending.identity == identity)
        {
            None => Err(PinboardError::StaleCapture),
            Some(pending) => match pending.selected_rect {
                None => Err(PinboardError::CaptureNotReady),
                Some(_) if pending.image.is_some() || pending.live.is_some() => {
                    Err(PinboardError::CaptureAlreadyStaged)
                }
                Some(rect)
                    if (binding.crop.width, binding.crop.height) != (rect.width, rect.height)
                        || !session.has_first_frame() =>
                {
                    Err(PinboardError::CaptureNotReady)
                }
                Some(_) => Ok(()),
            },
        };
        if let Err(error) = validation {
            self.retire_live_session(session);
            return Err(error);
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.live = Some((binding, session));
        }
        Ok(())
    }

    pub(crate) fn retire_live_session(&mut self, mut session: live::PinLiveCaptureSession) {
        if let Some(worker) = session.cancel_and_take_worker() {
            self.retired_live_workers.push(worker);
        }
        self.reap_finished_live_workers();
    }

    /// Publishes the new pin only after the caller has finished native cleanup.
    pub fn commit_capture(&mut self, identity: PinCaptureIdentity) -> Result<PinId, PinboardError> {
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| pending.identity == identity)
            .ok_or(PinboardError::StaleCapture)?;
        if self.pins.len() >= self.limits.max_pins {
            return Err(PinboardError::PinLimitReached {
                limit: self.limits.max_pins,
            });
        }
        let rect = pending
            .selected_rect
            .ok_or(PinboardError::CaptureNotReady)?;
        if pending.image.is_none() && pending.live.is_none() {
            return Err(PinboardError::CaptureNotReady);
        }
        let id = PinId(NonZeroU64::new(self.next_pin_id).ok_or(PinboardError::IdentityExhausted)?);
        let next_pin_id = self
            .next_pin_id
            .checked_add(1)
            .ok_or(PinboardError::IdentityExhausted)?;
        let pending = self.pending.take().ok_or(PinboardError::StaleCapture)?;
        let content = if let Some(image) = pending.image {
            let dimensions = image.pixels().dimensions();
            PinContent::Static(PinSnapshot {
                image,
                physical_dimensions: dimensions,
                capture_screen_rect: rect,
            })
        } else if let Some((binding, session)) = pending.live {
            if (binding.crop.width, binding.crop.height)
                != session
                    .current_frame()
                    .ok_or(PinboardError::CaptureNotReady)?
                    .image
                    .pixels()
                    .dimensions()
            {
                return Err(PinboardError::CaptureNotReady);
            }
            PinContent::Live(session)
        } else {
            return Err(PinboardError::CaptureNotReady);
        };
        self.next_pin_id = next_pin_id;
        self.pins.insert(
            id,
            Pin {
                id,
                content,
                display_bounds: rect,
                zoom: 1.0,
                opacity_percent: MAX_PIN_OPACITY_PERCENT,
                always_on_top: true,
                click_through: false,
            },
        );
        if let Some(Pin {
            content: PinContent::Live(session),
            ..
        }) = self.pins.get(&id)
        {
            session.activate();
        }
        Ok(id)
    }

    /// Drops all pending image references and releases their reservation.
    pub fn cancel_capture(&mut self, identity: PinCaptureIdentity) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.identity == identity)
        {
            if let Some(mut pending) = self.pending.take()
                && let Some((_, mut session)) = pending.live.take()
                && let Some(worker) = session.cancel_and_take_worker()
            {
                self.retired_live_workers.push(worker);
            }
            self.reap_finished_live_workers();
            true
        } else {
            false
        }
    }

    /// Closing removes the model. Retained image handles keep the allocation
    /// and its shared budget lease until the final owner releases them.
    pub fn close(&mut self, id: PinId) -> bool {
        let Some(mut pin) = self.pins.remove(&id) else {
            return false;
        };
        if let PinContent::Live(session) = &mut pin.content
            && let Some(worker) = session.cancel_and_take_worker()
        {
            self.retired_live_workers.push(worker);
        }
        self.reap_finished_live_workers();
        true
    }

    pub fn pause_live(&self, id: PinId) -> bool {
        self.pins
            .get(&id)
            .is_some_and(|pin| matches!(&pin.content, PinContent::Live(session) if session.pause()))
    }

    pub fn resume_live(&self, id: PinId) -> bool {
        self.pins.get(&id).is_some_and(
            |pin| matches!(&pin.content, PinContent::Live(session) if session.resume()),
        )
    }

    pub(crate) fn shutdown_live_sessions_and_join(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for worker in self.retired_live_workers.drain(..) {
            if worker.join().is_err() {
                errors.push("window capture worker terminated unexpectedly".to_owned());
            }
        }
        if let Some(pending) = self.pending.as_mut()
            && let Some((_, session)) = pending.live.as_mut()
            && let Err(error) = session.cancel_and_join()
        {
            errors.push(error);
        }
        for pin in self.pins.values_mut() {
            if let PinContent::Live(session) = &mut pin.content
                && let Err(error) = session.cancel_and_join()
            {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn reap_finished_live_workers(&mut self) {
        let mut remaining = Vec::with_capacity(self.retired_live_workers.len());
        for worker in self.retired_live_workers.drain(..) {
            if worker.is_finished() {
                if worker.join().is_err() {
                    tracing::error!("cancelled pin capture worker terminated unexpectedly");
                }
            } else {
                remaining.push(worker);
            }
        }
        self.retired_live_workers = remaining;
    }
}

impl Drop for Pinboard {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown_live_sessions_and_join() {
            tracing::error!("could not drain live pin capture workers on drop: {error}");
        }
    }
}

fn capture_byte_count(rect: ScreenRect) -> Result<usize, PinboardError> {
    rect.validate_capture()
        .map_err(|error| PinboardError::InvalidCaptureGeometry(error.to_string()))?;
    let pixels = usize::try_from(rect.width)
        .ok()
        .and_then(|width| {
            usize::try_from(rect.height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(PinboardError::CaptureTooLarge {
            requested: usize::MAX,
            limit: 0,
        })?;
    pixels.checked_mul(4).ok_or(PinboardError::CaptureTooLarge {
        requested: usize::MAX,
        limit: 0,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinVisibilityAck {
    Pending,
    Acknowledged,
    Failed(String),
}

/// M11-B's native pin manager must use this seam. `poll_hidden` may acknowledge
/// only after every materialized pin window is natively hidden; an empty window
/// registry is the only valid immediate acknowledgement. Restoration must skip
/// windows that closed or changed visibility while this transaction was active.
pub trait PinWindowVisibilityCoordinator: Send {
    fn request_hide_all(&mut self, identity: PinCaptureIdentity) -> Result<(), String>;
    fn poll_hidden(&mut self, identity: PinCaptureIdentity) -> PinVisibilityAck;
    fn request_restore_all(&mut self, identity: PinCaptureIdentity) -> Result<(), String>;
    fn poll_restored(&mut self, identity: PinCaptureIdentity) -> PinVisibilityAck;
    /// Cancels pending hide tickets and restores only windows still owned by
    /// this capture. `poll_restored` must drain every resulting native ticket.
    fn cancel_and_restore_all(&mut self, identity: PinCaptureIdentity) -> Result<(), String>;
    /// Shutdown path must synchronously drain tickets and release native-window
    /// references before the owner is dropped.
    fn shutdown_and_drain(&mut self, identity: PinCaptureIdentity) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinboard::live::{PinLiveCaptureBackend, test_support::FakePinLiveCaptureBackend};
    use crate::pinboard::source::{PinSourceCrop, PinSourceWindowIdentity};
    use std::sync::{Mutex, mpsc};
    use std::time::{Duration, Instant};

    fn operation_id(value: u64) -> PinCaptureOperationId {
        PinCaptureOperationId::new(value).unwrap()
    }

    #[test]
    fn pins_keep_independent_session_state_and_release_bounded_image_memory() {
        let mut board = Pinboard::with_limits(PinboardLimits::new(2, 32, 16)).unwrap();
        assert!(board.is_empty());
        let first_capture = board.reserve_capture(operation_id(1)).unwrap();
        assert_eq!(board.pending_bytes(), 16);
        let first_rect = ScreenRect::new(-4, 9, 2, 2);
        assert_eq!(
            board.reserve_selected_rect(first_capture, first_rect),
            Ok(16)
        );
        board
            .stage_capture(
                first_capture,
                first_rect,
                RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])),
            )
            .unwrap();
        let first_id = board.commit_capture(first_capture).unwrap();
        let retained_image = board
            .get(first_id)
            .unwrap()
            .snapshot()
            .unwrap()
            .image_handle();
        assert_eq!(retained_image.pixels().as_raw().len(), 16);
        let retained_pixels_before = retained_image.pixels().as_raw().clone();
        assert_eq!(
            board
                .get(first_id)
                .unwrap()
                .snapshot()
                .unwrap()
                .physical_dimensions(),
            (2, 2)
        );
        assert_eq!(
            board
                .get(first_id)
                .unwrap()
                .snapshot()
                .unwrap()
                .capture_screen_rect(),
            first_rect
        );
        assert_eq!(board.get(first_id).unwrap().display_bounds(), first_rect);
        assert_eq!(board.get(first_id).unwrap().zoom(), 1.0);
        assert_eq!(board.get(first_id).unwrap().opacity_percent(), 100);
        assert!(board.get(first_id).unwrap().always_on_top());
        assert!(!board.get(first_id).unwrap().click_through());

        let second_capture = board.reserve_capture(operation_id(2)).unwrap();
        let second_rect = ScreenRect::new(20, -30, 1, 1);
        assert_eq!(
            board.reserve_selected_rect(second_capture, second_rect),
            Ok(4)
        );
        board
            .stage_capture(
                second_capture,
                second_rect,
                RgbaImage::from_pixel(1, 1, image::Rgba([9, 8, 7, 255])),
            )
            .unwrap();
        let second_id = board.commit_capture(second_capture).unwrap();
        let second_pixels_before = board
            .get(second_id)
            .unwrap()
            .snapshot()
            .unwrap()
            .image_handle()
            .pixels()
            .as_raw()
            .clone();
        board.get_mut(second_id).unwrap().set_zoom(2.5).unwrap();
        assert_eq!(board.get(second_id).unwrap().opacity_percent(), 100);
        board
            .get_mut(second_id)
            .unwrap()
            .set_opacity_percent(42)
            .unwrap();
        assert!(
            board
                .get_mut(second_id)
                .unwrap()
                .set_opacity_percent(9)
                .is_err()
        );
        assert!(
            board
                .get_mut(second_id)
                .unwrap()
                .set_opacity_percent(101)
                .is_err()
        );
        board.get_mut(second_id).unwrap().set_always_on_top(false);
        board.get_mut(second_id).unwrap().set_click_through(true);
        board
            .get_mut(second_id)
            .unwrap()
            .set_display_bounds(ScreenRect::new(-100, 50, 4, 4));
        assert_ne!(first_id, second_id);
        assert_eq!(board.get(first_id).unwrap().zoom(), 1.0);
        assert_eq!(board.get(first_id).unwrap().opacity_percent(), 100);
        assert_eq!(board.get(second_id).unwrap().opacity_percent(), 42);
        assert_eq!(
            board
                .get(second_id)
                .unwrap()
                .snapshot()
                .unwrap()
                .image_handle()
                .pixels()
                .as_raw(),
            &second_pixels_before,
            "opacity never mutates the image owned by the changed pin"
        );
        assert_eq!(
            retained_image.pixels().as_raw(),
            &retained_pixels_before,
            "per-pin opacity never changes captured image pixels"
        );
        assert!(board.get(first_id).unwrap().always_on_top());
        assert!(!board.get(first_id).unwrap().click_through());
        assert_eq!(board.image_bytes(), 20);

        assert!(board.close(first_id));
        assert_eq!(board.image_bytes(), 20);
        let third_capture = board.reserve_capture(operation_id(3)).unwrap();
        assert_eq!(board.pending_bytes(), 12);
        assert!(matches!(
            board.reserve_selected_rect(third_capture, ScreenRect::new(0, 0, 2, 2)),
            Err(PinboardError::CaptureTooLarge {
                requested: 16,
                limit: 12
            })
        ));
        assert!(board.cancel_capture(third_capture));
        drop(retained_image);
        assert_eq!(board.image_bytes(), 4);
        assert!(board.close(second_id));
        assert_eq!(board.image_bytes(), 0);
        assert!(board.is_empty());
        assert!(Pinboard::default().is_empty());
    }

    #[test]
    fn capture_reservations_bound_pending_and_aggregate_bytes_with_overflow_checks() {
        let mut board = Pinboard::with_limits(PinboardLimits::new(1, 8, 8)).unwrap();
        let identity = board.reserve_capture(operation_id(1)).unwrap();
        assert_eq!(board.pending_bytes(), 8);
        assert!(matches!(
            board.reserve_selected_rect(identity, ScreenRect::new(0, 0, 2, 2)),
            Err(PinboardError::CaptureTooLarge {
                requested: 16,
                limit: 8
            })
        ));
        assert!(matches!(
            board.reserve_capture(operation_id(2)),
            Err(PinboardError::CaptureInProgress)
        ));
        assert!(board.cancel_capture(identity));
        assert_eq!(board.pending_bytes(), 0);

        let overflow = ScreenRect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX);
        assert!(matches!(
            capture_byte_count(overflow),
            Err(PinboardError::InvalidCaptureGeometry(_))
                | Err(PinboardError::CaptureTooLarge { .. })
        ));
        assert!(matches!(
            Pinboard::with_limits(PinboardLimits::new(0, 8, 4)),
            Err(PinboardError::InvalidLimits(_))
        ));

        let mut count_limited = Pinboard::with_limits(PinboardLimits::new(1, 8, 4)).unwrap();
        let identity = count_limited.reserve_capture(operation_id(3)).unwrap();
        let rect = ScreenRect::new(0, 0, 1, 1);
        count_limited.reserve_selected_rect(identity, rect).unwrap();
        count_limited
            .stage_capture(
                identity,
                rect,
                RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255])),
            )
            .unwrap();
        count_limited.commit_capture(identity).unwrap();
        assert!(matches!(
            count_limited.reserve_capture(operation_id(4)),
            Err(PinboardError::PinLimitReached { limit: 1 })
        ));
        assert_eq!(count_limited.image_bytes(), 4);

        let mut stale_commit_board = Pinboard::with_limits(PinboardLimits::new(1, 8, 8)).unwrap();
        let current = stale_commit_board.reserve_capture(operation_id(5)).unwrap();
        let stale = PinCaptureIdentity {
            generation: current.generation,
            operation_id: operation_id(6),
        };
        let current_rect = ScreenRect::new(-5, 11, 1, 1);
        stale_commit_board
            .reserve_selected_rect(current, current_rect)
            .unwrap();
        stale_commit_board
            .stage_capture(
                current,
                current_rect,
                RgbaImage::from_pixel(1, 1, image::Rgba([7, 6, 5, 255])),
            )
            .unwrap();
        assert!(matches!(
            stale_commit_board.commit_capture(stale),
            Err(PinboardError::StaleCapture)
        ));
        assert_eq!(stale_commit_board.active_capture(), Some(current));
        assert_eq!(stale_commit_board.pending_bytes(), 0);
        assert_eq!(stale_commit_board.image_bytes(), 4);
        assert!(stale_commit_board.commit_capture(current).is_ok());
        assert_eq!(stale_commit_board.image_bytes(), 4);
    }

    #[test]
    fn activity_baseline_synthetic_live_pin_cancel_and_warm_reopen() {
        let mut board = Pinboard::with_limits(PinboardLimits::new(3, 48, 16)).unwrap();
        for cycle in 0..2 {
            let identity = board
                .reserve_live_capture(operation_id(0x6000 + cycle))
                .unwrap();
            let rect = ScreenRect::new(0, 0, 2, 2);
            board.reserve_selected_rect(identity, rect).unwrap();
            let reservation = board.take_live_reservation(identity).unwrap();
            let binding = PinSourceBinding {
                window: PinSourceWindowIdentity {
                    hwnd: 51,
                    process_id: 52,
                    thread_id: 53,
                },
                captured_bounds: ScreenRect::new(0, 0, 12, 10),
                crop: PinSourceCrop {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
            };
            let (_tx, rx) = mpsc::channel();
            let backend = FakePinLiveCaptureBackend {
                first_image: RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])),
                first_content_size: (12, 10),
                stream: Mutex::new(Some(rx)),
                first_frame_gate: None,
            };
            let mut session = backend
                .start(
                    binding,
                    identity.generation,
                    reservation,
                    board.gpu_budget(),
                )
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Some(result) = session.try_first_frame_result() {
                    result.unwrap();
                    break;
                }
                assert!(Instant::now() < deadline, "synthetic first frame deadline");
                std::thread::yield_now();
            }
            board
                .stage_live_capture(identity, binding, session)
                .unwrap();
            let pin = board.commit_capture(identity).unwrap();
            let lease = board.get(pin).unwrap().current_image_handle().unwrap();
            let cancel = Instant::now();
            assert!(board.close(pin));
            let ack_us = cancel.elapsed().as_micros();
            board.shutdown_live_sessions_and_join().unwrap();
            let join_us = cancel.elapsed().as_micros();
            assert_eq!(
                board.image_bytes(),
                16,
                "external lease deliberately retains pixels after join"
            );
            drop(lease);
            assert_eq!(board.image_bytes(), 0);
            eprintln!(
                "ACTIVITY_BASELINE {}",
                serde_json::json!({
                    "owner":"synthetic_live_pin","cycle":cycle,"warm_cycle":cycle==1,
                    "operation":format!("{:?}",identity),"fixture_pixels":[2,2],
                    "cancel_ack_us":ack_us,"worker_join_us":join_us,
                    "retained_external_lease_bytes_after_join":16,"owner_bytes_after_lease_drop":0,
                    "native_capture":false,"ui_frame_latency":"not measured","cleanup":"joined"
                })
            );
        }
    }

    #[test]
    fn live_capture_worker_is_cancelled_and_joined_when_pinboard_drops() {
        let mut board = Pinboard::with_limits(PinboardLimits::new(3, 48, 16)).unwrap();
        let initial_operation_id = operation_id(20);
        let identity = board.reserve_live_capture(initial_operation_id).unwrap();
        let rect = ScreenRect::new(-17, 8, 2, 2);
        board.reserve_selected_rect(identity, rect).unwrap();
        let reservation = board.take_live_reservation(identity).unwrap();
        let binding = PinSourceBinding {
            window: PinSourceWindowIdentity {
                hwnd: 51,
                process_id: 52,
                thread_id: 53,
            },
            captured_bounds: ScreenRect::new(-30, -2, 12, 10),
            crop: PinSourceCrop {
                x: 3,
                y: 2,
                width: 2,
                height: 2,
            },
        };
        let (_frame_tx, frame_rx) = mpsc::channel();
        let backend = FakePinLiveCaptureBackend {
            first_image: RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255])),
            first_content_size: (12, 10),
            stream: Mutex::new(Some(frame_rx)),
            first_frame_gate: None,
        };
        let mut session = backend
            .start(
                binding,
                identity.generation,
                reservation,
                board.gpu_budget(),
            )
            .unwrap();
        let first_frame_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = session.try_first_frame_result() {
                result.unwrap();
                break;
            }
            assert!(
                Instant::now() < first_frame_deadline,
                "fake live worker did not start"
            );
            std::thread::yield_now();
        }
        board
            .stage_live_capture(identity, binding, session)
            .unwrap();
        let pin_id = board.commit_capture(identity).unwrap();
        assert_eq!(board.get(pin_id).unwrap().opacity_percent(), 100);
        let live_frame_before_opacity = board
            .get(pin_id)
            .unwrap()
            .current_image_handle()
            .unwrap()
            .pixels()
            .as_raw()
            .clone();
        board
            .get_mut(pin_id)
            .unwrap()
            .set_opacity_percent(37)
            .unwrap();
        assert_eq!(
            board
                .get(pin_id)
                .unwrap()
                .current_image_handle()
                .unwrap()
                .pixels()
                .as_raw(),
            &live_frame_before_opacity,
            "live pin opacity leaves the published source frame pixels unchanged"
        );
        assert_eq!(board.image_bytes(), 16);
        let static_pending = board.reserve_capture(operation_id(21)).unwrap();
        assert_eq!(board.pending_bytes(), 16);
        let final_budget_lease = board.image_budget.reserve(16).unwrap();
        assert!(board.image_budget.reserve(1).is_err());
        drop(final_budget_lease);
        assert!(board.cancel_capture(static_pending));

        let second_identity = board.reserve_live_capture(operation_id(22)).unwrap();
        board.reserve_selected_rect(second_identity, rect).unwrap();
        let second_reservation = board.take_live_reservation(second_identity).unwrap();
        let (_second_frame_tx, second_frame_rx) = mpsc::channel();
        let second_backend = FakePinLiveCaptureBackend {
            first_image: RgbaImage::from_pixel(2, 2, image::Rgba([2, 3, 4, 255])),
            first_content_size: (12, 10),
            stream: Mutex::new(Some(second_frame_rx)),
            first_frame_gate: None,
        };
        let mut second_session = second_backend
            .start(
                binding,
                second_identity.generation,
                second_reservation,
                board.gpu_budget(),
            )
            .unwrap();
        let second_first_frame_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = second_session.try_first_frame_result() {
                result.unwrap();
                break;
            }
            assert!(
                Instant::now() < second_first_frame_deadline,
                "second fake live worker did not start"
            );
            std::thread::yield_now();
        }
        board
            .stage_live_capture(second_identity, binding, second_session)
            .unwrap();
        let second_live_pin = board.commit_capture(second_identity).unwrap();
        assert_eq!(board.get(second_live_pin).unwrap().opacity_percent(), 100);
        assert_eq!(board.get(pin_id).unwrap().opacity_percent(), 37);
        assert_eq!(board.image_bytes(), 32);

        // Close one worker while another remains active, then stage a third
        // pending worker. Domain destruction drains every remaining owner.
        assert!(board.close(pin_id));

        let pending_identity = board.reserve_live_capture(operation_id(23)).unwrap();
        board.reserve_selected_rect(pending_identity, rect).unwrap();
        let pending_reservation = board.take_live_reservation(pending_identity).unwrap();
        let (_pending_frame_tx, pending_frame_rx) = mpsc::channel();
        let pending_backend = FakePinLiveCaptureBackend {
            first_image: RgbaImage::from_pixel(2, 2, image::Rgba([5, 6, 7, 255])),
            first_content_size: (12, 10),
            stream: Mutex::new(Some(pending_frame_rx)),
            first_frame_gate: None,
        };
        let mut pending_session = pending_backend
            .start(
                binding,
                pending_identity.generation,
                pending_reservation,
                board.gpu_budget(),
            )
            .unwrap();
        let pending_first_frame_deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = pending_session.try_first_frame_result() {
                result.unwrap();
                break;
            }
            assert!(
                Instant::now() < pending_first_frame_deadline,
                "pending fake live worker did not start"
            );
            std::thread::yield_now();
        }
        board
            .stage_live_capture(pending_identity, binding, pending_session)
            .unwrap();

        let budget = Arc::clone(&board.image_budget);
        drop(board);
        assert_eq!(budget.used_bytes(), 0);
    }
}
