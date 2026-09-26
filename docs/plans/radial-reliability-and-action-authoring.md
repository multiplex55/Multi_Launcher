# Radial reliability and action authoring — execution ledger

This is a mutable execution ledger. Statuses began pending; each change below records actual progress. Creating or updating this file is not application implementation or validation.

## Authority

Starting archive: `multi_launcher(20260924-210943).zip`

Archive SHA-256: `f21ca0ee92d1b7572f779b936bb3ce1de7e685314f685ae8711d1734952d2544`

Approved behavior: `../multi_launcher_radial_reliability_approved_requirements.md`

Implementation tasks: `../multi_launcher_radial_reliability_codex_plan.md`

Acceptance: `../multi_launcher_radial_reliability_acceptance_matrix.md`

Source evidence: `../multi_launcher_radial_reliability_source_notes.md`

Critical override of older plans: a short launcher tap toggles the grid AND dismisses the runtime radial. Designer and its draft remain independent.

## Baseline identity — fill from actual checkout/tools

| Field | Actual value |
|---|---|
| Branch / starting HEAD | `radial-menu-3` / `f6d0395f5b7cee987b09f1dd11f52361ba74fcf5`; upstream `origin/radial-menu-3`, ahead/behind 0/0 |
| Dirty diff / source manifest identity | Clean tracked and untracked tree before M0; empty staged/unstaged diff. M0 documentation move/update is recorded below and must not be confused with application source. |
| Archive comparison / intervening changes | Archive is identified by name and declared SHA-256 above but was unavailable in the supplied attachment directory and inspected repository/user locations. The source notes' 21 recorded fingerprints match only `AGENTS.md` and `src/dashboard/widgets/quick_tools.rs`; 19 files differ in bytes, including Cargo files and source/runner files. EOL differences may contribute; direct ZIP comparison and semantic attribution remain unavailable. Current checkout is the implementation source of truth. |
| Historical branch point / first feature commit (optional, verified only) | Verified `git merge-base HEAD origin/radial-menu-2` = `39cb72e251799a9c794e95f4e329f4dc88913c9b`; sole subsequent commit before M0 is `f6d0395f docs`, adding this requirements packet. Comparison metadata only. |
| Toolchain / Cargo / Nextest versions | `rustc 1.97.1 (8bab26f4f 2026-07-14)` x86_64-pc-windows-msvc; `cargo 1.97.1`; `cargo-nextest 0.9.135` |
| Target / profile / target directory | Single package `multi_launcher 0.1.0`, edition 2024; default Windows MSVC target; target directory `G:\Repos\rust\Multi_Launcher\target`. No build profile selected for M0. |
| Candidate application path / SHA-256 | No M0 source-matched candidate built. Existing `target\debug\multi_launcher.exe`: `3daa5c82d05315109ed22786dd56e2aa048963f2a3f45a9b5db872fec07c39ec`; existing release app: `d3ee35ea948d98f4a59cd7cd821a6da8fd6f3676706c94824f544608f83973f6`. Existing artifacts are not acceptance evidence. |
| Acceptance runner path / SHA-256 | No M0 source-matched runner built. Existing debug runner: `0d60b994dd40feefbd9c1c4438e99c27e8c5ecc187ed2eb596bdb22a9f49e2b4`; existing release runner: `e33a5f79001234e16b70074b132f60d08e0d26663d3e7e1c8e4de7454b08fafd`. |
| Build command and source-to-artifact record | None for M0; no existing acceptance report established as source matched. |
| Native environment / monitor-DPI / integrity | Windows 10.0.19045 Home 64-bit, interactive session reported. Monitor/DPI, process integrity, input desktop, and injection eligibility await native preflight; topology query was access denied. |
| Fixture / profile hashes | Pending M1 source-matched fixture and isolated profile. Current runner fixture configures F11. |
| Authorized consistent copied profile available | Not established; optional copied-profile coverage remains conditional. |

## Milestones

