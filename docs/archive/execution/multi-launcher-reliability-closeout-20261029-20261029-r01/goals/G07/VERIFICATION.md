# G07 verification

Baseline docs only; no Cargo/native archive probe. Source checkpoint caps1200s preparation,180s focusedrun (300 only explicit boundedZIPresource need),600s aggregateincremental,1–2 actualerrorreruns. Prefer libNextestfastdevofflinelocked/nonzeroselection. No fullbuild/suite/install/broadlint. Nativefilesystem proof is distinct from nativeGUI discovery; earliergoal errors are not blanketG07 evidence. Owned synthetic archives/output roots only.

## M1-A preparation history (2026-10-30)

Base `3f6408d672fc975279aa9cc75654af6baf456363`; source identity and exact commands/toolchain are in the [initial receipt](roles/implementer/verification/prepare-initial.receipt.json). Initial list preparation ran 05:09:27.6433160Z–05:12:18.4618277Z, 170.815s of its 1200s cap, exit101. The [complete first diagnostic](roles/implementer/verification/prepare-initial.stderr.log) records E0277 at new native owner Debug derives: existing `OpenDirectory<File>` and `OpenedObject<File>` do not implement Debug. The scoped correction implements opaque formatting on the two new owners; no shared native type changed.

Correction preparation began 05:12:32.1847729Z with the 600s aggregate correction allowance. Before any test run, source review identified a fixture false-positive risk: holding a read-only winner handle across the collision attempt could independently deny replacement. The final fixture must drop that handle before the attempt and re-observe identity afterward. An attempted cancellation targeted only the started receipt's PID15388 (`C:\Users\Jay\.cargo\bin\cargo.exe`, process name rustup, matching 05:12:32 start). The identity check first refused the unexpected process name; after matching executable path/start, `taskkill /PID 15388 /T /F` returned `ERROR: Access denied`. No source edit or other process target followed; the original runner is draining normally, and its actual elapsed time counts against the unchanged600s aggregate. No mixed-source executable will be run.

Parent approved existing `--features perf-harness` solely for the fixture's already-declared Windows IO binding. Test-only junction-conversion helpers/native cases are gated by this feature; normal production adapter code is not. No manifest, dependency or policy change. The final disposition is recorded below; prior compilation alone is not native proof.

## Final selected evidence and stop

**FAILED_VERIFICATION / all M1-A source UNACCEPTED.** Final selected result10/12 passing, with two mandatory native cases failing. No timeout or budget renewal. Parent approved the material-gate stop;188.694s of correction-preparation allowance remains unused. No further source, Cargo, probe, writer or UI work followed.

| Phase (UTC 2026-10-30) | Start–end | Elapsed / cap seconds | Cargo exit and result |
|---|---|---|---|
| initial preparation | 05:09:27.6433160–05:12:18.4618277 | 170.815 /1200 |101; two local E0277 errors |
| opaque Debug correction | 05:12:32.1847729–05:16:31.1911939 |239.002 /600 aggregate |0; drained normally after denied cancellation |
| corrected collision fixture preparation |05:17:21.7001570–05:18:49.1758573 |87.472 /360 remaining cap |0;12 selected |
| first focused execution |05:19:37.2720090–05:19:40.6540103 |3.378 /180 |100;8 pass,4 fail |
| native isolation/fixture preparation |05:23:24.7151726–05:24:49.5510139 |84.832 /273 remaining cap |0;12 selected |
| corrected focused execution |05:25:30.2753474–05:25:33.7709252 |3.492 /180 |100;10 pass,2 fail |

Incremental preparation239.002+87.472+84.832 =411.306/600s. The first actual run and one localized correction run both used the same12-case selection. [Selected inventory](roles/implementer/verification/selected-tests.json): six `g07_` tests (four preparation and two native) plus the exact six planner controls, zero ignored;6450 unselected tests were skipped. No domain target, binary, broad suite, lint, benchmark, install or GUI/app fixture ran.

Exact complete invocations, feature set, toolchain, HEAD and source/manifest blobs are in every `*.receipt.json`, specifically [final preparation](roles/implementer/verification/prepare-native-isolation.receipt.json) and [final execution](roles/implementer/verification/run-native-isolation.receipt.json). Command shape:

```text
cargo nextest list -p multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked -E "<exact 12-case filter in receipt>" --message-format json
cargo nextest run -p multi_launcher --lib --features perf-harness --cargo-profile fast-dev --offline --locked -E "<same filter>" --retries 0 --no-fail-fast --status-level pass --final-status-level fail
```

