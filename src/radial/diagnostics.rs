//! Typed radial preparation diagnostics with stable source identity.

use super::assets::AssetDiagnostic;
use super::font_cache::FontDiagnostic;
use super::model::{CellId, MenuId};
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{Hash, Hasher};

pub const MAX_EXPECTED_LAYOUT_DIAGNOSTICS: usize = 64;
/// A preparation/display boundary must remain bounded even when an asset or
/// font provider repeats an actionable fault for many cells.
pub const MAX_RADIAL_DIAGNOSTICS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RadialDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RadialDiagnosticKind {
    LabelTruncated,
    TooltipViewLimited,
    RequestedFamilyMissing(String),
    FallbackFamilyMissing,
    MissingGlyph(char),
    FontReadFailed(String),
    AssetUnavailable(AssetDiagnostic),
    SoundUnavailable(AssetDiagnostic),
    DiagnosticsOmitted(RadialDiagnosticOmission),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RadialDiagnosticSource {
    Cell { menu_id: MenuId, cell_id: CellId },
    Asset { menu_id: MenuId, identity: String },
    Menu { menu_id: MenuId },
    Aggregate,
}

/// Counts the unique diagnostics omitted by a bounded preparation/display
/// boundary. Expected label truncation remains separately classifiable while
/// actionable omissions remain visible to the user through this summary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RadialDiagnosticOmission {
    pub total: usize,
    pub actionable: usize,
    pub expected_layout: usize,
}

/// A diagnostic's stable identity deliberately excludes session, layout, and
/// repaint generations. Its fingerprint includes the relevant source text and
/// layout/style constraints supplied by the preparation boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadialDiagnostic {
    pub severity: RadialDiagnosticSeverity,
    pub kind: RadialDiagnosticKind,
    pub source: RadialDiagnosticSource,
    pub fingerprint: u64,
    pub message: String,
}

impl RadialDiagnostic {
    pub fn new(
        severity: RadialDiagnosticSeverity,
        kind: RadialDiagnosticKind,
        source: RadialDiagnosticSource,
        context: impl Hash,
        message: impl Into<String>,
    ) -> Self {
        let mut hasher = StableHasher::default();
        source.hash(&mut hasher);
        kind.hash(&mut hasher);
        context.hash(&mut hasher);
        Self {
            severity,
            kind,
            source,
            fingerprint: hasher.finish(),
            message: message.into(),
        }
    }

    pub fn from_font(
        menu_id: &MenuId,
        cell_id: &CellId,
        source_text: &str,
        layout_context: impl Hash,
        diagnostic: &FontDiagnostic,
    ) -> Self {
        let (severity, kind, message) = match diagnostic {
            FontDiagnostic::RequestedFamilyMissing(family) => (
                RadialDiagnosticSeverity::Warning,
                RadialDiagnosticKind::RequestedFamilyMissing(family.clone()),
                format!("requested font family `{family}` is unavailable"),
            ),
            FontDiagnostic::FallbackFamilyMissing => (
                RadialDiagnosticSeverity::Warning,
                RadialDiagnosticKind::FallbackFamilyMissing,
                "no preferred or fallback font family was found".into(),
            ),
            FontDiagnostic::LabelTruncated => (
                RadialDiagnosticSeverity::Info,
                RadialDiagnosticKind::LabelTruncated,
                "cell label is shortened in its wheel slot; the complete label remains available in its tooltip".into(),
            ),
            FontDiagnostic::TooltipViewLimited => (
                RadialDiagnosticSeverity::Warning,
                RadialDiagnosticKind::TooltipViewLimited,
                "tooltip preview is limited for this unusually long label; the complete source remains available in item details".into(),
            ),
            FontDiagnostic::MissingGlyph(character) => (
                RadialDiagnosticSeverity::Warning,
                RadialDiagnosticKind::MissingGlyph(*character),
                format!("font coverage is missing glyph U+{:04X}", *character as u32),
            ),
            FontDiagnostic::FontReadFailed(family) => (
                RadialDiagnosticSeverity::Error,
                RadialDiagnosticKind::FontReadFailed(family.clone()),
                format!("font family `{family}` could not be read"),
            ),
        };
        Self::new(
            severity,
            kind,
            RadialDiagnosticSource::Cell {
                menu_id: menu_id.clone(),
                cell_id: cell_id.clone(),
            },
            (source_text, layout_context),
            message,
        )
    }

    pub fn is_expected_layout(&self) -> bool {
        matches!(self.kind, RadialDiagnosticKind::LabelTruncated)
    }

    pub fn omission(&self) -> Option<RadialDiagnosticOmission> {
        match &self.kind {
            RadialDiagnosticKind::DiagnosticsOmitted(omission) => Some(*omission),
            _ => None,
        }
    }

