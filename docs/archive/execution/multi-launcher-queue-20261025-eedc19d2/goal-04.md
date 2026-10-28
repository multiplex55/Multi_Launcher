# Goal 04 execution record

Queue: `multi-launcher-queue-20261025-eedc19d2`.
Branch: `codex/windows-link-editor-queue-20261025`.
Source base / verified FINAL03: `6a3d612fcceafe1423341f835cf1f5f2fa317ff5`.

## M0-A baseline

The full Goal04 and Kickstart, AGENTS, master queue and durable ledger were read. The explicit cumulative queue contract selects FINAL03 as BASE04; no integration merge or branch history rewrite was performed. Normal `git fetch origin` succeeded; origin is the intended multiplex55/Multi_Launcher repository and its default branch is master. The new branch name was absent locally and in fetched remote refs. Source checkout and index were clean before branch creation; no pre-existing changes were incorporated.

Goals01-03 are complete and frozen on their own branches; their process/workflow helpers, isolated test conventions and table-core public facade are inherited in this base. Goal04 has no hard predecessor capabilities. No application work, builds, tests or benchmark runs preceded this branch baseline. This record establishes intent only; resulting baseline SHA and actual initial push are recorded externally after the commit.

## Scope and verification boundaries

Execute existing Goal04 checkpoints in order: attributable link/editor baselines; one accepted opt-in link/debug mode; scoped Neovim rust-analyzer configuration; equivalent editor comparison and final guidance. Inspect installed tools and actual configuration before choosing candidates. At most two link candidates, one scoped editor profile and only observed-contention cache comparison. Preserve global settings, trust/security defaults, full-debug and canonical release behavior, resources and production features. Use checked owners with compile1800s, editor300s and post-build smoke120s deadlines; cleanup5s. Controlled source edits require raw-byte backups and an outer restoration guard before the first write. Version/headless output is not interactive editor proof. No full suite is automatic; final appropriate first-party lint occurs after review/corrections and before exact-tree commit.

M1-A baseline evidence is complete and independently accepted. M1-B recovery has completed its app/root measurements and artifact, icon, source-line, selected-test, and corrected native-window checks; checkpoint review and acceptance remain pending. M2-A remains unaccepted and M2-B remains pending. The external queue ledger records planner/implementer/reviewer dispatch, actual evidence, failures, finite repairs, checkpoint identities and push outcomes. No editor acceptance is implied by this baseline.

## M1-A — Independently accepted baseline evidence

Evidence root: `C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2`.

### App and root-library baselines

Discovery recorded Rust/Cargo and rust-analyzer 1.99.1, Cargo Nextest 0.9.135, Neovim 0.12.3, and the native x86_64-pc-windows-msvc target. These baselines use default features and the existing fast-dev line-tables-only profile and shared Cargo target directory; profiles, toolchain, patches, and production features were not changed. The original-source app preparation and no-op were completed before the controlled edit. The source was at the Goal04 baseline (`48946184cb395d8bc501a251dc4ea77b4facf603`) and the exact app command was `cargo build -p multi_launcher --profile fast-dev --bin multi_launcher`.