| ID | Scope | Code status | Verification status | Commit/diff | Evidence / blocker |
|---|---|---|---|---|---|
| M0 | Source baseline and fixture/test map | complete | complete (metadata inspection) | `8c09d29b` | Direct ZIP unavailable; byte fingerprint discrepancy and native preflight limits recorded below. |
| M1 | Hotkey/grid/runtime reliability and exact-chord runner | complete | complete: Gate H, full Nextest, doctests, format and diff checks | starts at `8c09d29b`; candidate 24 source patch `d928edb8f050565aa30a8143c09c8140da0ada459810a7ebdb72accbcf56a46a` | Candidate 24 reports 6–7 pass all H cases, CLEANUP, R0 with typed latency and corrected expected fields; independent review finding resolved. Full Nextest 4,872/4,872 passed (8 skipped); doctests 0/0 passed. |
| M2 | Typed bindings and persistence migration | pending | pending | — | — |
| M3 | Shared query resolution and execution/handoff | pending | pending | — | — |
| M4 | Shared authoring editor and Add to radial | pending | pending | — | — |
| M5 | Multi-select/bulk editing/navigation | pending | pending | — | — |
| M6 | Presets/gallery/simple controls/density | pending | pending | — | — |
| M7 | Full regression/native acceptance/review | pending | pending | — | — |

Use `pending`, `in_progress`, `code_complete`, `complete`, or `blocked` explicitly. A milestone is `complete` only when its required acceptance passed. Record a known failure separately from unavailable environment evidence.

## Gates

| Gate | Meaning | Status | Exact source/candidate identity | Report/exit/counts |
|---|---|---|---|---|
| H | Rapid exact-chord native hotkey reliability | complete | Candidate 24: HEAD `8c09d29b32057fff7a4de7fd9b629c9d22e6ab69` + source patch `d928edb8f050565aa30a8143c09c8140da0ada459810a7ebdb72accbcf56a46a`; app `83601d4332110407b8fe22baa0fc6c5ea20846540fd696746bef966ba8d5f55d`; runner `9c2756fcd29941e79411d2da6146169e24e9370f899630c9f4d2e38daf5a2c9e` | `target/radial-reliability-m1-20260925-candidate24/hotkey-exact-{6,7}/report.json` both exit 0: all 13 H cases, CLEANUP, R0; H04 hidden+visible 1/2/5/10/25, 86 unique decisions, typed event/latency evidence, no pointer movement or intertap wait/refocus; exact chord and F11 control. Earlier candidate 24 attempts 1–3 failed documented external cursor movement, 4–5 failed single measured 101–102 ms H04 gaps; no failure was relabeled as passing. |
| P | Model/migration/store/package compatibility | pending | — | — |
| Q | Same-ranked query resolution and correct execution | pending | — | — |
| C | Core editor and insertion, H/Q rerun | pending | — | — |
| D | Bulk/nav usability | pending | — | — |
| S | Skin/geometry/appearance compatibility | pending | — | — |
| R | Final full suite/native/review | pending | — | — |

## Long-running jobs

