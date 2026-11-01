//! Pure, bounded Unicode observations for explicitly supplied text.
//!
//! This module keeps the caller's source string separate from the bounded row
//! model. It never normalizes or copies that source. Offsets are zero-based
//! UTF-8 byte and Unicode scalar indices; displayed line and scalar-column
//! coordinates are one-based. In CRLF, both scalars are reported on the old
//! line and the pair advances the next scalar by one line. A standalone CR or
//! LF advances the line after that scalar.
//!
//! Names and grapheme boundaries use Unicode 17.0. General categories and
//! binary properties supplied by `regex` use Unicode 16.0. The data sources
//! intentionally remain independent and this difference is exposed for UI
//! help instead of being presented as one uniform Unicode version.

use regex::Regex;
use std::fmt;
use std::sync::OnceLock;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_segmentation::UnicodeSegmentation;

/// Maximum admitted input size, measured in its original UTF-8 bytes.
pub const MAX_INPUT_BYTES: usize = 256 * 1024;

/// Maximum number of per-scalar result rows.
pub const MAX_SCALAR_ROWS: usize = 65_536;

/// Data versions used by the independently maintained Unicode tables.
pub const DATA_VERSION_NOTE: &str = "Names and grapheme boundaries use Unicode 17.0; general categories and regex binary properties use Unicode 16.0.";

const WHITESPACE: u8 = 1 << 0;
const CONTROL: u8 = 1 << 1;
const COMBINING: u8 = 1 << 2;
const DEFAULT_IGNORABLE: u8 = 1 << 3;
const BIDI_CONTROL: u8 = 1 << 4;

#[derive(Clone, Copy, PartialEq, Eq)]
struct CharacterFlags(u8);

impl CharacterFlags {
    fn from(character: char, category: GeneralCategory) -> Self {
        let properties = binary_properties();
        let mut flags = 0;
        if matches_property(&properties.whitespace, character) {
            flags |= WHITESPACE;
        }
        if category == GeneralCategory::Control {
            flags |= CONTROL;
        }
        if matches!(
            category,
            GeneralCategory::NonspacingMark
                | GeneralCategory::SpacingMark
                | GeneralCategory::EnclosingMark
        ) {
            flags |= COMBINING;
        }
        if matches_property(&properties.default_ignorable, character) {
            flags |= DEFAULT_IGNORABLE;
        }
        if matches_property(&properties.bidi_control, character) {
            flags |= BIDI_CONTROL;
        }
        Self(flags)
    }

    pub const fn is_whitespace(self) -> bool {
        self.0 & WHITESPACE != 0
    }

    pub const fn is_control(self) -> bool {
        self.0 & CONTROL != 0
    }

    pub const fn is_combining(self) -> bool {
        self.0 & COMBINING != 0
    }

    pub const fn is_default_ignorable(self) -> bool {
        self.0 & DEFAULT_IGNORABLE != 0
    }

    pub const fn is_bidi_control(self) -> bool {
        self.0 & BIDI_CONTROL != 0
    }
}

impl fmt::Debug for CharacterFlags {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CharacterFlags")
            .field("whitespace", &self.is_whitespace())
            .field("control", &self.is_control())
            .field("combining", &self.is_combining())
            .field("default_ignorable", &self.is_default_ignorable())
            .field("bidi_control", &self.is_bidi_control())
            .finish()
    }
}

struct BinaryProperties {
    whitespace: Regex,
    default_ignorable: Regex,
    bidi_control: Regex,
}

static BINARY_PROPERTIES: OnceLock<BinaryProperties> = OnceLock::new();

fn binary_properties() -> &'static BinaryProperties {
    BINARY_PROPERTIES.get_or_init(|| BinaryProperties {
        whitespace: Regex::new(r"\A\p{White_Space}\z")
            .expect("the pinned Unicode White_Space property is valid"),
        default_ignorable: Regex::new(r"\A\p{Default_Ignorable_Code_Point}\z")
            .expect("the pinned Unicode Default_Ignorable_Code_Point property is valid"),
        bidi_control: Regex::new(r"\A\p{Bidi_Control}\z")
            .expect("the pinned Unicode Bidi_Control property is valid"),
    })
}

