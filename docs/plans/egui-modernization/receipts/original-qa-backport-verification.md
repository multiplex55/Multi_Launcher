# M1-D original0.27 framebuffer source backport

2026-10-13. Baseline `72a01bd5451060d3f6b640b83acd081d7db76e77`; generated source tar SHA25680CC5FF972121F9D64FC139F0B358CFA59CFAB9A4DE8FDE165CBF4093041E23C. This is a separately attributed instrumented original, not the preserved unmodified binary or missing user ZIP.

One writer released the isolated archive. Independent specialized review accepted source order, App::update wrapper/normal-tail marker, finite N/N+1/N+2 scheduling, cancellation before buffered reply save, worker/exclusive IO and disabled/benchmark exclusion. Original screenshot events lack UserData; receipts explicitly label weaker first-expected-ROOT correlation with one outstanding request. No lifecycle/vendor/Track C backport.

Parent audited every regular source archive file against the preserved tar:843 remain byte-identical; exactly four existing files changed (Cargo.toml/main/gui-mod/render), plus the new probe module. No unexpected file remains. Diagnostic scratch was removed after exact content/hash attribution. This source audit is provenance verification, not a broad test campaign.

The reproducible [backport patch](original-qa-backport.patch), SHA2563781AEC526CE36DA7B6A1B713FD65ABEEE3B4637D7D8836131EE7A7FFF69A05E, was checked/applied to fresh baseline integration files; all five resulting normalized sources match the instrumented archive. Raw/normalized input hashes and audit live in `target/egui-modernization/original-qa-target/source-patch-receipt.json`. Windows checkout EOL conversion remains distinguished from Git blob identity.

This intentional receipt artifact is tracked explicitly despite the generic `*.patch` ignore. Unified-diff blank context lines contain a required space prefix; ordinary whitespace checking is scoped to the accompanying prose/helper files, while patch integrity is established by actual application and source comparison. This does not waive source formatting or permit accidental whitespace changes.

Direct stable Rust/Cargo1.99.0 MSVC, process PATH/RUSTC, `CARGO_TARGET_DIR=target/egui-modernization/original-qa-target`; all Cargo commands locked/offline and run in the archive:

| Command | Actual result | Log under original-qa-target |
| --- | --- | --- |
| `cargo test --locked --offline --lib --features qa-framebuffer-capture gui::qa_framebuffer::tests::` | 8 passed,0 failed,5211 filtered;2m47s build,0.02s tests | `qa-framebuffer-backport-tests.log` |
| `cargo check --locked --offline --features qa-framebuffer-capture --lib --bin multi_launcher` | PASS1m59s | `qa-framebuffer-backport-check.log` |
| `cargo check --locked --offline --lib --bin multi_launcher` | PASS49.16s | `qa-framebuffer-backport-default-check.log` |
| `cargo fmt --all -- --check` | PASS,empty output; writer receipt records exit0 | No zero-output log created |

Parent inspected actual logged summaries. Earlier zero-selected binary test is compile-only, not owner-test evidence. Eight owner tests cover timing, ownership cancellation, unsolicited/foreign events, focused unmodified F11, minimized fallback/benchmark exclusion, bounded queue/handoff failure and no-overwrite. Existing warnings remain unrelated.

Native fast-dev build/executable attribution and actual original PNG are pending. No original visual comparison is accepted from source tests. Current checkout source/preserved original binaries are unchanged by this archive backport. The current QR Screenshot-owner extension has a separate sole source writer after this handoff; parent owns original build/launch/docs/Git. Checkpoints remain local.
