# Blocked utility recovery and Timer/Alarm execution

Program: `multi-launcher-blocked-utility-recovery-20261101-r01`.
Authoritative contracts: [Goal](CODEX_GOAL_BLOCKED_FEATURE_RECOVERY_TIMER_ALARM.md), [Kickstart](CODEX_KICKSTART_BLOCKED_FEATURE_RECOVERY_TIMER_ALARM.md).

## Bootstrap and execution rules

Single existing checkout: `G:\Repos\rust\Multi_Launcher`. Intake clean on `master` at `ae6660996ddf212817fc505ecd719956b32c7678`, tracking origin/master. No Git operation markers observed. Integration target is master; it is not the continuation base. Local predecessor and remote-tracking predecessor both resolve to `25a9468450f4f91090658be23d129908c92ca501`. Merge `b4d551b1a8b4afcdd66a284f8e5d2c1a5067d168` has that exact second parent. Later master changes only archive prior plans and add these contracts. No newer completed program found.

Requested main identity: Sol medium. Repository configured main: `gpt-6.1-sol` medium; implementer: `gpt-6-luna` max; planner/reviewer: `gpt-6.1-sol` high. Role tools expose these configured roles; backend identity is not independently observable. Configuration is unchanged. Planner is read-only; one implementer owns source; parent owns ledger, verification, Git and fixtures. Review uses frozen source.

Initial Git mutation failed with `.git/index.lock: Permission denied` under sandbox. Supported escalation succeeded. No application edits/builds/tests preceded the M0-A baseline. Baseline push succeeded. Goal/Kickstart text restored from master as task-owned documentation after baseline; no application source imported.

| Category | Branch | parent_branch | parent_tip_sha | baseline_sha | Final tip / push |
| --- | --- | --- | --- | --- | --- |
| C01 | fix/utility-sizing-20261101-r01 | perf/large-data-startup-20261031 | 25a9468450f4f91090658be23d129908c92ca501 | c0bc9de129bce0cf75fbba0374d9edf7588e8b31 | pending; baseline pushed |
| C02 | pending | C01 | pending | pending | pending |
| C03 | pending | C02 | pending | pending | pending |
| C04 | pending | C03 | pending | pending | pending |
| C05 | pending | C04 | pending | pending | pending |

M0-A direct parent verified with `git rev-parse HEAD^`; equals C01 parent above. No worktrees, auxiliary checkouts/clones, intermediate history operations or user changes created/discarded.

Imported Goal/Kickstart Git blobs exactly equal master (`010ce29bc91967d101b295d86469698ee6c09da8` / `f8e58d1ef4cbc2f061007c2b3b4fa9e37e1578be`). Their existing Markdown two-space line breaks trigger full staged diff whitespace notices; source and task ledger diff checks pass. Contract text was preserved, not reformatted.

## C01 — utility sizing

| Checkpoint | State | Evidence / next action |
| --- | --- | --- |
| M0-A | COMPLETE | Exact predecessor baseline c0bc9de1; normal push successful; clean at baseline. |
| M1-A | TESTED / COMPLETE | Commit81eadd2bbb56a18f59c20be3919ef6c25575a229 pushed; clean at checkpoint. Canonical bounded versioned field outside GENERAL_SETTINGS_FIELDS; mapping preserves current preference. Registry owns coalesced writes, revisions/ack, retry and join/unsaved report. Independent semantic review resolved;19/19 selected tests pass. No dialogs migrated. |
| M1-B | TESTED / COMPLETE | Commit86d3feb946894a201fe4af4f50fa0cd0b25dd5f9 pushed; clean at checkpoint. Five actual Window tests pass after initial+two focused corrections. Both historic cases prove400×300 outer restore, real two-axis gripID, body/frame/viewport clip, settled persisted size and omitted-instance latest reopen. Stable reset plus full post-reset resize persistence, clamping/discard and title/reflow/collapse controls pass. Independent source review cleared. No production dialog migrated yet. |
| M1-C | TESTED / COMPLETE | Commit144a55e1aa138e05f80f4e193dc2ddff18d2b290 pushed; clean at checkpoint. Six production surfaces use one context capability/cache and installed registry, once-per-logical-root-frame drainage; no lock over contents. Competing Clipboard/Preview writers retired; Diff body fallback only Missing, Reset wins. Close/omission/shutdown and error dedup verified. All19 scoped cases have passing evidence across documented runs, not one clean19 run. |
| M1-D | in_progress / native capture partial | First coherent checkpoint assigned: remaining eligible embedded inventory through same capability, defaults/minima/IDs/nested keys preserved. Second checkpoint will add Settings Reset/Retry and native-safe restore only. Automatic native capture unfulfilled under supported interface; not complete. |

