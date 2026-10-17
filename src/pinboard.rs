//! Session-only screenshot pin models and capture resource accounting.
//!
//! Native pin windows are owned by the GUI layer. This module owns only the
//! immutable image snapshots and independent per-pin display state.

use crate::mkmacro::ScreenRect;
use image::RgbaImage;
use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::{Arc, atomic::AtomicUsize};

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
}

struct TrackedRgbaImage {
    image: RgbaImage,
    bytes: usize,
    budget: Arc<SharedImageBudget>,
}

impl Drop for TrackedRgbaImage {
    fn drop(&mut self) {
        let released = self.budget.allocated_bytes.fetch_update(
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
            |allocated| allocated.checked_sub(self.bytes),
        );
        debug_assert!(
            released.is_ok(),
            "tracked image budget released more than once"
        );
    }
}

struct SharedImageBudget {
    limit: usize,
    allocated_bytes: AtomicUsize,
}

impl SharedImageBudget {
    fn allocated_bytes(&self) -> usize {
        self.allocated_bytes
            .load(std::sync::atomic::Ordering::Acquire)
    }

    fn charge(&self, bytes: usize) -> Result<(), PinboardError> {
        self.allocated_bytes
            .fetch_update(
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
                |allocated| {
                    allocated
                        .checked_add(bytes)
                        .filter(|total| *total <= self.limit)
                },
            )
            .map(|_| ())
            .map_err(|allocated| PinboardError::ImageBudgetReached {
                available: self.limit.saturating_sub(allocated),
            })
    }
}

#[derive(Debug)]
pub struct Pin {
    id: PinId,
    snapshot: PinSnapshot,
    display_bounds: ScreenRect,
    zoom: f32,
    always_on_top: bool,
    click_through: bool,
}

impl Pin {
    pub const fn id(&self) -> PinId {
        self.id
    }

    pub const fn snapshot(&self) -> &PinSnapshot {
        &self.snapshot
    }

    pub const fn display_bounds(&self) -> ScreenRect {
        self.display_bounds
    }

    pub const fn zoom(&self) -> f32 {
        self.zoom
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

    pub fn set_always_on_top(&mut self, enabled: bool) {
        self.always_on_top = enabled;
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
    }
}

struct PendingCapture {
    identity: PinCaptureIdentity,
    reserved_bytes: usize,
    selected_rect: Option<ScreenRect>,
    image: Option<PinImageHandle>,
}

/// Owns session pin image references and reserves memory before native work.
/// This manager intentionally has no persistence or serialization path.
pub struct Pinboard {
    limits: PinboardLimits,
    pins: BTreeMap<PinId, Pin>,
    image_budget: Arc<SharedImageBudget>,
    pending: Option<PendingCapture>,
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
                allocated_bytes: AtomicUsize::new(0),
            }),
            pending: None,
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
        self.image_budget.allocated_bytes()
    }

    pub fn pending_bytes(&self) -> usize {
        self.pending
            .as_ref()
            .map_or(0, |pending| pending.reserved_bytes)
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
            .checked_sub(self.image_budget.allocated_bytes())
            .ok_or(PinboardError::ImageBudgetReached { available: 0 })?;
        let reserved_bytes = available.min(self.limits.max_capture_bytes);
        if reserved_bytes == 0 {
            return Err(PinboardError::ImageBudgetReached { available });
        }
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
            reserved_bytes,
            selected_rect: None,
            image: None,
        });
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
        if byte_count > pending.reserved_bytes {
            return Err(PinboardError::CaptureTooLarge {
                requested: byte_count,
                limit: pending.reserved_bytes,
            });
        }
        pending.reserved_bytes = byte_count;
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
        if pending.image.is_some() {
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
        if actual_bytes > pending.reserved_bytes {
            return Err(PinboardError::CaptureTooLarge {
                requested: actual_bytes,
                limit: pending.reserved_bytes,
            });
        }
        self.image_budget.charge(actual_bytes)?;
        pending.image = Some(PinImageHandle(Arc::new(TrackedRgbaImage {
            image,
            bytes: actual_bytes,
            budget: Arc::clone(&self.image_budget),
        })));
        pending.reserved_bytes = 0;
        Ok(())
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
        if pending.image.is_none() {
            return Err(PinboardError::CaptureNotReady);
        }
        let id = PinId(NonZeroU64::new(self.next_pin_id).ok_or(PinboardError::IdentityExhausted)?);
        let next_pin_id = self
            .next_pin_id
            .checked_add(1)
            .ok_or(PinboardError::IdentityExhausted)?;
        let pending = self.pending.take().ok_or(PinboardError::StaleCapture)?;
        let image = pending.image.ok_or(PinboardError::CaptureNotReady)?;
        let dimensions = image.pixels().dimensions();
        self.next_pin_id = next_pin_id;
        self.pins.insert(
            id,
            Pin {
                id,
                snapshot: PinSnapshot {
                    image,
                    physical_dimensions: dimensions,
                    capture_screen_rect: rect,
                },
                display_bounds: rect,
                zoom: 1.0,
                always_on_top: true,
                click_through: false,
            },
        );
        Ok(id)
    }

    /// Drops all pending image references and releases their reservation.
    pub fn cancel_capture(&mut self, identity: PinCaptureIdentity) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.identity == identity)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }

    /// Closing removes the model. Retained image handles keep the allocation
    /// and its shared budget lease until the final owner releases them.
    pub fn close(&mut self, id: PinId) -> bool {
        self.pins.remove(&id).is_some()
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
        let retained_image = board.get(first_id).unwrap().snapshot().image_handle();
        assert_eq!(retained_image.pixels().as_raw().len(), 16);
        assert_eq!(
            board
                .get(first_id)
                .unwrap()
                .snapshot()
                .physical_dimensions(),
            (2, 2)
        );
        assert_eq!(
            board
                .get(first_id)
                .unwrap()
                .snapshot()
                .capture_screen_rect(),
            first_rect
        );
        assert_eq!(board.get(first_id).unwrap().display_bounds(), first_rect);
        assert_eq!(board.get(first_id).unwrap().zoom(), 1.0);
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
        board.get_mut(second_id).unwrap().set_zoom(2.5).unwrap();
        board.get_mut(second_id).unwrap().set_always_on_top(false);
        board.get_mut(second_id).unwrap().set_click_through(true);
        board
            .get_mut(second_id)
            .unwrap()
            .set_display_bounds(ScreenRect::new(-100, 50, 4, 4));
        assert_ne!(first_id, second_id);
        assert_eq!(board.get(first_id).unwrap().zoom(), 1.0);
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
}
