# M3-A semantic theme model

Migration-only parent: `af9d11838db9448461be920790d926cb22b1cd40`. Local-only checkpoint; no dependency/backend change.

Added compatible defaulted theme preferences (preset, accent, density, typography scale) and a pure GUI resolver with semantic palette/metrics and separate structural identities. Existing modes, RGBA, maps, legacy missing-map behavior and independent query/list scales remain intact. Nonfinite/clamped metric normalization does not rewrite stored settings. Runtime application follows in M3-B.

Targeted Windows locked/offline fast-dev Nextest passed **18/18**, native exit0, 196.875s including compilation. Scope: new resolver/model and existing theme settings/dialog lib/domain tests. Raw command, source hashes, start/result and logs: `target/egui-modernization/m3a/model-001/nextest/`; runner: `target/egui-modernization/m3a/model-check.py`. All six source/manifest bindings remained unchanged. Scoped formatting/diff check passed. Independent source review found no substantive blocker; runtime must compare resolved appearance in addition to palette/metric identities.

No native, responsiveness or benchmark claim is made for this model-only checkpoint. Accepted historical performance debt and native QA gaps remain documented separately.