Historical evidence only: `docs/plans/execution/standalone/productivity-and-iteration/stalls/20261031T180743Z.md`. Exact old failures: `real_window_resize_grip_records_candidate_after_one_shot_restore` and `omitted_window_reopens_with_latest_size_from_another_instance`; actual outer 400×63 versus 400×300. Later content-minimum candidate compiled without test acceptance. Preserved candidate root: `C:\Users\Jay\AppData\Local\Temp\Multi_Launcher\productivity-iteration-20261031\c04-m4c-unverified-source_20261031T180444Z_0c876f539d7a42c184af0e8c0cacd196`. Manifest SHA256 independently matches `53EAE08BF76D7F9B1501353CD5F1CC34F84926D55C39B66BA2AFDA7021DEB7CF`; patch expected SHA256 `7A398AE3A4BF05B28F4FA3C58433A41C1E9C827D36E2CBC0B52AA26D0BD3687C`. Unaccepted design evidence, not applied.

### M1-A initial preparation and review

Source candidate limited to settings/{mod,model,store,utility_dialog_sizes}.rs, settings_editor/mapping.rs and settings_save.rs. Initial runner request refused before process creation because measure.ps1 requires evidence outside the worktree; evidence root corrected to `C:\Users\Jay\AppData\Local\Temp\Multi_Launcher\blocked-utility-recovery-20261101-r01`. No test selected/run by that refusal. Initial actual preparation: `cargo nextest list --locked --offline --cargo-profile fast-dev -p multi_launcher --lib --list-type binaries-only --message-format json`, 1200s cap; receipt `c01-m1a-prepare_20261101T111718744Z_7a609f3c63854771a79f1959e564d3cb`, outcome pending. Implementer changed count validation after announcing freeze; parent stopped further edits. These changed bytes require accounted correction preparation, not acceptance of a mismatched snapshot.

Initial preparation retired in 288.933818s, exit101, no timeout, cleanup and output drainage verified. First compiler error E0599 at utility_dialog_sizes.rs272 (Option::ok), then E0308 at363 (Result<bool> versus Result<()>). No inventory or tests ran. Candidate changed after initial freeze; this failed preparation establishes diagnostics only. Independent stable-source review required committed preference acknowledgement, settings-path binding, terminal unsaved outcomes on shutdown, and deterministic newer-event-during-write test. One focused correction packet assigned after retirement, also distinguishing Unsupported preference from Missing; no production dialogs touched. A read-only CIM process query was denied; no process/security workaround used.

Correction1 preparation `c01-m1a-correction1-prepare_20261101T112709463Z_9082050331b640febc6074b3360a9ac1` retired104.494352s exit101, no timeout, cleanup/drain verified, before/after workflow source fingerprint equal. E0599 Option::ok remained at285 despite implementer report; E0282 closure revision type3155 and E0502 overlapping state borrows705 newly exposed. No inventory/tests ran. Focused review confirms the four semantic findings resolved in this snapshot; no further semantic findings. Correction2 is restricted to those three compiler errors and actual-byte inspection before freeze. Recurring Option::ok has now used initial plus one correction; one final correction authorized.

Correction2 compilation finished successfully (Cargo exit0,478s reported), but enclosing preparation `c01-m1a-correction2-prepare_20261101T112947573Z_208f4476fab64a8b97b2d14b4a5e1746` FAILED484.435507s because owned descendants survived Cargo root. Runner retired all owned identities and verified cleanup/drain; no timeout. This is a preparation containment failure, not a passing run. Same frozen source metadata-only rerun `c01-m1a-prepared-metadata_20261101T113822363Z_8ab0fb4545814455a7dd50f2b10116ee` passed2.21688s, exit0 cleanup/drain true, with no recompilation/source change.

