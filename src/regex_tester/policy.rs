//! Preliminary interactive budgets. M11 profiling can tune these named limits.

pub const MAX_PATTERN_BYTES: usize = 4 * 1024;
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_CAPTURE_GROUPS: usize = 100;
pub const MAX_STORED_MATCHES: usize = 1000;
pub const MAX_MATERIALIZED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_REPLACEMENT_BYTES: usize = 4 * 1024;
pub const MAX_REPLACEMENT_OUTPUT_BYTES: usize = 1024 * 1024;

/// Normal editing ranges; larger accepted inputs show a warning before the
/// separate hard suspension limits are reached.
pub const NORMAL_PATTERN_BYTES: usize = 1024;
pub const NORMAL_TEXT_BYTES: usize = 16 * 1024;
pub const NORMAL_REPLACEMENT_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationPolicy {
    pub pattern_bytes: usize,
    pub text_bytes: usize,
    pub capture_groups: usize,
    pub stored_matches: usize,
    /// Aggregate cloned match text, capture values, and capture names.
    pub materialized_bytes: usize,
    pub replacement_bytes: usize,
    pub replacement_output_bytes: usize,
}

impl Default for EvaluationPolicy {
    fn default() -> Self {
        Self {
            pattern_bytes: MAX_PATTERN_BYTES,
            text_bytes: MAX_TEXT_BYTES,
            capture_groups: MAX_CAPTURE_GROUPS,
            stored_matches: MAX_STORED_MATCHES,
            materialized_bytes: MAX_MATERIALIZED_BYTES,
            replacement_bytes: MAX_REPLACEMENT_BYTES,
            replacement_output_bytes: MAX_REPLACEMENT_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvaluationLimit {
    PatternBytes,
    TextBytes,
    CaptureGroups,
    StoredMatches,
    MaterializedBytes,
    ReplacementBytes,
    ReplacementOutputBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationSuspension {
    pub limit: EvaluationLimit,
    pub maximum: usize,
    /// Observed size, or a saturating lower bound when arithmetic overflowed.
    pub observed: usize,
}

impl EvaluationPolicy {
    pub fn check_inputs(&self, pattern: &str, text: &str) -> Result<(), EvaluationSuspension> {
        check_limit(
            EvaluationLimit::PatternBytes,
            self.pattern_bytes,
            pattern.len(),
        )?;
        check_limit(EvaluationLimit::TextBytes, self.text_bytes, text.len())
    }
}

pub(crate) fn check_limit(
    limit: EvaluationLimit,
    maximum: usize,
    observed: usize,
) -> Result<(), EvaluationSuspension> {
    if observed > maximum {
        Err(EvaluationSuspension {
            limit,
            maximum,
            observed,
        })
    } else {
        Ok(())
    }
}
