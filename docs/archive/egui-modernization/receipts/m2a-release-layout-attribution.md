# M2-A instrumented release layout attribution

Status: diagnostic source, artifact, parity and bounded timing evidence independently accepted without unresolved findings. Production remains at `3a10f1de`; M2-A performance remains FAIL/open.

## Purpose and source boundary

Release geometry evidence at `a6c656f7` places98.6777–98.7815% of each measured1,000-row cold owner inside rebuilding. This experiment reuses the accepted text-layout observer to identify nested release cost before selecting another production change. Observer timings are instrumented evidence, not exact production costs or an original-to-current comparison.

Fresh ignored root: `target/egui-modernization/m2a/release-profile-adapter-001/root/`, copied from the accepted `raw-hit-setup-candidate/candidate/root/`. Its1,070-file copy audit passed. The accepted candidate, original canonical reference and prior artifacts remain untouched. Only the fresh copy's test module in `src/gui/render.rs` changed; its production prefix remains byte-identical to the accepted candidate.

The existing candidate already contains adopted early raw-hit behavior and the55-field observer. Focused correspondence confirms shared raw-cache eligibility, key/limits, early-hit return and plan/native miss flow with production. Observer timers, counters, snapshots and test ownership differ and may add overhead. The epaint manifest and root lock match production after CRLF-to-LF normalization; text-stack inputs remain HarfRust0.13.3/Skrifa0.46.2/read-fonts0.43.3. No live runtime, dependency, profile, asset or observer-clock changes.

## Profile-aware correctness gate

Unchanged reuse was rejected before compilation: egui's `Id::Debug` uses remembered construction chains with debug assertions and abbreviated `id_XXXX` formatting in release. Existing canonical snapshots and derived viewport/output receipts include that representation. The independently measured production release tuple is `adfb11ee80438eab / 18c853161fe60dfc / 89195c4b0382794f`; the accepted fast-dev tuple is `bd4860c5d2427fa2 / 18c853161fe60dfc / 52001c291b514345`.

The private adapter requires exact equality outside27 classified value fields:25 ordered painted-row widget-ID debug values and the independently bound output/viewport receipt values. It checks precise old/new syntax and literal row-index spelling. All action IDs, indices, rectangle bits, shaping inputs, glyph/font records, geometry/eager/scroll fields, line endings and surrounding bytes remain exact. Existing typed actual-versus-expected widget-ID assertions and per-sample receipt checks remain intact. This is bounded profile-aware parity; it does not claim full numeric widget-ID equality across profiles.

The complete actual snapshot is written with create-new semantics before comparison can fail. A complete classified diff is retained before asserting success. A source-review finding that parsing/rebuilding row indices could normalize `[00]` into `[0]` was corrected to literal index-text equality. No other source finding remains.

## Accepted preparation evidence

Paths below are relative to `release-profile-adapter-001/`:

| Artifact | SHA-256 |
| --- | --- |
| `root/src/gui/render.rs` | `c51176cfdff351352eaff467ecf2d2129aa6ef9565428d862df06536ada09b00` |
| `receipts/profile-adapter-render-diff-002.patch` | `84fe5e1c7758eccd0873e24e779935db6f3c49c387b3efe86d106bf1500e90c0` |
| `receipts/runtime-correspondence-001.json` | `a036b780645609c976712a4630a6efc2927928c49e37edc1da973eec0feab5f1` |
| `receipts/source-freeze-handoff-003.json` | `2ea4181c14ca74677b3d4a2ad9da665463a40771759309ca9d20799a56acd44b` |
| `inputs/accepted-raw-reference-snapshot-001.txt` | `66d25a29a4a62b08a8e12e83cb7204012fea7c65f2f1528dbc756563e740712c` |

Prior freeze/diff receipts and the analysis-only builder failure are preserved. Source preparation used no compiler or test body. Independent review accepted the corrected source freeze and focused runtime/lock correspondence without unresolved findings.

## Archive and execution gates

