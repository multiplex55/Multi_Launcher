//! Pure target identity, hint assignment, and input matching for Jump Mode.
//!
//! A target ID combines a semantic key, its owning surface, and the view
//! generation that produced it. Labels and visual order are presentation
//! metadata; geometry belongs to a frame-scoped UI registry and is not part of
//! this identity.

pub(crate) mod config;
pub(crate) mod paint;
pub(crate) mod ui;

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashSet};
use std::fmt;

const HINT_ALPHABET: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Stable semantic portion of a target's identity.
///
/// `RootResult` indexes are safe only as part of a [`TargetId`], whose surface
/// and generation bind the index to the result set that produced it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SemanticTargetId {
    /// A caller-defined stable identity, such as a record key or action role.
    Named(String),
    /// An index into root results, valid only for its accompanying generation.
    RootResult(usize),
}

impl SemanticTargetId {
    pub fn named(value: impl Into<String>) -> Self {
        Self::Named(value.into())
    }
}

/// The UI surface that owns a target.
///
/// Dialog and popup instance IDs distinguish repeated instances. A popup also
/// carries its owner so equal local IDs in separate dialogs remain distinct.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceId {
    Dashboard,
    LauncherResults,
    Settings {
        instance: u64,
    },
    Dialog {
        kind: String,
        instance: u64,
    },
    Popup {
        kind: String,
        instance: u64,
        owner: Box<SurfaceId>,
    },
}

impl SurfaceId {
    pub fn dialog(kind: impl Into<String>, instance: u64) -> Self {
        Self::Dialog {
            kind: kind.into(),
            instance,
        }
    }

    pub fn popup(kind: impl Into<String>, instance: u64, owner: SurfaceId) -> Self {
        Self::Popup {
            kind: kind.into(),
            instance,
            owner: Box::new(owner),
        }
    }
}

/// Version of the rendered view that supplied the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetGeneration(pub u64);

/// Stable target identity independent of labels, positions, and widget bounds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId {
    surface: SurfaceId,
    semantic: SemanticTargetId,
    generation: TargetGeneration,
}

impl TargetId {
    pub fn new(
        surface: SurfaceId,
        semantic: SemanticTargetId,
        generation: TargetGeneration,
    ) -> Self {
        Self {
            surface,
            semantic,
            generation,
        }
    }

    /// Build a root result identity that cannot be confused with the same
    /// index in another query generation.
    pub fn root_result(generation: TargetGeneration, index: usize) -> Self {
        Self::new(
            SurfaceId::LauncherResults,
            SemanticTargetId::RootResult(index),
            generation,
        )
    }

    pub fn surface(&self) -> &SurfaceId {
        &self.surface
    }

    pub fn semantic(&self) -> &SemanticTargetId {
        &self.semantic
    }

    pub fn generation(&self) -> TargetGeneration {
        self.generation
    }
}

/// The broad interaction kind a later UI adapter may use when committing focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TargetKind {
    Action,
    Button,
    TextEntry,
    Selectable,
    Checkbox,
    Slider,
    Tab,
    ComboBox,
}

/// Stable target information needed by rendering and focus dispatch.
///
/// `visual_order` supplies a deterministic traversal order within a surface;
/// ties are resolved by the full target ID, never by display text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetMetadata {
    id: TargetId,
    label: String,
    description: Option<String>,
    kind: TargetKind,
    visual_order: u64,
}

