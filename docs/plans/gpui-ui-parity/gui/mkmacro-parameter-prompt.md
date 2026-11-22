# Macro Parameters

G19 owns `mkmacro-parameter-prompt`, utility `mkmacro-parameters`. An enabled compiled invocation with unresolved required inputs queues the exact pre-run prompt. `mkmacro::invocation_prompt` owns the lease, request and runtime identity. The prompt owns transient fields and scoped native viewport. Executor-time prompts have a distinct owner.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-PARAM-01 | Show current String/Number/Boolean/Point, defaults, rules, choices and File/Folder text and Browse. Only actual Run proceeds toward existing runtime admission. Cancel, Escape and OS close retire only the matching request. |
| MK-PARAM-02 | Browse result checks exact active request, parameter identity, type and chooser kind both sides. Failure/cancel preserves editable value. Keep root focus/eligibility and hidden-root child bootstrap scoped to current owner. |
| MK-PARAM-03 | Claim each Run once. Recheck exact enabled plan, source intent/scope, current compatible signature, validated values and live runtime generation. Failed admission leaves the prompt editable. Clear values on accepted retirement. |

Select `parameter_prompt_ui_browse_and_choice_wait_for_explicit_run`, `parameter_prompt_ui_cancelled_or_failed_browse_keeps_editable_request`, `parameter_prompt_ui_late_path_result_cannot_mutate_replaced_request`, `token_covers_queue_gui_ownership_drop_and_registration_replacement`, `confirmation_revalidates_signature_subset_enablement_and_runtime_lifetime` and `hidden_parameter_notice_bootstraps_child_without_root_activation`. Existing fake backend exercises an awaiting-input fixture without playback. Native and GPUI remain pending.
