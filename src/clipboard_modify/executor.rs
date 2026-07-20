use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine;

use super::catalog::{canonical_command, operation_by_id};
use super::error::ClipboardModifyError;
use super::model::{ClipboardModifierCatalog, OperationId, StageSpec};
use super::pipeline::{find_pipeline, find_template, validate_executable_stages};

pub trait Cancellation {
    fn is_cancelled(&self) -> bool;
}

impl Cancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}
impl Cancellation for &AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}
impl<F: Fn() -> bool> Cancellation for F {
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageExecutionError {
    pub stage_number: usize,
    pub operation: String,
    pub label: String,
    pub reason: String,
}

impl std::fmt::Display for StageExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Stage {} ({}) failed: {}",
            self.stage_number, self.operation, self.reason
        )
    }
}
impl std::error::Error for StageExecutionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecuteError {
    Validation(StageExecutionError),
    Stage(StageExecutionError),
    Cancelled,
}
impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(e) | Self::Stage(e) => e.fmt(f),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}
impl std::error::Error for ExecuteError {}

pub fn execute_stages<C: Cancellation + ?Sized>(
    source: &str,
    stages: &[StageSpec],
    catalog: &ClipboardModifierCatalog,
    cancellation: &C,
) -> Result<String, ExecuteError> {
    validate_all(stages, catalog)?;
    let mut current = source.to_string();
    for (i, stage) in stages.iter().enumerate() {
        check(cancellation)?;
        let next = transform(&current, stage, catalog, cancellation).map_err(|e| match e {
            ClipboardModifyError::Cancelled => ExecuteError::Cancelled,
            other => ExecuteError::Stage(wrap(i, stage, other.to_string())),
        })?;
        check(cancellation)?;
        current = next;
    }
    Ok(current)
}

pub fn execute_pipeline<C: Cancellation + ?Sized>(
    source: &str,
    pipeline_name: &str,
    catalog: &ClipboardModifierCatalog,
    cancellation: &C,
) -> Result<String, ExecuteError> {
    let pipeline = find_pipeline(catalog, pipeline_name).ok_or_else(|| {
        ExecuteError::Validation(StageExecutionError {
            stage_number: 1,
            operation: "pipeline".into(),
            label: pipeline_name.into(),
            reason: format!("unknown pipeline {pipeline_name}"),
        })
    })?;
    execute_stages(source, &pipeline.stages, catalog, cancellation)
}

fn validate_all(
    stages: &[StageSpec],
    catalog: &ClipboardModifierCatalog,
) -> Result<(), ExecuteError> {
    for (i, stage) in stages.iter().enumerate() {
        if let Err(e) = validate_executable_stages(std::slice::from_ref(stage), catalog) {
            return Err(ExecuteError::Validation(wrap(i, stage, e.to_string())));
        }
    }
    Ok(())
}
fn wrap(i: usize, stage: &StageSpec, reason: String) -> StageExecutionError {
    StageExecutionError {
        stage_number: i + 1,
        operation: canonical_command(stage.operation).into(),
        label: operation_by_id(stage.operation)
            .map(|op| op.label.to_string())
            .unwrap_or_else(|| canonical_command(stage.operation).into()),
        reason,
    }
}
fn check<C: Cancellation + ?Sized>(c: &C) -> Result<(), ExecuteError> {
    if c.is_cancelled() {
        Err(ExecuteError::Cancelled)
    } else {
        Ok(())
    }
}
fn cancelled<C: Cancellation + ?Sized>(c: &C) -> Result<(), ClipboardModifyError> {
    if c.is_cancelled() {
        Err(ClipboardModifyError::Cancelled)
    } else {
        Ok(())
    }
}

