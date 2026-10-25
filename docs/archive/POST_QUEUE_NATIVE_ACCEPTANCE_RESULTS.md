# Post-queue native acceptance results

**Acceptance status: BLOCKED_PRODUCT_DEFECT.** M1 Escape leakage is unresolved. Bounded M1–M9 execution and final gates are complete; native acceptance sign-off remains blocked. Passing subphases do not establish native acceptance.

M5 now has partial native proof: guarded Start reached the real first-read fixture gate; guarded Cancel advanced generation, invalidated results, and kept Start disabled while draining. Normal and 1000×720 outer-window footer geometry passed. Release/drain completion/restart/results/Open/Reveal/Copy and explicit worker-joined acknowledgement remain unverified because the fixture completion validator used `fixture-release` while the published observation uses `release_gate`. Primary plus one causal recheck are closed; both applications exited normally, accepted mouse2/2 balanced across both, synthetic paths/bytes unchanged. Full M5 receipts are under `target/multi-goal-queue/post-queue-acceptance/m5/`. Final verification outcomes are recorded below.

Branch `qa/post-queue-native-acceptance`; verified base `6eee86801741d6f1c9ea6bcdef0824836e76a843`, baseline `c2c05199579f1eb693de43c0423ca3a227e202fd`. Base is the supplied-documents-only direct successor of cumulative G3 `7e0f74f117975024b25e7c7cbadef32f29937965`. Earlier branches remain protected. Final source verification and lint metadata are recorded below; the enclosing final documentation commit is reported in the handoff.

Environment: Windows 10 Home 22H2 build 19045.6466, one observed 2560×1440 monitor at (0, 0), 96 DPI; Rust/Cargo 1.99.1, Nextest 0.9.135, fast-dev unchanged, queue-focus zero retries/60s per test/10m group, standard stack.

## M1: shield isolation and recovery

| Requirement | Actual disposition | Evidence |
|---|---|---|
| Strict geometry rejection | PASS_NATIVE: one-column RectMismatch rejected | `target/multi-goal-queue/post-queue-acceptance/m1/probe-primary/native/stderr.log` |
| TOPMOST-only recovery | PASS_NATIVE: unchanged geometry, exactly one reassertion, strict postcheck | Same probe receipt |
| Probe HWND destruction | PASS_NATIVE: all3retained HWNDs destroyed; owned process tree absent | Same probe stderr and result.json |
| Drag isolation | PASS_NATIVE: exact80,117/248x29 rectangle, no receiver mouse/edit-selection change | `target/multi-goal-queue/post-queue-acceptance/m1/input-primary/native/stderr.log` |
| Second attempt / cancellation | Native Cancelled acknowledged after held Escape and pointer release | Same input receipt |
| Keyboard isolation | **PRODUCT_DEFECT, unresolved:** receiver observed Escape keyDown1/keyUp1/char1 against baseline0 | Same input receipt; source-backed planner/reviewer assessment |
| Ordinary post-cancel click | NOT_RUN: stopped after keyboard-isolation failure | Same terminal receipt |
| Input and receiver cleanup | PASS_NATIVE: accepted mouse2/2 and key1/1 balanced; root/edit/button destroyed; both owned processes absent | Same stderr and result.json |
| Native negative-origin display | NOT_RUN: no such physical monitor observed; signed-monitor deterministic test passed | group1-run / probe receipt |

Current smoke executable SHA256 `27d9544c0783507b236ab3a58e2541ae033494cc179960c28bafa4a7d472f1c8`. Build passed2m40s;6recorded processes absent. Source/diff/tool hashes were checked before both launches. Each primary ran once, no recheck; no desktop pixels or clipboard access. Probe exited0, input fixture returned1 normally; neither required forced cleanup. No worker was created by these fixtures. Runtime HWND/PID/birth/control/token/hit/foreground evidence is in the input stderr; source and per-scenario manifests are retained under `m1/`.

The standalone fixture bypasses full application service integration. Causal source inspection nevertheless confirms the missing boundary: VisualSelection becomes ExclusiveTool and forwards keyboard events; radial Escape ownership does not cover an ordinary visual selection; external injected events are accepted; the renderer detects Escape asynchronously. Existing operation-specific cancellation has no correlated keyboard-hook route. A hook-only swallow or unqualified cancel-current would be incomplete. No safe complete correction/recheck fits this campaign; M1 sign-off remains blocked by a product defect.

The harness correction is committed/pushed as `e6b675ac83658ac8ccce019ae3319a4b2f41e3a2`. Four shield tests passed0.076s; two repeated Pinboard lifecycle tests passed0.123s. Initial failed compilation is retained, followed by successful corrected listing/build and independent review. These passes do not erase the keyboard failure.