| Measurement | Result | Evidence |
| --- | --- | --- |
| Original-source app preparation | Exit 0; whole-command wall time 42.428178 s | `g04-m1a/app-preparation/g04-m1a-app-original-preparation_20261025T230946543Z_1ba6c0f274714e9c9ddb1d03aabe6679/measurement.json` |
| Same-source app no-op | Exit 0; whole-command wall time 0.954399 s | `g04-m1a/app-noop/g04-m1a-app-noop-original-source_20261025T231124253Z_31178be646004a0fa510cd07cec4caa2/measurement.json` |
| Controlled app leaf-body edit | Exit 0; whole-command wall time 41.135731 s | `g04-m1a/app-edit/g04-m1a-small-appleafbodyedit_20261025T232246698Z_761f2115c6784cf986aeb5751c54c9cc/measurement/g04-m1a-small-appleafbodyedit_20261025T232251290Z_49cfebb885d24bc8958ee11c9cdaec16/measurement.json` |
| Original-source root-library test-binary preparation | Exit 0; whole-command wall time 52.814020 s; binary listing only | `g04-m1a/root-lib-preparation/g04-m1a-root-lib-test-original-preparation_20261025T231255754Z_cf145bde8ab7480faedb59549d65312c/measurement.json` |
| Selected root-library case inventory | Exit 0; 1 selected test discovered in 1.826538 s; inventory only, no test executed | `g04-m1a/root-lib-case-inventory/g04-m1a-root-lib-selected-case-inventory_20261025T231517951Z_e60e4ee65f6e4162852ccca5b6ce1081/measurement.json` |
| Controlled selected root-library test diagnostic build | Exit 0; whole-command wall time 54.345498 s; no tests executed | `g04-m1a/root-lib-test-edit/g04-m1a-root-lib-test-diagnostic_20261025T232658760Z_f9553afd5f554f7aace9734c04989400/measurement/g04-m1a-selected-root-lib-test-diagnostic_20261025T232702717Z_9c12301fd5b549219d415da5ebd0f381/measurement.json` |

The inventory selected `table_utility_session::tests::table_session_latest_source_wins_across_close_and_reopen` from the root-library test binary, not the small core binary. The controlled app and root-library edits were temporary measurement fixtures, not product changes. The app fixture began from source SHA-256 `6ed3263edebfb4b140b04cb54a66bed17b238c01df0675203fb4efdeeac66d5f`; the app edited fixture was 83,399 bytes with SHA-256 `61be28b6080b85a1a033d0e56772d000966ec7178a75c426a88b8028f41e9d97`. The root-library diagnostic fixture changed only a test diagnostic string. The original bytes were restored from verified backups after both controlled runs, and each outer restoration guard passed.

These are native-process whole-command timings. They do not isolate compilation, linking, or linker-phase time. The installed MSVC linker inspected during discovery was `C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64\link.exe`, file version `14.44.35227.0`. Its help probe returned exit code 1100, although captured help text included `/TIME`; this is retained as a failed probe, not a successful timing measurement. The post-no-op PDB observation does not establish a PDB binding before the no-op or an unchanged-PDB result across that no-op. Exact invoked linker arguments and isolated link/PDB phase duration were not captured. No linker speedup or PDB stability claim is made.

The controlled app edit did bind both sides: its 118,015,488-byte executable changed from SHA-256 `757c78a5…4f43cf` to `bdaf6492…742fa0b`; its 358,395,904-byte PDB changed from `10638b29…038968` to `a24abb6b…7c85e53`. The root-library test binary was 172,524,032 bytes on both sides (`bd3346c4…f55886a` to `1d1ae2a8…8cfa858`); its PDB changed from 482,775,040 to 482,783,232 bytes (`bb096dab…5b5e8eb` to `29c1ebf4…2aec0f`). Full hashes and paths are in each controlled driver's result. Restoring source did not restore those diagnostic-built outputs. The later coexistence build rebuilt the app with original source; the test executable remains fixture-built and must be rebound before a future original-source comparison.

### Actual Neovim/LSP baseline and coexistence

The session used installed Neovim at `C:\Program Files\Neovim\bin\nvim.exe` with the user's installed Rustaceanvim v5 integration (commit `e9c5aaba16fead831379d5f44617547a90b913c7`). Its observed profile is `no_checks`, which disables check/diagnostic work and related Rust-analyzer features in this setup. No global configuration, trust setting, installation, or user configuration was changed. Thus edit-to-diagnostic behavior was unavailable and no diagnostic-latency claim is supported. The RPC-attached session establishes actual editor/LSP activity; no GUI paint or visual-render latency was measured.

