use once_cell::sync::Lazy;
use std::collections::{BTreeSet, HashMap};

use super::error::ClipboardModifyError;
use super::model::{OperationId, StageArguments};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationCategory {
    Wrap,
    Lines,
    Format,
    Encoding,
    Case,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgumentRequirements {
    None,
    CustomWrap,
    NamedWrap,
    CodeBlock,
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapperInfo {
    pub command: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub argument_requirements: ArgumentRequirements,
    pub pipeline_available: bool,
    pub help_examples: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlCommandInfo {
    pub syntax: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub argument_requirements: &'static str,
    pub examples: &'static [&'static str],
    pub pipeline_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationInfo {
    pub id: OperationId,
    pub command: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub category: OperationCategory,
    pub aliases: &'static [&'static str],
    pub argument_requirements: ArgumentRequirements,
    pub pipeline_available: bool,
    pub synchronous_small_input: bool,
    pub help_examples: &'static [&'static str],
}

impl ArgumentRequirements {
    pub fn validate(
        self,
        id: OperationId,
        args: &StageArguments,
    ) -> Result<(), ClipboardModifyError> {
        let op = canonical_command(id);
        match self {
            Self::None => {
                for (name, supplied) in [
                    ("prefix", args.prefix.is_some()),
                    ("suffix", args.suffix.is_some()),
                    ("name", args.name.is_some()),
                    ("language", args.language.is_some()),
                ] {
                    if supplied {
                        return Err(ClipboardModifyError::UnexpectedArgument {
                            operation: op.into(),
                            argument: name,
                        });
                    }
                }
            }
            Self::CustomWrap => {
                if args.prefix.is_none() {
                    return Err(ClipboardModifyError::MissingArgument {
                        operation: op.into(),
                        argument: "prefix",
                    });
                }
                if args.suffix.is_none() {
                    return Err(ClipboardModifyError::MissingArgument {
                        operation: op.into(),
                        argument: "suffix",
                    });
                }
                reject(args.name.is_some(), op, "name")?;
                reject(args.language.is_some(), op, "language")?;
            }
            Self::NamedWrap | Self::Template => {
                if args.name.is_none() {
                    return Err(ClipboardModifyError::MissingArgument {
                        operation: op.into(),
                        argument: "name",
                    });
                }
                reject(args.prefix.is_some(), op, "prefix")?;
                reject(args.suffix.is_some(), op, "suffix")?;
                reject(args.language.is_some(), op, "language")?;
            }
            Self::CodeBlock => {
                reject(args.prefix.is_some(), op, "prefix")?;
                reject(args.suffix.is_some(), op, "suffix")?;
                reject(args.name.is_some(), op, "name")?;
            }
        }
        Ok(())
    }
}
fn reject(v: bool, op: &str, a: &'static str) -> Result<(), ClipboardModifyError> {
    if v {
        Err(ClipboardModifyError::UnexpectedArgument {
            operation: op.into(),
            argument: a,
        })
    } else {
        Ok(())
    }
}

pub static OPERATIONS: Lazy<Vec<OperationInfo>> = Lazy::new(|| {
    vec![
        op(
            OperationId::SingleQuote,
            "single-quote",
            "Single quote",
            "Wrap in single quotes",
            OperationCategory::Wrap,
            &["sq", "quote-single", "quotes"],
            ArgumentRequirements::None,
            &["single-quote"],
        ),
        op(
            OperationId::DoubleQuote,
            "double-quote",
            "Double quote",
            "Wrap in double quotes",
            OperationCategory::Wrap,
            &["dq"],
            ArgumentRequirements::None,
            &["double-quote"],
        ),
        op(
            OperationId::Backticks,
            "backticks",
            "Backticks",
            "Wrap in backticks",
            OperationCategory::Wrap,
            &["tick"],
            ArgumentRequirements::None,
            &["backticks"],
        ),
        op(
            OperationId::CustomWrap,
            "custom-wrap",
            "Custom wrap",
            "Wrap with a supplied prefix and suffix",
            OperationCategory::Wrap,
            &["wrap-custom"],
            ArgumentRequirements::CustomWrap,
            &["custom-wrap --prefix '<' --suffix '>'"],
        ),
        op(
            OperationId::NamedWrap,
            "named-wrap",
            "Named wrap",
            "Apply a named wrapper",
            OperationCategory::Wrap,
            &["wrap"],
            ArgumentRequirements::NamedWrap,
            &["named-wrap markdown-quote"],
        ),
        op(
            OperationId::Template,
            "template",
            "Template",
            "Apply a saved template",
            OperationCategory::Wrap,
            &["tpl"],
            ArgumentRequirements::Template,
            &["template prompt-context"],
        ),
        op(
            OperationId::CodeBlock,
            "code-block",
            "Code block",
            "Wrap in a Markdown code block",
            OperationCategory::Wrap,
            &["fence"],
            ArgumentRequirements::CodeBlock,
            &["code-block rust"],
        ),
        op(
            OperationId::SortAscending,
            "sort-ascending",
            "Sort ascending",
            "Sort lines ascending",
            OperationCategory::Lines,
            &["sort", "sort-asc"],
            ArgumentRequirements::None,
            &["sort-ascending"],
        ),
        op(
            OperationId::SortDescending,
            "sort-descending",
            "Sort descending",
            "Sort lines descending",
            OperationCategory::Lines,
            &["sort-desc"],
            ArgumentRequirements::None,
            &["sort-descending"],
        ),
        op(
            OperationId::UniqueLines,
            "unique-lines",
            "Unique lines",
            "Remove duplicate lines",
            OperationCategory::Lines,
            &["uniq"],
            ArgumentRequirements::None,
            &["unique-lines"],
        ),
        op(
            OperationId::Trim,
            "trim",
            "Trim",
            "Trim surrounding whitespace",
            OperationCategory::Lines,
            &[],
            ArgumentRequirements::None,
            &["trim"],
        ),
        op(
            OperationId::TrimLines,
            "trim-lines",
            "Trim lines",
            "Trim each line",
            OperationCategory::Lines,
            &["strip-lines"],
            ArgumentRequirements::None,
            &["trim-lines"],
        ),
        op(
            OperationId::JsonPretty,
            "json-pretty",
            "JSON pretty",
            "Pretty-print JSON",
            OperationCategory::Format,
            &["pretty-json"],
            ArgumentRequirements::None,
            &["json-pretty"],
        ),
        op(
            OperationId::JsonMinify,
            "json-minify",
            "JSON minify",
            "Minify JSON",
            OperationCategory::Format,
            &["compact-json", "json-compact"],
            ArgumentRequirements::None,
            &["json-minify"],
        ),
        op(
            OperationId::Base64Encode,
            "base64-encode",
            "Base64 encode",
            "Encode as Base64",
            OperationCategory::Encoding,
            &["b64enc"],
            ArgumentRequirements::None,
            &["base64-encode"],
        ),
        op(
            OperationId::Base64Decode,
            "base64-decode",
            "Base64 decode",
            "Decode Base64",
            OperationCategory::Encoding,
            &["b64dec"],
            ArgumentRequirements::None,
            &["base64-decode"],
        ),
        op(
            OperationId::UrlEncode,
            "url-encode",
            "URL encode",
            "Percent-encode text",
            OperationCategory::Encoding,
            &[],
            ArgumentRequirements::None,
            &["url-encode"],
        ),
        op(
            OperationId::UrlDecode,
            "url-decode",
            "URL decode",
            "Decode percent-encoded text",
            OperationCategory::Encoding,
            &[],
            ArgumentRequirements::None,
            &["url-decode"],
        ),
        op(
            OperationId::Lowercase,
            "lowercase",
            "Lowercase",
            "Convert to lowercase",
            OperationCategory::Case,
            &["lower"],
            ArgumentRequirements::None,
            &["lowercase"],
        ),
        op(
            OperationId::Uppercase,
            "uppercase",
            "Uppercase",
            "Convert to uppercase",
            OperationCategory::Case,
            &["upper"],
            ArgumentRequirements::None,
            &["uppercase"],
        ),
        op(
            OperationId::TitleCase,
            "title-case",
            "Title case",
            "Convert to title case",
            OperationCategory::Case,
            &["title"],
            ArgumentRequirements::None,
            &["title-case"],
        ),
        op(
            OperationId::CamelCase,
            "camel-case",
            "Camel case",
            "Convert to camelCase",
            OperationCategory::Case,
            &["camel"],
            ArgumentRequirements::None,
            &["camel-case"],
        ),
        op(
            OperationId::PascalCase,
            "pascal-case",
            "Pascal case",
            "Convert to PascalCase",
            OperationCategory::Case,
            &["pascal"],
            ArgumentRequirements::None,
            &["pascal-case"],
        ),
        op(
            OperationId::SnakeCase,
            "snake-case",
            "Snake case",
            "Convert to snake_case",
            OperationCategory::Case,
            &["snake"],
            ArgumentRequirements::None,
            &["snake-case"],
        ),
        op(
            OperationId::ScreamingSnake,
            "screaming-snake",
            "Screaming snake",
            "Convert to SCREAMING_SNAKE_CASE",
            OperationCategory::Case,
            &["constant-case", "screaming-snake-case"],
            ArgumentRequirements::None,
            &["screaming-snake"],
        ),
        op(
            OperationId::KebabCase,
            "kebab-case",
            "Kebab case",
            "Convert to kebab-case",
            OperationCategory::Case,
            &["kebab"],
            ArgumentRequirements::None,
            &["kebab-case"],
        ),
    ]
});
fn op(
    id: OperationId,
    command: &'static str,
    label: &'static str,
    description: &'static str,
    category: OperationCategory,
    aliases: &'static [&'static str],
    argument_requirements: ArgumentRequirements,
    help_examples: &'static [&'static str],
) -> OperationInfo {
    OperationInfo {
        id,
        command,
        label,
        description,
        category,
        aliases,
        argument_requirements,
        pipeline_available: true,
        synchronous_small_input: true,
        help_examples,
    }
}

