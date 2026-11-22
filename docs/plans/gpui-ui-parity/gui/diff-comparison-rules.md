# Diff Comparison Rules

G08 owns `diff-comparison-rules`. The Text workbench opens this retained form for one workspace and view. `src/gui/diff_dialog/child_hosts.rs` owns docked and independent native presentation, including accepted parent-popout inheritance. `rules_dialog.rs` builds the draft and uses the shared `CompiledRules` validator.

| Criterion | Source-backed behavior |
| --- | --- |
| DIFF-RULES-01 | Seed the accepted live rule set into a draft. Show leading, trailing and all-whitespace controls, blank-line and case settings, line-ending equivalence, multiline regex replacements and unimportant-section regexes. Keep the footer reachable when the body scrolls. |
| DIFF-RULES-02 | Apply compiles the complete candidate. Show malformed `PATTERN => REPLACEMENT` line details or invalid regex feedback in the form and retain the draft. Cancel and ordinary close discard the draft. |
| DIFF-RULES-03 | Bind the accepted Apply to the current workspace, view, source identities/revisions and rule basis. A stale source or basis leaves the live comparison unchanged. Docking and popping out keep the same form and draft. |

Use `rendered_rules_apply_requires_retained_child_pass` and the existing rule compilation tests. Native verification needs a Text comparison, invalid then valid Apply and a retained host move. GPUI implementation and native evidence are pending.