One canonical-release Nextest archive build completed from the frozen ignored root with an isolated target, explicit stable1.99MSVC toolchain and locked/offline inputs. Actual native exit0, wall718.3853411seconds; stderr reports the optimized release profile and one archived test binary. No feature or profile/Rustflags override was present. Archive `receipts/canonical-release-001.nextest.tar.zst` is30,592,440bytes, SHA-256 `ec19024c97956f172ed9dd0909218906f6dceef60cac7ee399d4fd1d926a6dc8`. Parent independently rehashed archive and unchanged source. Start/result receipts retain actual command, cwd, environment and toolchain bindings; the owned build is terminal.

Nextest no-body listing passed with native exit0 in2.3951826seconds: one library suite,5,238 tests and the exact ignored owner. Actual release/MSVC metadata and the extracted executable were verified:87,122,944bytes, SHA-256 `a8ab96d5fa7e8e47e70dacab2d908e1d536052eef9aca263548b255f9e9e4662`. Parent and reviewer independently checked all16 bound files. Binding003 (`receipts/canonical-release-001.artifact-binding-003.json`, SHA-256 `3c247cdf2dd176a96241ace7ce6ba29281a0fb5ddfe3d2920426b6ec9044d3cb`) corrects a receipt-only extraction-directory field to match actual argv/metadata; prior receipts remain preserved and no native command was repeated. A missing listing start timestamp remains explicitly unavailable.

The source/profile/archive/executable/full-listing gate was independently accepted before parity. Complete actual parity/diff review then preceded the separately authorized timing body. Timing regenerated parity and retained two warmups plus five measured fresh-state1,000-row cold samples and all55-field vectors. No exclusions, automatic timing reruns, new baseline build or original benchmark repetition.

Nested intervals will be reported separately, without adding selected medians or subtracting unrelated production runs to estimate observer overhead. This experiment accepts no Full81, native, renderer, theming or performance gate. Both mission tracks remain required.

## First parity invocation: launch environment failure

The single authorized parity invocation returned native exit100 in2.3297983seconds. The exact owner ran but stopped at its `ML_QA_FRAMEBUFFER_REQUEST_MANIFEST` absence guard before creating the fixture. The owner output directory remains empty; there is no canonical snapshot, classified diff or timing evidence. Source/archive/reference/re-extracted executable hashes match the accepted bindings. Receipts/logs are `receipts/raw-hit-release-parity-001.*`; this failed attempt remains preserved.

Parent reproduced the environment boundary using a disposable unrelated variable: on PowerShell7.6.5/.NET10.0.11, `[Environment]::SetEnvironmentVariable(name,$null,'Process')` leaves the key present with an empty value, observed in a Python child. `Remove-Item -LiteralPath Env:<name>` removes it from the child environment. The QA-absence guard correctly distinguishes absence from an empty string. The wrapper's receipt assertion that QA variables were absent is therefore unsupported and requires an appended correction.

Correction receipt `receipts/raw-hit-release-parity-001.qa-environment-correction-002.json` (SHA-256 `ee3e4abfe871f129b06c1bfb116e0b020b00acccb711552e98394c23d6e8bb10`) withdraws the earlier absence claim; the failed analysis-only correction001 file is preserved. Independent review confirmed the pre-fixture failure and justified a corrected launch without reopening the accepted source/archive gates.

Parent explicitly authorized one corrected parity002 invocation using Python `subprocess.run` with both QA keys removed from its explicit child environment and actual absence asserted/recorded before launch. It uses the same accepted archive/source/reference and fresh empty directories. This corrects the failed harness boundary; it cannot discard measured samples because fixture setup never began. No source/archive rebuild or automatic test retry is authorized. Timing remains gated on complete actual parity review.

## Corrected actual parity

Parity002 passed with native exit0 in2.613673600135371seconds. Parent and reviewer independently compared all108,948 canonical lines:108,921 are byte-identical, with exactly25 ordered widget Debug-value spans and the two approved receipt values differing. All surrounding bytes, full glyph/font/input data, geometry/eager/scroll records, actions and rectangles remain equal. Full numeric cross-profile ID equality is not claimed.

