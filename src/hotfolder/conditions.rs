//! Cooperative parser only: no reader, source admission or action authority.
//! Native acquisition must supply genuine EOF and retain/revalidate its source.

use super::model::{
    ConditionBinding, ConditionEvaluation, ConditionUnavailable, LiteralContentCondition,
    MAX_CONTENT_LITERAL_BYTES, MAX_RULES,
};
use std::fmt;
use std::time::{Duration, Instant};

pub(super) const MAX_SCAN_BYTES: u64 = 8 * 1024 * 1024;
pub(super) const MAX_CHUNK_BYTES: usize = 64 * 1024;
const SCAN_DEADLINE: Duration = Duration::from_secs(2);
const CHECK_INTERVAL: usize = 256;

struct Pattern {
    condition: LiteralContentCondition,
    fallback: Vec<usize>,
}

fn literal(condition: &LiteralContentCondition) -> &[u8] {
    match condition {
        LiteralContentCondition::Contains(text) | LiteralContentCondition::DoesNotContain(text) => {
            text.as_bytes()
        }
    }
}

/// Immutable, bounded admission from one caller-owned snapshot. Its identity
/// is retained by evidence; equal-looking replacement sets are not substitutes.
pub(super) struct LiteralSet {
    patterns: Vec<Pattern>,
}

impl LiteralSet {
    pub(super) fn admit(
        conditions: &[LiteralContentCondition],
    ) -> Result<Self, ConditionEvaluation> {
        if conditions.is_empty() || conditions.len() > MAX_RULES {
            return Err(ConditionEvaluation::Invalid {
                reason: "content literal set has an unsupported count",
            });
        }
        // Validate the whole set before allocating/copying any pattern state.
        for condition in conditions {
            condition
                .validate()
                .map_err(|reason| ConditionEvaluation::Invalid { reason })?;
        }
        let patterns = conditions
            .iter()
            .map(|condition| {
                let bytes = literal(condition);
                debug_assert!(bytes.len() <= MAX_CONTENT_LITERAL_BYTES);
                let mut fallback = vec![0; bytes.len()];
                let mut prefix = 0;
                for index in 1..bytes.len() {
                    while prefix > 0 && bytes[index] != bytes[prefix] {
                        prefix = fallback[prefix - 1];
                    }
                    if bytes[index] == bytes[prefix] {
                        prefix += 1;
                    }
                    fallback[index] = prefix;
                }
                Pattern {
                    condition: condition.clone(),
                    fallback,
                }
            })
            .collect();
        Ok(Self { patterns })
    }
}

impl fmt::Debug for LiteralSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiteralSet")
            .field("count", &self.patterns.len())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ScanObservation {
    pub now: Instant,
    pub cancelled: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum StreamEnd {
    /// Caller observed actual EOF and supplies final source length.
    Eof {
        final_length: u64,
    },
    MissingEof,
    ReadFailed,
    Unsupported,
}

#[derive(Default)]
struct MatchState {
    prefix: usize,
    hit: bool,
}

pub(super) struct LiteralScan<'a> {
    set: &'a LiteralSet,
    states: Vec<MatchState>,
    expected_length: u64,
    // Bytes supplied by the caller, including chunks the parser refused.
    // Parser completion is tracked by sticky failure, not by this counter.
    received_bytes: u64,
    started: Instant,
    last_observation: Instant,
    utf8_carry: [u8; 4],
    utf8_len: usize,
    failure: Option<ConditionEvaluation>,
}

impl fmt::Debug for LiteralScan<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiteralScan")
            .field("received_bytes", &self.received_bytes)
            .field("failed", &self.failure.is_some())
            .finish_non_exhaustive()
    }
}

fn unavailable(reason: ConditionUnavailable) -> ConditionEvaluation {
    ConditionEvaluation::Unavailable { reason }
}

fn changed() -> ConditionEvaluation {
    ConditionEvaluation::Changed {
        reason: "content source or length changed",
    }
}

impl<'a> LiteralScan<'a> {
    pub(super) fn begin(
        set: &'a LiteralSet,
        expected_length: u64,
        started: Instant,
    ) -> Result<Self, ConditionEvaluation> {
        if expected_length > MAX_SCAN_BYTES {
            return Err(unavailable(ConditionUnavailable::ContentBudget));
        }
        Ok(Self {
            set,
            states: (0..set.patterns.len())
                .map(|_| MatchState::default())
                .collect(),
            expected_length,
            received_bytes: 0,
            started,
            last_observation: started,
            utf8_carry: [0; 4],
            utf8_len: 0,
            failure: None,
        })
    }