impl TargetMetadata {
    pub fn new(
        id: TargetId,
        label: impl Into<String>,
        kind: TargetKind,
        visual_order: u64,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            description: None,
            kind,
            visual_order,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn id(&self) -> &TargetId {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn kind(&self) -> TargetKind {
        self.kind
    }

    pub fn visual_order(&self) -> u64 {
        self.visual_order
    }
}

/// A validated uppercase code made only from A–Z.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hint(String);

impl Hint {
    fn new(value: String, alphabet: &config::HintAlphabet) -> Self {
        debug_assert!(!value.is_empty());
        debug_assert!(value.bytes().all(|byte| alphabet.contains(byte)));
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Hint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One immutable target-to-hint assignment within a hint map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintAssignment {
    target: TargetMetadata,
    hint: Hint,
}

impl HintAssignment {
    pub fn target(&self) -> &TargetMetadata {
        &self.target
    }

    pub fn hint(&self) -> &Hint {
        &self.hint
    }
}

/// Failure to create a map because a target identity was registered twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateTargetId(pub TargetId);

impl fmt::Display for DuplicateTargetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "duplicate Jump Mode target ID: {:?}", self.0)
    }
}

impl std::error::Error for DuplicateTargetId {}

/// Deterministic, immutable assignments for one visible target snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintMap {
    assignments: Vec<HintAssignment>,
}

impl HintMap {
    /// Sort targets by surface, visual order, and stable ID before assigning
    /// codes. The resulting map does not depend on source collection order.
    pub fn assign(
        targets: impl IntoIterator<Item = TargetMetadata>,
    ) -> Result<Self, DuplicateTargetId> {
        Self::assign_with_alphabet(targets, &config::HintAlphabet::default())
    }

    /// Assign hints using a frozen, validated symbol order.
    pub(crate) fn assign_with_alphabet(
        targets: impl IntoIterator<Item = TargetMetadata>,
        alphabet: &config::HintAlphabet,
    ) -> Result<Self, DuplicateTargetId> {
        let mut targets: Vec<_> = targets.into_iter().collect();
        let mut seen = HashSet::with_capacity(targets.len());
        for target in &targets {
            if !seen.insert(target.id.clone()) {
                return Err(DuplicateTargetId(target.id.clone()));
            }
        }

        targets.sort_by(|left, right| {
            left.id
                .surface
                .cmp(&right.id.surface)
                .then_with(|| left.visual_order.cmp(&right.visual_order))
                .then_with(|| left.id.semantic.cmp(&right.id.semantic))
                .then_with(|| left.id.generation.cmp(&right.id.generation))
        });

        let hints = allocate_hints(targets.len(), alphabet);
        let assignments = targets
            .into_iter()
            .zip(hints)
            .map(|(target, hint)| HintAssignment { target, hint })
            .collect();
        Ok(Self { assignments })
    }

    pub fn assignments(&self) -> &[HintAssignment] {
        &self.assignments
    }

    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    pub fn hint_for(&self, target: &TargetId) -> Option<&Hint> {
        self.assignments
            .iter()
            .find(|assignment| assignment.target.id == *target)
            .map(|assignment| &assignment.hint)
    }

    pub fn target_for(&self, hint: &Hint) -> Option<&TargetId> {
        self.assignments
            .iter()
            .find(|assignment| assignment.hint == *hint)
            .map(|assignment| &assignment.target.id)
    }
}

/// Current outcome after accepting or editing a hint prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchOutcome {
    /// The frozen snapshot contained no targets.
    NoTargets,
    /// The prefix is valid and still identifies this many live targets.
    Partial { matching_targets: usize },
    /// A full hint selected exactly one still-valid target.
    Completed(TargetId),
    /// The entered character was unsupported or had no code prefix; input was
    /// left unchanged.
    Invalid,
    /// The prefix mapped only to a target invalidated since the snapshot.
    NoTarget,
    /// The session was explicitly cancelled.
    Cancelled,
    /// The session already completed or was cancelled.
    SessionClosed,
}

/// One frozen hint snapshot and its mutable prefix/lifecycle state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HintSession {
    map: HintMap,
    prefix: String,
    invalidated: HashSet<TargetId>,
    lifecycle: SessionLifecycle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionLifecycle {
    Active,
    Cancelled,
    Resolved(TargetId),
}

impl HintSession {
    pub fn new(map: HintMap) -> Self {
        Self {
            map,
            prefix: String::new(),
            invalidated: HashSet::new(),
            lifecycle: SessionLifecycle::Active,
        }
    }

