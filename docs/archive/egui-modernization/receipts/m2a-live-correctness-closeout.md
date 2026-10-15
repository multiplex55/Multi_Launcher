# M2-A actual live correctness closeout — revised policy

Authority: POLICY checkpoint `cd82ceb7` and PERFORMANCE_ACCEPTANCE_ADDENDUM.md. Status: complete, `accepted_with_documented_regression`. No isolated optimization was ported and no new benchmark was run.

Chosen production implementation remains6527ddc1 following79653eca. Render hash83EBC497 and actual lock18180D23 are bound in native receipts. The only source correction is deliberate FullOutput disposal in `closed_show_deferred_frames_skip_catalog_and_real_retained_callback_drains_close`; final radial source9FDA93B5. All strict viewport, catalog reuse, pending-close, retained callback, SetTheme, session/identity and closed-frame assertions remain. No runtime guard or vendor change.

| Actual check | Result / evidence |
|---|---|
| Shared-target runs001/002 | native101 before tests: cached epaint metadata omitted a public method present in live source. Timestamp invalidation did not repair evidence. Both logs preserved; no source workaround |
| Fresh actual-source run003 | Actual epaint/egui/application compiled; Nextest native100/336.4663711s: ClipboardModify close cleanup and failed-save preservation passed; radial fixture failed debug texture-delta disposal. Raw failure retained |
| Corrected radial exact rerun004 | native0/61.6957759s, one passed. Reused validated fresh target; explicit headless output disposal corrected the responsible fixture lifecycle boundary |
| Actual Windows all-target check004 | `cargo check --locked --offline --profile fast-dev --all-targets`, native0/177.7887266s |
| Scoped formatting/diff | rustfmt check0; only named radial test changed; no assertions weakened |

Evidence roots: `target/egui-modernization/m2a/live-policy-closeout-001` through004. Run004 binds exact live source, working-tree test-only correction, argv/profile/tools, native exits and source/log hashes. Two unaffected Clipboard passes from003 remain applicable; they were not unnecessarily repeated. No unexecuted check is called a pass.

The integrated epaint text_layout/fonts/font and egui widget_text files match source796's frozen reference after known text EOL normalization only. Existing correctness contracts remain source-applicable; compact/row-storage/ASCII candidates remain deferred and unported. Actual live source compiles without the stale API error.

Independent coherent closeout review accepted without findings. Documented source796 release ROOT performance differences retain their historical failure and raw values but are accepted debt under the revised requirements. Actual source6527 guards were not newly benchmarked, and no exact timing improvement is claimed.

M2-D broad stabilization checks and native/manual qualification gaps remain separate. Current format check passed; Clippy/full Nextest run under `target/egui-modernization/m2d/stabilization-001` is active and not yet accepted. Continue to functional migration checkpoint and M3-A after this coherent stabilization, without reopening parity experiments.
