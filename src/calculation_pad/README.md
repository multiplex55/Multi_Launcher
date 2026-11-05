# Calculation Pad document grammar

The Calculation Pad engine evaluates a source document one physical line at a
time. It retains the original text, blank lines, comments, line order, and
line-ending bytes. Direct document evaluation always reads the current source.
The UI session caches its evaluation for the current source revision and
recomputes after an edit, undo, or redo changes that revision.

## Lines and names

* An empty or whitespace-only line is blank.
* A line whose first non-whitespace character is `#` is a comment.
* A result line may be an expression or an assignment: `total = 2 + 3`.
* Bare names use ASCII letters, Greek letters, digits after the first
  character, and underscores. Names are case-sensitive.
* Braced names such as `{顧客名}` may contain other Unicode characters and
  spaces. Braces and line breaks are not allowed inside a name. Names are
  limited to 128 UTF-8 bytes.
* Bare operator/function/constant names and `rN` row-reference names are
  reserved for assignments. `mod` is a contextual exception: `mod(` starts
  the modulo function, while bare `mod` remains an assignable/referencable
  name. Braces allow names that match an operator or constant, such as
  `{sin}`.
* Duplicate assignments, unknown names, forward references, and references to
  a failed result are errors on the affected line. A later valid line still
  evaluates independently.

`rN` refers to the result on the one-based physical source line `N`; comments
and blank lines count. For example, `r2 * 4` uses the result from physical line
2. A reference must point to an earlier line. Self, forward, missing, and
cyclic references are diagnosed.

## Scalar expressions

