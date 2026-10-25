# Clipboard Modify

Clipboard Modify transforms text through the `cm` launcher command or the
Clipboard Modify dialog. Commands and saved pipelines use the same operation
catalog. A command can run one operation or chain stages from left to right:

```text
cm remove-blank-lines | number-lines
cm join-lines " | "
cm split-lines "::" | reverse-lines
```

Quote a separator when it contains spaces or a pipe. Quoted arguments use the
existing command quoting rules; separators are treated as literal text by the
operation.

## Line operations

| Operation | Behavior |
| --- | --- |
| `remove-blank-lines` | Removes lines whose contents are empty after trimming whitespace. Other line contents are preserved. |
| `number-lines` | Prefixes logical lines with `1. `, `2. `, and so on. |
| `reverse-lines` | Reverses the order of logical lines. |
| `join-lines [separator]` | Joins logical lines with one space when omitted. A supplied separator is used literally; `""` explicitly joins with no separator. |
| `split-lines <separator>` | Splits on a required, nonempty literal separator and joins the resulting fields with line feeds. Empty fields are retained. |

Line operations use logical lines: CRLF and LF input are accepted, and their
output uses LF. A final source line ending is not retained by operations that
rebuild lines. `split-lines` also preserves empty fields, including a final
empty field when the input ends with its separator. Empty input remains empty.

Examples:

```text
cm number-lines
cm join-lines
cm join-lines ""
cm join-lines " — "
cm split-lines "a|b"
```

The first command numbers each logical line. The second joins with one space;
the third joins without a separator. In the last command, `a|b` is one literal
separator rather than a pipeline delimiter because it is quoted.

## Dialog and saved pipelines

In the dialog, choose an operation for each stage and edit only the arguments
that operation uses. Join Lines distinguishes the default one-space separator
from an explicitly supplied empty separator. Split Lines requires a nonempty
separator. In the Modify section, each draft stage retains its dialog identity
when you reorder or remove it. The saved-pipeline editor edits the pipeline's
ordered stage list; that list is persisted as part of the pipeline.

Preview evaluates the current source and stages without writing to the
clipboard. Apply writes the completed result through Clipboard Modify's guarded
clipboard path; Undo restores the prior text when the corresponding write is
available. Invalid stages fail preview and cannot be applied. Cancellation and
stale preview results do not produce clipboard writes.

Saved templates and pipelines remain in schema version 1. The new operation
arguments are optional in the serialized stage shape, so older stages load
with no separator. `cm join-lines` and the other operation commands continue
to name operations directly; `cm apply <name>` explicitly selects a saved
pipeline with that name. Existing saved entries using the new operation
spellings remain valid and are selected through their explicit route:
`cm template <name>` for a template or `cm apply <name>` for a pipeline.