Actual snapshot: `raw-hit-release-parity-output-002/raw-hit-canonical-snapshot-001.txt`,4,964,373bytes, SHA-256 `84bda12c3c2c6d35ee1f1880cf14fde949acbd2a783834ef49e39784ff8c444d`. Classified diff: `raw-hit-release-parity-output-002/raw-hit-profile-classified-diff-001.txt`,8,632bytes, SHA-256 `075ef5b63d31fbd27824b4f6f019066fd96dd9b28851592bc30895cdc2d07aa6`. Result receipt: `receipts/raw-hit-release-parity-002.result.json`, SHA-256 `da52fb39cb4625e66a09e202dc44308ce6c3bc6615df498350edad7e56c960b4`.

The actual child environment proves both QA keys absent. Source/archive/reference/re-extracted executable bindings remain unchanged. The owner reports4,047 runs/103,830 glyphs, one rebuild/1,000 measured/25 painted/one pass,3,072 raw hits/975 misses and975 plan lookups/native calls. Release output/structure/viewport tuple matches the independent production reference. The failed001 attempt remains preserved.

Actual parity review closed without findings. Parent separately authorized exactly one timing invocation from the same archive using the corrected explicit child environment and fresh directories. It regenerates parity, then retains two warmups and five measured samples. No performance acceptance or subsequent optimization is implied.

## Instrumented release timing

The single timing invocation passed with native exit0, wall3.8276592001784593seconds. It regenerated the same complete parity snapshot and classified diff hashes, then emitted two warmups and five measured samples. All seven vectors remain in execution order; the separate parity vector is retained as well. No exclusions or timing reruns.

Ordered measured owner nanoseconds: `[38736000,31265500,28696400,35831700,33757300]`.

| Existing interval | Five-sample median ms |
| --- | ---: |
| Frame owner | 33.7573 |
| Galley miss, inclusive | 31.9923 |
| Text layout section | 20.9049 |
| Shaped-run emission | 10.3810 |
| Native shaping | 3.3504 |
| Segmentation | 3.0892 |
| Galley lookup | 0.0521 |

These intervals are nested/inclusive and must not be added. Existing clocks/counters remain active; `observer_clock=off` disables only the post-section clocks. Do not subtract the unrelated uninstrumented production median to estimate overhead. This identifies a substantial instrumented emission interval, not an exact production cost or proof of a specific source defect.

Every sample reports4,047 runs/103,830 emitted glyphs, all103,830 glyph allocation cache hits and zero allocation misses. Plan lookups/hits and native calls are975; raw hits3,072/misses975/bypasses0. Raw insertions/evictions are975 per sample, with128 maximum entries/175,833 maximum payload bytes/14,336 shallow metadata bytes. Plan evictions are zero. Work remains one rebuild/1,000 measured/25 painted/one adapter pass with the accepted identity tuple.

Raw logs: `receipts/raw-hit-release-timing-001.{stdout,stderr}.txt`; regenerated snapshots are in `raw-hit-release-timing-output-001/`. The initial wrapper result parser searched obsolete post-section prefixes and incorrectly counted zero timing lines; raw logs were intact. Append-only `receipts/raw-hit-release-timing-001.parser-correction-001.json` (SHA-256 `ee8261001a688e38fe0e61765e4ccc1be22a1dea5150b5a924558053f9cef61d`) preserves the actual schema and all eight ordered vectors (parity plus seven warmup/sample records). This analysis-only correction caused no rerun. A status-message raw/plan eviction mix-up was corrected; the retained schema vectors already contain the proper values.

Parent and independent reviewer checked raw sample order, all55-field vectors, work/IDs, reported medians, command/environment/native receipts and unchanged source/archive/executable/snapshot bindings. Review accepted the bounded descriptive evidence without findings. Planner read-only investigation of the repeated emission/cache-hit path is requested; no optimization, new instrumentation or performance waiver is activated.
