# Clipboard Modify

Clipboard Modify is launched with the `cm` prefix. It reads the current text clipboard, transforms it, and writes the transformed result back to the clipboard for execution actions. The dialog Help section is generated from the operation registry plus the current template and saved-pipeline catalogs.

## Baseline operation catalog

| Operation | Aliases | Syntax examples | Pipeline |
|---|---|---|---|
| `single-quote` | `sq`, `quote-single`, `quotes` | `cm single-quote` | yes |
| `double-quote` | `dq` | `cm double-quote` | yes |
| `backticks` | `tick` | `cm backticks` | yes |
| `custom-wrap` | `wrap-custom` | `cm custom-wrap "<" ">"`, `cm wrap "<!-- " " -->"` | yes |
| `named-wrap` | `wrap` | `cm named-wrap markdown-quote`, `cm wrap quotes` | yes |
| `template` | `tpl` | `cm template prompt-context`, pipeline stage `template prompt-context` | yes as a stage |
| `code-block` | `fence` | `cm code-block rust` | yes |
| `sort-ascending` | `sort`, `sort-asc` | `cm sort-ascending` | yes |
| `sort-descending` | `sort-desc` | `cm sort-descending` | yes |
| `unique-lines` | `uniq` | `cm unique-lines` | yes |
| `trim` | — | `cm trim` | yes |
| `trim-lines` | `strip-lines` | `cm trim-lines` | yes |
| `json-pretty` | `pretty-json` | `cm json-pretty` | yes |
| `json-minify` | `compact-json`, `json-compact` | `cm json-minify` | yes |
| `base64-encode` | `b64enc` | `cm base64-encode` | yes |
| `base64-decode` | `b64dec` | `cm base64-decode` | yes |
| `url-encode` | — | `cm url-encode` | yes |
| `url-decode` | — | `cm url-decode` | yes |
| `lowercase` | `lower` | `cm lowercase` | yes |
| `uppercase` | `upper` | `cm uppercase` | yes |
| `title-case` | `title` | `cm title-case` | yes |
| `camel-case` | `camel` | `cm camel-case` | yes |
| `pascal-case` | `pascal` | `cm pascal-case` | yes |
| `snake-case` | `snake` | `cm snake-case` | yes |
| `screaming-snake` | `constant-case`, `screaming-snake-case` | `cm screaming-snake` | yes |
| `kebab-case` | `kebab` | `cm kebab-case` | yes |

## Commands and pipeline syntax

- `cm` opens the Modify section.
- `cm template` opens Templates; `cm template <name-or-alias>` applies that template immediately.
- `cm apply` opens Saved Pipelines; `cm apply <name-or-alias>` runs that saved pipeline immediately.
- `cm undo` restores the clipboard text captured before the last Clipboard Modify write.
- Pipeline stages are separated with `|`, for example `cm trim-lines | unique-lines | sort-ascending`.
- Custom wrapper values can be quoted with single or double quotes when they contain spaces, pipes, or quote characters: `cm wrap "<!-- " " -->"`.

## Template file schema

The configuration file is `clipboard_modifiers.json` next to the main settings file. Version 1 uses strict JSON validation with unknown fields rejected:

```json
{
  "schema_version": 1,
  "templates": [
    {
      "id": "prompt-context",
      "label": "Prompt context",
      "aliases": ["context"],
      "template": "Context:\n{{clipboard}}",
      "processor": "literal"
    }
  ],
  "pipelines": [
    {
      "id": "clean-lines",
      "label": "Clean lines",
      "aliases": ["tidy-lines"],
      "stages": [
        { "operation": "trim-lines" },
        { "operation": "unique-lines" }
      ]
    }
  ]
}
```

## Structured stages

Saved pipeline stages contain an `operation` plus optional `arguments`:

- `custom-wrap`: `arguments.prefix` and `arguments.suffix` are required.
- `named-wrap` and `template`: `arguments.name` is required.
- `code-block`: `arguments.language` is optional.
- No-argument operations reject extra arguments.

## Placeholder rules

Templates must include `{{clipboard}}`. The default `literal` processor inserts clipboard text as-is. The `rust-raw-string` processor replaces the placeholder with a safe Rust raw string literal.

## Undo semantics and external clipboard races

Before Clipboard Modify writes, it records the clipboard text it read and the transformed text it wrote. `cm undo` restores the recorded original text for the most recent Clipboard Modify write. If another application changes the clipboard between read and write, Clipboard Modify detects the race on the commit path and avoids blindly overwriting newer external content.

## Large-input behavior

The dialog requires confirmation for sources larger than 5 MiB before previewing transformations. This prevents expensive previews from surprising users. Execution paths still operate on clipboard text but can surface errors for invalid input such as malformed JSON or Base64.

## Privacy and clipboard history

Clipboard Modify operates on local clipboard text. It does not send clipboard contents to a network service. Because transformed output is written back to the system clipboard, operating-system clipboard history and third-party clipboard managers may record both original and transformed content according to their own settings.

## Schema versions, validation, and recovery

Only supported `schema_version` values are loaded. Future versions are rejected to avoid unsafe downgrades. The file has a 5 MiB size limit and strict validation for unknown fields, duplicate ids/aliases, reserved names, missing placeholders, invalid aliases, invalid stages, and missing template references.

To recover from a bad configuration:

1. Open the Clipboard Modify management UI and fix the reported validation issue, or edit `clipboard_modifiers.json` manually.
2. Reload configuration; invalid reloads retain the last valid in-memory catalog.
3. If startup cannot load the file, Multi Launcher falls back to built-in defaults in memory.
4. To reset completely, move or delete `clipboard_modifiers.json` and restart; default templates and pipelines will be recreated when possible.
