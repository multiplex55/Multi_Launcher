# M2-A partial production text-stack adoption

Production epaint now selects HarfRust 0.13.3, Skrifa 0.46.2 and read-fonts 0.43.3. The change is limited to two dependency bounds, the root lockfile and vendor provenance notes. No Rust or font-asset changes. Existing per-face FIFO8 shaping-plan ownership and public key semantics remain intact; production has no experimental instrumentation or shaping-result cache.

The root lock matches the accepted isolated candidate. font-types 0.12.6 and once_cell 1.21.4 remain unchanged. Old Skrifa/read-fonts entries remain in the lock for other-platform or optional paths; Windows reverse trees show them inactive. Stock vendor Cargo.toml.orig and vendor Cargo.lock remain package provenance, while the root lock controls this build.

| Required verification | Actual result |
|---|---|
| Existing private cache/key/default-font/variable-font tests | 4/4 passed; actual Bahnschrift wght 300/700 parity; native 0; command 14.657 s |
| Existing focused root filter | 20/20 passed; native 0; build plus execution 182.945 s |
| Locked/offline all-target check | Passed; native 0; 117.705 s |
| Existing 100/1,000 cold/warm diagnostic | Passed; native 0; 22.264 s; all 20 records retained |
| Root/vendor formatting and diff checks | Passed |

The fresh ignored private harness targets the actual live vendor library and aligns only its exact HarfRust/Skrifa pins. Its previous manifest, lock, binaries and receipts remain preserved. The root checks retain existing warnings; no unrelated warning cleanup.

Parent independently parsed all diagnostic records. Owner medians: 100 cold 36.8599 ms / warm 2.4288 ms; 1,000 cold 362.1193 ms / warm 2.3810 ms. Each cell has two warmups and five samples. All samples paint 25 rows in one pass with stable output identities. Cold cells rebuild once and measure the requested count; warm cells perform zero geometry work. Variability and outliers remain. These are live correctness diagnostics, not a matched before/after performance claim.

Independent review verified the graph, all required results, 42 unchanged source/asset hashes and preserved binaries. Evidence resides in `target/egui-modernization/m2a/production-adoption-validation/receipts/`. Private binary SHA256 `2c1acf46f324925d939e431b396730bc610ef80d07ab971b1567d86b8c910506`; root binary `58fd7bb534e295b4c3a1bbd46b2e9638dd097e85f9187198a758c7930654a0f5` (138,964,992 bytes). Root lock SHA256 `0aa62f31d471e9b42f59209361ebc756572b5eff682351668a03f212fd1eda8c`.

The diagnostic raw logs preserve an inherited `ML_TRACK_C_SOURCE_SHA=f389905...` label from the old dependency stack. This is stale instrumentation metadata, not the current source identity. An additive receipt binds the actual run and binary to the current source/lock/manifests; raw logs remain unchanged. Prose-only provenance corrections do not change runtime inputs or require repeating successful tests.

Final source freeze `final-source-graph-freeze-004.txt` SHA256 `696386be12a91d5ff5f02e0d2643d006826bbc1da070208e9032ffb2eb171771`; file inventory `final-source-graph-files-004.tsv` SHA256 `ece9345a6a7b10961cc40a1a7313a9f45089424827ffd13eb5fb8ca15b5bfc46`; additive correction `final-provenance-correction-003.txt` SHA256 `62bf9f9729ca74deff9f2e2b25d3e2793e3b67ac6a9b2f37d49254e4c32f85f0`. Geometry raw log remains SHA256 `9b04b29a2c235d36b69a6484664dd4164565c60f58d84e778729a76fd7192b6e`.

This is a partial correction. The separate matched archive experiment reduced the cold-owner median 390.7041 to 370.1820 ms with exact full output parity; its scope and limitations are in m2a-released-text-stack.md. Original cold/warm parity remains unresolved. No Full81, release or native benefit is claimed; M2-A remains incomplete. The proposed archive-only raw-result reuse experiment is separately planned and reviewed, with activation requiring this adoption's accepted checkpoint and writer release.
