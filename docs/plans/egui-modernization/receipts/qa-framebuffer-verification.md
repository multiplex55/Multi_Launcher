# M1-D opt-in framebuffer probe source verification

2026-10-13. Base source checkpoint `b30bc6d00a4b36abc3ec7c3d8da22738c1e480ad`. This is the bounded QA prerequisite in [the implementation packet](../qa-framebuffer-plan.md). Source acceptance is distinct from native PNG acceptance, which remains pending.

Sole specialized implementer released source ownership after the checks below. Independent specialized reviewer accepted the final correction snapshot: nonblocking save handoff failures terminate, modeless File Search remains eligible, and disabled/terminal paths avoid event scans, channel polling, eligibility work and Context cloning. Request correlation uses egui UserData Arc identity. No new production dependency or vendor change.

Commands used direct stable Rust/Cargo1.99.0 MSVC binaries through process PATH/RUSTC, from repository root. All Cargo commands below include `--locked --offline`.

| Command | Actual result | Log under `target/egui-modernization/qa-framebuffer/` |
| --- | --- | --- |
| `cargo test --lib --features qa-framebuffer-capture gui::qa_framebuffer::tests --locked --offline` | 9 passed,0 failed,5229 filtered; build4m16s, execution0.01s | `state-tests-20261013-r2.log` |
| `cargo check --all-targets --features qa-framebuffer-capture --locked --offline` | PASS30.83s | `feature-check-20261013-r4.log` |
| `cargo check --all-targets --locked --offline` | PASS1m00s | `default-check-20261013-r1.log` |
| `cargo fmt --all -- --check` | PASS | `fmt-check-20261013.log` |
| `git diff --check` | PASS; checkout EOL warnings only | `diff-check-20261013.log` |

Parent inspected actual test/check summaries and integration diff. The nine tests cover issue/capture/reply frames, hidden/OCR/tool cancellation, same-frame buffered reply cancellation, matching/foreign tokens, focused unmodified F11, performance-mode rejection, bounded queue delivery, full/disconnected save handoff failures, and exclusive PNG no-overwrite behavior. Earlier compiler failures remain in their original logs; they are not runtime passes. Existing deprecation warnings remain outside this change.

Normal builds compile out the module and its lifecycle hooks. Feature builds install no request in test/benchmark constructors. Capture builds reject performance instrumentation/Track C benchmark mode. The probe performs one-shot PNG encoding/hash/IO on a worker; CPU capture-frame provenance does not prove GPU/display completion.

Not yet established: actual native readable PNG, current/original visual comparison, M1-D native smoke, or integrated M2-D migration acceptance. No application was launched by the implementer. Parent owns attributable build, isolated fixture launch and PNG inspection. Checkpoints remain local.

## Native-discovered visibility follow-up

Capture-enabled checkpoint585d9b1e built successfully in225.391s; preserved executable SHA25654F0EE312D80A96CDE942465711FA4CCDCE93081DE8469821351219A7443A90C. Run001 rejected its manifest because a launch-side null assignment left Track C mode present; the process receipt corrects the original intended-environment claims. Run002 explicitly removed and checked absence of benchmark variables, created the request output directory and exposed a focused accessible ROOT, but produced no PNG. Both exact owned processes were gracefully closed. These are failed native attempts, not capture passes.

Source diagnosis: egui visibility can be None when occlusion is unsupported, and the probe's false fallback differs from Glow's true fallback. A narrow QA correction now preserves raw `root_visibility_reported: Option<bool>` alongside effective renderability and matches Glow. Explicit minimized/occluded states still reject capture. Independent review accepted this delta; actual raw native visibility was not measured in run002.

`cargo test --lib --features qa-framebuffer-capture gui::qa_framebuffer::tests::unknown_occlusion_keeps_root_eligible_but_explicit_hidden_states_do_not --locked --offline` passes1/1,5238 filtered,0.01s after2m12s build in `unknown-visibility-test-20261013-r2.log`. This final rerun includes the raw provenance field; the earlier1/1 pass preceded that addition. Fresh `cargo check --all-targets --features qa-framebuffer-capture --locked --offline` passes56.52s in `feature-check-qa002-20261013.log`; formatting and diff checks pass. Parent inspected actual summaries; sole source owner released. Native retry remains pending. No default check repeated: the entire changed owner is feature-gated.
