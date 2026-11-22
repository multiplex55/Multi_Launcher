# Hot Folder Target Picker

G21 owns `hot-folder-target-picker`, the embedded manager pane for Registered launcher action or MkMacro. It reads the retained catalog snapshot. The editor owns the selected target draft.

| Criterion | Source-backed behavior |
| --- | --- |
| HOT-FOLDER-TARGET-01 | Show typed choices and disabled reasons. Select only the exact persistent launcher target or available macro identity and fingerprints, under the current retained editor. Choosing a row changes only the draft. |
| HOT-FOLDER-TARGET-02 | Close and Escape return to the draft and retire picker snapshots. Keep an unbound external action invalid for Save. Enablement and dispatch recheck current capabilities and fingerprints. |

Select `capability_resolves_only_exact_unattended_registered_process_actions`, `capability_marks_filesystem_risk_and_macro_staleness_without_dispatch` and `macro_transitive_change_invalidates_fingerprint_and_recomputes_strong_enablement`. `G21-EGUI-HOT-FOLDER-AUTHORING` verifies native Registered launcher action browsing and Escape without a selected target. Other native choices and GPUI remain pending.
