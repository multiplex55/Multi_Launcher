# M1-D QR native capture prerequisite verification

Scope contract: ../qa-secondary-framebuffer-plan.md. This supports integration evidence and later M2-B; it is not broader native parity or theming. Source is independently accepted and checks pass; native PNG acceptance remains pending. User requires local checkpoints only.

## Executed checks

Direct stable Rust/Cargo1.99.0 MSVC, locked/offline application commands. Logs retained beneath ignored target/egui-modernization/qa-secondary-framebuffer.

- `cargo check --all-targets --features qa-framebuffer-capture --locked --offline`: PASS,34.48s final log feature-check-final.log.
- `cargo check --all-targets --locked --offline`: PASS,1m05s final log default-check-final.log.
- `cargo nextest run -p multi_launcher --lib --features qa-framebuffer-capture --locked --offline qr_`: PASS26/26,126.481s command including compilation,0.824s test execution. focused-qr-group-final.log.
- Focused `gui::qa_framebuffer::tests` group with the same feature/lock/offline flags: PASS15/15,0.174s test execution. focused-root-and-qr-probe-tests.log. This overlaps QR cases above; selected test counts must not be summed as unique coverage.

The existing qr_viewport_has_scannable_size_and_embedded_escape_cleans_up test initially failed with unapplied TexturesDelta. Its adapter now explicitly consumes FullOutput using drop_without_applying_deltas, as required by egui0.36, retaining viewport size, Escape, source and texture cleanup assertions. Exact isolated rerun and final group pass. Failed diagnostic logs are retained.

Coverage includes actual Immediate callback identity, embedded/foreign-child rejection, target/token isolation, discarded issue-parent child advances before final dispatch baseline, capture/reply child progression in the same later parent frame, cancellation before save, bounded repaints and unopened timeout. Tests establish state/provenance behavior, not native pixels or GPU timestamps.

## Vendor routing and source checks

Vendor controlled-read routing test: PASS. Temporary vendor workspace online refresh ran five eframe library cases,5/5 passed; exact locked/offline Screenshot routing rerun passed1/1,4 filtered. vendor-workspace-refresh-and-focused-test-escalated.log records72.275s including refresh/build and both runs. Temporary manifests/lock removed; root Cargo.lock SHA256454C3DFD509A4F1FB9D8177A27BB2F5F1E4B2E12C8EB8BA374523A5BA7B37003 unchanged. Standalone test refresh uses a separate graph (including image0.25.10); application's locked graph remains image0.25.6 and passed both all-target checks. Writer removed its newly generated vendor workspace build output; actual test logs remain, without a retained standalone test binary claim. Exact /vendor/target/ ignore added for future invocations. Earlier offline resolution/root package selection and a malformed PowerShell invocation failed before execution; their logs remain diagnostics, not passes.

Targeted rustfmt --check and source diff whitespace check PASS. Independent review, attributable QA fast-dev build, native QR child PNG and preserved ROOT capture regression remain pending. No native QR capture success is claimed from embedded/headless tests. Normal feature-off builds exclude probe metadata/state; vendor Screenshot servicing is a production owner correction and was included in default compilation.

## Review correction

Independent bounded review found one combined integration defect: hidden-tick and same-logic revalidation LauncherApp wrappers retained the older active-only guard while the QR probe intentionally treats Armed as requiring eligibility. Both now use phase_requires_eligibility, preserving ROOT-Armed pretrigger behavior. New actual-wrapper hidden/modal regression passes1/1 (110.160s including compilation); full affected probe group passes16/16,0.217s execution. Earlier passing groups did not exercise these wrappers; their coverage gap is not concealed. Reviewer accepted the correction, conditional on targeted checks, which now pass: updated feature all-target compilation31.71s,fmt/diff. Other inspected dispatch/native/token/save paths had no substantive findings. Logs review-fix-newcase.log/review-fix-probe-group.log/review-fix-feature-check.log retained. Source ownership released; parent owns native build and PNG gate.