Scalar expressions use the `exmex` expression grammar with finite `f64`
results. Scientific notation is supported, for example `1.25e-2` and
`6.02E23`. Scalar integer literals may use `0x`, `0b`, or `0o` prefixes;
their values are deliberately bounded to 2^53, the exactly representable
integer range endpoint (larger even integers are also rejected). The engine
supports `+`, `-`, `*`, `/`, `^`, and `%`; unary `+` and `-`; the functions
`abs`, `signum`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
`sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `floor`, `round`, `ceil`,
`trunc`, `fract`, `exp`, `sqrt`, `cbrt`, `ln`, `log`, `log2`, `log10`,
`atan2`, `min`, and `max`; and the constants `PI`, `π`, `E`, `e`, `TAU`, and
`τ`. A named value can be used in later expressions:

```text
radius = 3
PI * radius ^ 2
```

Non-finite operands and intermediate results remain invalid even when a later
operation could otherwise hide them, as in `min(sqrt(-1), 1)`.

`a % b` returns the remainder with the dividend's sign (`-7 % 3` is `-1`).
`mod(a, b)` returns the non-negative Euclidean remainder (`mod(-7, 3)` is
`2`). Both forms reject a zero divisor. When `%` or contextual `mod(a, b)` is
used, `*`, `/`, and `%` group left to right; expressions without these forms
keep the original operator precedence.

## Exact integer expressions

Programmer expressions use exact fixed-width integers instead of `f64`. They
are selected by a programmer operator, an integer constructor/helper, or a
reference to an earlier exact integer result. Untyped programmer expressions
use `u64`; bare numeric literals adopt the mode selected by a typed operand.
Available modes are signed and unsigned 8-, 16-, 32-, 64-, and 128-bit
integers. `int(value)` is a checked signed 64-bit constructor. Width-specific
constructors such as `u8(value)` and `i128(value)` select their named mode.
Mixed widths or signedness require an explicit constructor and checked cast.

Decimal and `0x`, `0o`, and `0b` integer literals are parsed exactly from the
source token. Integer `+`, `-`, `*`, `^`, `idiv`, `rem`, and `%` use checked
arithmetic; `^` remains exponentiation. `/` remains the scalar division
operator. Integer bit operations are `&`, `|`, `~`, `and`, `or`, `xor`, and
`not`; shifts use `<<` and `>>`. Left shifts discard bits above the selected
width, signed right shifts extend the sign bit, and shift counts must be
between zero and width minus one. `rotl(value, count)` and
`rotr(value, count)` rotate modulo the operand width. `popcount(value)` returns
the number of set bits in the operand's mode. `mask(value, start, count)` makes
a same-mode field mask, while `extract(value, start, count)` returns the
selected bits as an unsigned value of the same width. Bit positions count from
the least-significant bit; empty fields are allowed when their start is at or
before the width. Prefix `not value` is supported; use `not(value)` or `~value`
when a signed operand follows.

Programmer precedence from tightest to loosest is: calls and grouping; `^`
(right-associative and included in prefix operands); unary `+`, `-`, `~`, and
prefix `not`; multiplicative `*`, `idiv`, `rem`, and `%`; additive `+` and `-`;
shifts; `&` and `and`; `xor`; then `|` and `or`. All binary operators except
`^` associate left. `/` is unavailable in typed programmer expressions;
ordinary scalar `/` keeps its existing behavior.

For example:

```text
0xFF << 8
byte = u8(255)
byte << 1
not u8(0)
i8(-1) >> 1
extract(u8(0xb5), 2, 4)
```

Exact integer names and `rN` references stay typed in later programmer
expressions. Scalar results are not implicitly converted. An explicit
constructor can convert a scalar reference only when its value is finite,
integral, within the exactly representable scalar integer range, and fits the
requested mode. Floating-point and scientific-notation literal tokens are
rejected in integer constructors to prevent decimal rounding. Physical
quantities cannot be converted to integers. Integer overflow, division by zero,
invalid shifts, and mode mismatches are line-local diagnostics. This syntax
does not change scalar expressions: for example, `2 ^ 3` remains scalar unless
a programmer operator or typed reference selects integer evaluation.

## Physical quantities

Direct unit conversions use the shared catalog and conversion parser:

```text
6 ft 2 in to cm
1 MB to Mb
1 month to day
```

An earlier physical result can be converted again with a row reference:

```text
1 km to m
r1 to cm
```

The value keeps its physical unit and any approximation metadata. A scalar
expression cannot use a physical quantity as a scalar.

Linear quantities also support bounded dimensional arithmetic using units from
that same catalog:

```text
10 m + 20 cm
(2 m) * (3 m)
(5 m) / (2 s)
area = (2 m) * (3 m)
area to cm^2
```

Addition and subtraction require matching dimensions. A quantity may be scaled
by a scalar; multiplication and division combine dimensions; `^` accepts a
small integer exponent. Compatible targets can be written with `to`, including
compound units such as `km/h`; in a slash unit, a following power applies to
the denominator (`m/s^2` means `m/s²`). Existing catalog categories such as energy,
torque, angle, and data remain distinct when their dimensions might otherwise
look alike. Linear catalog compounds such as `MB/s` may be used in expressions;
nonlinear temperature and fuel-economy units remain conversion-only. Month and
year approximation metadata is retained when units cancel or are raised to a
power.

Unit aliases are contextual: a name such as `m` can still be assigned and used
as a variable in scalar expressions. Once a reference resolves to a physical
quantity, arithmetic uses its dimensional value instead of converting it to a
scalar.

## Exact integer bases

Unsigned integers can be converted exactly between binary, octal, decimal,
and hexadecimal, without using floating point:

```text
9007199254740993 dec to hex
r1 to bin
```

Values from zero through `u128::MAX` are supported. The output digits are
lowercase, with no base prefix or leading zeroes. This conversion syntax is
separate from scalar prefixes: for example, `0bad hex to dec` means the exact
hexadecimal value `0x0bad`, while `0x0bad` by itself is a scalar literal.

## Remembered source

**Remember last Calculation Pad source** is off by default in Settings. When
enabled, an explicit open restores the most recently accepted source as a
dirty draft. It does not open the pad at application startup. The bounded
recovery record is separate from named documents; disabling the preference
clears only that record. A recovered expression may still contain ordinary
evaluation errors, and remains editable.

## Resource limits

The evaluator checks these bounds before invoking expression or unit parsers:

| Limit | Maximum |
| --- | ---: |
| Source document | 64 KiB |
| Physical source lines | 256 |
| Expression or conversion line | 2,048 UTF-8 bytes |
| Normalized scalar expression | 8 KiB |
| Parenthesis nesting | 32 levels |
| Programmer expression tokens | 256 tokens |
| Identifier occurrences per line | 64 |
| Identifier occurrences per document | 512 |
| Name | 128 UTF-8 bytes |

Lines that exceed a line-level bound receive a diagnostic; documents beyond
the byte or line bound receive a document diagnostic without partial
evaluation.
