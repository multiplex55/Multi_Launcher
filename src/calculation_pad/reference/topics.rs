use super::super::examples::{CalculationExample, EXAMPLE_GROUPS, ExampleExpectation};
use super::{ReferenceCategory, ReferenceEntry, ReferenceId};
use ExampleExpectation::{Boolean, ExactInteger, Scalar, ScalarWithSkipped, Vector};

macro_rules! topic {
    ($id:literal, $category:ident, $syntax:literal, $summary:literal, $params:literal,
        $returns:literal, $limits:literal, $example:expr, [$($alias:literal),*],
        [$($concept:literal),*], [$($related:expr),+]) => {
        ReferenceEntry {
            id: Topic($id),
            category: ReferenceCategory::$category,
            spelling: Some($syntax),
            aliases: &[$($alias),*],
            concepts: &[$($concept),*],
            summary: $summary,
            parameters: $params,
            returns: $returns,
            limits: $limits,
            example: $example,
            variants: &[],
            related: &[$($related),+],
            unit_category: None,
        }
    };
}

use ReferenceId::{Call, Topic};

const fn same_title(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn grouped(title: &str) -> CalculationExample {
    let mut group = 0;
    while group < EXAMPLE_GROUPS.len() {
        let examples = EXAMPLE_GROUPS[group].examples;
        let mut index = 0;
        while index < examples.len() {
            if same_title(examples[index].title, title) {
                return examples[index];
            }
            index += 1;
        }
        group += 1;
    }
    panic!("reference points to an unknown grouped example")
}

const fn sample(
    title: &'static str,
    source: &'static str,
    expected: ExampleExpectation,
) -> CalculationExample {
    CalculationExample::new(title, source, title, expected)
}

pub(super) const TOPICS: &[ReferenceEntry] = &[
    topic!(
        "Scalar literals",
        Mathematics,
        "decimal, scientific notation, 0b…, 0o…, 0x…",
        "Write finite ordinary numbers in decimal, scientific notation, or as base-prefixed integers.",
        "Whole digits may use decimal or binary, octal or hexadecimal prefixes. Decimal-point and scientific notation use floating point.",
        "Ordinary dimensionless scalar.",
        "Unadorned integer tokens are bounded to 2^53. Nonfinite results report an error.",
        grouped("Mix scalar integer bases"),
        ["0b", "0o", "0x"],
        ["floating point", "ordinary scalar", "literal"],
        [
            Topic("Exact radix conversion"),
            Topic("Scalar arithmetic"),
            Topic("Resource limits")
        ]
    ),
    topic!(
        "Scalar constants",
        Mathematics,
        "PI, π, pi, pie, E, e, TAU, τ",
        "Use π, Euler's number and one full turn in radians.",
        "No arguments. Unbound pi and pie match case-insensitively. An explicit preceding name or braced reference uses normal scope.",
        "Ordinary scalar.",
        "A named binding can take precedence over a contextual alias.",
        grouped("Built-in constants"),
        ["PI", "π", "pi", "pie", "E", "e", "TAU", "τ"],
        ["constant", "Euler", "circle"],
        [Call("sin"), Call("ln"), Topic("Scalar literals")]
    ),
    topic!(
        "Scalar arithmetic",
        Mathematics,
        "+x, -x, a + b, a - b, a * b, a / b, a ^ b, (…)",
        "Add, subtract, multiply, divide, raise to a power, or change a number's sign.",
        "Finite ordinary numbers. A divisor must differ from zero. Parentheses group subexpressions.",
        "Ordinary scalar. Approximation provenance is propagated where supported.",
        "A nonfinite operand or result reports an error. Use parentheses to specify evaluation order. Exact, typed and dimensional operands have their own arithmetic rules.",
        sample(
            "Group scalar arithmetic",
            "((+2 + 3) * 4 - -2) / 2 + 2 ^ 3",
            Scalar(19.0)
        ),
        ["+", "-", "*", "/", "^", "(", ")"],
        ["arithmetic operators", "grouping", "power"],
        [
            Topic("Signed remainder"),
            Topic("Exact arithmetic"),
            Topic("Vector arithmetic")
        ]
    ),
    topic!(
        "Signed remainder",
        Mathematics,
        "a % b, a mod b",
        "Return the remainder with the dividend's sign.",
        "Finite ordinary numbers a and b, with b nonzero.",
        "Ordinary scalar.",
        "The scalar remainder grammar groups *, / and % left to right. Call-form mod(a, b) computes Euclidean remainder. Typed % is checked integer remainder.",
        sample(
            "Keep the dividend's sign",
            "(-7 mod 3) + (-7 % 3)",
            Scalar(-2.0)
        ),
        ["%", "infix mod"],
        ["signed remainder", "remainder operator"],
        [Call("mod"), Topic("Checked integer arithmetic")]
    ),
    topic!(
        "Scalar infix functions",
        Mathematics,
        "a min b, a max b, y atan2 x",
        "Use minimum, maximum and two-argument arctangent between ordinary numbers.",
        "Two finite ordinary scalar operands.",
        "Ordinary scalar.",
        "Use grouping to disambiguate expressions. A terminal min after a number can denote minutes.",
        sample(
            "Use minimum and angle infix",
            "(7 min 3) + (0 atan2 1)",
            Scalar(3.0)
        ),
        ["infix min", "infix max", "infix atan2"],
        ["minimum operator", "maximum operator"],
        [Call("min"), Call("max"), Call("atan2")]
    ),
    topic!(
        "Variables and assignments",
        ControlFlow,
        "name = expression",
        "Bind a name to the result of an expression. Later references use the latest preceding binding.",
        "Case-sensitive bare names or braced names, and a value expression.",
        "Assigned value, available on its source row and through the binding.",
        "Names are at most 128 UTF-8 bytes. Reserved scalar function names and physical references require braces when used as names. Forward and failed references report errors. Failed evaluated collection reassignments retain the preceding valid binding.",
        grouped("Reassignment and earlier values"),
        ["="],
        ["variables", "binding", "reassign"],
        [
            Topic("Names and physical rows"),
            Topic("Compound assignments"),
            Topic("Vector values")
        ]
    ),
    topic!(
        "Names and physical rows",
        ControlFlow,
        "name, {braced name}, rN",
        "Read a previous result by its case-sensitive name or one-based physical source row.",
        "Bare names use ASCII/Greek letters, underscores, and digits after the first character. Braces allow spaces and Unicode. Comments and blank lines count in rN.",
        "Referenced value in its original type.",
        "Names are limited to 128 UTF-8 bytes. Forward, self, missing and failed row references report an error. Braces cannot contain another brace or a line break.",
        grouped("Reference a physical source line"),
        ["rN", "{name}"],
        ["source row", "line reference", "Unicode name"],
        [
            Topic("Variables and assignments"),
            Topic("Comments and multiline source")
        ]
    ),
    topic!(
        "Compound assignments",
        ControlFlow,
        "+=, -=, *=, /=, %=, ^=",
        "Update a previously bound name using the corresponding arithmetic operation.",
        "Existing binding on the left and one full expression on the right.",
        "Updated value in the arithmetic result's type.",
        "The original type, precision and units rules apply. Division assignment is unavailable for typed integers. An unbound name reports an error.",
        grouped("Subtract, divide, remainder, and power assignments"),
        ["+=", "-=", "*=", "/=", "%=", "^="],
        ["update variable", "compound assignment"],
        [
            Topic("Variables and assignments"),
            Topic("Scalar arithmetic")
        ]
    ),
    topic!(
        "Comments and multiline source",
        ControlFlow,
        "# comment, // comment, grouped multiline expressions",
        "Place one statement on a source row, or continue an expression inside grouping. Lines starting with a comment marker or whitespace only have no value.",
        "Group continued expressions with matching delimiters. Use spaces for block indentation.",
        "An expression produces its ordinary result; comments and blank lines produce no result.",
        "Comments and blank rows still count toward physical row references and limits. Indentation alone does not continue ordinary statements.",
        grouped("Multiline formula in a conditional loop"),
        ["#", "//"],
        ["blank line", "line continuation", "multiline expression"],
        [
            Topic("Names and physical rows"),
            Topic("Local formulas"),
            Topic("Resource limits")
        ]
    ),
    topic!(
        "Boolean values",
        ControlFlow,
        "true, false",
        "Use Boolean literals or the results of comparisons and logical operations.",
        "No arguments. Literal spellings are lowercase.",
        "Boolean.",
        "Conditions require a Boolean. A nonzero number does not act as true.",
        sample("Use a Boolean literal", "true", Boolean(true)),
        ["true", "false"],
        ["Boolean", "truth value"],
        [Topic("Comparisons"), Topic("Logical operators"), Call("if")]
    ),
    topic!(
        "Comparisons",
        ControlFlow,
        "<, <=, >, >=, ==, !=",
        "Compare compatible values and get a Boolean.",
        "Two values that share a comparison domain, such as ordinary numbers, compatible quantities or compatible calendar values.",
        "Boolean.",
        "Incompatible operands report an error. Comparisons have no numeric truth value.",
        grouped("Comparison operators"),
        ["<", "<=", ">", ">=", "==", "!="],
        ["equal to", "less than", "greater than"],
        [Topic("Boolean values"), Topic("Logical operators")]
    ),
    topic!(
        "Logical operators",
        ControlFlow,
        "a && b, a || b, !a",
        "Combine or reverse Boolean values. AND and OR evaluate their right side only when needed.",
        "Boolean operands.",
        "Boolean.",
        "An evaluated non-Boolean operand reports an error. Word operators and/or/not remain programmer bitwise syntax.",
        sample(
            "Short-circuit a false AND",
            "false && (1 / 0 > 1)",
            Boolean(false)
        ),
        ["&&", "||", "!"],
        ["logical and", "logical or", "logical not", "short circuit"],
        [
            Topic("Boolean values"),
            Topic("Bitwise operators"),
            Call("if")
        ]
    ),
    topic!(
        "Conditional blocks",
        ControlFlow,
        "if condition: … elif condition: … else: …",
        "Execute the first branch with a true Boolean condition, or the optional else branch.",
        "An if header with a Boolean condition, optional elif headers, optional else, and indented bodies.",
        "The executed statements publish their results. Skipped rows are reported as not executed.",
        "Use consistent space indentation and include an actual body. Nesting and evaluation work limits apply.",
        sample(
            "Choose the first matching branch",
            "score = 85\nif score >= 80:\n  score += 10\nelif score > 0:\n  score = 1 / 0\nelse:\n  score = 0\nscore",
            ScalarWithSkipped(95.0)
        ),
        ["elif", "else", "if block"],
        ["conditional statement", "branch"],
        [Call("if"), Topic("Boolean values"), Topic("Iteration")]
    ),
    topic!(
        "Iteration",
        ControlFlow,
        "for item in vector_or_range: …",
        "Execute an indented loop body for each element in order.",
        "Iterator name and a numeric vector or range sequence. Use spaces for body indentation.",
        "Successful assignments and the iterated name retain their last executed values.",
        "Empty iterables skip the body. At most eight nested loops and 10,000 iterations per evaluation. Failure rolls back and poisons affected loop bindings.",
        grouped("Iterate over a vector literal"),
        ["for", "in", "for in"],
        ["loop", "iteration"],
        [
            Call("range"),
            Topic("Loop controls"),
            Topic("Resource limits")
        ]
    ),
    topic!(
        "Loop controls",
        ControlFlow,
        "break, continue",
        "Break exits the innermost loop. Continue skips the rest of the current iteration.",
        "A standalone statement inside a loop body, optionally inside a nested conditional block.",
        "A control row and subsequent results from the path that executes.",
        "Outside a loop both are rejected. An unexecuted statement has no effect.",
        sample(
            "Break on the first match",
            "first_match = 0\nfor n in range(1, 15):\n  if n % 7 == 0:\n    first_match = n\n    break\nfirst_match",
            Scalar(7.0)
        ),
        ["break", "continue"],
        ["exit loop", "skip iteration"],
        [Topic("Iteration"), Topic("Conditional blocks")]
    ),
    topic!(
        "Local formulas",
        ControlFlow,
        "fn name(parameters) = expression",
        "Declare a named expression and call it later with explicit arguments.",
        "Case-sensitive name, up to 16 local parameters, and one expression using only those parameters, pure built-ins and earlier formulas.",
        "The call returns the body expression's type. The definition row has no numeric result.",
        "No document-value capture, recursion, forward calls or duplicate parameter names. At most 64 definitions and 16 call levels.",
        grouped("Calculate a discounted price"),
        ["fn"],
        ["user function", "formula arguments", "no capture"],
        [
            Call("map"),
            Topic("Comments and multiline source"),
            Topic("Resource limits")
        ]
    ),
    topic!(
        "Vector values",
        Vectors,
        "[component, …], []",
        "Collect finite ordinary dimensionless scalars in one immutable ordered value.",
        "Between zero and 1,024 ordinary scalar expressions. Components may refer to prior names and rows.",
        "Vector.",
        "Nested vectors, Booleans, exact, approximate and typed integer values, and quantities cannot be components.",
        sample(
            "Build a vector from an earlier scalar",
            "seed = 3\n[1, seed, sqrt(16)]",
            Vector(&[1.0, 3.0, 4.0])
        ),
        ["[]", "vector literal"],
        ["array", "list", "collection"],
        [
            Topic("Vector arithmetic"),
            Call("len"),
            Call("append"),
            Topic("Chart series")
        ]
    ),
    topic!(
        "Vector arithmetic",
        Vectors,
        "v + w, v - w, v * w, v / w, a * v, v * a, v / a, v · w, v × w",
        "Combine vector components element by element, scale a vector, or compute dot/cross products.",
        "Equal-length vectors for element-wise operations and dot. Cross uses two three-component vectors. Scalar scaling needs an ordinary scalar.",
        "Vector, except dot product gives an ordinary scalar.",
        "Scalar addition/subtraction do not broadcast. Zero divisors, mismatched lengths and nonfinite intermediate results report errors.",
        grouped("Element-wise operations"),
        ["·", "×", "vector +", "vector -", "vector *", "vector /"],
        [
            "element-wise",
            "scale vector",
            "dot operator",
            "cross operator"
        ],
        [Call("dot"), Call("cross"), Topic("Vector values")]
    ),
    topic!(
        "Exact arithmetic",
        ProgrammerExact,
        "decimal(…), fraction(…), +, -, *, /, comparisons",
        "Retain exact decimal and rational results across supported arithmetic and comparisons.",
        "Exact values, compatible typed integers and finite ordinary integral scalars within ±2^53. Construct nonintegral operands explicitly.",
        "Exact decimal or reduced rational. Comparisons return Boolean.",
        "Decimal division needs a terminating representable result. Numerator, denominator and coefficient are limited to 4,096 bits, and decimal scale to 1,000.",
        grouped("Add exact decimals"),
        ["exact decimal", "rational arithmetic"],
        ["exact number", "arbitrary precision"],
        [
            Call("decimal"),
            Call("fraction"),
            Call("decimal_div"),
            Call("to_float")
        ]
    ),
    topic!(
        "Exact radix conversion",
        ProgrammerExact,
        "digits source_base to target_base, reference to target_base",
        "Convert an unsigned integer between binary, octal, decimal and hexadecimal without floating-point rounding.",
        "Digits in the source base (or an earlier exact base result), and one target base. Base aliases are case-insensitive.",
        "Exact unsigned base value. Digits are lowercase, without a prefix or left padding.",
        "Range is 0 through 2^128 - 1. A digit invalid in the source base or an out-of-range number reports an error.",
        grouped("Convert an exact unsigned base value"),
        [
            "bin",
            "binary",
            "oct",
            "octal",
            "dec",
            "decimal",
            "hex",
            "hexadecimal"
        ],
        ["radix", "base conversion"],
        [Topic("Scalar literals"), Call("u128")]
    ),
    topic!(
        "Checked integer arithmetic",
        ProgrammerExact,
        "+x, -x, a + b, a - b, a * b, a ^ b, a idiv b, a rem b, a % b",
        "Calculate checked fixed-width integer results in the mode selected by typed operands.",
        "Same explicitly typed mode or untyped source integer literals. The programmer path defaults to u64 when no mode is set.",
        "Typed integer in the selected mode.",
        "Overflow, zero divisors, invalid powers or mixed modes report errors. Use idiv for a truncating quotient. The scalar division operator / is unavailable here.",
        grouped("Checked integer quotient"),
        ["idiv", "rem", "integer %"],
        ["integer arithmetic", "overflow", "division", "quotient"],
        [
            Call("int"),
            Topic("Bitwise operators"),
            Topic("Integer shifts")
        ]
    ),
    topic!(
        "Bitwise operators",
        ProgrammerExact,
        "a & b, a | b, a and b, a or b, a xor b, ~a, not a",
        "Combine bit patterns or complement a pattern at its fixed width.",
        "Compatible integer operands. Prefix not, ~ and the not() call take one integer.",
        "Typed integer in its mode.",
        "Mixed signedness or widths require explicit checked conversion. Use not(value) or ~value before an explicitly signed operand.",
        sample(
            "Mask, complement and combine a byte",
            "(u8(7) & u8(3)) xor (~u8(254) or u8(0))",
            ExactInteger {
                mode: "u8",
                value: "2"
            }
        ),
        ["&", "|", "~", "and", "or", "xor", "prefix not"],
        ["bits", "bitwise and", "bitwise or", "exclusive or"],
        [Call("not"), Call("mask"), Topic("Integer shifts")]
    ),
    topic!(
        "Integer shifts",
        ProgrammerExact,
        "value << count, value >> count",
        "Shift within the left operand's width. A signed right shift repeats the sign bit.",
        "An integer value and integral shift count from 0 through width - 1.",
        "Same-mode typed integer.",
        "Left shifts discard bits above the width. Negative counts and counts at or above width report errors.",
        grouped("Width-limited left shift"),
        ["<<", ">>"],
        ["shift left", "shift right", "arithmetic shift"],
        [Call("rotl"), Call("rotr"), Topic("Bitwise operators")]
    ),
    topic!(
        "Calendar value kinds",
        CalendarTime,
        "date, time, offset timestamp, named-zone timestamp, weekday",
        "Work with explicit calendar dates, local times, instants and named weekday results.",
        "Construct values from quoted literals; compare and combine compatible kinds with the corresponding calendar APIs.",
        "Calendar value, Boolean comparison or physical elapsed duration, depending on operation.",
        "Years are bounded to 0000–9999. Dates have no time of day. Wall-clock times need a timestamp to represent a change of day.",
        grouped("Keep an explicit offset and fractional seconds"),
        ["calendar date", "time of day", "timestamp", "weekday"],
        ["calendar types", "UTC offset"],
        [
            Call("date"),
            Call("time"),
            Call("datetime"),
            Call("zoned_datetime"),
            Call("weekday")
        ]
    ),
    topic!(
        "Resource limits",
        ControlFlow,
        "document size, syntax, evaluation work",
        "Keep evaluations bounded. Standard evaluates 64 KiB / 256 rows and Extended 128 KiB / 1,024 rows.",
        "Ordinary expression: 2,048 bytes; vector literal: up to 32 KiB; nesting: 32; identifiers: 64 per line and 512 per document; name: 128 bytes.",
        "Over-limit source or expressions report diagnostics. An oversized document is preserved for editing.",
        "Each evaluation has 50,000 ordinary work and 4,194,304 vector work units. Vectors and range sequences have 1,024 elements. Programmer/quantity expressions have 256 tokens. Absolute source admission is 256 KiB / 2,048 rows.",
        grouped("Range size limit"),
        ["work limit", "document limits"],
        ["budgets", "maximum", "large documents"],
        [
            Call("range"),
            Topic("Iteration"),
            Topic("Local formulas"),
            Topic("Vector values")
        ]
    ),
    topic!(
        "Chart series",
        Charts,
        "Charts → Line, Scatter or Bar",
        "Plot current vectors or range sequences from the active Pad, or switch the source to a completed Sensitivity Table.",
        "Ordinary numeric vectors or sequences of matching lengths for X and Y. The default X is a zero-based index; Sensitivity uses the sampled input as X.",
        "An interactive chart. The example computes the vector named y for use in Charts.",
        "Up to 1,024 points per series. Charts report stale or unavailable results without triggering evaluation. Use explicit positive X for log X.",
        sample(
            "Build matching X and Y data",
            "x = [1, 2, 3]\ny = [2, 4, 8]\ny",
            Vector(&[2.0, 4.0, 8.0])
        ),
        ["chart", "line chart", "scatter plot", "bar chart"],
        ["plot", "vector series", "XY"],
        [
            Topic("Histogram"),
            Topic("Cumulative chart"),
            Topic("Chart axes"),
            Topic("Sensitivity chart"),
            Topic("Vector values")
        ]
    ),
    topic!(
        "Histogram",
        Charts,
        "Charts → Histogram",
        "Group one current vector into equal-width bins, displaying counts or percentages. Auto uses the ceiling of the square root of the observation count for nonconstant data.",
        "Select one numeric vector and Auto, a bin count from 1 through 1,024, or a finite positive fixed width.",
        "Histogram and an accessible summary of each bin. The example returns the sample vector.",
        "Only the final bin includes its right endpoint. Empty input shows No data and constant input uses one exact-value bin. Widths requiring more than 1,024 bins report a limit. The zero-based value axis requires a linear scale.",
        sample(
            "Supply observations for a histogram",
            "observations = [1, 1, 2, 3, 4]\nobservations",
            Vector(&[1.0, 1.0, 2.0, 3.0, 4.0])
        ),
        ["histogram", "bins"],
        ["distribution", "frequency", "count", "percentage"],
        [
            Topic("Cumulative chart"),
            Topic("Chart axes"),
            Call("percentile")
        ]
    ),
    topic!(
        "Cumulative chart",
        Charts,
        "Charts → Cumulative",
        "Plot the fraction of observations at or below each distinct threshold.",
        "Select one ordinary numeric vector. An empty vector shows No data.",
        "Cumulative step chart and a threshold summary with counts and percentages. The example returns the sample vector.",
        "Equal values share a threshold. The final step reaches a fraction of 1, corresponding to all observations. Unavailable or stale values are reported.",
        sample(
            "Supply observations with ties",
            "observations = [3, 1, 1, 2]\nobservations",
            Vector(&[3.0, 1.0, 1.0, 2.0])
        ),
        ["cdf", "cumulative"],
        ["empirical distribution", "step plot"],
        [Topic("Histogram"), Call("quantile"), Topic("Chart axes")]
    ),
    topic!(
        "Chart axes",
        Charts,
        "Charts → Chart Options",
        "Set automatic or fixed limits, linear or base-10 log scales, ticks, grid, references and series appearance.",
        "Fixed limits must be finite and ordered. A reference line takes a literal coordinate or an available named scalar from the current evaluation.",
        "A staged chart definition update after Apply. The example returns a reference scalar and defines positive X and Y vectors.",
        "Log coordinates and limits must be positive. Bar, Histogram and Cumulative use zero-based value axes and require a linear Y scale. Reset Axes resets definition axes; Reset View resets the viewport.",
        sample(
            "Prepare positive data and a named reference",
            "x = [1, 10, 100]\ny = [5, 25, 125]\nthreshold = 25\nthreshold",
            Scalar(25.0)
        ),
        ["log axes", "reference lines", "fixed bounds"],
        ["chart options", "log scale", "gridlines", "legend"],
        [
            Topic("Chart series"),
            Topic("Histogram"),
            Topic("Cumulative chart")
        ]
    ),
    topic!(
        "Sensitivity chart",
        Charts,
        "Sensitivity Table → Generate; Charts → Sensitivity",
        "Plot selected outputs from a completed current Sensitivity Table against its sampled input.",
        "Select a literal input such as rate and an output such as cost. Choose a range and a count or step, generate the table, then select Sensitivity in Charts.",
        "A chart whose X coordinate is the sampled input. The example defines a literal input and an output to sweep.",
        "At most 1,000 sensitivity samples within the aggregate work limit. A stale table must be generated again. Histogram and Cumulative require a separate ordinary vector result.",
        sample(
            "Define an input and output for a sweep",
            "rate = 2\ncost = rate * 10\ncost",
            Scalar(20.0)
        ),
        ["sensitivity plot", "sweep"],
        ["what-if", "input sample", "plot outputs"],
        [
            Topic("Chart series"),
            Topic("Chart axes"),
            Topic("Resource limits")
        ]
    ),
];
