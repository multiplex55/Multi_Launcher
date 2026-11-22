# Checkpoints and continuation

## P00 — Planning package prepared

Planning reference: master f7bc610a86bcad1c14b69dce65b1d804df1b7934, November 21, 2026. User requirements translated into a master Goal, orchestration Kickstart, 27 initial goal packets and execution record system.

No migration repository branch or source modification is claimed. No application tests, builds, runtime observations or GUI acceptance occurred in preparation.

**Next action:** G00-A: inspect current checkout/instructions, fetch origin/master, reconcile source changes and existing user work, create the dedicated branch safely, materialize the package, record execution baseline, commit the initial coherent ledger and attempt one normal push.

## Subsequent checkpoints

Use the continuation template. Include branch/HEAD, working-tree state, owner, exact current checkpoint, implemented vs accepted GUI IDs, partial changes, commands/evidence and limits, revalidation requirements, blockers/dependencies and exact next action. Keep historical entries factual and concise; do not overwrite the only resume state with a vague “continue migration.”

## G00-A — Execution baseline

The parent owns Git and this ledger. The branch is `feat/gpui-ui-parity` and the parent is `98bbc0ccc22a297e8eafae7e844c2f7e45854856`. The baseline commit introduces this entry. The checkout was clean before the milestone and this checkpoint touches only the execution records. No application source was modified and no GUIs have been implemented or accepted.

`git fetch origin master` succeeded with Git write access. `FETCH_HEAD`, `origin/master` and the starting `HEAD` agreed. Source and environment observations are in [SOURCE_BASELINE](SOURCE_BASELINE.md). G00-B discovery and the G01 real-binary feasibility handoff are next. Runtime baseline, build and test evidence remain pending.

The baseline commit is `3e67f7396f127d6dcb7bec432e9bb1684e9e6ae3`. The single `git push` attempt succeeded and local `HEAD` and `origin/feat/gpui-ui-parity` agreed immediately after it.

## G00-B/D — Discovery and review increment

Source discovery increment `e449f6f785ebbb1b466cda361d984214951fb599` was committed and pushed. The [review](reviews/G00-source-discovery.md) and [crosswalk](GUI_ROUTE_CROSSWALK.md) reconcile 77 Panel variants, 60 Jump Mode kinds and 110 persisted geometry keys. These registrations overlap. Coverage remains provisional for feature-specific controls, dynamic plug-ins and runtime confirmation.

G00-C observed egui launcher keyboard routing, a Hash Generator popout and persisted presentation change on the identified G01-A executable. See [Validation log](VALIDATION_LOG.md). Other baseline scenarios remain assigned to their feature records before acceptance. G01-C native probes and G02-B Hash Generator session extraction have bounded plans.

## G01-A — Same-binary foundation linked and reviewed

The G01-A checkpoint adds official Kit 0.10.0 and its aligned GPUI 0.3.12 family, early startup selection and a native foundation window. The existing brightness helper remains first. The canonical instance guard and startup recovery remain before frontend initialization. Option-free and explicit-egui startup took the established egui route. Explicit GPUI entered Kit on the main thread. The debug executable linked and all three focused selector tests passed. See [evidence](VALIDATION_LOG.md) and [review](reviews/G01-A-foundation.md).

GPUI foundation is a feasibility surface. No production GUI parity was implemented or accepted. G01-B needs a retained editable field with actual Windows input. G01-C and G01-D still need native lifecycle and release evidence. G03 owns actionable data-lock conflict and controlled restart. Continue from the G01-A commit on the same branch with the G01-B handoff.

The foundation checkpoint is `6b7f623a0` on `feat/gpui-ui-parity`. Its ordinary push succeeded. G00 registration checkpoint `b91320a3a` also pushed successfully.

## G01-B — Editable native Kit proof