    pub fn hint_map(&self) -> &HintMap {
        &self.map
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn lifecycle(&self) -> &SessionLifecycle {
        &self.lifecycle
    }

    /// Matching visible assignments for the accepted prefix. Invalidated
    /// targets remain in the frozen map but are never offered as candidates.
    pub fn matching_assignments(&self) -> impl Iterator<Item = &HintAssignment> {
        self.map.assignments.iter().filter(|assignment| {
            !self.invalidated.contains(&assignment.target.id)
                && assignment.hint.as_str().starts_with(&self.prefix)
        })
    }

    /// Add one letter. Invalid input leaves the previous valid prefix intact.
    pub fn push_char(&mut self, input: char) -> MatchOutcome {
        if !matches!(self.lifecycle, SessionLifecycle::Active) {
            return MatchOutcome::SessionClosed;
        }
        if self.map.is_empty() {
            return MatchOutcome::NoTargets;
        }

        let Some(upper) = input
            .to_ascii_uppercase()
            .is_ascii()
            .then_some(input.to_ascii_uppercase())
            .filter(|character| character.is_ascii_uppercase())
        else {
            return MatchOutcome::Invalid;
        };

        let mut candidate = self.prefix.clone();
        candidate.push(upper);
        let mapped: Vec<_> = self
            .map
            .assignments
            .iter()
            .filter(|assignment| assignment.hint.as_str().starts_with(&candidate))
            .collect();
        if mapped.is_empty() {
            return MatchOutcome::Invalid;
        }

        self.prefix = candidate;
        let live: Vec<_> = mapped
            .into_iter()
            .filter(|assignment| !self.invalidated.contains(&assignment.target.id))
            .collect();
        if live.is_empty() {
            return MatchOutcome::NoTarget;
        }

        if let Some(assignment) = live
            .iter()
            .find(|assignment| assignment.hint.as_str() == self.prefix)
        {
            let target = assignment.target.id.clone();
            self.prefix.clear();
            self.lifecycle = SessionLifecycle::Resolved(target.clone());
            MatchOutcome::Completed(target)
        } else {
            MatchOutcome::Partial {
                matching_targets: live.len(),
            }
        }
    }

    /// Remove exactly one accepted prefix character and recompute its outcome.
    pub fn backspace(&mut self) -> MatchOutcome {
        if !matches!(self.lifecycle, SessionLifecycle::Active) {
            return MatchOutcome::SessionClosed;
        }
        self.prefix.pop();
        self.current_outcome()
    }

    /// Cancel the active session and clear its prefix.
    pub fn cancel(&mut self) -> MatchOutcome {
        if !matches!(self.lifecycle, SessionLifecycle::Active) {
            return MatchOutcome::SessionClosed;
        }
        self.prefix.clear();
        self.lifecycle = SessionLifecycle::Cancelled;
        MatchOutcome::Cancelled
    }

    /// Reconcile a later visible population against this frozen snapshot.
    /// Removed targets become stale; newly registered targets are reported but
    /// receive no hint until the caller starts a new session.
    pub fn reconcile_targets(
        &mut self,
        current: impl IntoIterator<Item = TargetId>,
    ) -> Result<PopulationChange, DuplicateTargetId> {
        let mut current_ids = HashSet::new();
        for id in current {
            if !current_ids.insert(id.clone()) {
                return Err(DuplicateTargetId(id));
            }
        }

        let snapshot_ids: HashSet<_> = self
            .map
            .assignments
            .iter()
            .map(|assignment| assignment.target.id.clone())
            .collect();
        let mut added: Vec<_> = current_ids.difference(&snapshot_ids).cloned().collect();
        let removed: Vec<_> = self
            .map
            .assignments
            .iter()
            .map(|assignment| assignment.target.id.clone())
            .filter(|id| !current_ids.contains(id) && !self.invalidated.contains(id))
            .collect();
        added.sort();
        self.invalidated.extend(removed.iter().cloned());
        Ok(PopulationChange { added, removed })
    }

    fn current_outcome(&self) -> MatchOutcome {
        if self.map.is_empty() {
            return MatchOutcome::NoTargets;
        }
        let count = self.matching_assignments().count();
        if count == 0 {
            MatchOutcome::NoTarget
        } else {
            MatchOutcome::Partial {
                matching_targets: count,
            }
        }
    }
}

/// Population changes discovered while preserving the existing hint map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PopulationChange {
    /// Current targets that were not part of the frozen snapshot.
    pub added: Vec<TargetId>,
    /// Snapshot targets that have disappeared and were invalidated.
    pub removed: Vec<TargetId>,
}