Actual nonignored library inventory19, `c01-m1a-inventory_20261101T113835117Z_a3f2a31542d8464188a9ea0818b2fc70`, passed1.753265s. Selection: `test(utility_dialog_sizes)` (14) plus settings_save tests `general_intent_changes_only_editor_owned_fields_and_plugin_keys`, `acknowledged_apply_publication_merges_post_persistence_unrelated_writes`, `confirmed_runtime_failure_rolls_back_owned_fields_and_keeps_concurrent_values`, and settings/store tests `concurrent_updates_preserve_both_logical_changes`, `failed_update_returns_no_value_to_publish` (5). Prepared metadata reused for inventory/run, no redundant compilation. Execution `c01-m1a-execution_20261101T113848343Z_4657bf06d31a4adba0b0bb07ceff1539` passed19/19, Nextest0.248s/runner1.091802s, exit0 no timeout, cleanup/drain true. Test profile100s/test; run cap600s. Workflow source fingerprint before/after `a8643c92450501253a0de8aef1cba531d7b655ceb35783929684819472fc6eb6` identical. No native/UI acceptance claimed. Scoped diff inspected and diff --check passes; existing unrelated warnings unchanged. No owned job remains.

### M1-B initial candidate

Frozen candidate adds only gui/utility_dialog_size.rs and module registration (rustfmt also sorts adjacent test-only module declaration). Five tests include both exact historical names. Initial locked/offline fast-dev library binaries-only preparation `c01-m1b-prepare_20261101T115133942Z_7c145d386e0f45118da49cf05f5ec206` passed179.20901s, exit0 cleanup/drain true,600s cap. Existing `verified-msvc-vctip` preflight passed1.718755s, verified Microsoft signature+held file identity/hash; descendant9536 matched it and was retired with final checks verified. This does not retrospectively change the earlier default-policy failure.

Inventory `c01-m1b-inventory_20261101T115455143Z_adcd2b19125544669f18e13ab9ed5184` passed1.901544s; actual5 nonignored library tests selected by `test(gui::utility_dialog_size::tests::)`. Execution `c01-m1b-execution_20261101T115507288Z_e5d75dfcad9b4629942ae4192f8e4108` exit100, Nextest0.105s/runner0.914624s,3passed2failed, no timeout, cleanup/drain verified. Both historic names and clamp/discarded case passed; these do NOT satisfy acceptance yet. First failure title/content control at926 asserted unchanged height while changing content from2to4lines during drag; reset at855 observed outer720×300 instead of default400×300. Independent review additionally found next-frame reset rebound and missing explicit actual dragged-grip ID and body/frame/viewport clip assertions. Correction1 assigned only those findings: preserve same-content title-drag invariants, separately test content reflow causes no save, supported one-shot restoration constraint with subsequent-frame+resize proof, strengthen grip/clip checks. No production migration or geometry acceptance yet.

M1-B correction1 preparation `c01-m1b-correction1-prepare_20261101T120000682Z_9e6b8b5dba48453398fc3a9dd34e8062` passed95.760752s, inventory `...correction1-inventory_20261101T120145463Z_e53b15b56b544bf68b03ee6283136d2c` passed1.748974s actual5nonignored. Source review found prior two findings resolved; no remaining source finding. Execution `c01-m1b-correction1-execution_20261101T120158522Z_4c5aaa22107b4f659c8b5ea0ef85d4ca` exit100,4passed1failed, Nextest0.119s/runner0.90667s, no timeout, cleanup/drain true. Both historical cases now include actual grip-ID and body/frame/viewport clip assertions and pass; reset subsequent frames+grip and clamp pass. Only title-control height assertion at1014 still fails. Before rectangle is captured after one render; focused review is checking whether settled baseline is required and whether reset fixture closes admission before its subsequent drag. No source edits or migration until final focused correction packet. Initial+one correction used for title finding; one remains.