## M2: pin click-through and recovery

**HARNESS_DEFECT; current native NOT_RUN.** The short-mode attempt could not establish its exact completed child-viewport Response invariant. Four selected tests ran: three existing owner tests passed; the new guard regression failed. Its one causal Area-sizing correction/recheck still failed the completed-owner assertion. All owned list/test processes retired. No native build, input or desktop pixels occurred. The two attempted source files were restored to HEAD after preserving the complete ignored patch (`m2/attempted-harness.patch`, SHA256 `960397dd75835afda5f06c4aa396a1ada4d317376c06c4ae3c1bb5c009f8125a`). No failing test is retained in maintained source.

Actual checkbox toggle, native click-through acknowledgement, underlying ROOT delivery, Pin List Recover, restored child delivery and current native retirement remain unverified. Historical static/generated-live composition/export/Reset acceptance is unchanged; it does not establish routing or OS WGC capture. Exact test logs and cleanup receipts are under `target/multi-goal-queue/post-queue-acceptance/m2/`.

## M3: saved-target repair

**HARNESS_DEFECT with partial native proof.** Nine selected tests passed (UI/guard5 in1.407s, transaction/fixture4 in0.106s). Current native build passed3m04s; app SHA256 `9a8bfe7bebff135fce95da604421014a358d46155a24e892b83b95437aa2751e`. Source/diff, exact runner bytes, fixture hashes and runtime identities are bound in the local manifests.

| Outcome / phase | Actual result |
|---|---|
| Exact saved action and folder Repair entry | PASS_NATIVE: guarded real query/result/Action Sheet/Repair opened the matching request |
| File picker Cancel | NOT_RUN: Browse was clipped inside the form scroller; driver lacked reveal step |
| File confirm / durable repair | NOT_RUN: same Browse gap; no picker path or confirm input |
| Folder picker opening | PASS_NATIVE: exact owned native folder modal |
| Folder picker Escape / fresh ROOT return | PASS_NATIVE on one causal recheck; same token/revision, empty draft, live five-store bytes unchanged, zero activation |
| Folder path / confirm / committed reference rekey | HARNESS_GAP / NOT_RUN: filename Edit ownership unproved; no path or confirmation input |
| Input and process retirement | All156key pairs/7mouse pairs balanced. Three normal app0 exits; one primary required exact owned-process termination. All recorded processes absent. No desktop pixels or clipboard/target execution |

The FileCancel prelaunch encoding failure sent zero input and launched no app; a proven UTF8 metadata correction preceded its one causal run. Folder primary stopped on an impossible fresh-ROOT requirement while synchronous rfd blocked ROOT; its reviewed Escape-only correction preserved current native modal ownership and matching-Up cleanup. This did not grant filename input.

Actions/folders/favorites/pins and generated target inputs remained byte-identical in all four app sessions. Usage retained exact identities/counts7/5/11; normal exit reordered its HashMap serialization. Retain the raw usage-byte failures and earlier Browse-readiness failures separately. Neither proves a repair mutation. Raw receipts: `dev/jump_mode/evidence/jump_g3_m3_target_repair_*_20261024T152045Z_2f928ed9`, `...T152403Z_46a61e68`, `...T152541Z_3ca5ac0b`, `...T153202Z_81e09ad6`; complete paths and hashes are in `target/multi-goal-queue/post-queue-acceptance/m3/native-phase-review.json` and the execution ledger.

## M4: Notes replacement — terminal harness gap

Five selected tests passed: Notes UI/history4 in1.595s and ON/OFF fixture equivalence1 in0.012s. The corrected non-native input-ledger self-test passed5 admission/balance cases, without native input. The initial self-test failed before entry on optional-parameter revalidation; both receipts are retained. An initial compiled listing selected zero because the parent passed a spaced filter through Windows argument joining; the corrected listing selected exactly5, and no tests ran in the empty selection.

Current build passed1m05s/all8recorded processesAbsent. AppSHA `23f51edd6422d581495f5097d12c1745e8b17333726e2920a290ec78ac90cb92`; fixtureSHA `8331aa1a2fc19eaea5f4c10dc3277db218ac3a1f78ffcb9fb21482c8db17daca`; compiled-source diff `cb31ce080e8d9517b1cda2cb025e94572514ddceb51a95b9f106c9562ed4d970` onHEAD87c69b85. Native campaign16:26:14–16:46:14Z, never reset by correction.