impl PopulationChange {
    pub fn requires_new_session(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

/// Create a compact prefix-free set by repeatedly expanding the shallowest
/// greatest leaf in the alphabet rank order. Choosing the rightmost leaf
/// preserves the shortest visible codes for earlier targets. For N above the
/// alphabet size, expanding a leaf adds alphabet-size-minus-one available codes,
/// keeping longer hints minimal
/// at each depth rather than moving the population to fixed-width codes. With
/// A–Z this retains the previous 25-code expansion and exact default ordering.
fn allocate_hints(count: usize, alphabet: &config::HintAlphabet) -> Vec<Hint> {
    if count == 0 {
        return Vec::new();
    }

    // Rank paths make expansion and final ordering follow the frozen alphabet
    // rather than the lexical order of its characters.
    let mut leaves: BTreeSet<(usize, Reverse<Vec<u8>>)> = alphabet
        .symbols()
        .iter()
        .map(|symbol| {
            (
                1,
                Reverse(vec![
                    alphabet
                        .rank(*symbol)
                        .expect("every alphabet symbol has a rank"),
                ]),
            )
        })
        .collect();

    while leaves.len() < count {
        let (depth, Reverse(prefix)) = leaves
            .pop_first()
            .expect("a nonempty prefix tree always has a leaf");
        for symbol in alphabet.symbols() {
            let mut child = prefix.clone();
            child.push(alphabet.rank(*symbol).expect("alphabet rank is present"));
            leaves.insert((depth + 1, Reverse(child)));
        }
    }

    let mut codes: Vec<_> = leaves.into_iter().map(|(_, Reverse(code))| code).collect();
    codes.sort_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)));
    codes.truncate(count);
    codes
        .into_iter()
        .map(|ranks| {
            let bytes: Vec<_> = ranks
                .into_iter()
                .map(|rank| alphabet.symbols()[usize::from(rank)])
                .collect();
            Hint::new(
                String::from_utf8(bytes).expect("alphabet symbols are ASCII"),
                alphabet,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn target(index: usize, label: impl Into<String>) -> TargetMetadata {
        TargetMetadata::new(
            TargetId::root_result(TargetGeneration(7), index),
            label,
            TargetKind::Selectable,
            index as u64,
        )
    }

    fn targets(count: usize) -> Vec<TargetMetadata> {
        (0..count)
            .map(|index| target(index, format!("Result {index}")))
            .collect()
    }

    fn map_for(count: usize) -> HintMap {
        HintMap::assign(targets(count)).expect("unique test target IDs")
    }

    fn codes_for(count: usize, alphabet: &config::HintAlphabet) -> Vec<String> {
        HintMap::assign_with_alphabet(targets(count), alphabet)
            .unwrap()
            .assignments()
            .iter()
            .map(|assignment| assignment.hint.as_str().to_owned())
            .collect()
    }

    fn code_digest(codes: &[String]) -> String {
        use sha2::{Digest, Sha256};

        let digest = Sha256::digest(codes.join("\n").as_bytes());
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join("")
    }

    fn hint_lengths(map: &HintMap) -> (usize, f64) {
        let longest = map
            .assignments()
            .iter()
            .map(|assignment| assignment.hint.as_str().len())
            .max()
            .unwrap_or(0);
        let mean = if map.is_empty() {
            0.0
        } else {
            map.assignments()
                .iter()
                .map(|assignment| assignment.hint.as_str().len() as f64)
                .sum::<f64>()
                / map.len() as f64
        };
        (longest, mean)
    }

    #[test]
    fn hint_lengths_remain_compact_for_representative_populations() {
        // Variable-length leaf expansion retains short single-letter hints.
        // (target count, longest code, mean code length)
        let expected = [
            (0, 0, 0.0),
            (1, 1, 1.0),
            (2, 1, 1.0),
            (26, 1, 1.0),
            (27, 2, 29.0 / 27.0),
            (52, 2, 80.0 / 52.0),
            (100, 2, 1.77),
            (1_000, 3, 2.337),
        ];

        for (count, expected_longest, expected_mean) in expected {
            let map = map_for(count);
            let (longest, mean) = hint_lengths(&map);
            assert_eq!(map.len(), count, "count={count}");
            assert_eq!(longest, expected_longest, "count={count}");
            assert!((mean - expected_mean).abs() < 1e-9, "count={count}: {mean}");
        }
    }

    #[test]
    fn assignments_are_unique_prefix_free_and_use_only_uppercase_ascii() {
        for count in [1, 26, 27, 52, 100, 1_000] {
            let map = map_for(count);
            let hints: Vec<_> = map
                .assignments()
                .iter()
                .map(|assignment| assignment.hint.as_str())
                .collect();
            let unique: HashSet<_> = hints.iter().copied().collect();
            assert_eq!(unique.len(), count, "count={count}");
            assert!(hints.iter().all(|hint| {
                !hint.is_empty() && hint.bytes().all(|byte| HINT_ALPHABET.contains(&byte))
            }));
            for (index, hint) in hints.iter().enumerate() {
                assert!(
                    hints.iter().enumerate().all(|(other_index, other)| {
                        index == other_index || !other.starts_with(hint)
                    }),
                    "prefix collision for {hint} at count={count}"
                );
            }
        }
    }

    #[test]
    fn assignment_is_independent_of_hash_map_insertion_order() {
        let make_hash_map = |indices: Vec<usize>| {
            indices
                .into_iter()
                .map(|index| {
                    let item = target(index, format!("Label {}", index % 3));
                    (item.id.clone(), item)
                })
                .collect::<HashMap<_, _>>()
        };
        let first = make_hash_map((0..100).collect());
        let second = make_hash_map((0..100).rev().collect());
        let first_map = HintMap::assign(first.into_values()).unwrap();
        let second_map = HintMap::assign(second.into_values()).unwrap();

        assert_eq!(first_map, second_map);
    }

    #[test]
    fn default_assignments_match_prechange_golden_maps() {
        let alphabet = config::HintAlphabet::default();
        assert_eq!(
            codes_for(26, &alphabet),
            "A B C D E F G H I J K L M N O P Q R S T U V W X Y Z"
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            codes_for(27, &alphabet),
            "A B C D E F G H I J K L M N O P Q R S T U V W X Y ZA ZB"
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            codes_for(52, &alphabet),
            "A B C D E F G H I J K L M N O P Q R S T U V W X YA YB YC YD YE YF YG YH YI YJ YK YL YM YN YO YP YQ YR YS YT YU YV YW YX YY YZ ZA ZB"
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            code_digest(&codes_for(100, &alphabet)),
            "bd1047927574a1faddbc70d9791c9db7ac6a4c85d7778d018897ffe297a7e0b2"
        );
        assert_eq!(
            code_digest(&codes_for(1_000, &alphabet)),
            "18eeda4b3392b4de7e705a091b4604827819d3e31392b964a77842b794983296"
        );
    }

    #[test]
    fn ordered_alphabets_preserve_rank_compactness_and_determinism() {
        for alphabet_text in ["ZY", "ASDFJKLQ", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"] {
            let alphabet = config::HintAlphabet::parse(alphabet_text).unwrap();
            let boundary = alphabet.symbols().len();
            for count in [0, 1, 2, boundary, boundary + 1, 100, 1_000] {
                let map = HintMap::assign_with_alphabet(targets(count), &alphabet).unwrap();
                let reversed =
                    HintMap::assign_with_alphabet(targets(count).into_iter().rev(), &alphabet)
                        .unwrap();
                assert_eq!(map, reversed, "alphabet={alphabet_text}, count={count}");
                let codes: Vec<_> = map
                    .assignments()
                    .iter()
                    .map(|assignment| assignment.hint.as_str())
                    .collect();
                assert_eq!(codes.len(), count);
                assert_eq!(
                    codes.iter().copied().collect::<HashSet<_>>().len(),
                    count,
                    "alphabet={alphabet_text}, count={count}"
                );
                assert!(
                    codes
                        .iter()
                        .all(|code| code.bytes().all(|symbol| alphabet.contains(symbol))),
                    "alphabet={alphabet_text}, count={count}"
                );
                let rank_path = |code: &str| {
                    code.bytes()
                        .map(|symbol| alphabet.rank(symbol).unwrap())
                        .collect::<Vec<_>>()
                };
                let ordered: Vec<_> = codes
                    .iter()
                    .map(|code| (code.len(), rank_path(code)))
                    .collect();
                assert!(ordered.windows(2).all(|pair| pair[0] <= pair[1]));
                for (index, code) in codes.iter().enumerate() {
                    assert!(
                        codes.iter().enumerate().all(|(other_index, other)| {
                            index == other_index || !other.starts_with(code)
                        }),
                        "prefix collision for {code}, alphabet={alphabet_text}, count={count}"
                    );
                }
            }
        }
    }

    #[test]
    fn custom_rank_expansion_and_frozen_map_use_only_their_alphabet() {
        let custom = config::HintAlphabet::parse("ASDFJKLQ").unwrap();
        assert_eq!(
            codes_for(9, &custom),
            ["A", "S", "D", "F", "J", "K", "L", "QA", "QS"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );

        let reverse_pair = config::HintAlphabet::parse("ZY").unwrap();
        assert_eq!(
            codes_for(3, &reverse_pair),
            ["Z", "YZ", "YY"].map(str::to_owned)
        );

        let frozen = HintMap::assign_with_alphabet(targets(27), &custom).unwrap();
        let session = HintSession::new(frozen.clone());
        let _later_alphabet = config::HintAlphabet::parse("ZY").unwrap();
        assert_eq!(session.hint_map(), &frozen);
        assert!(frozen.assignments().iter().all(|assignment| {
            assignment
                .hint
                .as_str()
                .bytes()
                .all(|symbol| custom.contains(symbol))
        }));
    }

    #[test]
    fn duplicate_labels_and_unicode_metadata_keep_distinct_typed_targets() {
        let ids = [
            TargetId::new(
                SurfaceId::Dashboard,
                SemanticTargetId::named("dashboard:alpha"),
                TargetGeneration(1),
            ),
            TargetId::new(
                SurfaceId::Dashboard,
                SemanticTargetId::named("dashboard:beta"),
                TargetGeneration(1),
            ),
            TargetId::new(
                SurfaceId::dialog("notes", 3),
                SemanticTargetId::named("note:42"),
                TargetGeneration(8),
            ),
        ];
        let map = HintMap::assign([
            TargetMetadata::new(ids[0].clone(), "Same label", TargetKind::Button, 0),
            TargetMetadata::new(ids[1].clone(), "Same label", TargetKind::Button, 1),
            TargetMetadata::new(ids[2].clone(), "東京 🌟", TargetKind::Action, 0)
                .with_description("München — résumé"),
        ])
        .unwrap();

        assert_eq!(map.len(), 3);
        assert_ne!(map.hint_for(&ids[0]), map.hint_for(&ids[1]));
        let unicode = map
            .assignments()
            .iter()
            .find(|assignment| assignment.target.id == ids[2])
            .unwrap();
        assert_eq!(unicode.target.label(), "東京 🌟");
        assert_eq!(unicode.target.description(), Some("München — résumé"));
        assert_eq!(unicode.target.kind(), TargetKind::Action);
    }

    #[test]
    fn duplicate_identity_is_rejected_even_when_metadata_differs() {
        let id = TargetId::root_result(TargetGeneration(7), 0);
        let result = HintMap::assign([
            TargetMetadata::new(id.clone(), "One", TargetKind::Button, 0),
            TargetMetadata::new(id.clone(), "Two", TargetKind::Action, 1),
        ]);

        assert_eq!(result, Err(DuplicateTargetId(id)));
    }

    #[test]
    fn root_result_identity_is_bound_to_surface_and_generation() {
        let first = TargetId::root_result(TargetGeneration(7), 4);
        let next_query = TargetId::root_result(TargetGeneration(8), 4);
        let dashboard = TargetId::new(
            SurfaceId::Dashboard,
            SemanticTargetId::RootResult(4),
            TargetGeneration(7),
        );

        assert_ne!(first, next_query);
        assert_ne!(first, dashboard);
    }

    #[test]
    fn matching_is_case_insensitive_and_invalid_input_keeps_the_prefix() {
        let map = map_for(27);
        let target_for_za = map.assignments()[25].target.id.clone();
        let mut session = HintSession::new(map);

        assert_eq!(
            session.push_char('z'),
            MatchOutcome::Partial {
                matching_targets: 2
            }
        );
        assert_eq!(session.prefix(), "Z");
        assert_eq!(session.push_char('1'), MatchOutcome::Invalid);
        assert_eq!(session.prefix(), "Z");
        assert_eq!(
            session.push_char('a'),
            MatchOutcome::Completed(target_for_za)
        );
        assert!(matches!(session.lifecycle(), SessionLifecycle::Resolved(_)));
        assert_eq!(session.prefix(), "");
        assert_eq!(session.push_char('A'), MatchOutcome::SessionClosed);
    }

    #[test]
    fn backspace_and_cancel_clear_prefix_without_resolving_a_target() {
        let mut session = HintSession::new(map_for(27));
        assert!(matches!(
            session.push_char('z'),
            MatchOutcome::Partial { .. }
        ));
        assert_eq!(
            session.backspace(),
            MatchOutcome::Partial {
                matching_targets: 27
            }
        );
        assert_eq!(session.prefix(), "");
        assert_eq!(
            session.push_char('z'),
            MatchOutcome::Partial {
                matching_targets: 2
            }
        );
        assert_eq!(session.cancel(), MatchOutcome::Cancelled);
        assert_eq!(session.prefix(), "");
        assert_eq!(session.backspace(), MatchOutcome::SessionClosed);
    }

    #[test]
    fn empty_map_reports_no_targets_and_can_be_cancelled() {
        let mut session = HintSession::new(map_for(0));
        assert_eq!(session.push_char('A'), MatchOutcome::NoTargets);
        assert_eq!(session.backspace(), MatchOutcome::NoTargets);
        assert_eq!(session.cancel(), MatchOutcome::Cancelled);
    }

    #[test]
    fn reconciliation_invalidates_stale_targets_without_reassigning_hints() {
        let original = targets(3);
        let removed = original[1].id.clone();
        let retained = original[0].id.clone();
        let added = TargetId::root_result(TargetGeneration(8), 9);
        let map = HintMap::assign(original.clone()).unwrap();
        let removed_hint = map.hint_for(&removed).unwrap().clone();
        let retained_hint = map.hint_for(&retained).unwrap().clone();
        let mut session = HintSession::new(map);
        let change = session
            .reconcile_targets([retained.clone(), original[2].id.clone(), added.clone()])
            .unwrap();

        assert_eq!(change.removed, vec![removed.clone()]);
        assert_eq!(change.added, vec![added.clone()]);
        assert!(change.requires_new_session());
        assert_eq!(session.hint_map().hint_for(&removed), Some(&removed_hint));
        assert_eq!(session.hint_map().hint_for(&retained), Some(&retained_hint));
        assert!(session.hint_map().hint_for(&added).is_none());
        assert_eq!(
            session.push_char(removed_hint.as_str().chars().next().unwrap()),
            MatchOutcome::NoTarget
        );
        assert_eq!(session.prefix(), removed_hint.as_str());
    }
}