fn matches_property(property: &Regex, character: char) -> bool {
    let mut encoded = [0; 4];
    property.is_match(character.encode_utf8(&mut encoded))
}

/// A single source Unicode scalar and its exact offsets and classifications.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ScalarObservation {
    byte_offset: usize,
    scalar_index: usize,
    line: usize,
    column: usize,
    character: char,
    category: GeneralCategory,
    flags: CharacterFlags,
}

/// A scalar's UTF-8 encoding in a fixed four-byte buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf8Encoding {
    bytes: [u8; 4],
    len: u8,
}

impl Utf8Encoding {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// A scalar's UTF-16 encoding in a fixed two-unit buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Encoding {
    units: [u16; 2],
    len: u8,
}

impl Utf16Encoding {
    pub fn units(&self) -> &[u16] {
        &self.units[..usize::from(self.len)]
    }
}

impl ScalarObservation {
    /// Zero-based byte offset in the original UTF-8 input.
    pub const fn byte_offset(&self) -> usize {
        self.byte_offset
    }

    /// Zero-based Unicode scalar offset in the original input.
    pub const fn scalar_index(&self) -> usize {
        self.scalar_index
    }

    /// One-based logical line number. CRLF advances only after its LF scalar.
    pub const fn line(&self) -> usize {
        self.line
    }

    /// One-based scalar column within `line`.
    pub const fn column(&self) -> usize {
        self.column
    }

    pub const fn character(&self) -> char {
        self.character
    }

    pub const fn code_point(&self) -> u32 {
        self.character as u32
    }

    pub const fn utf8_byte_count(&self) -> u8 {
        self.character.len_utf8() as u8
    }

    pub const fn utf16_unit_count(&self) -> u8 {
        self.character.len_utf16() as u8
    }

    /// Return the actual UTF-8 bytes without allocating.
    pub fn utf8_encoding(&self) -> Utf8Encoding {
        let mut bytes = [0; 4];
        let len = self.character.encode_utf8(&mut bytes).len() as u8;
        Utf8Encoding { bytes, len }
    }

    /// Return the actual UTF-16 code units without allocating.
    pub fn utf16_encoding(&self) -> Utf16Encoding {
        let mut units = [0; 2];
        let len = self.character.encode_utf16(&mut units).len() as u8;
        Utf16Encoding { units, len }
    }

    pub const fn category(&self) -> GeneralCategory {
        self.category
    }

    pub fn category_abbreviation(&self) -> &'static str {
        self.category.abbreviation()
    }

    pub const fn is_whitespace(&self) -> bool {
        self.flags.is_whitespace()
    }

    pub const fn is_control(&self) -> bool {
        self.flags.is_control()
    }

    pub const fn is_combining(&self) -> bool {
        self.flags.is_combining()
    }

    pub const fn is_default_ignorable(&self) -> bool {
        self.flags.is_default_ignorable()
    }

    /// This is the Unicode `Bidi_Control` binary property, not a bidi-class
    /// analysis of the surrounding text.
    pub const fn is_bidi_control(&self) -> bool {
        self.flags.is_bidi_control()
    }

    /// Return the standard Unicode name when one is assigned.
    ///
    /// The compact name iterator is created on demand; rows do not each store
    /// an allocated name string.
    pub fn unicode_name(&self) -> Option<unicode_names2::Name> {
        unicode_names2::name(self.character)
    }

    /// Return an ASCII-only representation suitable for result labels.
    /// Non-ASCII scalars, controls, and directional characters are escaped.
    pub fn escaped(&self) -> String {
        self.character.escape_default().collect()
    }
}