| Primary outcome | Actual evidence |
|---|---|
| Jump ON | HARNESS_GAP: exact ROOT click and3query characters; single-shot observation guard then failed before character4. Keys3/3mouse1/1balanced; app33772 normal0/noforced/all3Absent. No note opening/replacement. Exact failed-read subtype was not retained; final saved observation is focused/current/ownerallowed. |
| Jump OFF | HARNESS_GAP: zero input,20s startup timeout. Reused Settings startup wrongly required the intentionally absent Jump registry entry. Actual current query Response exists in note_replace_root. App40288 normal0/noforced/all3Absent. |
| Source integrity | Both44-file generated profiles checked: all note files unchanged; only settings.json changed on normal exit. That post-close serialization is separate from unperformed live persistence assertions. |

Receipts: `dev/jump_mode/evidence/jump_g3_m9b_note_replace_20261024T162752Z_0fc4b1d7` and `...T162855Z_4df8e82b`; wrappers/manifests under `target/multi-goal-queue/post-queue-acceptance/m4/`. The single PS-only correction was independently reviewed and bound to fresh profiles with unchanged compiled Rust/app. ON causal recheck (`...T163726Z_8a22935a`, app26172) and OFF causal recheck (`...T163814Z_03ed9c34`, app2156) both stopped before input on a native ownership guard that combines root/foreground/PID mismatch. Exact cause is not retained; current egui geometry does not establish native foreground. This is an unverified ownership/harness gap, not confirmed environment denial or product failure.

Both rechecks had zero input, balanced/no pending releases, graceful-close10s timeout and exact retained-handle termination; app−1/all3recorded processesAbsent each. Thus normal exits2/forced retirements2 across4sessions; total key3/3mouse1/1 balanced. All32notes in each of4profiles remain byte-identical. All44generated bytes survived each causal run; normal-primary exit changed settings.json only. No pixels/clipboard access. Both attempt caps are exhausted and the campaign is closed. Exact note open/unsaved marker/ReplaceCurrent/All/Undo/Redo/Split/Preview/Reveal/Save/Discard remain NOT_RUN. Full phase hashes, receipts and integrity are in `m4/native-phase-review.json`. No production Notes behavior was changed.

## M5: DFF cancellation — partial native proof, closed harness gap

Five selected tests passed: three completed-response/dialog/action tests in 1.174s and two lifecycle tests in 0.050s. The existing dialog flow additionally refuses a stale renamed alias through real validation before the fake effect adapter. The initial list compilation failed on two local integration errors; corrected listing selected exactly five. All recorded list/test processes retired.

Current smoke build passed in 2m37s; executable SHA256 `10b96de416c658618c372e32b9d97e8cc0ecc311a4ce56e6efd1903c4c0effa0`, compiled on HEAD `2b000ee140af56911267e1f32f8676053f6289b7` plus source diff `14b572306c96fbc23e328846b7df6d824a8c91abf8abce4b2b2ddd678f5e3004`. The original campaign deadline was 18:04:17 UTC and is closed. Each fixture retained eight source paths/thirteen nodes, including supported hardlink and junction cases.

| Phase | Actual result |
|---|---|
| Primary initial receipt | HARNESS_DEFECT: driver demanded another idle frame without an event; zero input, app normal exit0, all four recorded processes Absent |
| One causal correction | Reused a completed eligible initial footer receipt, retaining fresh post-pointer hit and all native ownership guards; fresh fixture and manifest, unchanged Rust/executable/deadline |
| Normal/constrained footer | PASS_NATIVE at normal1196×899 and constrained1000×720 outer geometry |
| Start / first read | PASS_NATIVE: exact current Start Response reached generation1 held-read gate |
| Cancel | PASS_NATIVE: exact current Cancel advanced to generation2, cancelled publication, zero groups, Start disabled while old work was held |
| Release / drain / restart | HARNESS_DEFECT / NOT_RUN: candidate `fixture-release` never promotes because published response key is `release_gate`; guard refused before new input |
| Current groups / Open / Reveal / Copy / explicit worker join | NOT_RUN in native workflow; deterministic fake-effects/stale-alias coverage remains separate |
| Input / retirement / files | Causal mouse2Down/2Up, pending0; normal app exit0, no forced cleanup, all two recorded processes Absent; original paths and bytes unchanged |

No keyboard, desktop pixel, real clipboard or Explorer effects occurred. Normal process retirement is distinct from explicit worker-joined proof. Neither native session proves restart or the later actions. Production DFF behavior was unchanged. Commit `6da17c6d63d62fb190606efaf6c8d4accbbe771f` was pushed.