The first owned UI-attached session initialized rust-analyzer, observed its Rust `LspAttach` at 2.319 s, and completed a reversible unsaved-buffer edit/restore: changedtick advanced 2→3→4, the original modified state was restored, no source file was written, and the source disk hash remained unchanged. A definition-capable client was present, but the first semantic request failed to produce an applicable definition in 15.795 s. This session did not establish indexing readiness. Its result is preserved at `g04-m1a/editor-observer/owned-neovim-lsp-rpc-20261026/editor-observation.json`.

One distinct editor-validation build was run while that first live editor session was open. It exited 0 in 76.021845 s, with output drain and cleanup verified. The owned Neovim process (PID 15156, creation time `2026-10-25T23:40:11.021603Z`) overlapped the build interval `23:40:40.059Z`–`23:41:56.080Z`; the parent independently reconciled the overlap. This is a build-coexistence observation, not an editor-responsiveness result. The receipt is `g04-m1a/editor-observer/owned-neovim-lsp-rpc-20261026/coexistence-build-complete.json`.

The first semantic-only retry timed out after 188.344 s in a synchronous Neovim RPC wait and produced neither a quiescent marker nor a semantic result. It is preserved as failed evidence; it does not establish that the language server never became quiescent. The final bounded semantic retry used an asynchronous definition callback and captured the actual Rustaceanvim `experimental/serverStatus` quiescent event before chaining the installed handler. In that separate owned UI-attached session, the observer-clock quiescent event occurred at 6.0236924 s, the matching Rust `LspAttach` event at 884.0712 ms, and the single accepted definition request (ID 13) returned one location without an LSP error 31.4616624 s after request. The response selected `source_is_inline_editable` at line 711, characters 41–66, and returned target selection range line 1550, characters 3–28. The attach and quiescent event times use the Lua observer's setup clock; 31.4616624 s uses the distinct request-to-callback interval and is not project-open readiness or indexing latency.

The returned source and target URIs differed only in the case of the Windows drive letter (`G:` versus `g:`). The observer's strict ordinal URI comparison therefore recorded `applicable=false`, `semanticReady=false`, and the raw owned run ended with overall `failed`/exit 2. These raw fields and status remain unchanged. The parent separately performed read-only Windows file-identity adjudication: the paths resolve to the same existing file, the selected source SHA-256 matches `6ed3263edebfb4b140b04cb54a66bed17b238c01df0675203fb4efdeeac66d5f`, and the returned selection range identifies `source_is_inline_editable`. The final observation and adjudication are under `g04-m1a/editor-observer/owned-neovim-semantic-quiescent-retry2-20261026/`; that directory's `parent-windows-file-identity-adjudication.json` records the read-only check. It supports the returned definition's identity but does not rewrite the observer receipt or erase its strict-comparison failure. The Neovim child exited 0 after requested `qa!`; RPC EOF was expected, output pumps joined with zero bytes, cleanup/drain passed, source hash was unchanged, and the environment override was absent both before and after and exactly restored. Diagnostic events remained unavailable under the actual `no_checks` profile.

### Preservation, process cleanup, and disposition

The exact final repair-2 controller, runner, wire helper, and self-check inputs were snapshotted before execution under `g04-m1a/editor-observer/approved-repair2-inputs-20261026`; the manifest SHA-256 is `224d222a79f51f4d96bc38cedde6abdff7784c16ed847740479e7f6b8aed6c4a`. The previously executed repair-1 controller and runner were recovered byte-for-byte from the recorded tool output, hash-verified, and preserved under `g04-m1a/editor-observer/recovered-repair1-used-inputs` with recovery provenance. Original receipts and failed attempt records remain intact.

All checked build and editor owners reported captured output, joined output drains, and cleanup. The parent independently audited the final semantic-retry owner receipt: 140 identities were checked after cleanup; 139 were absent, one PID was reused by a different identity, and zero of the recorded identities remained alive. The first-session/build overlap and final retry process identities have separate preserved audits. The earlier semantic timeout's 261 identities were also independently verified absent. The untracked `nvim.log` observed during this milestone was left untouched.