    fn fail(&mut self, failure: ConditionEvaluation) -> ConditionEvaluation {
        match self.failure {
            Some(existing) if !matches!(failure, ConditionEvaluation::Changed { .. }) => existing,
            _ => {
                self.failure = Some(failure);
                failure
            }
        }
    }

    fn check(
        &mut self,
        observe: &mut impl FnMut() -> ScanObservation,
    ) -> Result<(), ConditionEvaluation> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let observation = observe();
        let failure = if observation.cancelled {
            Some(unavailable(ConditionUnavailable::Cancelled))
        } else if observation.now < self.last_observation
            || observation.now.duration_since(self.started) >= SCAN_DEADLINE
        {
            Some(unavailable(ConditionUnavailable::Deadline))
        } else {
            None
        };
        self.last_observation = observation.now;
        match failure {
            Some(failure) => Err(self.fail(failure)),
            None => Ok(()),
        }
    }

    fn validate_byte(&mut self, byte: u8) -> bool {
        if byte == 0 {
            return false;
        }
        if self.utf8_len == 0 && byte.is_ascii() {
            return true;
        }
        self.utf8_carry[self.utf8_len] = byte;
        self.utf8_len += 1;
        match std::str::from_utf8(&self.utf8_carry[..self.utf8_len]) {
            Ok(_) => self.utf8_len = 0,
            Err(error) if error.error_len().is_none() && self.utf8_len < 4 => {}
            Err(_) => return false,
        }
        true
    }

    /// Consumes each supplied chunk once for the entire set. Success here is
    /// provisional: even an early hit must pass all later input and EOF checks.
    pub(super) fn feed(
        &mut self,
        chunk: &[u8],
        observe: &mut impl FnMut() -> ScanObservation,
    ) -> Result<(), ConditionEvaluation> {
        // Receipt is independent of successful processing. Count even after a
        // terminal refusal so EOF can distinguish unavailable parsing from a
        // genuinely short/long stream. Saturation cannot hide an overrun since
        // admitted expected lengths are at most MAX_SCAN_BYTES.
        self.received_bytes = self.received_bytes.saturating_add(chunk.len() as u64);
        if self.received_bytes > self.expected_length {
            return Err(self.fail(changed()));
        }
        self.check(observe)?;
        if chunk.len() > MAX_CHUNK_BYTES {
            return Err(self.fail(unavailable(ConditionUnavailable::ContentChunk)));
        }
        if self.received_bytes > MAX_SCAN_BYTES {
            return Err(self.fail(unavailable(ConditionUnavailable::ContentBudget)));
        }
        for block in chunk.chunks(CHECK_INTERVAL) {
            self.check(observe)?;
            for &byte in block {
                if !self.validate_byte(byte) {
                    return Err(self.fail(unavailable(ConditionUnavailable::ContentEncoding)));
                }
                for (pattern, state) in self.set.patterns.iter().zip(&mut self.states) {
                    if state.hit {
                        continue;
                    }
                    let bytes = literal(&pattern.condition);
                    while state.prefix > 0 && byte != bytes[state.prefix] {
                        state.prefix = pattern.fallback[state.prefix - 1];
                    }
                    if byte == bytes[state.prefix] {
                        state.prefix += 1;
                    }
                    if state.prefix == bytes.len() {
                        state.hit = true;
                    }
                }
            }
        }
        self.check(observe)
    }

    pub(super) fn finish(
        mut self,
        end: StreamEnd,
        binding: ConditionBinding,
        observe: &mut impl FnMut() -> ScanObservation,
    ) -> LiteralEvidence<'a> {
        // Known source changes outrank earlier unavailable input. A failed
        // parser can become more restrictive, but can never be revived.
        let known_change = matches!(binding, ConditionBinding::Changed { .. })
            || matches!(end, StreamEnd::Eof { final_length }
                if final_length != self.expected_length || self.received_bytes != self.expected_length);
        let result = if known_change {
            Err(changed())
        } else {
            self.check(observe).and_then(|()| match end {
                StreamEnd::MissingEof => Err(unavailable(ConditionUnavailable::ContentEof)),
                StreamEnd::ReadFailed => Err(unavailable(ConditionUnavailable::ContentRead)),
                StreamEnd::Unsupported => {
                    Err(unavailable(ConditionUnavailable::ContentUnsupported))
                }
                StreamEnd::Eof { .. } if self.utf8_len != 0 => {
                    Err(unavailable(ConditionUnavailable::ContentEncoding))
                }
                StreamEnd::Eof { .. } => {
                    self.check(observe)?;
                    Ok(CompletedLiterals {
                        hits: self.states.iter().map(|state| state.hit).collect(),
                    })
                }
            })
        };
        LiteralEvidence {
            set: self.set,
            result,
        }
    }
}

