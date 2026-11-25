use arboard::Clipboard;

pub fn copy_history_result(index: usize) -> anyhow::Result<()> {
    if let Some(entry) =
        crate::plugins::calc_history::load_history(crate::plugins::calc_history::CALC_HISTORY_FILE)
            .unwrap_or_default()
            .get(index)
            .cloned()
    {
        let mut cb = Clipboard::new()?;
        cb.set_text(entry.result)?;
    }
    Ok(())
}

/// Execute the published scalar result using the legacy best-effort history-before-copy order.
pub fn copy_result(result: &str, expression: Option<&str>) -> anyhow::Result<()> {
    copy_result_with(
        result,
        expression,
        |expr, value| {
            crate::plugins::calc_history::append_entry(
                crate::plugins::calc_history::CALC_HISTORY_FILE,
                crate::plugins::calc_history::CalcHistoryEntry {
                    expr: expr.to_owned(),
                    result: value.to_owned(),
                },
                crate::plugins::calc_history::MAX_ENTRIES,
            )
        },
        crate::actions::clipboard::calc_to_clipboard,
    )
}

fn copy_result_with(
    result: &str,
    expression: Option<&str>,
    append: impl FnOnce(&str, &str) -> anyhow::Result<()>,
    copy: impl FnOnce(&str) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if let Some(expression) = expression {
        let _ = append(expression, result);
    }
    copy(result)
}

#[cfg(test)]
mod scalar_tests {
    use super::copy_result_with;
    use std::cell::RefCell;

    #[test]
    fn scalar_copy_preserves_legacy_history_order_and_best_effort_failure() {
        let calls = RefCell::new(Vec::new());
        copy_result_with(
            "42",
            Some(" 6 * 7 "),
            |expr, result| {
                calls.borrow_mut().push(format!("history:{expr}:{result}"));
                anyhow::bail!("history failed")
            },
            |result| {
                calls.borrow_mut().push(format!("copy:{result}"));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["history: 6 * 7 :42", "copy:42"]);
        calls.borrow_mut().clear();
        assert!(
            copy_result_with(
                "42",
                Some("6*7"),
                |expr, result| {
                    calls.borrow_mut().push(format!("history:{expr}:{result}"));
                    Ok(())
                },
                |result| {
                    calls.borrow_mut().push(format!("copy:{result}"));
                    anyhow::bail!("copy failed")
                }
            )
            .is_err()
        );
        assert_eq!(*calls.borrow(), ["history:6*7:42", "copy:42"]);
        calls.borrow_mut().clear();
        assert!(
            copy_result_with(
                "42",
                None,
                |_, _| panic!("unexpected history"),
                |_| anyhow::bail!("copy failed")
            )
            .is_err()
        );
        assert!(calls.borrow().is_empty());
    }
}