| Job ID/PID | Command + working directory | Source identity | Started | Log/metadata path | Last observed state | Exit code |
|---|---|---|---|---|---|---|
| M1 candidate build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + source-code patch SHA-256 `d8fef9516b9c8a99ee6823d79d5f312375e797f3792dd05fa810b543cfd7ebf6` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-033629/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 exact 1 | source-matched runner `--suite hotkey --hotkey shift-alt-win-end --mouse-gestures enabled` (sandboxed) | same candidate | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-033629/hotkey-exact-1/report.json` and `.meta.txt` | failed: SendInput access denied; case artifacts saturated report cap | 1 |
| M1 exact 2 | same runner and flags, interactive-desktop sandbox escalation | same candidate | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-033629/hotkey-exact-2/report.json` and `.meta.txt` | failed: H08 passed; H04/H01/H02/H06/H07/H09 and later cases failed, some due runner oracle/foreground obstruction; report artifacts saturated cap | 1 |
| M1 candidate 2 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + source-code patch SHA-256 `7784f688f53a3265101fe48fb707870b2aa453bec5643b1198951aa0750d6e13`; app `f5120a47019824049bea9faa877bad0dcc38729ad7e0ca81987c83435f64766d`; runner `107849842a77e089f9739be06d5bdbc9a4a7f0ea37952c55a112a6630b174b32` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-001520/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 exact 3–4 | same source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 2 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-001520/hotkey-exact-{3,4}/report.json` and `.meta.txt` | failed at preflight: Shift, Alt, and Left Win persisted down from previous native input; no functional case executed. Explicit key-up recovery confirmed all five reported modifier states up. | 1 each |
| M1 exact 5 | same source-matched exact-chord runner after key recovery | candidate 2 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-001520/hotkey-exact-5/report.json` and `.meta.txt` | failed: H02 and CLEANUP passed; H04/H01 counted repeated queued visibility telemetry; H06 onward blocked by unowned DWM HWND PID 1812 covering anchor. Source/runner remediation under review. | 1 |
| M1 F11 diagnostic 1 | source-matched runner `--suite hotkey --hotkey f11 --mouse-gestures enabled`, interactive-desktop sandbox escalation | candidate 2 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-001520/hotkey-f11-1/report.json` and `.meta.txt` | failed: H02 and CLEANUP passed; H04/H01 same queued visibility telemetry; H06 onward same DWM PID 1812 focus obstruction. Failure is independent of exact chord. | 1 |
| M1 candidate 3 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + source-code patch SHA-256 `9b6f1cb52d0d41ef5c0311be80ef9b872f52ddb09f5b4928619dba2280c78bac`; app `0aa19e7367374e52204e1d69f48fbd561a24481ea8a605d7fc5f0c5c21f99115`; runner `8a403a7c16d41936f1bee29848a692fdc18951f5623eb1e154ba43476d4ae4cb` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-005434/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 candidate 3 exact 1 | source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 3 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-005434/hotkey-exact-1/report.json` and `.meta.txt` | failed before functional H cases: nine anchor placement probes all hit non-owned Chrome/shell HWNDs; cleanup passed; report integrity signaled pre-final case count. | 1 |
| M1 candidate 4 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + `Cargo.toml`/source patch SHA-256 `47023f54d711a596edea01b4144b462766d4813504fe3a73cc889316fb2bb7bb`; app `de7748cbc52d2a0145584d707b1a0f7f685a838ccdf30735bc717b66a2df9e7c`; runner `bbc548f87ef824062d98001280066ae87e6bcae05479a190bdc60f04943a4ea0` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-011310/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 candidate 4 exact 1 | source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 4 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-011310/hotkey-exact-1/report.json` and `.meta.txt` | failed: H02/CLEANUP passed; H04/H01 trace validation failed; H06 onward anchor was ghosted by Windows after missing message pumping, blocking focus. | 1 |
| M1 candidate 5 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + `Cargo.toml`/source patch SHA-256 `37c0bc74235fcc1841fab7b600fb3b9afc68e4ebbc10da517e7896856263fdce`; app `4f35af5df3a76817cf33877ef5dc365a6d6dff59a78d093fca5a3b063847cd85`; runner `8b135c86d451d57863e1221dd86e4d58862588b9e8201402aebd146c197913b9` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-015148/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 candidate 5 exact 1 | source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 5 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-015148/hotkey-exact-1/report.json` and `.meta.txt` | failed: H01/H02/H07/H08/H09/CLEANUP passed; H04 trace, H06/H10 hover setup, H11/H12 Designer readiness, H17 hook identity, and dependent cases failed. | 1 |
| M1 candidate 6 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + `Cargo.toml`/source patch SHA-256 `108ae46d7844212e47421d3bf76f2d2d46ab060b45a351d5afbe195c32e067d0`; app `09fe17cdfab43f3da6815fa2b880638dbd22d770242bc922f722fbe0e96c8002`; runner `faaaf8cc54289c7006a4ae27c924c2ebcee0b74cc0a6dbdae13d4f6871f0d186` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-022412/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 candidate 6 exact 1 | source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 6 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-022412/hotkey-exact-1/report.json` and `.meta.txt` | failed: H01/H02/H06/H07/H08/H09/H10/H12/H18/CLEANUP passed; H04 cadence, H11 focus, H16 direct trigger, H17 alternate child failed. | 1 |
| M1 candidate 7 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + `Cargo.toml`/source patch SHA-256 `b077dcc5621522e724a9e597108c4faa65319b0e69b9339275b265099144266c`; app `afce88ea4073cf19253ff7c26d92c10a07b882a29418c02166a9ab002a569387`; runner `74cea01f228d6cf022cbec119dbcddf041e4347334fc5d89a946e182622a0641` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-031105/candidate-build.log` and `.meta.txt` | completed | 0 |
| M1 candidate 7 exact 1 | source-matched exact-chord runner, interactive-desktop sandbox escalation | candidate 7 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-031105/hotkey-exact-1/report.json` and `.meta.txt` | failed after 281 s: H04 show emitted native RestoreRequested without terminal result; next short tap reached hook but never published visibility revision. GUI/UIA/cleanup failures cascaded from one app liveness wedge. Activation gate held across native calls and synchronous same-process title query are both being removed. | 1 |
| M1 candidate 8 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `f263802174912c11c07332631057fc691bae478ffb492bc2767992e2aec3573c`; app `3023e0f1429e467a0565cf6695f8de30379f17dc2e7cd8ad644910b22657fc41`; runner `c960a2414099102e27c95da4f850b36ce1b9e56caf825dd0d7ecad49b7ee7730` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-035519/candidate-build.log` and `.meta.txt` | completed; native run withheld after independent review found stale-focus race | 0 |
| M1 candidate 9 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `a33be4a677af2a049371e7cd4d418e4346bf64ddd8219036e2d018e850a511bc`; Cargo.lock `6e88f1…` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-044344/candidate-build.log` and `.meta.txt` | completed; native run withheld after independent review found in-flight focus handoff and HWND reuse races | 0 |
| M1 candidate 10 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `9fc8e7cd7ca0b62b8ef5e7344a9875405e3c8fa38b38ba4b5b9e6f1aa5305296`; app `db96bde495637f70f0643b7efbe79a661abc8bf7950e4cfc46bfb4097b32f5f1`; runner `b0bec4cb7902736edb066985a6e59447e9352b2332cbcac1d6f50ea122ea3807` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-103758/candidate-build.log`, `.meta.txt`, `candidate.identity.txt` | completed | 0 |
| M1 candidate 10 exact 1 | exact-chord hotkey runner, interactive desktop | candidate 10 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-103758/hotkey-exact-1/runner.log` | runner rejected nonempty output directory before executing cases | 1 |
| M1 candidate 10 exact 2 | source-matched exact-chord hotkey runner with mouse gestures enabled, interactive desktop | candidate 10 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-103758/hotkey-exact-2/report.json`, `report.txt`, sibling runner log/meta | H01/H02/H06–H10/H12/H17/H18/CLEANUP passed; H04/H11/H16 and R0 failed. H04 press admitted and exact runner-tagged chord observed, but owned release modifier state changed after foreign injected Alt pair. H11 Designer-preserving no-activate show was wrongly required to emit native activation terminal. H16 fixture wrote a different log path than proof read. | 1 |
| M1 candidate 11 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `4f87f74eb19d8bf2846324924b8b4e1e701c0eb2f19cbc1280571d85ac90fbf6`; app `0349c65b9c964fe188717d8991977c8b5be9558b49c5bc7420520f258477fb86`; runner `aeb307b658ef2a664c5ea3763f882c24fab167588bd23743ed287034aa910e64` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-112937/candidate-build.log`, `.meta.txt`, `candidate.identity.txt` | completed | 0 |
| M1 candidate 11 exact 1 | source-matched exact-chord hotkey runner with mouse gestures enabled, interactive desktop | candidate 11 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-112937/hotkey-exact-1/report.json`, `report.txt`, sibling runner log/meta | H01/H02/H06–H12/H16–H18/CLEANUP passed. H04 failed InputInjection after foreground changed from owned anchor HWND 8849298/PID 10836 to ROOT HWND 41814130/PID 9736 during matrix setup; R0 failed because H04 was incomplete. | 1 |
| M1 candidate 12 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `d84d6b3dfaae9fa7d30e973a51c081008cf7892cc40bf8c0c9067e25882f6a7a`; app `8b9972a8642d980c1b1c328c57a37fb9bb9e79dc9d1bf83333e2208c8c9bb6e2`; runner `e29fdc77bb761b8fa07d855ffee840efed9ece61b5f99ccba4db9b7670510050` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-114603/candidate-build.log`, `.meta.txt`, `candidate.identity.txt` | completed | 0 |
| M1 candidate 12 exact 1 | source-matched exact-chord hotkey runner with mouse gestures enabled, interactive desktop | candidate 12 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-114603/hotkey-exact-1/report.json`, `report.txt`, sibling runner log/meta | H04 passed hidden+visible 1/2/5/10/25 bursts: 86 unique decisions; H01/H02/H06–H12/H16–H18/CLEANUP passed. R0 alone failed report integrity due H17's main/alternate profile evidence lacking ordinary `hotkey=` field. | 1 |
| M1 candidate 13 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` in checkout | HEAD `8c09d29b` + code/Cargo patch SHA-256 `1a8c03eed205c6999ff5713f2f36d4a5dfe5babc3e901653a39746caed8e374c`; app `d5937a2fe3b389908df4dc660f952ea2af40b7409fc0a45931f8cabe9d829db1`; runner `191236efcf7877864dceda1a33ff11b99c22435be06d2952b6c9bb64c9b81b5f` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-120251/candidate-build.log`, `.meta.txt`, `candidate.identity.txt` | completed | 0 |
| M1 candidate 13 exact 1 | source-matched exact-chord hotkey runner with mouse gestures enabled, interactive desktop | candidate 13 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-120251/hotkey-exact-1/report.json`, `report.txt`, sibling runner log/meta | H04, H01/H02/H06–H12/H16–H18, CLEANUP, and R0 passed. 86 unique burst decisions; no intertap wait/refocus; isolated profile cleanup and foreground restore verified. | 0 |
| M1 candidate 23 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` | HEAD `8c09d29b` + source patch SHA-256 `ac2a38c066c4883d1dd3f1fd193bca4b5bf84cb5`; app `a696f4bc345bdff6a2c723eb8e97976486a5c99a858743ecbee50566ec382eb4`; runner `5a7214ab99770d125c5225a1004c8b5ed2df51f7cbac9411fe0d6fbd131cca3e` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate23/build.log`, `build.meta.txt`, `source-identity.txt` | completed | 0 |
| M1 candidate 23 exact 1–2 | source-matched exact-chord hotkey runner with mouse gestures enabled, interactive desktop | candidate 23 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate23/hotkey-exact-{1,2}/report.json`, `report.txt`, sibling runner log/meta | Both passed all 13 H cases, CLEANUP, and R0. H04 reported 86 unique decisions across hidden+visible 1/2/5/10/25 bursts, 1 matrix attempt, zero contamination; typed packet v4 included 1,925 candidate events and per-event latency spans. Independent review found one contradictory H06 expected string and ambiguous H16 string; correction and final source-matched retest pending. | 0 each |
| M1 candidate 23 full Nextest | `cargo nextest run --locked --no-fail-fast` | candidate 23 source | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate23/full-nextest.log`, `.meta.txt` | completed: 4,870 run, 4,870 passed, 8 skipped; final source edit requires rerun | 0 |
| M1 candidate 24 build | `cargo build --locked --bin multi_launcher --bin radial_acceptance` | HEAD `8c09d29b` + source patch `d928edb8f050565aa30a8143c09c8140da0ada459810a7ebdb72accbcf56a46a`; app `83601d4332110407b8fe22baa0fc6c5ea20846540fd696746bef966ba8d5f55d`; runner `9c2756fcd29941e79411d2da6146169e24e9370f899630c9f4d2e38daf5a2c9e` | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate24/build.log`, `build.meta.txt`, `source-identity.txt` | completed | 0 |
| M1 candidate 24 exact 1–5 | same source-matched exact-chord runner, interactive desktop | candidate 24 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate24/hotkey-exact-{1,2,3,4,5}/report.json` and sibling runner logs/metas | Attempts 1–3 failed due measured external cursor displacement during H cases. After quiet-desktop confirmation, attempts 4–5 failed H04 because a physical released gap measured 101 ms and 102 ms, respectively, above 100 ms; other cases and R0 passed. These are retained as failed attempts, not acceptance evidence. | 1 each |
| M1 candidate 24 exact 6–7 | same source-matched exact-chord runner, interactive desktop | candidate 24 | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate24/hotkey-exact-{6,7}/report.json`, `report.txt`, sibling runner logs/metas | Both complete reports passed all H cases, CLEANUP, and R0. H04 matrix had 86 unique decisions, one attempt, no contamination, measured 25 ms down/75 ms scheduled release, no intertap polling/refocus or pointer movement. | 0 each |
| M1 candidate 24 full Nextest | `cargo nextest run --locked --no-fail-fast` | candidate 24 source | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate24/full-nextest.log`, `full-nextest.meta.txt` | completed: 4,872 run, 4,872 passed, 8 skipped | 0 |
| M1 candidate 24 doctests | `cargo test --doc --locked` | candidate 24 source | 2026-09-25 UTC | `target/radial-reliability-m1-20260925-candidate24/doctest.log`, `doctest.meta.txt` | completed: 0 doctests present, 0 failed | 0 |

One expensive job at a time. Check actual process state before starting another. Use completion notification or the user's 10–20 minute observation cadence for long jobs. Keep native test timing independent.

## Milestone record template

### M0 — Baseline and fixture map (complete)

Objective/requirement IDs: Establish the current source, archive reference, toolchain, test/fixture map, and evidence limits before M1.

Starting and ending source/commit/diff: Clean `f6d0395f5b7cee987b09f1dd11f52361ba74fcf5` at start; documentation-only move/update of this ledger at end. No application source changes.

Changed files/architectural owners: This ledger moved from `docs/` to the plan's documented `docs/plans/` path and gained baseline facts.

Important decisions and intentional behavior changes: The checked-out implementation remains authoritative; no ZIP content was substituted. Older radial-survival assertions must be migrated in M1 to the approved short-tap dismissal contract. No behavior changed in M0.

Tasks completed: Checked Git state and verified branch comparison metadata; inspected archive fingerprint notes, Cargo targets, runner fixture/CLI, existing artifacts, toolchain, and native prerequisites.

Tests added/migrated and why: None; M0 is metadata inspection. Test map: library and binary inline tests; 67 integration targets including GUI/focus/trigger visibility, hotkey, command, query, history, settings, mouse and macro suites; `src/bin/radial_acceptance.rs` with `native.rs`, `suite.rs`, and `copied_profile.rs`. The runner presently uses F11 and has 31 case IDs against a 32-case cap; `--suite`/`--hotkey` are proposed, not implemented.

Commands, discovered counts, pass/fail/skip counts, exit codes: `git status --porcelain=v2 --branch`, `git diff --stat`, `git diff --cached --stat`, `git rev-parse HEAD`, `git merge-base HEAD origin/radial-menu-2`, `rustc -Vv`, `cargo -Vv`, `cargo nextest --version`, `cargo metadata --no-deps --format-version 1`, file/hash inspection; inspection commands succeeded except the unavailable archive and access-denied topology query. No tests or builds run, so no test count or pass claim.

Candidate/runner/profile identities: Existing binary hashes are recorded in Baseline identity; source-matched M1 candidates and isolated profile are pending.

Native cases and report path: None run. Existing reports are not credited toward Gate H.

Source-backed root-cause evidence versus remaining hypotheses: Focused ROOT failure cause is not established. Async restoration is a hypothesis to test, not a confirmed diagnosis.

Performance measurements, if any: None.

Unresolved blockers/limitations: Direct ZIP comparison was impossible because the named archive was unavailable; native monitor/DPI/integrity/input-desktop checks await runner preflight. These do not reopen approved product decisions.

Next bounded milestone: M1 hotkey/grid/runtime ordering, deterministic tests, and exact-chord native runner proof.

### M1 — Hotkey/grid/runtime reliability and exact-chord runner (complete)

Objective/requirement IDs: Gate H, including H01–H18 applicable native and deterministic cases, ROOT presentation and runtime dismissal, Designer independence, exact configured chord, bounded source-matched evidence and cleanup.

Starting and ending source/commit/diff: Started at M0 commit `8c09d29b32057fff7a4de7fd9b629c9d22e6ab69`. Final precommit application/runner source patch SHA-256 `d928edb8f050565aa30a8143c09c8140da0ada459810a7ebdb72accbcf56a46a`; candidate 24 binary hashes are in Gate H above. The ledger change is excluded from that application/runner patch hash.

Changed files/architectural owners: The launcher hotkey invocation layer owns admitted tap/hold identity and release handling. Visibility, activation, ROOT window management, and Screen Draw parking own ordered presentation and focus restoration. The runtime radial controller owns dismissal and stale-open cancellation; Designer keeps its own session/draft lease. The native acceptance runner owns physical chord injection/observation, case-specific expected and observed evidence, bounded typed event packets, latency spans, report integrity, and cleanup.

Important decisions and intentional behavior changes: A launcher short tap always toggles ROOT desired visibility and dismisses the runtime radial, including direct-trigger radials, without action selection or dispatch. The Designer and its native preview survive independently. Native restore completion is fenced against newer hide/show revisions; the hook state lock is released before forwarding to avoid UI-thread contention. Runner evidence treats physical cursor interference and out-of-range measured cadence as failures rather than silently retrying within a matrix.

Tasks completed: Migrated older runtime-survival expectations; integrated per-invocation and revision traces; added ROOT HWND/PID/bounds and focus proof, F11 control, H16 legacy/direct route, H17 alternate profile, finite report overflow behavior, and case-specific expected/latency evidence. Independent core, runner, and final review findings were resolved; narrow re-review confirmed H06/H16 report contract correction.

Tests added/migrated and why: Deterministic hook admission, timer/release, ordered restore/cancellation, Screen Draw, runtime radial, Designer/preview, exact-chord packet, foreign-edge classification, bounded report/readback, expected-state consistency, and cleanup tests. The final full Nextest count is 4,872 passed, 8 skipped; candidate 23 had 4,870 passed before the two final report-contract tests were added.

Commands, discovered counts, pass/fail/skip counts, exit codes: `cargo build --locked --bin multi_launcher --bin radial_acceptance` exit 0; final source-matched native exact-chord reports 6 and 7 each exit 0 with all 13 H cases, CLEANUP, R0; `cargo nextest run --locked --no-fail-fast` exit 0, 4,872/4,872 passed, 8 skipped; `cargo test --doc --locked` exit 0, 0 doctests present; `git diff --check` exit 0. Focused 3/3 report-contract tests, runner check, and rustfmt check passed in the implementation handoff.

Candidate/runner/profile identities: Candidate 24 app SHA-256 `83601d4332110407b8fe22baa0fc6c5ea20846540fd696746bef966ba8d5f55d`, runner SHA-256 `9c2756fcd29941e79411d2da6146169e24e9370f899630c9f4d2e38daf5a2c9e`; isolated profile settings/radial/actions hashes are embedded in each report. Copied profile was not run because no authorized consistent copy was established.

Native cases and report path: `target/radial-reliability-m1-20260925-candidate24/hotkey-exact-{6,7}/report.json`. H04 each exercised hidden+visible 1/2/5/10/25 bursts, exactly 86 unique short decisions, one clean matrix attempt, no intertap UI polling/refocus or pointer movement, and no hold promotion. Earlier failed attempts and their actual causes remain in the job table and reports.

Source-backed root-cause evidence versus remaining hypotheses: Older activation/presentation work could outlive a newer visibility decision; the revision fence prevents stale ROOT restore/focus side effects. Holding the hook state mutex while forwarding could delay gesture handling; release-before-forward is covered by deterministic tests. Native final reports prove current behavior under the tested standard Windows interactive desktop; they do not establish behavior on unavailable monitor/integrity configurations.

Performance measurements, if any: The typed native packet records physical release-to-intent, release-to-ROOT-command, and ROOT-command-to-presentation spans per tap; H04 final reports include actual 10–100 ms accepted physical cadence and 86 decisions. No product latency target was invented from these diagnostic measurements.

Unresolved blockers/limitations: The starting ZIP remained unavailable for direct comparison. Copied-profile and additional monitor/elevation combinations were not established as supported in this environment. Neither prevents the standard Gate H pass recorded above.

Next bounded milestone: M2 typed saved-query/exact-command bindings, v3 migration and persistence/package compatibility (Gate P).

### Mx — title

Objective/requirement IDs:

Starting and ending source/commit/diff:

Changed files/architectural owners:

Important decisions and intentional behavior changes:

Tasks completed:

Tests added/migrated and why:

Commands, discovered counts, pass/fail/skip counts, exit codes:

Candidate/runner/profile identities:

Native cases and report path:

Source-backed root-cause evidence versus remaining hypotheses:

Performance measurements, if any:

Unresolved blockers/limitations:

Next bounded milestone:

## Review findings

| Finding | Severity | Source location | Required remediation | Status | Retest evidence |
|---|---|---|---|---|---|
| M1 snapshot capture can pair old flags with new revision | high | `src/gui/universal_action_executor.rs` | Capture flags/revision atomically; interleaving test | in_progress | pending retest |
| Screen Draw ROOT restore bypasses activation fence | high | `src/gui/render.rs` | Use revision-fenced ROOT activation | in_progress | pending retest |
| Closing active radial may survive new pending session | medium | `src/radial/controller.rs` | Retire old close ack during new open; lifecycle test | in_progress | pending retest |
| Open idle Designer rebuilds catalog per frame | medium | `src/gui/radial_editor/mod.rs` | Demand/revision caching and build-count test | in_progress | pending retest |
| ROOT viewport show/focus outside revision fence | medium | `src/gui/render.rs` | Gate or reconcile commands; ordering test | in_progress | pending retest |
| Same-process HWND reuse may pass activation fence | medium | `src/window_activation.rs` | Include ROOT lifetime identity; fake-backend test | in_progress | pending retest |
| Selected-cell handoff may be dropped by later tap | investigate | `src/radial/controller.rs` | Define selection commit boundary; selected-then-tap test | in_progress | pending retest |
| Burst guard can refocus between taps without reporting it | high | `src/bin/radial_acceptance/native.rs` | Count/fail recovery and preserve uninterrupted burst | in_progress | pending retest |
| Burst cadence and partial-injection cleanup lack complete evidence | high | `src/bin/radial_acceptance/native.rs`, `suite.rs` | Record monotonic edges/gaps; surface and recheck owned-key cleanup failures | in_progress | pending retest |
| Hotkey visible oracle accepts virtual-screen monitor gaps | high | `src/bin/radial_acceptance/suite.rs` | Use physical monitor intersection and owned HWND/PID | in_progress | pending retest |
| Burst count/parity can hide wrong gesture ordering | high | `src/bin/radial_acceptance.rs` | Correlate release, tap, revision, and ROOT command by invocation/source | in_progress | pending retest |
| Saturation may omit CLEANUP record | medium | `src/bin/radial_acceptance.rs` | Reserve CLEANUP and R0 slots, test capacity | in_progress | pending retest |

## Final report checklist

Approved behavior implemented; intentional old-test changes explained; final Nextest results; applicable doctests; final candidate and runner hashes/source manifest; mandatory exact-chord/native cases; query/UI/no-flash/confirmation checks; Designer/persistence/skin checks; cleanup; copied-profile result or honest absence; independent review/remediation; remaining environment limitations; no unsupported “no regressions” claim.
