# Password Generator

G16 owns `password-generator`. Exact `password` emits payload-free `password:generator`. `UtilityWindowHost` owns the retained surface. The canonical generator owns policy validation, result identity and entropy. The explicit random generate-and-copy wire command is a separate route.

| Criterion | Source-backed behavior |
| --- | --- |
| PASSWORD-01 | Password length, character classes and ambiguous exclusion, and passphrase count, separator, capitalization and optional digit/symbol preserve the shared generator and entropy rules. Keep attribution and actionable validation feedback. |
| PASSWORD-02 | Accepted policy changes auto-save canonical settings and retire previous results. Generate/Regenerate creates an ephemeral result. Reveal keeps secret text out of accessibility and text selection. Rehost conceals while retaining the current session. True close retires secret and reveal state. |
| PASSWORD-03 | Explicit Copy is accepted only with current owner, result ID, policy revision and session ticket. Use protected secret publication and preserve private marker, disabled history/roaming and retryable uncertainty or failure. Keep generated secret out of logs and evidence. |

Select `password_generator_discovery_is_an_exact_payload_free_open`, `password_wire_is_strict_and_literal_clipboard_is_preserved`, `password_dialog_revision_and_copy_retry_are_bound`, `password_dialog_reveal_omits_secret_from_accessibility_tree` and `password_generator_keyboard_activation_persists_preferences_and_registers_native_controls`. Scratch generation with concealed result can exercise policy and lifecycle. Native and GPUI evidence are pending.