M1-B correction2 changed fixtures only per focused reviewer: title baseline settles3unchangedframes, preserving size/movement/no-save assertions; reset waits boundedly for acknowledgement without closing worker admission, then full grip press/move/release/settle and final persistence proof. Preparation `c01-m1b-correction2-prepare_20261101T120440630Z_22214cab5fdf4736b491102b8cc6e3fe` passed88.541728s; inventory `...correction2-inventory_20261101T120618623Z_1a3e5fd44423471b8a95b06fc8df9c1b` passed1.695463s actual5nonignored; execution `c01-m1b-correction2-execution_20261101T120630343Z_d222f4436e2542b78fdceeb4625343b5` passed5/5,Nextest0.114s/runner0.925687s,exit0,cleanup/drain true,no timeout. Source fingerprint `059e2d0f4013312c170b89e0945bc8fe76f5ea5bc6e8f63a81627cdb6a6d9243` stable before/after. Initial+two correction bound honored; no further geometry correction cycle. This is actual egui fixture evidence, not native desktop proof. Scoped diff inspected; only helper+registration and task ledger changed. Planner now preparing M1-C six representative integrations; full inventory waits M1-D.

### M1-C bounded ownership addendum

Read-only M1-D inventory exposed an actual integration constraint before six-surface migration: mouse_gestures::settings_ui(ui,value) has no app parameter, and eligible nested child windows render within parent contents. Parent approved the foundational M1-C adjustment now: ONE app-created egui Context capability contains the same manager and acknowledged UtilityDialogSizes cache; no process-global/static owner or second cache/store. Split prepare/show/observe so no lock/borrow is held across caller content rendering. App alone advances logical frame and drains acknowledgement; startup registry installation rebinds the same capability before events. Default/headless rendering without app capability remains ordinary Window behavior. Source writer had only helper argument narrowing and app field initialization underway; no dialog migration or competing implementation preceded this decision. Native viewport adapters and other surfaces remain M1-D.

M1-C actual builder inspection also requires explicit caller minimums: Hash/Clipboard420×320, Table600×420. Parent approved a narrow minimum_outer_size adapter input merged into one-shot bounds; preferred remains distinct from applied minimum/viewport constraints, with no invalid min>max. Existing defaults/minima are not otherwise changed. One representative production-minimum case is required, not an exhaustive matrix.

M1-C source inspection found main.rs closes registry admission at both shutdown paths BEFORE joining GUI, so on_exit's final completed-grip flush would be rejected. Parent inspected both actual paths and authorized moving admission closure until GUI retirement (join_all already closes it), preserving existing service/listener/hotkey receiver teardown and rollback behavior. This is a necessary adjacent owner fix, not a new lifecycle framework. Relevant main binary target must therefore compile and a narrow owner test be inventoried if present; no full binary suite. Existing File Search UI close persistence is explicitly preserved; only Clipboard geometry is removed from the close writer.

M1-C initial preparation `c01-m1c-prepare_20261101T123043038Z_bc826c92838a4dcc99887788b04a0808` selected lib+bin multi_launcher because main shutdown order changed. Failed exit101 in126.092427s, no timeout, cleanup/drain true, before/after source fingerprint `ed38b2aa6d0338507675f2fe3628dc31eb83bfbd9fb0aee8943845f1d479be4f` equal. First E0384 Diff window immutable reassignment956; then E0502 helper instance/state.frame borrow268. No inventory/tests ran. Changed Rust rustfmt --check passed. Independent review found ordinary close/omission discards completed resize before settlement and Clipboard cleanup assertion uses a disconnected preference object. Correction1 assigned those actual owner defects plus two compiler fixes and minimal deduplicated production save-failure notice (full Settings Reset/Retry UI remains M1-D). All six routes, no-lock content boundary, minima, registry binding and named legacy-writer retirement otherwise source-confirmed. No functional M1-C acceptance yet.

M1-C correction1 preparation `c01-m1c-correction1-prepare_20261101T124526209Z_ba50598958d7445e87ae7d2a86c8f009` failed188.058292s exit101, no timeout, cleanup/drain true; source fingerprint `15e2877766c4700fdedc9fa5825ae1189a03e86a64cd214419606513ee866f82` stable. Only remaining compiler error E0502 render.rs11531: fixture profile_root read while its returned app is mutably borrowed. Correction2 restricted to computing that path before app construction and mechanical introduced unused bindings. No inventory/tests yet. Focused reviewer cleared canonical close/omission/shutdown submit, actual X+omission fixture, connected Clipboard cleanup/reopen and revision-dedup error route. Reviewer suspected fallback Option mismatch, but actual `.then(...)?` is valid and compiler found none; parent corrected that false-positive disposition. No fallback semantic change authorized.