impl fmt::Debug for ScalarObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code_point = format!("U+{:04X}", self.code_point());
        formatter
            .debug_struct("ScalarObservation")
            .field("byte_offset", &self.byte_offset)
            .field("scalar_index", &self.scalar_index)
            .field("line", &self.line)
            .field("column", &self.column)
            .field("code_point", &code_point)
            .field("utf8_byte_count", &self.utf8_byte_count())
            .field("utf16_unit_count", &self.utf16_unit_count())
            .field("category", &self.category_abbreviation())
            .field("flags", &self.flags)
            .finish()
    }
}

/// Bounded observations for one explicitly supplied input string.
///
/// The caller remains the sole owner of the original source. This result only
/// stores source scalars and offsets once per row; it never clones the input.
#[derive(Clone, PartialEq, Eq)]
pub struct Inspection {
    byte_count: usize,
    scalar_count: usize,
    grapheme_count: usize,
    utf16_unit_count: usize,
    rows: Vec<ScalarObservation>,
}

impl Inspection {
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }

    pub const fn scalar_count(&self) -> usize {
        self.scalar_count
    }

    pub const fn grapheme_count(&self) -> usize {
        self.grapheme_count
    }

    pub const fn utf16_unit_count(&self) -> usize {
        self.utf16_unit_count
    }

    pub fn rows(&self) -> &[ScalarObservation] {
        &self.rows
    }
}

impl fmt::Debug for Inspection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Inspection")
            .field("byte_count", &self.byte_count)
            .field("scalar_count", &self.scalar_count)
            .field("grapheme_count", &self.grapheme_count)
            .field("utf16_unit_count", &self.utf16_unit_count)
            .field("row_count", &self.rows.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectionError {
    InputTooManyBytes { actual: usize, limit: usize },
    InputTooManyScalars { actual: usize, limit: usize },
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooManyBytes { actual, limit } => write!(
                formatter,
                "Input is {actual} UTF-8 bytes; the limit is {limit} bytes"
            ),
            Self::InputTooManyScalars { actual, limit } => write!(
                formatter,
                "Input has {actual} Unicode scalars; the row limit is {limit}"
            ),
        }
    }
}

impl std::error::Error for InspectionError {}