// Neither this type nor its fields can be constructed by another module.
struct CompletedLiterals {
    hits: Vec<bool>,
}

pub(super) struct LiteralEvidence<'a> {
    set: &'a LiteralSet,
    result: Result<CompletedLiterals, ConditionEvaluation>,
}

impl fmt::Debug for LiteralEvidence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiteralEvidence")
            .field("complete", &self.result.is_ok())
            .field("failure", &self.result.as_ref().err())
            .finish_non_exhaustive()
    }
}

impl LiteralEvidence<'_> {
    pub(super) fn evaluate(
        &self,
        set: &LiteralSet,
        condition: &LiteralContentCondition,
    ) -> ConditionEvaluation {
        if let Err(reason) = condition.validate() {
            return ConditionEvaluation::Invalid { reason };
        }
        if !std::ptr::eq(self.set, set) {
            return changed();
        }
        let Some(index) = set
            .patterns
            .iter()
            .position(|pattern| &pattern.condition == condition)
        else {
            return changed();
        };
        match &self.result {
            Err(failure) => *failure,
            Ok(complete) => {
                let matched = match condition {
                    LiteralContentCondition::Contains(_) => complete.hits[index],
                    LiteralContentCondition::DoesNotContain(_) => !complete.hits[index],
                };
                if matched {
                    ConditionEvaluation::Match
                } else {
                    ConditionEvaluation::NoMatch {
                        reason: "content literal condition did not match",
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(now: Instant) -> impl FnMut() -> ScanObservation {
        move || ScanObservation {
            now,
            cancelled: false,
        }
    }

    fn conditions(text: &str) -> [LiteralContentCondition; 2] {
        [
            LiteralContentCondition::Contains(text.into()),
            LiteralContentCondition::DoesNotContain(text.into()),
        ]
    }

    fn assert_modes(
        evidence: &LiteralEvidence<'_>,
        set: &LiteralSet,
        conditions: &[LiteralContentCondition],
        hit: bool,
    ) {
        for (index, condition) in conditions.iter().enumerate() {
            let result = evidence.evaluate(set, condition);
            if hit == (index == 0) {
                assert_eq!(result, ConditionEvaluation::Match);
            } else {
                assert!(matches!(result, ConditionEvaluation::NoMatch { .. }));
            }
        }
    }

    #[test]
    fn hotfolder_condition_literal_split_utf8_modes_case_bom_and_empty() {
        for (text, needle, hit) in [
            ("", "x", false),
            ("before aé𝄞z after", "aé𝄞z", true),
            ("aaaaab", "aaab", true),
            ("Case", "case", false),
            ("\u{feff}abc", "\u{feff}a", true),
            ("abc", "\u{feff}a", false),
            ("\u{1}\t\r\n\u{7f}", "\t\r", true),
        ] {
            let conditions = conditions(needle);
            let set = LiteralSet::admit(&conditions).unwrap();
            let bytes = text.as_bytes();
            // Includes every split inside multibyte sequences and literals.
            for split in 0..=bytes.len() {
                let now = Instant::now();
                let mut probe = observation(now);
                let mut scan = LiteralScan::begin(&set, bytes.len() as u64, now).unwrap();
                scan.feed(&bytes[..split], &mut probe).unwrap();
                scan.feed(&bytes[split..], &mut probe).unwrap();
                let evidence = scan.finish(
                    StreamEnd::Eof {
                        final_length: bytes.len() as u64,
                    },
                    ConditionBinding::Current,
                    &mut probe,
                );
                assert_modes(&evidence, &set, &conditions, hit);
            }
        }
    }

    #[test]
    fn hotfolder_condition_literal_early_hit_never_bypasses_terminal_failure() {
        let conditions = conditions("hit");
        let set = LiteralSet::admit(&conditions).unwrap();
        for scenario in 0..10 {
            let now = Instant::now();
            let mut probe = observation(now);
            let expected = if matches!(scenario, 0..=2 | 8) { 4 } else { 3 };
            let mut scan = LiteralScan::begin(&set, expected, now).unwrap();
            scan.feed(b"hit", &mut probe).unwrap();
            if scenario <= 2 {
                let tail = match scenario {
                    0 => [0xff],
                    1 => [0],
                    _ => [0xc3],
                };
                let result = scan.feed(&tail, &mut probe);
                assert_eq!(result.is_err(), scenario < 2);
            }
            let end = match scenario {
                3 => StreamEnd::ReadFailed,
                4 => StreamEnd::MissingEof,
                5 => StreamEnd::Unsupported,
                _ => StreamEnd::Eof {
                    final_length: expected,
                },
            };
            let binding = if scenario == 9 {
                ConditionBinding::Changed {
                    reason: "fixture changed",
                }
            } else {
                ConditionBinding::Current
            };
            let mut terminal_probe = || ScanObservation {
                now: now
                    + if scenario == 7 {
                        SCAN_DEADLINE
                    } else {
                        Duration::ZERO
                    },
                cancelled: scenario == 6,
            };
            let evidence = scan.finish(end, binding, &mut terminal_probe);
            for condition in &conditions {
                let result = evidence.evaluate(&set, condition);
                if scenario >= 8 {
                    assert!(
                        matches!(result, ConditionEvaluation::Changed { .. }),
                        "scenario {scenario}"
                    );
                } else {
                    assert!(
                        matches!(result, ConditionEvaluation::Unavailable { .. }),
                        "scenario {scenario}"
                    );
                }
            }
        }
    }

    #[test]
    fn hotfolder_condition_literal_limits_are_candidate_wide_and_eof_is_explicit() {
        let conditions = [
            LiteralContentCondition::Contains("a".into()),
            LiteralContentCondition::Contains("b".into()),
            LiteralContentCondition::DoesNotContain("z".into()),
        ];
        let set = LiteralSet::admit(&conditions).unwrap();
        let now = Instant::now();
        assert!(matches!(
            LiteralScan::begin(&set, MAX_SCAN_BYTES + 1, now),
            Err(ConditionEvaluation::Unavailable {
                reason: ConditionUnavailable::ContentBudget
            })
        ));
        let chunk = vec![b'a'; MAX_CHUNK_BYTES];
        let mut probe = observation(now);
        let mut scan = LiteralScan::begin(&set, MAX_SCAN_BYTES, now).unwrap();
        for _ in 0..MAX_SCAN_BYTES / MAX_CHUNK_BYTES as u64 {
            scan.feed(&chunk, &mut probe).unwrap();
        }
        assert_eq!(scan.received_bytes, MAX_SCAN_BYTES);
        assert_eq!(scan.states.len(), 3);
        let evidence = scan.finish(
            StreamEnd::Eof {
                final_length: MAX_SCAN_BYTES,
            },
            ConditionBinding::Current,
            &mut probe,
        );
        assert_eq!(
            evidence.evaluate(&set, &conditions[0]),
            ConditionEvaluation::Match
        );
        assert!(matches!(
            evidence.evaluate(&set, &conditions[1]),
            ConditionEvaluation::NoMatch { .. }
        ));
        assert_eq!(
            evidence.evaluate(&set, &conditions[2]),
            ConditionEvaluation::Match
        );

        let mut scan = LiteralScan::begin(&set, 1, now).unwrap();
        scan.feed(b"a", &mut probe).unwrap();
        let evidence = scan.finish(StreamEnd::MissingEof, ConditionBinding::Current, &mut probe);
        assert_eq!(
            evidence.evaluate(&set, &conditions[0]),
            unavailable(ConditionUnavailable::ContentEof)
        );
        let mut scan = LiteralScan::begin(&set, MAX_CHUNK_BYTES as u64 + 1, now).unwrap();
        assert_eq!(
            scan.feed(&vec![b'a'; MAX_CHUNK_BYTES + 1], &mut probe),
            Err(unavailable(ConditionUnavailable::ContentChunk))
        );
        assert_eq!(
            scan.feed(&[], &mut probe),
            Err(unavailable(ConditionUnavailable::ContentChunk))
        );
        // Further supplied bytes really exceed the admitted length, even
        // though the earlier resource refusal prevented parser processing.
        assert_eq!(scan.feed(b"a", &mut probe), Err(changed()));
        let evidence = scan.finish(StreamEnd::ReadFailed, ConditionBinding::Current, &mut probe);
        assert_eq!(evidence.evaluate(&set, &conditions[0]), changed());
    }

    #[test]
    fn hotfolder_condition_literal_received_eof_preserves_resource_refusal() {
        let conditions = conditions("hit");
        let set = LiteralSet::admit(&conditions).unwrap();
        for scenario in 0..4 {
            let now = Instant::now();
            let prefix = if scenario == 0 { &b""[..] } else { &b"hit"[..] };
            let tail = vec![
                b'x';
                if scenario <= 1 {
                    MAX_CHUNK_BYTES + 1
                } else {
                    1
                }
            ];
            let expected_length = (prefix.len() + tail.len()) as u64;
            let failure = unavailable(match scenario {
                0 | 1 => ConditionUnavailable::ContentChunk,
                2 => ConditionUnavailable::Cancelled,
                _ => ConditionUnavailable::Deadline,
            });
            let mut scan = LiteralScan::begin(&set, expected_length, now).unwrap();
            scan.feed(prefix, &mut observation(now)).unwrap();
            let mut entry_probe = || ScanObservation {
                now: now
                    + if scenario == 3 {
                        SCAN_DEADLINE
                    } else {
                        Duration::ZERO
                    },
                cancelled: scenario == 2,
            };
            assert_eq!(scan.feed(&tail, &mut entry_probe), Err(failure));
            assert_eq!(scan.received_bytes, expected_length);
            // Clearing a cancellation/clock observation cannot revive parsing.
            assert_eq!(scan.feed(&[], &mut observation(now)), Err(failure));
            let evidence = scan.finish(
                StreamEnd::Eof {
                    final_length: expected_length,
                },
                ConditionBinding::Current,
                &mut observation(now),
            );
            assert!(evidence.result.is_err());
            for condition in &conditions {
                assert_eq!(
                    evidence.evaluate(&set, condition),
                    failure,
                    "scenario {scenario}"
                );
            }
        }
    }

    #[test]
    fn hotfolder_condition_literal_received_eof_retains_real_change_precedence() {
        let conditions = conditions("hit");
        let set = LiteralSet::admit(&conditions).unwrap();
        for refusal in 0..3 {
            for change in 0..4 {
                let now = Instant::now();
                let tail = vec![b'x'; if refusal == 0 { MAX_CHUNK_BYTES + 1 } else { 1 }];
                let supplied_length = 3 + tail.len() as u64;
                let expected_length = supplied_length + u64::from(change == 0);
                let mut scan = LiteralScan::begin(&set, expected_length, now).unwrap();
                scan.feed(b"hit", &mut observation(now)).unwrap();
                let failure = unavailable(match refusal {
                    0 => ConditionUnavailable::ContentChunk,
                    1 => ConditionUnavailable::Cancelled,
                    _ => ConditionUnavailable::Deadline,
                });
                assert_eq!(
                    scan.feed(&tail, &mut || ScanObservation {
                        now: now
                            + if refusal == 2 {
                                SCAN_DEADLINE
                            } else {
                                Duration::ZERO
                            },
                        cancelled: refusal == 1,
                    }),
                    Err(failure)
                );
                if change == 1 {
                    assert_eq!(scan.feed(b"!", &mut observation(now)), Err(changed()));
                }
                let end = if change == 1 {
                    // A known overrun stays Changed even without terminal EOF.
                    StreamEnd::ReadFailed
                } else {
                    StreamEnd::Eof {
                        final_length: expected_length + u64::from(change == 2),
                    }
                };
                let binding = if change == 3 {
                    ConditionBinding::Changed {
                        reason: "fixture changed",
                    }
                } else {
                    ConditionBinding::Current
                };
                let evidence = scan.finish(end, binding, &mut observation(now));
                assert!(evidence.result.is_err());
                for condition in &conditions {
                    assert_eq!(
                        evidence.evaluate(&set, condition),
                        changed(),
                        "refusal {refusal}, change {change}"
                    );
                }
            }
        }
        // An entry cancellation cannot conceal an overrun in that same chunk.
        let now = Instant::now();
        let mut scan = LiteralScan::begin(&set, 3, now).unwrap();
        scan.feed(b"hit", &mut observation(now)).unwrap();
        assert_eq!(
            scan.feed(b"!", &mut || ScanObservation {
                now,
                cancelled: true
            }),
            Err(changed())
        );
        let evidence = scan.finish(
            StreamEnd::Eof { final_length: 3 },
            ConditionBinding::Current,
            &mut observation(now),
        );
        for condition in &conditions {
            assert_eq!(evidence.evaluate(&set, condition), changed());
        }
    }

    #[test]
    fn hotfolder_condition_literal_admission_bounds_and_redacted_debug() {
        let max_literal = LiteralContentCondition::Contains("x".repeat(MAX_CONTENT_LITERAL_BYTES));
        let set = LiteralSet::admit(std::slice::from_ref(&max_literal)).unwrap();
        let now = Instant::now();
        let mut probe = observation(now);
        let mut scan = LiteralScan::begin(&set, MAX_CONTENT_LITERAL_BYTES as u64, now).unwrap();
        scan.feed(&vec![b'x'; MAX_CONTENT_LITERAL_BYTES - 1], &mut probe)
            .unwrap();
        scan.feed(b"x", &mut probe).unwrap();
        let evidence = scan.finish(
            StreamEnd::Eof {
                final_length: MAX_CONTENT_LITERAL_BYTES as u64,
            },
            ConditionBinding::Current,
            &mut probe,
        );
        assert_eq!(
            evidence.evaluate(&set, &max_literal),
            ConditionEvaluation::Match
        );
        assert!(LiteralSet::admit(&vec![max_literal.clone(); MAX_RULES]).is_ok());
        assert!(matches!(
            LiteralSet::admit(&vec![max_literal; MAX_RULES + 1]),
            Err(ConditionEvaluation::Invalid { .. })
        ));
        for invalid in [
            "".to_owned(),
            "x".repeat(MAX_CONTENT_LITERAL_BYTES + 1),
            "bad\0literal".into(),
        ] {
            assert!(matches!(
                LiteralSet::admit(&[LiteralContentCondition::Contains(invalid)]),
                Err(ConditionEvaluation::Invalid { .. })
            ));
        }
        assert!(matches!(
            LiteralSet::admit(&[]),
            Err(ConditionEvaluation::Invalid { .. })
        ));

        let sentinel = "private-content-sentinel";
        let conditions = conditions(sentinel);
        let set = LiteralSet::admit(&conditions).unwrap();
        let mut scan = LiteralScan::begin(&set, sentinel.len() as u64, now).unwrap();
        scan.feed(sentinel.as_bytes(), &mut probe).unwrap();
        let before = format!("{set:?} {scan:?}");
        let evidence = scan.finish(
            StreamEnd::Eof {
                final_length: sentinel.len() as u64,
            },
            ConditionBinding::Current,
            &mut probe,
        );
        assert!(!before.contains(sentinel));
        assert!(
            !format!("{evidence:?} {:?}", evidence.evaluate(&set, &conditions[0]))
                .contains(sentinel)
        );
    }

    #[test]
    fn hotfolder_condition_literal_deadline_cancellation_and_clock_never_renew() {
        let conditions = conditions("hit");
        let set = LiteralSet::admit(&conditions).unwrap();
        for cancel in [false, true] {
            let now = Instant::now();
            let mut calls = 0;
            let mut probe = || {
                calls += 1;
                ScanObservation {
                    now: now
                        + if !cancel && calls >= 3 {
                            SCAN_DEADLINE
                        } else {
                            Duration::ZERO
                        },
                    cancelled: cancel && calls >= 3,
                }
            };
            let mut scan = LiteralScan::begin(&set, 1024, now).unwrap();
            let failure = scan.feed(&vec![b'a'; 1024], &mut probe).unwrap_err();
            assert_eq!(
                failure,
                unavailable(if cancel {
                    ConditionUnavailable::Cancelled
                } else {
                    ConditionUnavailable::Deadline
                })
            );
            assert_eq!(scan.feed(&[], &mut observation(now)), Err(failure));
            let evidence = scan.finish(
                StreamEnd::Eof { final_length: 1024 },
                ConditionBinding::Current,
                &mut observation(now),
            );
            assert_eq!(evidence.evaluate(&set, &conditions[0]), failure);
        }
        let now = Instant::now();
        let mut scan = LiteralScan::begin(&set, 4, now).unwrap();
        scan.feed(b"hit", &mut observation(now + Duration::from_millis(1900)))
            .unwrap();
        assert_eq!(
            scan.feed(b"!", &mut observation(now + SCAN_DEADLINE)),
            Err(unavailable(ConditionUnavailable::Deadline))
        );
        let mut scan = LiteralScan::begin(&set, 1, now).unwrap();
        scan.feed(&[], &mut observation(now + Duration::from_secs(1)))
            .unwrap();
        assert_eq!(
            scan.feed(b"x", &mut observation(now)),
            Err(unavailable(ConditionUnavailable::Deadline))
        );

        // A cancellation arriving on the final check also invalidates a hit.
        let mut scan = LiteralScan::begin(&set, 3, now).unwrap();
        scan.feed(b"hit", &mut observation(now)).unwrap();
        let mut calls = 0;
        let evidence = scan.finish(
            StreamEnd::Eof { final_length: 3 },
            ConditionBinding::Current,
            &mut || {
                calls += 1;
                ScanObservation {
                    now,
                    cancelled: calls == 2,
                }
            },
        );
        assert_eq!(
            evidence.evaluate(&set, &conditions[0]),
            unavailable(ConditionUnavailable::Cancelled)
        );
    }

    #[test]
    fn hotfolder_condition_literal_source_changes_and_set_identity_refuse_old_evidence() {
        let conditions = conditions("hit");
        let set = LiteralSet::admit(&conditions).unwrap();
        let replacement = LiteralSet::admit(&conditions).unwrap();
        let now = Instant::now();
        let mut scan = LiteralScan::begin(&set, 3, now).unwrap();
        scan.feed(b"hit", &mut observation(now)).unwrap();
        let evidence = scan.finish(
            StreamEnd::Eof { final_length: 3 },
            ConditionBinding::Current,
            &mut observation(now),
        );
        assert_eq!(
            evidence.evaluate(&set, &conditions[0]),
            ConditionEvaluation::Match
        );
        assert!(matches!(
            evidence.evaluate(&replacement, &conditions[0]),
            ConditionEvaluation::Changed { .. }
        ));
        assert!(matches!(
            evidence.evaluate(&set, &LiteralContentCondition::Contains("other".into())),
            ConditionEvaluation::Changed { .. }
        ));
        assert!(matches!(
            evidence.evaluate(
                &replacement,
                &LiteralContentCondition::Contains(String::new())
            ),
            ConditionEvaluation::Invalid { .. }
        ));
        for final_length in [2, 4] {
            let mut scan = LiteralScan::begin(&set, 3, now).unwrap();
            scan.feed(b"hit", &mut observation(now)).unwrap();
            let evidence = scan.finish(
                StreamEnd::Eof { final_length },
                ConditionBinding::Current,
                &mut observation(now),
            );
            assert!(matches!(
                evidence.evaluate(&set, &conditions[0]),
                ConditionEvaluation::Changed { .. }
            ));
        }
        let mut scan = LiteralScan::begin(&set, 3, now).unwrap();
        scan.feed(b"hit", &mut observation(now)).unwrap();
        assert!(matches!(
            scan.feed(b"!", &mut observation(now)),
            Err(ConditionEvaluation::Changed { .. })
        ));
        let evidence = scan.finish(
            StreamEnd::Eof { final_length: 3 },
            ConditionBinding::Current,
            &mut observation(now),
        );
        assert!(matches!(
            evidence.evaluate(&set, &conditions[0]),
            ConditionEvaluation::Changed { .. }
        ));
        let mut scan = LiteralScan::begin(&set, 1, now).unwrap();
        scan.feed(&[0xff], &mut observation(now)).unwrap_err();
        let evidence = scan.finish(
            StreamEnd::ReadFailed,
            ConditionBinding::Changed {
                reason: "fixture changed",
            },
            &mut observation(now),
        );
        assert!(matches!(
            evidence.evaluate(&set, &conditions[0]),
            ConditionEvaluation::Changed { .. }
        ));
    }
}