**Disposition:** The configured independent Reviewer accepted M1-A without substantive findings, including the actual same-file semantic response adjudication. Review evidence is `g04-m1a/reviewer-review.md` beneath the queue evidence root. Parent checkpoint commit and actual push identities are recorded externally. At the time of M1-A acceptance, M1-B and later checkpoints were pending; see the updated dispositions below. The raw failed retry and strict URI-comparison result remain preserved alongside the parent’s read-only file-identity adjudication. No speed-improvement, diagnostic-latency, isolated linker-time, or unchanged-PDB claim is made.

The initial direct version probes had an ownership-recording gap, and one early checked discovery serializer failed before its receipt was saved. Their available outputs and limitations were retained; a corrected consolidated checked discovery supplied the authoritative seven-probe record (six successful version probes and the nonzero linker-help probe). No earlier gap is relabeled as a checked pass. The parent separately verified all recorded build/discovery, editor/coexistence and semantic-attempt process identities retired, including PID reuse distinctions. No further M1-A semantic repair runs are authorized under that exhausted two-repair boundary; the actual returned definition was accepted from the preserved final response by file and symbol identity.

## M1-B — Verification complete; checkpoint review pending

The initial M1-B app-preview sequence was blocked before candidate Cargo compilation, and its finite preview repair allowance was exhausted. Those historical failures remain recorded below. A later, narrowly authorized recovery continued the same M1-B checkpoint through candidate measurements and post-build checks. The first bounded default-feature native-app smoke did not satisfy its actual-window and normal-close criteria; its failure is preserved below. A focused correction to the new native verification boundary passed once under the ordinary remedy policy. All M1-B verification gates are complete; independent checkpoint review and acceptance remain pending.

### Initial app-preview failures (preserved)

The exact boundary and preserved receipts are recorded in the external
`g04-m1b/BLOCKED_RESUME.md`. In brief, r05 stopped before `cargo.ps1` because
StrictMode read optional `ExternalPath` from a normal manifest record where
that property was absent (exit 1, 1.677078 s, stdout 0/stderr 797 bytes,
cleanup and output drain verified, five identities). One property-safe
correction produced r06, which stopped before `cargo.ps1` because
`support.Cargo` is a path string but the preview assumed `support.Cargo.Sha256`
(exit 1, 2.092646 s, stdout 0/stderr 791 bytes, cleanup and output drain
verified, fourteen identities). The preserved inner compile record is
`NOT_STARTED`: no stdout/stderr artifacts, and inner Exit/Wall/OutputBytes are
null with raw cleanup/drain false. An earlier r04 outer owner failed after
5.749684 s with exit 1 while outer cleanup/drain were verified; that does not
change the inner `NOT_STARTED` result.

The r05 inner result is
`g04-m1b/app-preview/runs/app-preview-20261026T005324293Z-c31b27fe020e433794be564595f337f1/app-preview-result.json`;
the r06 inner result is
`g04-m1b/app-preview/runs/app-preview-20261026T005411686Z-259ea32921e84c3fb72322b96a8613e7/app-preview-result.json`.
Their owner receipts are
`g04-m1b/app-preview/outer-owner/r05/g04-m1b-forwarding-app-preview-r05-owner.json`
and `g04-m1b/app-preview/outer-owner/r06/g04-m1b-forwarding-app-preview-r06-owner.json`.
At the original stop, the three partial M1-B files were recorded at these
historical hashes; the authorized recovery later changed their contents, so
these are not current-file hash claims: `dev/track_b/cargo.ps1`
`50EEB64CC564D4531138D7C86EC0FE96317AC393C21AA1419D3429A3FA963007`,
`dev/workflows/test_run.ps1`
`4992302849112B95FFA7DCA767E9EBC38E2BD467FE554D64E48AFA4732BF0339`, and
`dev/track_b/linker_mode.ps1`
`3BAE68E3004A617FF643DF0355F2EBC3B13F92339A2233FEADC6E91BF00D3D4D`.
The untracked `nvim.log` remains untouched and empty
(`E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855`).
No M1-B retry or candidate compilation was run during M2-A's original bounded
probe phase.