Primary receipt directory: `target/multi-goal-queue/post-queue-acceptance/m5/run_20261024T174443Z_7d3b5975`; causal: `.../run_20261024T174708Z_9a67d523`; complete classification: `m5/native-phase-review.json`. The causal runner was frozen byte-for-byte (SHA256 `e08f9e00ef3065fbd7ac112e3908bcabefc78eb039d41332e0d11101cc12a1d3`). The primary runner was reconstructed by reversing only the idle-frame correction, verified against its original bound SHA256 `821f1034c84b31e088eaa55e15488e92b3be68004e41c7574d4da297ab95ae7d`; that reconstructed archive was never executed.
## M6: Settings search — terminal harness gap, no native launch

Scoped final listing selected exactly eight tests. Group1 ran five: four passed and the new OFF current-control receipt test failed (1.813s). Group2 passed three command/fixture tests (0.181s). Its one actual causal test recheck, after real result-row reveal, failed the same receipt assertion (1.093s). Initial missing-export compilation, a parent selector module-name correction, and two test tuple-typing compilation failures are retained separately; none is counted as an executed test pass. All recorded processes retired.

The exact actual query/slider/numeric control receipt remained unproved. Both native profiles therefore remain **NOT_RUN**: no native build, input, pixels, clipboard or preference mutation. This is HARNESS_DEFECT, not a demonstrated Settings product defect. All eight attempted source files were archived with full patch SHA256 `ba455e919e318f556e32623ef192bffccd6cde9a5201889dcc876786a968ec6e`, then restored to clean entry HEAD6da17c6d. The partial QA implementation, failing regression and proposed OFF fixture spelling are not retained. Evidence is under `target/multi-goal-queue/post-queue-acceptance/m6/`, especially `attempt-archive.json` and the original group/recheck receipts. Jump ON/OFF numeric edits, Cancel and live persistence require manual review using supported profiles/UI.
## M7: XML fixture synchronization and query timing

Reviewed test-only changes are committed/pushed as `1e2aa4ea9904b513ebf9064e28e2b57a0a6d55ac`. The XML status test now gates the exact fake `items` request before asserting Pending and unchanged revision/tree/output; it releases normally or on unwind. The query oracle retains all fixtures, identities, expected checks, rendering budgets and the10,000-item case, with opt-in payload-free progress outside measured rendering.

Default-feature listing selected exactly5. The standalone oracle passed41.957s; mixed group passed5 in41.269s (oracle41.241s, XMLstatuses0.067s, coalescing0.058s, Settings0.971s, DFF1.192s). All recorded processes retired. All six oracle cases completed preparation, rendering, validation and teardown. At10,000items, validation25.412s/25.721s dominated measured rendering4.199s/4.023s in standalone/mixed. The other mixed tests finished by1.192s, so overlap was limited to the initial phase; these passes do not establish full-suite contention reliability. No application/query algorithm change, fixture reduction, assertion weakening, serialization or timeout/stack increase was made.

The isolated full-suite source is exact commit1e2aa4ea in fresh detached `target/pq-full-261eabc5`. Names-only preflight confirmed all five protected untracked files absent and no ML_/stack/target overrides inherited. Original private clipboard configuration was not read or copied. The first cross-worktree receipt write was denied by the default filesystem sandbox; the checks had completed and authorized persistence subsequently succeeded (`m7/full-preflight.json`). Cold compile/list passed (9m10compile/9m53total,6969entries/all194Absent). One unfiltered default-feature full run finished: **6965passed,3failed,1timed out,48skipped** of6969executed,263.841s, exit100/all134recorded processesAbsent. XML status passed0.748s; oracle timed out60.063s during10000-item validation step7 after continuing progress (step6validation5.582s). No deadlocked application worker was proved. No second full run was performed or justified by a proven new regression. Historical G2 counts remain6,793pass/1XMLfail/1querytimeout/48skipped and are not substituted for current results.
## Full-suite failures and their limits

| Current full-run result | Failure evidence |
|---|---|
| `file_operations::tests::direct_scan_keeps_child_directory_blocker_when_listing_is_denied` | Restricted directory row missing;0.093s |
| `gui::radial_editor::action_editor::tests::live_ranked_editor_search_scrolls_and_assigns_rows_beyond_fifty` | Beyond-50 Pin control not reached;3.989s |
| `gui::universal_action_catalog::tests::acceptance_fixture_queries_reach_real_ranked_actions_and_authoring_rows` | Execute row rank90 versus60;0.724s |
| `gui::render::tests::track_c2g_oracle_query_sequences_match_usable_cpu_frames` | TIMEOUT60.063s during10000-item validation; focused standalone/mixed passed |

