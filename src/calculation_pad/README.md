# Calculation Pad document grammar

The Calculation Pad engine evaluates a source document one physical line at a
time. It retains the original text, blank lines, comments, line order, and
line-ending bytes. Each evaluation starts from the current source; it does not
reuse results from an earlier evaluation.

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
  reserved for assignments. Braces allow names that match an operator or
  constant, such as `{sin}`.
* Duplicate assignments, unknown names, forward references, and references to
  a failed result are errors on the affected line. A later valid line still
  evaluates independently.

`rN` refers to the result on the one-based physical source line `N`; comments
and blank lines count. For example, `r2 * 4` uses the result from physical line
2. A reference must point to an earlier line. Self, forward, missing, and
cyclic references are diagnosed.

## Scalar expressions

Scalar expressions use the `exmex` expression grammar with finite `f64`
results. The engine supports `+`, `-`, `*`, `/`, `^`; unary `+` and `-`; the
functions `abs`, `signum`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`,
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
expression cannot use a physical quantity as a scalar, and the engine does not
perform unit algebra. Incompatible units produce the catalog's conversion
diagnostic.

## Exact integer bases

Unsigned integers can be converted exactly between binary, octal, decimal,
and hexadecimal, without using floating point:

```text
9007199254740993 dec to hex
r1 to bin
```

Values from zero through `u128::MAX` are supported. The output digits are
lowercase, with no base prefix or leading zeroes.

## Resource limits

The evaluator checks these bounds before invoking expression or unit parsers:

| Limit | Maximum |
| --- | ---: |
| Source document | 64 KiB |
| Physical source lines | 256 |
| Expression or conversion line | 2,048 UTF-8 bytes |
| Parenthesis nesting | 32 levels |
| Identifier occurrences per line | 64 |
| Identifier occurrences per document | 512 |
| Name | 128 UTF-8 bytes |

Lines that exceed a line-level bound receive a diagnostic; documents beyond
the byte or line bound receive a document diagnostic without partial
evaluation.