M1-C correction2 preparation `c01-m1c-correction2-prepare_20261101T125041765Z_4fb805181c8945188a20e4014efb051e` passed194.688056s, lib+application test targets compiled, cleanup/drain true. Inventory `c01-m1c-inventory_20261101T125419590Z_7126e39cb4bb4241833d8bef022daac1` passed2.634996s:19nonignored selected library cases,67main cases compiled but unselected. Initial parent JSON guard incorrectly flattened two suites and refused before execution; explicit per-suite parsing verified the actual19, no inventory rerun/source change needed.

Execution `c01-m1c-execution_20261101T125459197Z_8b01aa775a9642e68c09983ac2202b99` exit100,Nextest1.556s/runner2.384386s:10passed1failed8notrun. Only Hash minimum fixture failed932 with420×214 versus>=420×320. Same-source fail-fast remainder inventory8 was verified from existingJSON and executed separately: `c01-m1c-unrun-execution_20261101T125556271Z_828d5d450f7247ec81c5e83f2bc873a8` passed8/8,Nextest0.942s/runner1.816722s. All cleanup/drain true,no timeout. This is not a clean19-case run. Focused review found Hash fixture reads after ONE intrinsic/invisible pass; production applied target already respects420×320. Authorized fixture-only three-frame settling with unchanged minimum/preference assertions, then exact failed-case rerun; no speculative production bounds change or redundant rerun of18unchanged passing cases.

M1-C exact minimum-fixture correction preparation `c01-m1c-minimum-fixture-prepare_20261101T125747139Z_2ce9d9b0d2d24a38a7c72a24fd78221f` passed78.208893s (lib only; main/source unchanged). Exact inventory1nonignored `...minimum-fixture-inventory_20261101T130010835Z_ab451f1d0a604c6c926ef7e3483dd2b2` passed1.663892s; execution `c01-m1c-minimum-fixture-execution_20261101T130141574Z_04b05289d9f34b089e7fd5dc4d7593e8` passed1/1,Nextest0.079s/runner0.874214s,exit0,cleanup/drain true. Current source fingerprint `6bd53fa506bdfe36a50f499f3d617c8b1612c7d52eb04aac68d85476ba05c666` stable. Earlier18passes reused only for unchanged production/test paths; no clean aggregate19 claim. Previous source fingerprint for initial/remainder `c6ca0385cec771339703b740419068e5d849451c8b69dc3d10814705f90b0c40`. Search found no obsolete Clipboard/Preview size fields or close writer, and no production Diff window_size assignment (remaining assignments tests only). Scoped cumulative diff inspected, Rust format/diff checks pass. No owned process remains. Main application test target compilation is current for unchanged production source; no native acceptance claimed.

### M1-D native capability boundary

Pinned ViewportInfo exposes inner_rect/outer_rect/native_pixels_per_point/monitor_size and state flags; ViewportEvent only Close. egui_winit converts Resized/Moved to repaint, and asynchronous InnerSize commands use the same path. raw_input_hook has converted input, no completed-user-resize cause. Rectangles are UI points divided by zoom×nativeDPI; child monitor_size gives dimensions, not identity/origin/work area. Root content_rect is unsuitable for child bounds.

Bounded repository owner check found no exact child-handle/completion bridge: window_manager::get_hwnd(Frame) and visibility::RootWindowBridge capture root handles; immediate callbacks receive Context/ViewportClass only. screenshot_countdown and passive_tooltip subclass directly owned native windows; pinboard lookup is pin-specific with exact lifetime/title/process constraints, not an eframe child creation bridge. No WM_ENTERSIZEMOVE/WM_EXITSIZEMOVE/WM_SIZING/WM_SIZE handlers found. No enumeration, native action, hook, vendor edit, or new framework performed.

Automatic native capture for Regex/XML/QR and macro parameter/runtime prompts is therefore UNFULFILLED/BLOCKED by the supported interface, not an observed native test failure. Integrate embedded variants normally; preserve native defaults/resizing and allow only proven one-shot restore/reset from known preferences with measured inner/outer conversion if implemented. Never persist arbitrary native rectangle/close observations as user intent. C01 cannot be represented fully complete while this mandatory capture boundary remains. Other independent categories may proceed from its actual safe closing tip.