    fn omitted_summary(omission: RadialDiagnosticOmission) -> Self {
        let message = if omission.actionable == omission.total {
            format!(
                "{} additional actionable diagnostics omitted (display limited to the configured cap)",
                omission.total
            )
        } else {
            format!(
                "{} additional diagnostics omitted ({} actionable, {} expected layout)",
                omission.total, omission.actionable, omission.expected_layout
            )
        };
        Self::new(
            if omission.actionable == 0 {
                RadialDiagnosticSeverity::Info
            } else {
                RadialDiagnosticSeverity::Warning
            },
            RadialDiagnosticKind::DiagnosticsOmitted(omission),
            RadialDiagnosticSource::Aggregate,
            omission,
            message,
        )
    }
}

/// Deduplicates every diagnostic by its stable fingerprint and applies both
/// the collapsed expected-layout limit and the overall display limit. Faults
/// are collected first so a flood of expected truncation notices cannot hide
/// actionable asset/font/configuration failures.
pub fn bound_diagnostics(
    diagnostics: Vec<RadialDiagnostic>,
    expected_limit: usize,
    total_limit: usize,
) -> Vec<RadialDiagnostic> {
    let mut seen = BTreeSet::new();
    let mut actionable = Vec::new();
    let mut expected = Vec::new();
    let mut actionable_count = 0usize;
    let mut expected_count = 0usize;
    let mut inherited_omission = RadialDiagnosticOmission::default();
    for diagnostic in diagnostics {
        if !seen.insert(diagnostic.fingerprint) {
            continue;
        }
        if let Some(omission) = diagnostic.omission() {
            inherited_omission.total = inherited_omission.total.saturating_add(omission.total);
            inherited_omission.actionable = inherited_omission
                .actionable
                .saturating_add(omission.actionable);
            inherited_omission.expected_layout = inherited_omission
                .expected_layout
                .saturating_add(omission.expected_layout);
            continue;
        }
        if diagnostic.is_expected_layout() {
            expected_count = expected_count.saturating_add(1);
            if expected.len() < expected_limit {
                expected.push(diagnostic);
            }
        } else {
            actionable_count = actionable_count.saturating_add(1);
            if actionable.len() < total_limit {
                actionable.push(diagnostic);
            }
        }
    }
    let mut omission = inherited_omission;
    omission.actionable = omission
        .actionable
        .saturating_add(actionable_count.saturating_sub(actionable.len()));
    let remaining = total_limit.saturating_sub(actionable.len());
    expected.truncate(expected_limit.min(remaining));
    omission.expected_layout = omission
        .expected_layout
        .saturating_add(expected_count.saturating_sub(expected.len()));
    omission.total = omission.actionable.saturating_add(omission.expected_layout);
    actionable.extend(expected);
    if omission.total > 0 {
        // The disclosure occupies one bounded slot so the returned
        // collection itself never exceeds the configured limit.
        if actionable.len() >= total_limit && total_limit > 0 {
            if let Some(removed) = actionable.pop() {
                if removed.is_expected_layout() {
                    omission.expected_layout = omission.expected_layout.saturating_add(1);
                } else {
                    omission.actionable = omission.actionable.saturating_add(1);
                }
                omission.total = omission.actionable.saturating_add(omission.expected_layout);
            }
        }
        actionable.push(RadialDiagnostic::omitted_summary(omission));
    }
    actionable
}

/// Compatibility wrapper for callers whose intent is specifically to bound
/// expected layout notices. The overall limit also protects those callers
/// from retaining an unbounded actionable diagnostic vector.
pub fn bound_expected_layout_diagnostics(
    diagnostics: Vec<RadialDiagnostic>,
    limit: usize,
) -> Vec<RadialDiagnostic> {
    bound_diagnostics(diagnostics, limit, MAX_RADIAL_DIAGNOSTICS)
}