pub fn operations() -> &'static [OperationInfo] {
    &OPERATIONS
}
pub fn operation_by_id(id: OperationId) -> Option<&'static OperationInfo> {
    operations().iter().find(|o| o.id == id)
}
pub fn canonical_command(id: OperationId) -> &'static str {
    operation_by_id(id).map(|o| o.command).unwrap_or("unknown")
}
pub fn operation_lookup(name: &str) -> Option<&'static OperationInfo> {
    LOOKUP
        .get(&normalize_name(name))
        .and_then(|i| operations().get(*i))
}
static LOOKUP: Lazy<HashMap<String, usize>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (i, o) in operations().iter().enumerate() {
        m.insert(normalize_name(o.command), i);
        for a in o.aliases {
            m.insert(normalize_name(a), i);
        }
    }
    m
});

pub fn normalize_name(input: &str) -> String {
    let mut out = String::new();
    let mut hyphen = false;
    for c in input.trim().chars().flat_map(char::to_lowercase) {
        if c == '_' || c.is_whitespace() || c == '-' {
            if !hyphen && !out.is_empty() {
                out.push('-');
                hyphen = true;
            }
        } else {
            out.push(c);
            hyphen = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn reserved_names() -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for w in [
        "clipboard",
        "cb",
        "modify",
        "template",
        "apply",
        "undo",
        "wrap",
    ] {
        s.insert(normalize_name(w));
    }
    for o in operations() {
        s.insert(normalize_name(o.command));
        for a in o.aliases {
            s.insert(normalize_name(a));
        }
    }
    s
}

pub static WRAPPERS: Lazy<Vec<WrapperInfo>> = Lazy::new(|| {
    vec![
        WrapperInfo {
            command: "wrap",
            description: "Custom wrapper shorthand",
            aliases: &["custom-wrap"],
            argument_requirements: ArgumentRequirements::CustomWrap,
            pipeline_available: true,
            help_examples: &["wrap '<' '>'", "wrap \"<!-- \" \" -->\""],
        },
        WrapperInfo {
            command: "wrap markdown-quote",
            description: "Apply the markdown-quote named wrapper",
            aliases: &["wrap md-quote"],
            argument_requirements: ArgumentRequirements::NamedWrap,
            pipeline_available: true,
            help_examples: &["wrap markdown-quote", "wrap md-quote"],
        },
    ]
});

pub static CONTROL_COMMANDS: Lazy<Vec<ControlCommandInfo>> = Lazy::new(|| {
    vec![
        ControlCommandInfo {
            syntax: "cm",
            description: "Open the Modify dialog section",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm modify",
            description: "Open the Modify dialog section",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm modify"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm template",
            description: "Open the Templates dialog section",
            aliases: &[],
            argument_requirements: "optional template name",
            examples: &["cm template", "cm template prompt-context"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm apply",
            description: "Open the Saved Pipelines dialog section",
            aliases: &[],
            argument_requirements: "optional saved pipeline name",
            examples: &["cm apply", "cm apply clean-lines"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm manage-templates",
            description: "Open the Manage Templates dialog section",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm manage-templates"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm manage-pipelines",
            description: "Open the Manage Pipelines dialog section",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm manage-pipelines"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm help",
            description: "Open Clipboard Modify syntax help",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm help"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm undo",
            description: "Restore the clipboard text captured before the last Clipboard Modify write",
            aliases: &[],
            argument_requirements: "none",
            examples: &["cm undo"],
            pipeline_available: false,
        },
        ControlCommandInfo {
            syntax: "cm <stage> | <stage>",
            description: "Run pipeline-capable stages left-to-right",
            aliases: &["pipe"],
            argument_requirements: "one or more pipeline-capable stages",
            examples: &["cm trim-lines | unique-lines | sort-ascending"],
            pipeline_available: false,
        },
    ]
});

pub fn wrappers() -> &'static [WrapperInfo] {
    &WRAPPERS
}
pub fn control_commands() -> &'static [ControlCommandInfo] {
    &CONTROL_COMMANDS
}
pub fn wrapper_lookup(name: &str) -> Option<&'static WrapperInfo> {
    let n = normalize_name(name);
    wrappers().iter().find(|w| {
        normalize_name(w.command) == n || w.aliases.iter().any(|a| normalize_name(a) == n)
    })
}