The existing selector entered the selected runtime in all three Windows launch forms. A retained Kit input received native Unicode text and an editing key, with focus and the revised value observed through UI Automation. The owned GPUI process closed normally. See [G01-B-EDITABLE-STARTUP](VALIDATION_LOG.md#execution-receipts) for the linked artifact identity and limits. The representative egui workflow from G00-C is also recorded there.

G01-B is `a2c90c2c3` on `feat/gpui-ui-parity`, pushed successfully. The next source work starts from this checkpoint.

Continue G01-C with the existing hotkey and foreground service boundaries. Prove repeated hide/show, an independent utility and native passive overlay behavior with this binary lineage. Display capture is pending. G01-D release build and fallback after a controlled GPUI startup failure remain required. No production GPUI GUI is accepted.

The plugin-route and Launcher specification checkpoint `ffa8dadea` is pushed. G01-C1 is the active source slice. The implementer owns the GPUI runtime source and its Cargo verification. The parent owns the desktop fixture, Git and the ledger. Use the [G01 packet](goals/G01-windows-and-kit-feasibility.md) and resume from its latest source checkpoint. The [shared Hash session map](CORE_CONTRACTS.md) and [selector lifecycle handoff](FRONTEND_SELECTION.md) describe queued G02/G03 work.

## G01-C1 — In-progress continuation

At 9:31 p.m. America/New_York on November 21, 2026, the existing checkout was on `feat/gpui-ui-parity` with local and upstream HEAD `ea9d1c2ccf08d3f5a3a05fb7849900d0d5b3e3db`. G01-B at `a2c90c2c33ff53aa3bb295094726bdc7b7567929` was the last verified source checkpoint. The index was clear. The G01-C1 draft was preserved unstaged in `src/gpui_frontend.rs` and `src/main.rs`. No other application source was changed. No GPUI production GUI has been accepted.

The draft passes the configured hotkey to one prepared listener and connects its wake channel to the retained Kit root. It also adds checked native window visibility, saved foreground identity and a second utility input window. Initial `cargo build --locked -p multi_launcher --bin multi_launcher` exposed integration errors in the library module paths, GPUI foreground callback arguments, and the Kit `open_window` return shape. These are ordinary compile corrections. The implementer is also simplifying task ownership and ensuring secondary activation runs off the GUI thread. No successful C1 build or native C1 check was recorded at this snapshot.

Continue the current C1 source. Freeze it after the locked real-binary build, check the diff and run bounded independent review. Then use the disposable Windows fixture to test the configured hotkey, focus restoration, utility lifetime with the root hidden, and service cleanup. A helper that launches the real fixture binary from its scratch working directory made the installed desktop controller usable for G01-B. The helper and fixture are outside the checkout under the `multi-launcher-gpui-test-64f9862f49fc4c5995f48eef0c49b8cc` temporary directory. No owned fixture processes were running at this snapshot. Refresh the fixture binary from the exact successful C1 build before launch. A foreground-control dialog can be started from the same fixture using the deliberately malformed selector, which runs before acquiring the data lock.

Proceed to C2 for the passive overlay and monitor/DPI observations, then the release/fallback check in G01-D. Visual capture is still pending. G02-B has an independently scoped shared Hash Generator extraction, with validation centered on existing library regressions. Neither the visual limitation nor normal integration errors prevent source work on the next eligible slice.

Subsequent C1 draft build linked, but its Kit root access failed on launch. The native error was observed and the same dev executable started egui after the failure process closed. Correct root and secondary windows to retain the Kit untyped window handle and content entity separately. Rebuild and verify that the corrected GPUI input window actually opens before testing hotkey or utility behavior. See `G01-C1-ACTUAL-ROOT-FAILURE` in the validation log.

The untyped Kit handle correction linked and native startup/close succeeded. See `G01-C1-ROOT-CORRECTED`. The reviewer identified a race where a foreground change between sampling and hide could authorize restoration over the user's current window, and an overlapping utility open/close race. The implementer is resolving those issues and adding a keyboard route to the proof controls. The scratch root currently has no running fixture process. Repeat only the changed C1 scenarios with the next linked artifact. The initial F2 input automation result is unverified, and screenshot/pointer geometry is still unavailable.

## G01-C1 — Native utility proof, global invocation pending

The [review](reviews/G01-C1-lifecycle.md) and [validation log](VALIDATION_LOG.md) record the lifecycle fixes, the linked Windows executable and actual utility behavior. The configured listener, checked HWND root adapter and guarded foreground requests are implemented in the proof shell. The exact utility hide/show/close path has Windows evidence. Global F2 produced no recorded accepted wake with the available injected key action. Root-hidden utility and foreground restoration claims remain pending. No test fixture process was left running.

The C1 source checkpoint is `f9839f8b5f294bce5959d796dff47c51afb6b2b2`, pushed to `origin/feat/gpui-ui-parity`. The checkout was clean after the commit and push.

Continue G01-C2 using the existing Coordinate Tool controller and native passive surfaces. Preserve the pending C1 native input receipts and use a known-duration supported key input or a physical key for its next distinct attempt. After the native overlay implementation, build the canonical release and verify fallback and packaging at G01-D. Continue independent Hash extraction and selection work if a specific native capability remains unavailable.

## G01-C2 — Shared native Coordinate HUD adapter

GPUI can start and stop the existing native Coordinate Tool controller. One pending UI operation keeps start/stop off the foreground executor. Runtime teardown waits for the existing controller shutdown before returning to the data-lock owner. The Windows proof started the controller, preserved root keyboard editing and observed an actual 96 × 96 DPI sample. See `G01-C2-HUD`. Visual and pointer behavior and multiple display scales remain pending. No owned fixture process was left running.

Run ordinary `cargo build --release` from the pinned branch next. Verify the same packaged executable with bare, explicit egui and explicit GPUI selection from the disposable non-source directory. Inspect the embedded native icon. A disposable `F13` launcher hotkey can exercise this proof's controlled startup error because the parser accepts it while the legacy polling listener has no mapping. Verify explicit egui recovery from the same root once the error process exits. This is a probe listener limitation and does not waive later configured-hotkey parity.

## G01-D — Release, resources and named fallback verified

Source checkpoint and pushed upstream `e73bb09c1` contain the verified G01-C2 adapter. The worktree and index were clean when `cargo build --release` started at 10:16 p.m. America/New_York. Cargo PID 2448 and unified exec session 72495 completed the build and replaced the earlier release artifact.

The cumulative read-only review found no new blocking source defect in the C1 guard and C2 runtime owner. The existing controller creates its worker only on user action, and it retires before the canonical mutex releases. The native 96 × 96 sample supports actual DPI sampling. Pointer pass-through, the HUD frame and scaled/mixed placement remain pending.

The owned build finished successfully in 17 min 22 s. See `G01-D-RELEASE` and `G01-D-FALLBACK` for exact artifact identity, resource inspection, all three selected launches, native GPUI editing and fallback. The scratch fixture has no running process. G01 remains Validation pending for the C1 global hotkey, root-hidden utility and restoration checks, and C2 native passive-window visuals, hit testing and display placement. The next independent code checkpoint is **G02-B**, extracting Hash Generator session ownership into the existing hash domain while preserving the real egui adapter. Preserve this G01 release evidence unless a later build or startup change affects it.

## G02-B — Shared Hash Generator session verified through egui

`HashSession` now owns the draft, exact source, generation, worker and result lifecycle. The egui adapter still owns presentation, file picker plumbing, focus and effect admission. The scoped Nextest run passed 34 tests, including Hash, native-host File Search handoff, Jump Mode focus and size acknowledgement. The independent [review](reviews/G02-B-hash-session.md) found no remaining blocking finding. See `G02-B-HASH-SESSION` for the test receipt.

The G02-B source checkpoint is `d942d9b65`, pushed to `origin/feat/gpui-ui-parity`.

Next, establish the coalesced Hash worker wake path and a real typed GPUI Hash open and Calculate route. The general launcher query and full Hash panel parity remain with G05 and G04. The G03 baseline confirms that an explicit duplicate selector still exits silently in a same-desktop test. No fixture process is running.

## G02-C/D — Shared Hash wake and typed GPUI proof verified

The source started at `f95359a70` with the `d942d9b65` Hash session and baseline documentation. The first build and review identified two Kit API mismatches, a stale digest after text edits, and synthetic Alt activation during reopen. The verified source observes input changes, uses guarded activation, replaces the blocking receiver wait and tests wake ordering at delivery.

The locked application build and all 39 selected Nextest cases passed. The exact disconnect-order test passed again after a test-only warning cleanup. Native keyboard calculation, stale digest clearing, retained reopen, fresh reopen and shutdown are recorded as `G02-C-D-GPUI-HASH`. See the [review](reviews/G02-C-D-hash-adapters.md) and `G02-C-D-FOCUSED` for the scoped evidence. Full Hash Generator parity remains G04 work.

Next is G03-B duplicate feedback. The prior release supplies the egui Text and comparison baseline. The File source appeared through accessibility and the desktop tool could not select it. The scratch `hash-fixture.txt` contains exact bytes for later picker tests. The launch helper can record child status for bare, explicit-egui and explicit-GPUI duplicates. No owned fixture process is running.
