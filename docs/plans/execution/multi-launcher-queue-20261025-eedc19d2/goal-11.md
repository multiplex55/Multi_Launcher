# Goal 11 — Jump configuration execution record

Queue: multi-launcher-queue-20261025-eedc19d2.
Branch: codex/jump-configuration-queue-20261027.
BASE11_OVERRIDE=CARRY: ceff1f17fb09545d8b9a6010aae9eca7892a7ea7.
Own M0-A baseline: 11fc9591305f3844f4736a31b3a075beb738b40c, empty/same source tree as CARRY, pushed.

Goal09 is deferred incomplete; Goal10 is skipped for its missing prerequisite. This isolated branch includes no unaccepted source from deferred Goals04–09. CARRY does not advance until every mandatory Goal11 criterion succeeds.

## M1-A — accepted

One validated chord/alphabet boundary lives in gui/jump_mode/config.rs, publicly exposed through the existing settings facade so external Settings struct-update construction remains compatible. Persisted keys jump_mode_activation_chord and jump_mode_hint_alphabet default to Shift+Space and ABCDEFGHIJKLMNOPQRSTUVWXYZ; experimental_jump_mode_enabled remains authoritative/off by default. Per-field loading recovery warns and defaults only malformed preferences, retaining valid siblings and unrelated settings. Serialization canonicalizes modifier names and uppercase letter order.

The supported local main keys are ASCII letters/digits, Space, Tab, Delete, Home/End, PageUp/PageDown, arrows and F1–F24 with independently representable Ctrl/Shift/Alt. Escape/Enter/Backspace remain reserved; unmodified letters/digits/Space, multiple main keys, malformed components, AltGr/ambiguous Ctrl+Alt and unrepresentable modifier distinctions are rejected. No global registration changes.

The existing allocator now uses alphabet ranks for shallowest/rightmost leaf expansion and final ordering. Default assignment delegates to this same implementation. Ordered alphabets contain 2–26 unique ASCII letters; target identity/sort order, prefix freedom, frozen maps and case-insensitive matching remain intact. SettingsEditor carries the preferences through an unrelated edited-setting save. Runtime controller/UI/directory integration remains pending M1-B/M2-A.

A focused test exposed the existing shared parser's function-key guard shadowing bare F. The guard now requires more than one character, letting F reach its existing letter conversion; function-key ranges, aliases and global token parsing remain unchanged. The direct parser regression covers F/Ctrl+F, F1/F12/F24 and invalid function keys.

Verification used the existing checked owner: fast-dev multi_launcher library test artifacts, separate compilation deadlines2700s and focused tests600s, cleanup5s. Prepare1 failed a test-only nonexistent SettingsEditor field; the final fixture correction edits actual show_launcher_icon and proves it changes while custom preferences survive. Prepare2 passed272.484008s. Exact22 ordinary cases produced21PASS/1FAIL2.570201s; the sole Ctrl+F failure led to the narrow parser fix. Prepare3 passed112.225533s; only the two affected chord cases and three parser cases reran, fivePASS0.986088s. The preserved cumulative map contains25 distinct passing cases, not a whole-suite result. Default26/27/52 literals and100/1000 SHA256 goldens, ordered2/8/26 properties, serialization/recovery, mapping and directly affected identity/session invariants pass. Historical failures remain recorded.

All required owner cleanup/output-drain receipts passed with final recorded identities Absent. Touched-only rustfmt(skip_children) and diff check passed. No native smoke, benchmark, release build or final Clippy was run at this checkpoint.

Review: authorized labeled parent-review fallback after configured Reviewer unavailability; this is not configured independent Reviewer acceptance. Exact scoped production/test diff inspected, public API/import/fixture findings resolved, no open M1-A correctness/scope findings.

Evidence root: C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g11.
See M1A_PARENT_REVIEW.md, m1a-cumulative-coverage.json, numbered prepare/inventory/run receipts, source bindings and commit/push receipts. Commit identifiers/push results are maintained externally to avoid rewriting this accepted checkpoint.

## M1-B — accepted

The existing JumpModeUi now owns committed chord/alphabet preferences. Loaded settings initialize the real LauncherApp controller; successful committed settings application updates preferences, including preference-only changes. Identical preferences preserve the active session. Configured leaders are recognized before generic shortcut filtering, with Windows command accepted only as a physical-Ctrl mirror. The actual session uses the same rank-based allocator with its frozen alphabet; the old fixed leader predicate is removed.

Real preference changes retire old prefixes, pending focus and navigation authority. One transient cleanup path preserves captured-key release fences through disablement, hidden frames and retired viewports; disabled input drains only fenced keys/paired text, leaving ordinary input available. Existing focus-only resolution, separate Enter execution, owner gates and repeat handling remain intact. No global hooks, second input observer or replacement settings transaction was introduced.

Verification: prepare1 passed144.175654s, exact13ordinarycases produced12PASS/1FAIL2.170393s. The failing new fixture sent a fresh F8 press without releasing the preceding rejected key; egui correctly derived repeat from physical key state. The fixture alone was corrected to release between distinct presses, retain the held-repeat case and strengthen ordinary-input assertions. Production code stayed unchanged. Prepare2 passed93.942977s; only that one case reran and passed0.872135s. The cumulative map verifies13distinctpassingcases, preserving the other12passes and the original failure evidence. Both final hidden/disabled release-fence regressions pass, including 27 rebuilt visible targets before fresh activation. Default input, foreign/native editor, viewport/epoch, settings owner, disabled state and focus-only guards pass. The app fixture clears Paste Sequence next/cancel shortcuts before construction.

Touched-only rustfmt(skip_children), scoped diff review and all required cleanup/output-drain receipts passed, with recorded owned identities Absent. Review is the authorized labeled parent-review fallback, not configured independent Reviewer acceptance. Evidence: M1B_PARENT_REVIEW.md, m1b-cumulative-coverage.json, numbered prepare/inventory/run receipts and16inputbindings in the same external g11 evidence root. No native or final Clippy claim is made at M1-B.

## Remaining original checkpoints

M1-B accepted; checkpoint commit/push identifiers are recorded in the external ledger.
M2-A pending: settings/search/directory/help integration, required native default/custom/reset smoke and final affected first-party Clippy.
Goal11 is not complete.