Versions: cargo1.99.1 `5f94df478`; rustc1.99.1 `58cf8f9c3`; cargo-nextest0.9.135 `610eefb88762529a316373f4a50f5fd9194c3c35`, x86_64-pc-windows-msvc. The actual source snapshot is [SOURCE_REVALIDATION](SOURCE_REVALIDATION.md); no source changed after final execution. Raw all-test inventories and orchestration script remain under ignored `target/closeout-g07-m1a-20261030-r01`; [artifact hashes/sizes](roles/implementer/verification/ignored-artifacts.json) preserve them without tracking the unselected dump.

### Preserved failures and scoped corrections

The [first execution log](roles/implementer/verification/run-focused.stderr.log) preserves all four original failures. The source/native fixtures constructed paths with a forward-slash joined suffix, which is invalid after the existing native helper adds the extended Windows prefix; they failedOS123. Component-by-component fixture joins corrected spelling without altering native path policy. The out-of-region local-offset fixture failed inside the existing catalog as `Malformed(InvalidEntryRecord)`; the test now asserts that earlier refusal rather than assuming catalog admission. Assertions remain fail-closed. A separate before-run fixture correction released the read-only winner diagnostic handle before the collision attempt, so that handle cannot independently supply no-replace protection.

The [final execution log](roles/implementer/verification/run-native-isolation.stderr.log) preserves both remaining material failures:

- `g07_native_source_holding_and_ancestor_conversion_are_fenced`: `fs::remove_file(spelling).is_err()` failed at source line9653 in the ordered source/alias loop. Exact failing spelling is UNKNOWN. Successful observations before it: unguarded in-place junction conversion on the same native object identity; refusal of the preexisting reparse directory; hard-link creation; existing-writer admission refusal; ordinary source admission; write/truncate refusal for the current loop entry. No post-delete binding/bytes or subsequent source-parent mutation result is claimed.
- `g07_native_stages_publish_no_replace_and_retire_exact_ownership`: ordinary guarded publication at line9730 fails in `SetFileInformationByHandle(FileRenameInfo, no replace)` with rawOS32, “The process cannot access the file because it is being used by another process.” Ordinary destination admission, tested parent replacement/conversion refusal, stage creation/identity and fixed-byte write/flush/sync occurred before the failure. A test-only zero-byte control stage with the same exclusive creation access/sharing under the existing weaker directory policy returns `Ok(())` and is removed by its held handle; both control names are absent afterward. This is diagnostic comparison, not production authority or a claim about the exact kernel mechanism. The source assertion still requires guarded publication to succeed. No relative RootDirectory retry or guard relaxation occurred.

Collision, final payload/identity, pending-deletion and final stage-lifecycle assertions after ordinary publication are NOT_REACHED, not passed. The four preparation tests did pass: physical IDs/ranges without payload reads; exact headers/range/unsupported refusal; captured tighter quotas; actual stage cleanup after cancellation and injected second-create failure, preserving a preexisting sentinel. The six read-only/native-default controls passed. This does not accept an extraction writer.

Warning ownership: new preparation source uses `AtomicU64::fetch_update`, deprecated as renamed by this Rust toolchain (retained at stop). Existing `table_utility_session` unused variable is unrelated. Existing dependency warnings: epaint1, egui1, image9 and table-core3. No warnings were muted and no unrelated warning fixes were made.

Cleanup: [receipt](roles/implementer/verification/cleanup.json) at05:27:17.1599674Z shows zero entries in exact owned `G:\Repos\rust\Multi_Launcher\target\productivity_expansion\archive_native` and no Cargo/rustc/test processes. Denied taskkill targeted only the matching task-owned process; it exited normally. All native fixtures were confined to that root and retired through RAII/held-file cleanup; no caller output or app fixture. Static diff/path/link checks are recorded separately. See [bounded stop](stalls/M1-A-20261030T052717Z.md) and the complete [continuation Goal](roles/implementer/follow-on/native-guard-publication-compatibility/GOAL.md)/[Kickstart](roles/implementer/follow-on/native-guard-publication-compatibility/KICKSTART.md).

[Static checks](roles/implementer/verification/static-checks.json): `git diff --check` exit0, nine owned Markdown files with no broken links, zero final tested-source/manifest blob mismatches. Changed-path inspection found only the three scoped source files, own/shared G07 records and mutable original Goal, plus the separately preserved parent orchestrator record. `publish_fixture` remains cfg(test); no normal publication caller was introduced. No further source mutation or test command after the bounded stop.