The first three files have no task diff against baseline. Independent review found no causal link from the retained M7 fixture/progress changes; baseline/environment attribution remains unproved. They are not called pre-existing or silently repaired. The current full suite is **not green**. All71binary targets were covered at default features/normal parallelism; no filter/exclusion/retry/stack or timeout expansion was used. `m7/full-run/{stderr.log,result.json}` and `m7/full-preflight.json` retain exact commands/counts/process identities. The owned detached test worktree is retained as local evidence; no earlier worktree was modified.

## M8: real brightness inventory NOT_RUN

No existing owned read-only inventory evidence runner or normal GUI session was available. The private helper is a framed service protocol that can Set; no new protocol client was created. Physical monitor support, topology and native min/current/max remain **UNKNOWN**. No hardware inventory, Refresh, Level change, real Set, driver install or display wake was performed. Source is unchanged; historical fake coverage is not hardware proof. Optional user-consented Set/readback/baseline restoration is in the checklist.

## Final review and manual executable provenance

Independent cumulative changed-owner review found no new substantive source issue. M2 and M6 attempts were archived/restored completely; their failing regressions and partial implementations are absent from maintained source. M5's closed alias gap remains unverified, not a product Cancel failure. README/Quick Help were unchanged because this effort corrected fixtures/observations rather than user-visible production behavior.

The manual launcher/fixture are actual retained M4 Cargo artifacts. Compiled HEAD `87c69b85b07385f18f86abdebcb9e5f5408a2cd2` plus source diff `cb31ce080e8d9517b1cda2cb025e94572514ddceb51a95b9f106c9562ed4d970` was committed as M4 source `2b000ee140af56911267e1f32f8676053f6289b7`. Later retained DFF changes are dev/test observation/fixture paths; oracle/XML changes are cfg(test). Independent review confirmed normal runtime behavior is preserved. These are **not executables built from final HEAD**; M6 native build never ran.

| Artifact | Actual path | SHA256 |
|---|---|---|
| Manual launcher | `target/fast-dev/multi_launcher.exe` | `23f51edd6422d581495f5097d12c1745e8b17333726e2920a290ec78ac90cb92` |
| Profile generator | `target/fast-dev/jump_mode_fixture.exe` | `8331aa1a2fc19eaea5f4c10dc3277db218ac3a1f78ffcb9fb21482c8db17daca` |
| Separate DFF fake-effects smoke | `target/fast-dev/duplicate_finder_smoke.exe` | `10b96de416c658618c372e32b9d97e8cc0ecc311a4ce56e6efd1903c4c0effa0` |

Paths are relative to `G:\Repos\rust\Multi_Launcher`; the checklist verifies hashes before launch. The generator has no `phase2-settings-jump-disabled` variant. Settings OFF uses a separate ON-generated profile changed through normal UI before recording its baseline. Raw ignored evidence and binaries are local, not embedded in Git pushes.

## Final gate outcomes

M9 list selected11; group1 PASS6/1.639s and group2 PASS5/1.121s, all recorded processes closed. Nine Rust files passed rustfmt check, retained runner parsed, manual PowerShell blocks parsed, source diff/static review completed. Exact selectors/commands are in the execution ledger and m9 receipts. Both final all-target Clippy configurations PASS on source `f53bb83906548e1dd12ea2cee85949f5b14014fc`: default48.96s compile/all15Absent; dev-smoke-binaries,qa-framebuffer-capture1m56compile/all22Absent, both fast-dev with `-- -D warnings` and owned1800s caps. Initial style diagnostics were fixed equivalently and committed/pushed in8bace0bf/f53bb839 before these final gates. No further code/tests/format/help/dependency changes or code verification followed. Only outcome metadata in the three reports changed afterward.

Manual priorities: shield/click-through; exact file/folder repair; Notes Replace/history/Save; DFF drain/restart/actions/retirement; Settings numeric Cancel in both Jump modes. Real OS live acquisition remains distinct from generated-live proof. Brightness Set is optional and requires explicit consent. See [manual checklist](POST_QUEUE_MANUAL_VERIFICATION.md) and [execution ledger](post_queue_native_acceptance_execution.md).
Final safety/Git metadata: all owned processes closed and accepted native input balanced; forced cleanup versus normal exit/join is retained per phase. All checkpoints through verified source f53bb839 pushed; final metadata-only commit SHA/push is in the handoff. Protected integration/prior-goal branches unchanged; five original untracked files excluded, original clipboard configuration unread/unchanged. No real brightness Set. Final source differs from the full-run source1e2aa4ea only by equivalent dev/test lint syntax plus documentation; no second full run. The retained detached full-test worktree is local evidence, not a new feature branch. Final classification: **BLOCKED_PRODUCT_DEFECT**.