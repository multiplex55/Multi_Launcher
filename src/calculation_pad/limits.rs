//! Source capacity is separate from evaluation work limits and serialized envelopes.
use super::persistence::DocumentProfile;

pub const STANDARD_BYTES: usize = 64 * 1024;
pub const STANDARD_LINES: usize = 256;
pub const EXTENDED_BYTES: usize = 128 * 1024;
pub const EXTENDED_LINES: usize = 1024;
pub const ABSOLUTE_BYTES: usize = 256 * 1024;
pub const ABSOLUTE_LINES: usize = 2048;
pub const SERIALIZED_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;
// Physical row indices are always below this boundary; runtime bindings follow it.
pub(super) const SLOT_BASE: usize = ABSOLUTE_LINES;

#[derive(Clone, Copy)]
pub(crate) struct SourceLimits { pub bytes: usize, pub lines: usize }

pub(crate) fn profile_limits(profile: DocumentProfile) -> SourceLimits {
    match profile {
        DocumentProfile::Standard => SourceLimits { bytes: STANDARD_BYTES, lines: STANDARD_LINES },
        DocumentProfile::Extended => SourceLimits { bytes: EXTENDED_BYTES, lines: EXTENDED_LINES },
    }
}

pub(crate) fn physical_rows(source: &str) -> usize {
    source.bytes().filter(|byte| *byte == b'\n').count() + 1
}

pub(crate) fn source_admitted(source: &str) -> bool {
    source.len() <= ABSOLUTE_BYTES && physical_rows(source) <= ABSOLUTE_LINES
}