M1-D uses two coherent source checkpoints on the same C01 branch: remaining embedded inventory migration, then Reset/native-safe adapter and final inventory/review closeout. These are bounded increments of approved M1-D, not new categories or repair waves.

M1-D embedded investigation confirmed many builders lack full explicit defaults. Candidate adds typed per-axis intrinsic/outer/width/height recipes and returned response.layer_id grip provenance (pinned namespace Id::new(layer_id).with("edge_drag")). Tracking IDs must preserve per-instance identity. Shared physical-ID kind transitions submit an old completed release before clearing it and retain observed geometry for a smaller new-kind restore. Original close-button presence is preserved. First sweep reported70literal-key sites plus dynamic package variants and screenshot builder across68files; most line churn is callback indentation (non-whitespace diff1737insertions/314deletions at initial snapshot). Candidate not accepted yet.

M1-D embedded preparation `c01-m1d-embedded-prepare_20261101T133343337Z_36c92ad76a8d4fd19084848d2cc45498` failed122.516156s exit101,cleanup/drain true,no timeout,no inventory/tests. Fingerprint `30686097001527fd51b0533622575547067d0ce632d7cb39b5f89f4d5feed72f` stable. Eleven compiler errors reduce to missing egui namespace in MkMacro primary/recording review, missing extension trait import, and nested Some around Window::show Option. Independent review found unspecified intrinsic axes filled on later idle frames; comparison-rules tracking constant despite dynamic actual window ID; Settings full minimum overwrite lowers implicit width instead of setting only height. No further dynamic-ID/X mismatch; domain bodies unchanged except formatting; crop branch close distinctions intact. Correction1 assigned those exact findings and one trailing-whitespace line. New four grouped fixtures cover intrinsic baseline, same-ID kind transition, width-only reset/reopen, and nested plugin picker; actual inventory/results pending.

M1-D embedded correction1 preparation `c01-m1d-embedded-correction1-prepare_20261101T134450256Z_ed59fe26fd59474b85a9727525f98c27` passed208.82689s,lib+bin,cleanup/drain true. Focused reviewer cleared intrinsic-idle behavior, dynamic Diff tracking, and Settings axis minimum; no remaining finding. Inventory `c01-m1d-embedded-inventory_20261101T134846319Z_a9bb901d2db14aab8acf7d9a55a67580` passed2.769807s,23nonignored selected library cases(main compiled/unselected). Expression: inventory/integration prefixes, all common helper cases, exact Jump layer inventory, theme density lifecycle+saved typography, Clipboard cleanup, settings shutdown. Execution `c01-m1d-embedded-execution_20261101T134900716Z_51812ba25de544618d794b45332413b1` passed23/23 in1.632s Nextest/2.481164s runner,exit0,no timeout,cleanup/drain true. Stable source fingerprint `d2d16eb4cc570901ef6ac3ea43b4a25c30fd8690d4272062a7af512134539e66`. Initial+one correction bound used for named findings; no additional cycle. Static literal scan found72call sites including fixtures,70unique keys,all in canonical allowlist; dynamic package variants source-reviewed. No native capture or interaction accepted. Diff/format checks pass; no owned job remains. Embedded checkpoint commit follows before Reset/native-safe increment.

### M1-D ordinary-dialog inventory

Planner revalidated production builders; omitted resizable flag means egui default true. Embedded rows are now source-integrated with scoped verification below; native rows remain explicitly unfulfilled. This inventory does not claim native interaction acceptance. Paths below are under src/. Same context capability is required for nested children and plugin settings. No missing stable key found. Native adapter work remains M1-D and uses child viewport/monitor bounds, never launcher bounds.