impl fmt::Display for RadialDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Default)]
struct StableHasher(u64);

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        if self.0 == 0 {
            0xcbf29ce484222325
        } else {
            self.0
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.0 == 0 {
            0xcbf29ce484222325
        } else {
            self.0
        };
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::MenuId;

    #[test]
    fn diagnostic_identity_is_stable_across_preparation_generations() {
        let first = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell"),
            "A long label",
            (14_000_u32, 360_000_u32),
            &FontDiagnostic::LabelTruncated,
        );
        let repeated = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell"),
            "A long label",
            (14_000_u32, 360_000_u32),
            &FontDiagnostic::LabelTruncated,
        );
        let changed = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell"),
            "A different label",
            (14_000_u32, 360_000_u32),
            &FontDiagnostic::LabelTruncated,
        );
        assert_eq!(first.fingerprint, repeated.fingerprint);
        assert_ne!(first.fingerprint, changed.fingerprint);
        assert!(first.is_expected_layout());
    }

    #[test]
    fn tooltip_view_limit_is_actionable_and_not_collapsed_expected_layout() {
        let diagnostic = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell"),
            "an unusually long tooltip",
            (13_000_u32, 240_000_u32),
            &FontDiagnostic::TooltipViewLimited,
        );
        assert_eq!(diagnostic.severity, RadialDiagnosticSeverity::Warning);
        assert!(!diagnostic.is_expected_layout());
    }

    #[test]
    fn expected_layout_diagnostics_are_deduped_and_bounded_without_dropping_faults() {
        let first = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell-0"),
            "Long label",
            12_000_u32,
            &FontDiagnostic::LabelTruncated,
        );
        let mut diagnostics = vec![first.clone(), first];
        diagnostics.extend((1..=70).map(|index| {
            RadialDiagnostic::from_font(
                &MenuId::new("menu"),
                &CellId::new(format!("cell-{index}")),
                "Long label",
                12_000_u32,
                &FontDiagnostic::LabelTruncated,
            )
        }));
        diagnostics.push(RadialDiagnostic::new(
            RadialDiagnosticSeverity::Error,
            RadialDiagnosticKind::AssetUnavailable(AssetDiagnostic::NotFound),
            RadialDiagnosticSource::Asset {
                menu_id: MenuId::new("menu"),
                identity: "missing-icon".into(),
            },
            "missing-icon",
            "asset missing",
        ));

        let bounded = bound_expected_layout_diagnostics(diagnostics, 64);
        assert_eq!(
            bounded
                .iter()
                .filter(|diagnostic| diagnostic.is_expected_layout())
                .count(),
            64
        );
        assert!(bounded.iter().any(|diagnostic| {
            matches!(
                &diagnostic.kind,
                RadialDiagnosticKind::AssetUnavailable(AssetDiagnostic::NotFound)
            )
        }));
    }

    #[test]
    fn all_diagnostics_are_stably_deduped_and_overall_bounded() {
        let repeated = RadialDiagnostic::from_font(
            &MenuId::new("menu"),
            &CellId::new("cell"),
            "A",
            12_000_u32,
            &FontDiagnostic::MissingGlyph('🚀'),
        );
        let mut diagnostics = vec![repeated.clone(), repeated];
        diagnostics.extend((0..(MAX_RADIAL_DIAGNOSTICS + 32)).map(|index| {
            RadialDiagnostic::new(
                RadialDiagnosticSeverity::Error,
                RadialDiagnosticKind::AssetUnavailable(AssetDiagnostic::NotFound),
                RadialDiagnosticSource::Asset {
                    menu_id: MenuId::new("menu"),
                    identity: format!("missing-{index}"),
                },
                index,
                "asset missing",
            )
        }));
        let bounded = bound_diagnostics(
            diagnostics,
            MAX_EXPECTED_LAYOUT_DIAGNOSTICS,
            MAX_RADIAL_DIAGNOSTICS,
        );
        let retained = bounded
            .iter()
            .filter(|diagnostic| diagnostic.omission().is_none())
            .collect::<Vec<_>>();
        assert_eq!(bounded.len(), MAX_RADIAL_DIAGNOSTICS);
        assert_eq!(retained.len(), MAX_RADIAL_DIAGNOSTICS - 1);
        let fingerprints = bounded
            .iter()
            .filter(|diagnostic| diagnostic.omission().is_none())
            .map(|diagnostic| diagnostic.fingerprint)
            .collect::<BTreeSet<_>>();
        assert_eq!(fingerprints.len(), retained.len());
        assert_eq!(
            bounded.iter().find_map(|diagnostic| diagnostic.omission()),
            Some(RadialDiagnosticOmission {
                total: 34,
                actionable: 34,
                expected_layout: 0,
            })
        );
        assert!(retained.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            RadialDiagnosticKind::AssetUnavailable(AssetDiagnostic::NotFound)
        )));
    }

    #[test]
    fn diagnostic_bound_reports_unique_omitted_actionable_count() {
        let diagnostics = (0..5)
            .map(|index| {
                RadialDiagnostic::new(
                    RadialDiagnosticSeverity::Error,
                    RadialDiagnosticKind::AssetUnavailable(AssetDiagnostic::NotFound),
                    RadialDiagnosticSource::Asset {
                        menu_id: MenuId::new("menu"),
                        identity: format!("missing-{index}"),
                    },
                    index,
                    "asset missing",
                )
            })
            .flat_map(|diagnostic| [diagnostic.clone(), diagnostic])
            .collect();
        let bounded = bound_diagnostics(diagnostics, 0, 2);
        assert_eq!(
            bounded
                .iter()
                .filter(|diagnostic| diagnostic.omission().is_none())
                .count(),
            1
        );
        let summary = bounded
            .iter()
            .find_map(|diagnostic| diagnostic.omission())
            .expect("bounded diagnostics disclose omitted faults");
        assert_eq!(
            summary,
            RadialDiagnosticOmission {
                total: 4,
                actionable: 4,
                expected_layout: 0,
            }
        );
        assert!(bounded.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("4 additional actionable diagnostics omitted")
        }));
    }
}