fn transform<C: Cancellation + ?Sized>(
    input: &str,
    stage: &StageSpec,
    catalog: &ClipboardModifierCatalog,
    c: &C,
) -> Result<String, ClipboardModifyError> {
    use OperationId::*;
    let a = &stage.arguments;
    match stage.operation {
        SingleQuote => Ok(format!("'{}'", input)),
        DoubleQuote => Ok(format!("\"{}\"", input)),
        Backticks => Ok(format!("`{}`", input)),
        CustomWrap => Ok(format!(
            "{}{}{}",
            a.prefix.as_deref().unwrap_or(""),
            input,
            a.suffix.as_deref().unwrap_or("")
        )),
        NamedWrap | Template => Ok(find_template(catalog, a.name.as_deref().unwrap_or(""))
            .unwrap()
            .render(input)),
        CodeBlock => Ok(format!(
            "```{}\n{}\n```",
            a.language.as_deref().unwrap_or(""),
            input
        )),
        SortAscending => line_op(input, c, |mut v| {
            v.sort();
            v.join("\n")
        }),
        SortDescending => line_op(input, c, |mut v| {
            v.sort_by(|a, b| b.cmp(a));
            v.join("\n")
        }),
        UniqueLines => line_op(input, c, |v| {
            let mut seen = BTreeSet::new();
            v.into_iter()
                .filter(|l| seen.insert(l.to_string()))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        Trim => Ok(input.trim().to_string()),
        TrimLines => line_op(input, c, |v| {
            v.into_iter().map(str::trim).collect::<Vec<_>>().join("\n")
        }),
        JsonPretty => {
            let v: serde_json::Value = serde_json::from_str(input)
                .map_err(|e| ClipboardModifyError::Transform(e.to_string()))?;
            cancelled(c)?;
            let s = serde_json::to_string_pretty(&v)
                .map_err(|e| ClipboardModifyError::Transform(e.to_string()))?;
            cancelled(c)?;
            Ok(s)
        }
        JsonMinify => {
            let v: serde_json::Value = serde_json::from_str(input)
                .map_err(|e| ClipboardModifyError::Transform(e.to_string()))?;
            cancelled(c)?;
            let s = serde_json::to_string(&v)
                .map_err(|e| ClipboardModifyError::Transform(e.to_string()))?;
            cancelled(c)?;
            Ok(s)
        }
        Base64Encode => {
            let mut out = String::new();
            for chunk in input.as_bytes().chunks(48 * 1024) {
                cancelled(c)?;
                out.push_str(&base64::engine::general_purpose::STANDARD.encode(chunk));
            }
            Ok(out)
        }
        Base64Decode => {
            cancelled(c)?;
            let b = base64::engine::general_purpose::STANDARD
                .decode(input)
                .map_err(|e| ClipboardModifyError::Transform(e.to_string()))?;
            cancelled(c)?;
            String::from_utf8(b).map_err(|e| ClipboardModifyError::Transform(e.to_string()))
        }
        UrlEncode => Ok(urlencoding::encode(input).into_owned()),
        UrlDecode => urlencoding::decode(input)
            .map(|c| c.into_owned())
            .map_err(|e| ClipboardModifyError::Transform(e.to_string())),
        Lowercase => Ok(input.to_lowercase()),
        Uppercase => Ok(input.to_uppercase()),
        TitleCase => Ok(words(input).map(cap).collect::<Vec<_>>().join(" ")),
        CamelCase => {
            let mut it = words(input);
            let first = it.next().unwrap_or_default().to_lowercase();
            Ok(first + &it.map(cap).collect::<String>())
        }
        PascalCase => Ok(words(input).map(cap).collect::<String>()),
        SnakeCase => Ok(words(input)
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join("_")),
        ScreamingSnake => Ok(words(input)
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join("_")),
        KebabCase => Ok(words(input)
            .map(|w| w.to_lowercase())
            .collect::<Vec<_>>()
            .join("-")),
    }
}
fn line_op<C: Cancellation + ?Sized, F: FnOnce(Vec<&str>) -> String>(
    input: &str,
    c: &C,
    f: F,
) -> Result<String, ClipboardModifyError> {
    let mut v = Vec::new();
    for (i, l) in input.lines().enumerate() {
        if i % 1024 == 0 {
            cancelled(c)?;
        }
        v.push(l);
    }
    cancelled(c)?;
    Ok(f(v))
}
fn words(input: &str) -> impl Iterator<Item = &str> {
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
}
fn cap(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::model::{ClipboardTemplate, StageArguments, TemplateProcessor};
    use std::cell::Cell;

    fn st(operation: OperationId) -> StageSpec {
        StageSpec {
            operation,
            arguments: StageArguments::default(),
        }
    }
    fn named(operation: OperationId, name: &str) -> StageSpec {
        StageSpec {
            operation,
            arguments: StageArguments {
                name: Some(name.into()),
                ..Default::default()
            },
        }
    }
    fn catalog() -> ClipboardModifierCatalog {
        ClipboardModifierCatalog {
            templates: vec![ClipboardTemplate {
                id: "wrap".into(),
                label: "Wrap".into(),
                aliases: vec!["w".into()],
                template: "<{{clipboard}}>".into(),
                processor: Some(TemplateProcessor::Literal),
            }],
            pipelines: vec![crate::clipboard_modify::model::SavedPipeline {
                id: "pipe".into(),
                label: "Pipe".into(),
                aliases: vec![],
                stages: vec![st(OperationId::Trim)],
            }],
        }
    }

    #[test]
    fn correct_stage_ordering() {
        assert_eq!(
            execute_stages(
                " b\na ",
                &[st(OperationId::TrimLines), st(OperationId::SortAscending)],
                &catalog(),
                &|| false
            )
            .unwrap(),
            "a\nb"
        );
    }

    #[test]
    fn template_after_transformation() {
        assert_eq!(
            execute_stages(
                "hi",
                &[
                    st(OperationId::Uppercase),
                    named(OperationId::Template, "wrap")
                ],
                &catalog(),
                &|| false
            )
            .unwrap(),
            "<HI>"
        );
    }

    #[test]
    fn code_block_after_json_pretty_formatting() {
        let out = execute_stages(
            "{\"b\":1}",
            &[
                st(OperationId::JsonPretty),
                StageSpec {
                    operation: OperationId::CodeBlock,
                    arguments: StageArguments {
                        language: Some("json".into()),
                        ..Default::default()
                    },
                },
            ],
            &catalog(),
            &|| false,
        )
        .unwrap();
        assert_eq!(out, "```json\n{\n  \"b\": 1\n}\n```");
    }

    #[test]
    fn custom_wrapper_pipeline_stages() {
        let s = StageSpec {
            operation: OperationId::CustomWrap,
            arguments: StageArguments {
                prefix: Some("[".into()),
                suffix: Some("]".into()),
                ..Default::default()
            },
        };
        let cat = ClipboardModifierCatalog {
            templates: catalog().templates,
            pipelines: vec![crate::clipboard_modify::model::SavedPipeline {
                id: "brackets".into(),
                label: "Brackets".into(),
                aliases: vec![],
                stages: vec![s],
            }],
        };
        assert_eq!(
            execute_pipeline("x", "brackets", &cat, &|| false).unwrap(),
            "[x]"
        );
    }

    #[test]
    fn atomic_failure_with_no_partial_result() {
        let err = execute_stages(
            "abc",
            &[st(OperationId::Uppercase), st(OperationId::Base64Decode)],
            &catalog(),
            &|| false,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .starts_with("Stage 2 (base64-decode) failed:")
        );
    }

    #[test]
    fn no_op_success() {
        assert_eq!(
            execute_stages("same", &[], &catalog(), &|| false).unwrap(),
            "same"
        );
    }

    #[test]
    fn cancellation_before_first_stage() {
        let flag = AtomicBool::new(true);
        assert_eq!(
            execute_stages("x", &[st(OperationId::Uppercase)], &catalog(), &flag).unwrap_err(),
            ExecuteError::Cancelled
        );
    }

    #[test]
    fn cancellation_between_stages() {
        let n = Cell::new(0);
        let err = execute_stages(
            "x",
            &[st(OperationId::Uppercase), st(OperationId::Lowercase)],
            &catalog(),
            &|| {
                let v = n.get();
                n.set(v + 1);
                v >= 1
            },
        )
        .unwrap_err();
        assert_eq!(err, ExecuteError::Cancelled);
    }

    #[test]
    fn cancellation_during_large_line_operations() {
        let input = (0..5000)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let n = Cell::new(0);
        let err = execute_stages(&input, &[st(OperationId::TrimLines)], &catalog(), &|| {
            let v = n.get();
            n.set(v + 1);
            v >= 3
        })
        .unwrap_err();
        assert_eq!(err, ExecuteError::Cancelled);
    }

    #[test]
    fn stale_catalog_snapshots_continue_with_captured_catalog() {
        let old = catalog();
        let mut new = old.clone();
        new.templates[0].template = "[{{clipboard}}]".into();
        assert_eq!(
            execute_stages("x", &[named(OperationId::Template, "wrap")], &old, &|| {
                false
            })
            .unwrap(),
            "<x>"
        );
        assert_eq!(
            execute_stages("x", &[named(OperationId::Template, "wrap")], &new, &|| {
                false
            })
            .unwrap(),
            "[x]"
        );
    }

    #[test]
    fn invalid_saved_stages() {
        let s = StageSpec {
            operation: OperationId::CustomWrap,
            arguments: StageArguments::default(),
        };
        let err = execute_stages("x", &[s], &catalog(), &|| false).unwrap_err();
        assert!(matches!(err, ExecuteError::Validation(_)));
    }

    #[test]
    fn nested_pipeline_rejection() {
        let err = execute_stages(
            "x",
            &[named(OperationId::Template, "pipe")],
            &catalog(),
            &|| false,
        )
        .unwrap_err();
        assert!(matches!(err, ExecuteError::Validation(_)));
        assert!(err.to_string().contains("nested saved pipeline"));
    }
}

#[cfg(test)]
mod comprehensive_transform_regressions {
    use super::*;
    use crate::clipboard_modify::model::{StageArguments, StageSpec};

    fn st(operation: OperationId) -> StageSpec {
        StageSpec {
            operation,
            arguments: StageArguments::default(),
        }
    }
    fn arg(operation: OperationId, arguments: StageArguments) -> StageSpec {
        StageSpec {
            operation,
            arguments,
        }
    }
    fn run(input: &str, stage: StageSpec) -> Result<String, ExecuteError> {
        execute_stages(
            input,
            &[stage],
            &crate::clipboard_modify::defaults::default_catalog(),
            &|| false,
        )
    }

    #[test]
    fn empty_unicode_and_newline_edges() {
        assert_eq!(run("", st(OperationId::Uppercase)).unwrap(), "");
        assert_eq!(
            run("ß café", st(OperationId::Uppercase)).unwrap(),
            "SS CAFÉ"
        );
        assert_eq!(
            run(" a\r\nb \n c\r", st(OperationId::TrimLines)).unwrap(),
            "a\nb\nc"
        );
        assert_eq!(
            run("b\na\n", st(OperationId::SortAscending)).unwrap(),
            "a\nb"
        );
        assert_eq!(run(" \n\t\n", st(OperationId::TrimLines)).unwrap(), "\n");
    }

    #[test]
    fn wrappers_preserve_embedded_delimiters_and_backticks() {
        assert_eq!(
            run("a'b\"c", st(OperationId::SingleQuote)).unwrap(),
            "'a'b\"c'"
        );
        assert_eq!(
            run("a`b``c", st(OperationId::Backticks)).unwrap(),
            "`a`b``c`"
        );
        assert_eq!(
            run(
                "x",
                arg(
                    OperationId::CustomWrap,
                    StageArguments {
                        prefix: Some("<<".into()),
                        suffix: Some(">>".into()),
                        ..Default::default()
                    }
                )
            )
            .unwrap(),
            "<<x>>"
        );
    }

    #[test]
    fn json_url_base64_errors_and_escapes() {
        assert_eq!(
            run("a\n\"b", st(OperationId::JsonMinify))
                .unwrap_err()
                .to_string()
                .contains("Stage 1"),
            true
        );
        assert_eq!(
            run("{\"s\":\"a\\nb\"}", st(OperationId::JsonPretty)).unwrap(),
            "{\n  \"s\": \"a\\nb\"\n}"
        );
        assert!(run("%zz", st(OperationId::UrlDecode)).is_err());
        assert!(run("not base64!", st(OperationId::Base64Decode)).is_err());
        assert!(run("//8=", st(OperationId::Base64Decode)).is_err());
        assert_eq!(run("✓", st(OperationId::Base64Encode)).unwrap(), "4pyT");
    }

    #[test]
    fn stable_equivalent_key_ordering_for_sort_and_unique() {
        assert_eq!(
            run("b\na\na", st(OperationId::SortAscending)).unwrap(),
            "a\na\nb"
        );
        assert_eq!(
            run("b\na\nb\na", st(OperationId::UniqueLines)).unwrap(),
            "b\na"
        );
        assert_eq!(
            run("item2\nitem10\nitem1", st(OperationId::SortAscending)).unwrap(),
            "item1\nitem10\nitem2"
        );
    }
}