### Recovery continuation (2026-10-26)

Original-source app preparation measured 270.374517 s and original-source
root-library test-binary preparation measured 250.240402 s; both are excluded
from the edit comparisons. The exact M1-A app and root-library small-edit
fixtures were each compiled once, then restored through their outside-job
guards. The edited app build measured 36.693609 s versus its 41.135731 s M1-A
baseline (10.7986947% lower whole-command time). The edited root-library
diagnostic build measured 47.465428 s versus its 54.345498 s baseline
(12.659871% lower whole-command time). These are single-pair, host-specific
Cargo command observations; they do not isolate linker time. The original
source was restored exactly, and the candidate executables and PDBs were bound
by path, length, hash, and matching PE/PDB identity in the preserved evidence.
The measured candidate app executable/PDB were 118,012,928 and 366,055,424
bytes; the root-library test executable/PDB were 172,518,400 and 492,756,992
bytes. These are artifact sizes, not a measured total Cargo-cache footprint.

The candidate app's icon-resource assertion passed against the bound app
binary. The focused DbgHelp checks matched each executable's PE CodeView GUID
and age to its PDB and resolved exactly one requested source line: app fixture
line 1553 and root-library fixture line 1642. Symbol cleanup and DbgHelp library
release succeeded for both probes. The exact selected root-library test was
found as the sole nonignored inventory item and passed once (native duration
0.864068 s; case duration 0.0243429 s). The shared structured-results
aggregate remained `Complete=false` because its libtest JSON bookkeeping
classified 46 ignored/mismatch starts as unrecognized; the parent preserved
that parser limitation and independently reconciled those records with the
inventory, the target's started/ok events, all 47 suite terminal events, and
the one passed/zero failed case. No parser change or rerun was made.

The app comparison's controller-exit-code field was not captured, although its
checked worker and Cargo both exited 0 and its external restoration guard
verified exact source and worktree restoration; this gap was preserved without
repeating the measurement. The root comparison recorded controller exit 0.
The parent independently verified retirement of the recorded process
identities for both comparisons and the icon/selected-test runs. The separate
PDB probes and icon/test receipts are under the external
`g04-recovery-20261026/m1b-acceptance-r04/` evidence directory; app/root
comparison details and restoration receipts are retained beside them.

The first bounded native default-feature app smoke (native-r01) failed its
actual-root-window and graceful-close criteria after 16.763944 s. PowerShell's
`Process.MainWindowHandle` identified visible auxiliary HWND `0x42513cc` for
the exact app PID 15448 (creation `2026-10-26T13:05:52.7985016Z`); the handle
was responsive but its client area was only 16x16. The capture was unavailable
because the candidate was not the foreground window, and no unrelated desktop
pixels were collected. `CloseMainWindow()` returned true for that auxiliary
window, but the app did not exit normally; forced cleanup was used, the process
exited -1, and the exact PID/creation identity was absent afterward. Raw output
drains and owned cleanup were verified: app stdout 0/0 bytes, stderr 452/452;
fixture-generator stdout 2,770/2,770 and stderr 0/0. App startup log output
continued after the auxiliary window was first observed. The preserved result
and original helper snapshot are under
`g04-recovery-20261026/m1b-acceptance-r04/native-r01/`.

The focused correction enumerated top-level windows for the exact app PID and
selected the actual `Multi Lnchr` launcher root with 900x560 client bounds. It
recorded each observed window's class, title, and bounds without treating an
unverified class string as an acceptance filter. The selected root responded
to WM_NULL; the checker revalidated that HWND and sent normal WM_CLOSE, after
which the app exited 0 without forced termination. The bounded owner completed
in 5.796434 s with exit 0, cleanup and output drains verified, and all five
recorded process identities retired. Generated settings contained both
Next/Cancel shortcut properties explicitly set to null before app
construction. The inspected 916x599 capture shows the launcher title, icon,
search field, and generated notes dashboard, with no Paste Sequence error
toast. The result and capture are under
`g04-recovery-20261026/m1b-acceptance-r04/native-r03/`; the initial failed
native-r01 run and unused r02 revision remain preserved.