| Owner | Stable kinds | Current disposition |
| --- | --- | --- |
| actions_editor.rs | custom-apps | embedded implemented; scoped tests pass |
| settings_editor/render.rs | settings | embedded implemented; scoped tests pass |
| plugin_editor.rs | plugin-settings | embedded implemented; scoped tests pass |
| help_window.rs | quick-help, command-help | embedded implemented; scoped tests pass |
| multi_manager/ui.rs | multi-manager, multi-manager-settings | embedded implemented; scoped tests pass |
| gui/action_sheet.rs | action-sheet | embedded implemented; scoped tests pass; retain anchor |
| gui/activity_dialog.rs | activity | embedded implemented; scoped tests pass |
| gui/add_action_dialog.rs | add-action | embedded implemented; scoped tests pass |
| gui/add_bookmark_dialog.rs | add-bookmark | embedded implemented; scoped tests pass |
| gui/alias_dialog.rs | folder-alias | embedded implemented; scoped tests pass |
| gui/bookmark_alias_dialog.rs | bookmark-alias | embedded implemented; scoped tests pass |
| gui/tempfile_alias_dialog.rs | tempfile-alias | embedded implemented; scoped tests pass |
| gui/batch_rename_dialog.rs | batch-rename | embedded implemented; scoped tests pass primary |
| gui/brightness_dialog.rs | brightness | embedded implemented; scoped tests pass |
| gui/calendar_popover.rs | calendar | embedded implemented; scoped tests pass |
| gui/calendar_event_editor.rs | calendar-event-editor | embedded implemented; scoped tests pass |
| gui/calendar_event_details.rs | calendar-event-details | embedded implemented; scoped tests pass |
| gui/clipboard_dialog.rs | clipboard-history | embedded implemented; scoped tests pass production history |
| gui/cpu_list_dialog.rs | cpu-usage | embedded implemented; scoped tests pass |
| gui/dashboard_editor_dialog.rs | dashboard-editor | embedded implemented; scoped tests pass primary |
| gui/data_recovery_dialog.rs | data-recovery | embedded implemented; scoped tests pass |
| gui/diff_dialog/{rules_dialog,folder_rules_dialog}.rs; text input | diff-comparison-rules, diff-folder-rules, diff-text-input | embedded implemented; scoped tests pass children |
| gui/duplicate_finder_dialog.rs | duplicate-finder | embedded implemented; scoped tests pass |
| gui/fav_dialog.rs | favorites | embedded implemented; scoped tests pass |
| gui/file_search_dialog.rs | file-search | embedded implemented; scoped tests pass |
| gui/hotfolder_dialog.rs | hotfolder-manager | embedded implemented; scoped tests pass |
| gui/hotkey_directory_dialog.rs | hotkey-directory | embedded implemented; scoped tests pass |
| gui/image_compare_dialog.rs | image-comparison | embedded implemented; scoped tests pass |
| gui/image_panel.rs | image-viewer | embedded implemented; scoped tests pass; retain instance ID |
| gui/image_crop_editor.rs | image-crop-editor | embedded implemented; scoped tests pass shared crop |
| gui/mkmacro_dialog/image_crop_editor.rs | mkmacro-reference-crop | embedded implemented; scoped tests pass all load/error branches |
| gui/json_utility_dialog.rs | json-utility | embedded implemented; scoped tests pass |
| gui/macro_dialog.rs | legacy-macros | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/mod.rs | mkmacro | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/action_catalog.rs | mkmacro-action-catalog | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/action_editor.rs | mkmacro-action-editor | embedded implemented; scoped tests pass primary |
| gui/mkmacro_dialog/launcher_action_picker.rs | mkmacro-launcher-picker | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/package_ui.rs | mkmacro-package-export, mkmacro-package-import, mkmacro-template-save, mkmacro-templates, mkmacro-help | embedded implemented; scoped tests pass; shared ID, variant key |
| gui/mkmacro_dialog/recording_review.rs | mkmacro-recording-review | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/search.rs | mkmacro-step-search | embedded implemented; scoped tests pass Find/Replace/Jump |
| gui/mkmacro_dialog/window_picker.rs | mkmacro-window-picker | embedded implemented; scoped tests pass |
| gui/mkmacro_dialog/parameter_prompt.rs | mkmacro-parameters | unintegrated native capture; default-resizable |
| gui/render.rs::show_macro_prompt | mkmacro-runtime-prompt | unintegrated native capture; default-resizable |
| gui/mouse_settings_dialog.rs | mouse-settings | embedded implemented; scoped tests pass |
| gui/mouse_gesture_settings_dialog.rs | gesture-settings, gesture-settings-window-picker | embedded implemented; scoped tests pass |
| gui/mouse_gestures_dialog.rs | mouse-gestures, gesture-binding-editor | embedded implemented; scoped tests pass |
| plugins/mouse_gestures.rs | gesture-ignore-window-picker | embedded implemented; scoped tests pass; shared context capability |
| gui/note_graph_dialog.rs | note-graph | embedded implemented; scoped tests pass |
| gui/note_panel.rs | note-editor | embedded implemented; scoped tests pass; retain instance ID |
| gui/notes_dialog.rs | quick-notes, note-templates | embedded implemented; scoped tests pass |
| gui/ocr_view.rs | ocr-result | embedded implemented; scoped tests pass |
| gui/paste_sequence_dialog.rs | paste-sequences | embedded implemented; scoped tests pass |
| gui/pinboard_windows.rs::show_pin_list | screenshot-pin-list | embedded implemented; scoped tests pass |
| gui/{qr_dialog,regex_tester_dialog,xml_toolkit_dialog}.rs | qr-generator, regex-tester, xml-toolkit | embedded implemented/tested; native capture unfulfilled |
| gui/search_replace_dialog.rs | search-replace | embedded implemented; scoped tests pass primary |
| gui/screenshot_editor.rs | screenshot-editor | embedded implemented; scoped tests pass ordinary editor |
| gui/shell_cmd_dialog.rs | shell-commands | embedded implemented; scoped tests pass |
| gui/snippet_dialog.rs | snippets | embedded implemented; scoped tests pass |
| gui/snippet_prompt_dialog.rs | snippet-prompt | embedded implemented; scoped tests pass; retain anchor |
| gui/tempfile_dialog.rs | create-tempfile | embedded implemented; scoped tests pass |
| gui/toast_log_dialog.rs | toast-log | embedded implemented; scoped tests pass |
| gui/todo_dialog.rs | todos | embedded implemented; scoped tests pass |
| gui/unused_assets_dialog.rs | unused-note-assets | embedded implemented; scoped tests pass |
| gui/volume_dialog.rs | volume | embedded implemented; scoped tests pass |
| M1-C six representatives | diff, clipboard-modify, file-preview, hash-generator, table-utility, theme-settings | implemented/tested M1-C |
| Later category features | unicode-inspector, zip-extractor | reserved/absent until accepted UI |
| gui/convert_panel.rs; gui/todo_view_dialog.rs; Timer/Alarm | corresponding fixed dialogs | excluded: explicit resizable(false) |
| Timer Finished; confirmation wrappers; Batch Rename Apply/Undo; Search Replace Apply/recovery; Diff folder-operation preview | transient alerts/confirmations | excluded: ephemeral confirmations |
| Dashboard deletion; note insert/link/discard/collision; template deletion; crop/XML/reference overwrite; recording annotation | transient editor prompts | excluded: fixed/ephemeral |
| Radial Designer | radial_designer.window_size | existing_equivalent independent native transactional geometry owner; no competing store; not new clamp/reset proof |
| Radial cell/add/delete/resolve/unsaved prompts | radial prompts | excluded: fixed/ephemeral |
| Main launcher; radial/input/capture/screen-draw/toolbar; pinned screenshot native windows | specialized geometry | excluded: launcher/overlay-specific |
| Native OS file/folder pickers; smoke/test-only constructors | OS or fixtures | excluded: not ordinary production tool ownership |

M1-D packet: after M1-C commit, integrate remaining eligible rows via the same capability; one bounded native viewport adapter for Regex/XML/QR and macro prompts with explicit inner/outer units and child-monitor bounds; Settings selector+Reset only for actually integrated kinds, canonical immediate queue outside general draft ownership, pending/error/explicit retry; deduplicated error feedback and same-registry shutdown. Focused grouped cases: real embedded resize/reopen, nested/plugin capability without locks across contents, Reset ack/default/stale draft/failure, native adapter one-shot/unit/bounds semantics, selected theme/Jump including expanded_dialog_layer_inventory_matches_actual_window_ids. Native interaction/restart proof remains C05. No full suite or Clippy here.

## C02 — Unicode UI

Pending C01 closeout and direct-child baseline.

## C03 — ZIP

Pending C02 closeout and direct-child baseline. Native safety must pass before writer and UI implementation.

## C04 — Timer/Alarm

Pending C03 closeout and direct-child baseline.

## C05 — native and cumulative closeout

Pending C04 closeout and direct-child baseline. One supported native capability attempt and at most one documented recovery; one end-only first-party Clippy. No native acceptance claimed.