/// Inspect the complete input or reject it before allocating result rows.
pub fn inspect(input: &str) -> Result<Inspection, InspectionError> {
    let byte_count = input.len();
    if byte_count > MAX_INPUT_BYTES {
        return Err(InspectionError::InputTooManyBytes {
            actual: byte_count,
            limit: MAX_INPUT_BYTES,
        });
    }

    let scalar_count = input.chars().count();
    if scalar_count > MAX_SCALAR_ROWS {
        return Err(InspectionError::InputTooManyScalars {
            actual: scalar_count,
            limit: MAX_SCALAR_ROWS,
        });
    }

    let mut rows = Vec::with_capacity(scalar_count);
    let mut line = 1;
    let mut column = 1;
    let mut chars = input.char_indices().peekable();
    while let Some((byte_offset, character)) = chars.next() {
        let category = get_general_category(character);
        rows.push(ScalarObservation {
            byte_offset,
            scalar_index: rows.len(),
            line,
            column,
            character,
            category,
            flags: CharacterFlags::from(character, category),
        });

        match character {
            '\r' if chars.peek().is_some_and(|(_, next)| *next == '\n') => column += 1,
            '\r' | '\n' => {
                line += 1;
                column = 1;
            }
            _ => column += 1,
        }
    }

    let grapheme_count = input.graphemes(true).count();
    let utf16_unit_count = input.encode_utf16().count();
    debug_assert_eq!(rows.len(), scalar_count);

    Ok(Inspection {
        byte_count,
        scalar_count,
        grapheme_count,
        utf16_unit_count,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_general_category::GeneralCategory;

    #[test]
    fn empty_input_is_a_valid_zero_observation() {
        let inspection = inspect("").unwrap();
        assert_eq!(inspection.byte_count(), 0);
        assert_eq!(inspection.scalar_count(), 0);
        assert_eq!(inspection.grapheme_count(), 0);
        assert_eq!(inspection.utf16_unit_count(), 0);
        assert!(inspection.rows().is_empty());
    }

    #[test]
    fn composed_and_decomposed_text_keeps_exact_scalar_and_grapheme_offsets() {
        let source = "e\u{0301}é";
        let inspection = inspect(source).unwrap();
        assert_eq!(source, "e\u{0301}é");
        assert_eq!(inspection.scalar_count(), 3);
        assert_eq!(inspection.grapheme_count(), 2);
        assert_eq!(inspection.byte_count(), 5);
        assert_eq!(
            inspection
                .rows()
                .iter()
                .map(ScalarObservation::byte_offset)
                .collect::<Vec<_>>(),
            [0, 1, 3]
        );
        assert!(inspection.rows()[1].is_combining());
        assert_eq!(inspection.rows()[2].character(), 'é');
    }

    #[test]
    fn emoji_zwj_sequence_counts_scalars_graphemes_and_encodings_separately() {
        let inspection = inspect("👩\u{200d}💻").unwrap();
        assert_eq!(inspection.scalar_count(), 3);
        assert_eq!(inspection.grapheme_count(), 1);
        assert_eq!(inspection.byte_count(), 11);
        assert_eq!(inspection.utf16_unit_count(), 5);
        assert_eq!(inspection.rows()[1].code_point(), 0x200d);
        assert!(inspection.rows()[1].is_default_ignorable());
    }

    #[test]
    fn scalar_rows_expose_actual_utf8_bytes_and_utf16_units() {
        let inspection = inspect("😀\u{0301}\n").unwrap();
        assert_eq!(
            inspection.rows()[0].utf8_encoding().bytes(),
            &[0xf0, 0x9f, 0x98, 0x80]
        );
        assert_eq!(
            inspection.rows()[0].utf16_encoding().units(),
            &[0xd83d, 0xde00]
        );
        assert_eq!(inspection.rows()[1].utf8_encoding().bytes(), &[0xcc, 0x81]);
        assert_eq!(inspection.rows()[1].utf16_encoding().units(), &[0x0301]);
        assert_eq!(inspection.rows()[2].utf8_encoding().bytes(), &[0x0a]);
        assert_eq!(inspection.rows()[2].utf16_encoding().units(), &[0x000a]);
        assert_eq!(inspection.rows()[0].utf8_byte_count(), 4);
        assert_eq!(inspection.rows()[0].utf16_unit_count(), 2);
    }

    #[test]
    fn unicode_whitespace_ignorable_combining_and_bidi_properties_are_distinct() {
        let inspection =
            inspect("\u{00a0}\u{200b}\u{200d}\u{feff}\t\n\u{202e}\u{2066}\u{0301}").unwrap();
        let rows = inspection.rows();
        assert!(rows[0].is_whitespace());
        assert_eq!(rows[0].code_point(), 0x00a0);
        assert!(rows[1].is_default_ignorable());
        assert!(rows[2].is_default_ignorable());
        assert!(rows[3].is_default_ignorable());
        assert!(rows[4].is_whitespace() && rows[4].is_control());
        assert!(rows[5].is_whitespace() && rows[5].is_control());
        assert!(rows[6].is_bidi_control() && rows[6].is_default_ignorable());
        assert!(rows[7].is_bidi_control() && rows[7].is_default_ignorable());
        assert!(rows[8].is_combining());
    }

    #[test]
    fn crlf_keeps_both_scalars_and_advances_one_logical_line() {
        let source = "a\r\n😀e\u{0301}";
        let inspection = inspect(source).unwrap();
        assert_eq!(inspection.scalar_count(), 6);
        assert_eq!(inspection.grapheme_count(), 4);
        assert_eq!(inspection.byte_count(), 10);
        assert_eq!(inspection.utf16_unit_count(), 7);
        assert_eq!(
            inspection
                .rows()
                .iter()
                .map(ScalarObservation::byte_offset)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3, 7, 8]
        );
        assert_eq!(
            inspection
                .rows()
                .iter()
                .map(|row| (row.line(), row.column()))
                .collect::<Vec<_>>(),
            [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (2, 3)]
        );
    }

    #[test]
    fn standalone_cr_and_lf_each_advance_the_next_line() {
        let inspection = inspect("a\rb\nc").unwrap();
        assert_eq!(
            inspection
                .rows()
                .iter()
                .map(|row| (row.line(), row.column()))
                .collect::<Vec<_>>(),
            [(1, 1), (1, 2), (2, 1), (2, 2), (3, 1)]
        );
    }

    #[test]
    fn names_categories_and_unavailable_fallbacks_are_truthful() {
        let inspection = inspect("A\u{e000}\u{0378}").unwrap();
        assert_eq!(
            inspection.rows()[0]
                .unicode_name()
                .map(|name| name.to_string())
                .as_deref(),
            Some("LATIN CAPITAL LETTER A")
        );
        assert_eq!(inspection.rows()[0].category_abbreviation(), "Lu");
        assert!(inspection.rows()[1].unicode_name().is_none());
        assert_eq!(inspection.rows()[1].category(), GeneralCategory::PrivateUse);
        assert!(inspection.rows()[2].unicode_name().is_none());
        assert_eq!(inspection.rows()[2].category(), GeneralCategory::Unassigned);
    }

    #[test]
    fn escaped_labels_are_ascii_and_debug_never_contains_input_or_raw_scalars() {
        let source = "SECRET\u{00a0}\u{200b}\u{202e}\t\n";
        let inspection = inspect(source).unwrap();
        assert_eq!(source, "SECRET\u{00a0}\u{200b}\u{202e}\t\n");
        let escaped = inspection
            .rows()
            .iter()
            .map(ScalarObservation::escaped)
            .collect::<Vec<_>>();
        assert!(escaped.iter().all(|label| label.is_ascii()));
        assert!(escaped.contains(&"\\u{a0}".to_owned()));
        assert!(escaped.contains(&"\\u{200b}".to_owned()));
        assert!(escaped.contains(&"\\u{202e}".to_owned()));
        assert!(escaped.contains(&"\\t".to_owned()));
        assert!(escaped.contains(&"\\n".to_owned()));
        let debug = format!("{inspection:?} {:?}", inspection.rows()[0]);
        assert!(!debug.contains(source));
        assert!(!debug.contains('\u{202e}'));
        assert!(!debug.contains('\u{200b}'));
    }

    #[test]
    fn exact_byte_and_scalar_limits_are_accepted_and_overages_reject_whole_input() {
        let exact = "😀".repeat(MAX_SCALAR_ROWS);
        assert_eq!(exact.len(), MAX_INPUT_BYTES);
        let inspection = inspect(&exact).unwrap();
        assert_eq!(inspection.scalar_count(), MAX_SCALAR_ROWS);
        assert_eq!(inspection.rows().len(), MAX_SCALAR_ROWS);

        let too_many_bytes = "x".repeat(MAX_INPUT_BYTES + 1);
        assert_eq!(
            inspect(&too_many_bytes),
            Err(InspectionError::InputTooManyBytes {
                actual: MAX_INPUT_BYTES + 1,
                limit: MAX_INPUT_BYTES,
            })
        );

        let too_many_scalars = "x".repeat(MAX_SCALAR_ROWS + 1);
        assert_eq!(
            inspect(&too_many_scalars),
            Err(InspectionError::InputTooManyScalars {
                actual: MAX_SCALAR_ROWS + 1,
                limit: MAX_SCALAR_ROWS,
            })
        );
    }
}