All M1-B verification gates are now complete. The single-pair timings above do
not establish a generally reproducible speedup or isolated linker improvement.
M1-B awaits checkpoint review and acceptance; no commit or push is claimed in
this worker report.

## M2-A — Adapter candidate remains unaccepted

The experimental adapter and focused self-check are present at
`dev/editor/rust_analyzer_profile.lua` and
`dev/editor/test_rust_analyzer_profile.lua`. Their final file hashes are
`9725EBC0E227F87CEF786765D1C0BDD6C14DCF2E8ED3B2315D62CD6C0B1B6B00`
(15,022 bytes) and
`03FCCAEC1A495E8FD1610CEC1173EA12E35CD977E1286BD0291A77EC8BC2EF51`
(13,322 bytes). They are candidate-only; their existence does not establish
accepted profile settings or an editor workflow.

The first headless self-check was invoked directly, not through the checked
owner:

~~~powershell
& 'C:\Program Files\Neovim\bin\nvim.exe' --headless -u NONE -l 'dev/editor/test_rust_analyzer_profile.lua'
~~~

Its first input snapshot is `g04-m2a/probe-inputs-0` (manifest SHA-256
`AFEC6F568853EEAE3D8F21C17A5ADE4EC901BD60D92C65DAEABF1FED0D2DBB57`). The
raw console output and process receipt were not retained. Only a summary of the
failure labels remains; the exact exit code, PID/creation identity, output
drain, and cleanup state are unknown and are not claimed as verified.

Two subsequent focused self-check attempts used the checked owner with a
60-second deadline and five-second cleanup grace. The first (`repair1`) used
snapshot `g04-m2a/probe-inputs-2`, manifest SHA-256
`C8A8B7E740D15A88CDD4BD50D2CE4F1BF1D3ED2569DACB2D6D2E2961C5D692AE`; its
receipt is `g04-m2a/selfcheck-repair1-checked/result.json`. Neovim exited 1
after 0.403582 s; stdout was 0/0 bytes and stderr was read/recorded as
551/551 bytes. Cleanup and output drain were verified. The preserved stderr
identified `E95: Buffer with this name already exists` while setting a second
temporary buffer to the same path. Recorded process identities were PID 8320
(`nvim.exe`, creation `2026-10-26T01:19:46.6470956Z`) and PID 16948 (owned
process, creation `2026-10-26T01:19:46.6709933Z`); both were absent after
cleanup.

The second and final permitted self-check (`repair2`) used snapshot
`g04-m2a/probe-inputs-3`, manifest SHA-256
`DB3FBA29573AE2CC28BDA2675AF3F8351EC8168A42C3E99EEC54DC2B8904F9D3`; its
receipt is `g04-m2a/selfcheck-repair2-checked/result.json`. Neovim exited 1
after 0.313109 s; stdout was 0/0 bytes and stderr was read/recorded as
195/195 bytes. Cleanup and output drain were verified. Exact assertions that
failed were `matching-root pre-start admission delegates to Rustaceanvim
default` and `cached hook delegates to Rustaceanvim default after reversion`.
Recorded process identities were PID 5688 (`nvim.exe`, creation
`2026-10-26T01:20:22.9420406Z`) and PID 29128 (owned process, creation
`2026-10-26T01:20:22.9638551Z`); both were absent after cleanup. The two-repair
boundary is exhausted; no further self-check or code correction was performed.

For both checked attempts, `NVIM_LOG_FILE` pointed to the corresponding
external evidence directory and its prior process value was restored. Frozen
M1-B file hashes and the empty `nvim.log` hash matched before/after execution.
The original `src/table_utility_session.rs` remained 83,352 bytes with SHA-256
`6ed3263edebfb4b140b04cb54a66bed17b238c01df0675203fb4efdeeac66d5f`; no
temporary Rust fixture was written.

No UI-attached Rustaceanvim/rust-analyzer session was run. Consequently there
is no accepted effective client configuration, compiler error/clearance,
applicable semantic response, or actual foreign-workspace refusal/reversion.
No workspace/all-target Cargo check was accepted as M2-A adapter evidence. A
separate, direct Cargo command was run once after the self-check boundary was
closed; its command-level result is recorded below, but it does not establish
adapter activation or effective server command/settings.

The one independently authorized full-check command used Cargo defaults and
did not invoke the frozen Track B Cargo wrapper, linker helper, or an editor:

~~~text
C:\Users\Jay\.cargo\bin\cargo.exe check --manifest-path G:\Repos\rust\Multi_Launcher\Cargo.toml --workspace --all-targets --message-format=json
~~~

Its read-only pre-run snapshot is `g04-m2a/probe-inputs-4-fullcheck/input-manifest.json`
(SHA-256 `ED66CBA4EA45B7150EEDAEB496E4EA29337831E0394D2CEE90C0F4A4935D589A`).
The checked measurement is
`g04-m2a/direct-fullcheck-checked/g04-m2a-direct-full-test-cargo-check_20261026T012531722Z_ffa2d6df3a20415f816e16f8a7975df3/measurement.json`.
The parsed target proof is at
`g04-m2a/direct-fullcheck-checked/cargo-json-target-observation.json` (the
parent of the measurement run directory).
It exited 0 in 20.431543 s (start `2026-10-25T21:25:36.2691679-04:00`, end
`2026-10-25T21:25:56.700725-04:00`), within its 1,800-second deadline; output
drain and cleanup passed. Captured output was stdout 454,392/454,392 bytes and
stderr 73/73 bytes. The verified MSVC auxiliary-process preflight passed.
The verified `vctip.exe` identity had SHA-256
`dfaee6bdd1d491a73a2fd5a04c2d8b19764967eade20be84fd2f3c882a8cee48` and a
Microsoft signature. The Cargo owner recorded PID 31592 (`cargo.exe`, creation
`2026-10-26T01:25:36.2788367Z`) and owned-process PIDs 16620 and 20568
(creation `2026-10-26T01:25:36.2850577Z` and
`2026-10-26T01:25:36.313775Z`); all three identities were absent after
cleanup. The auxiliary preflight recorded PID 24800 (`pwsh.exe`, creation
`2026-10-26T01:25:34.6134203Z`) and owned-process PID 34632 (creation
`2026-10-26T01:25:34.6212957Z`); both were absent after cleanup.
Cargo JSON reported 532 compiler artifacts, 47 build-script events, 11
compiler messages and a successful build-finished event; 516 distinct target
identities were observed, 74 workspace-local. This command successfully checked workspace all-targets, including test and
bench targets, without executing tests. All 532 compiler-artifact events
reported fresh=true; no fresh compilation is claimed. Its
successful direct result is not evidence that the adapter's configured editor
command or effective client settings work.

The bounded parent fallback review at `g04-m2a/parent-partial-review.md` was
not an independent review or acceptance. It noted that the current nearest-
Cargo.toml admission check excludes the declared table-core workspace member,
the foreign-root fixture uses a hardcoded unrelated checkout instead of an
owned temporary project, and the two default-admission assertions fail with
the current scratch-buffer fixtures. These are source/fixture review findings,
not additional runtime results; no code or assertion changes followed. The
same partial review noted M1-B portability/contract concerns around accepted
case variants and the line-oriented Cargo TOML scan; these do not explain its
preserved pre-compilation schema failures or establish runtime behavior.

The test self-check failures do not establish the missing runtime behaviors.

## M2-B and Goal04 disposition

M2-B remains pending because the M1-B checkpoint is awaiting review and the
M2-A editor-profile checkpoint is unaccepted. No comparative benchmark,
editor speedup/no-gain result, or final Clippy run was performed for these
candidate checkpoints. No Goal04 success commit or push was created by this
work. Goal05 remains not started pending Goal04 completion.
