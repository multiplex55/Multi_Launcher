# Local epaint shaping-cache patch

This directory vendors the crates.io `epaint` 0.36.2 package. The original
archive SHA-256 is
`a0ec0308dc23130fc1a7d4623a8534c2ed069c6dc84e258ee4f3de99615aa1a3` (the
checksum recorded in the accepted root lockfile). The package's
`.cargo_vcs_info.json` identifies upstream commit
`49682f8baa058bf49e011035cfbd6e825f88a5ef`, path `crates/epaint`. The
`LICENSE-APACHE` and `LICENSE-MIT` files are copied from the repository's
vendored eframe 0.36.2 package, which identifies the same upstream commit.

`src/text/font.rs` owns an initially empty, eight-entry FIFO of HarfRust
`ShapePlan`s on each `FontFace`. A plan is reused only when HarfRust's
`ShapePlanKey` matches the script, direction, language, feature-variation
selection for the supplied instance, and features. HarfRust 0.13.3 keys an
instance by its feature-variation selection; distinct axis coordinates may
correctly share a plan when that selection is unchanged, while the actual
instance is still supplied for every shaping operation. A miss builds the
same plan for the same shaping inputs, evicting the oldest plan only when the
face reaches capacity. `src/text/text_layout.rs` passes the selected face
mutably to that owner and emits glyphs from the returned layout-shaping
carrier. Shaping flags, segmentation, variation ordering, buffer reuse, and
visible glyph output remain unchanged.

Private tests cover each key component, face isolation and recreation, the
bounded FIFO behavior, multilingual default-font glyph parity, and parity at
the supported minimum and maximum `wght` coordinates of the available local
Bahnschrift variable font. The test reads the system font only when it exists;
it does not bundle or require that asset on other hosts.

`src/text/font.rs` also owns a per-face LRU of up to 128 raw shaping results.
The cache key contains the complete run text, resolved script and direction,
language, and buffer flags; face identity is provided by the owning `FontFace`.
Only the proven layout-owned new-or-cleared buffer path with default cluster
policy and no actual HarfRust instance is admitted. Any actual variation
instance, unknown buffer provenance, pre-populated buffer, non-default cluster
policy, oversized text, or oversized entry follows normal shaping. Hits retain
the exact `GlyphInfo` and `GlyphPosition` arrays and return the cleared input
buffer for reuse before glyph emission.

An eligible raw-result hit returns before building the HarfRust shaper or
consulting the FIFO plan cache. Native misses and bypasses retain the existing
shaper, plan lookup/build, shaping, and admission path. A hit therefore does
not refresh or repopulate a plan that was evicted from the FIFO. The focused
`raw_hit_returns_before_repopulating_an_evicted_shape_plan` test verifies this
ordering using a retained raw result and eight distinct public plan keys.

The raw key is complete for the fixed layout call contract: empty feature list,
default HarfRust font-unit scale, and no point-size or custom shaping functions.
The proven fresh-or-cleared input also has empty pre/post context and the
default missing-variation-selector replacement policy. These options remain
outside the key because this production call site fixes them; future changes to
that contract must either extend the key or bypass raw reuse.

The cache allows at most 128 entries, 1 MiB of accounted payload per face,
4 KiB of run text, and 64 KiB per entry. Accounted payload is owned UTF-8 run
text, dynamic language bytes, and the complete retained glyph-info and
glyph-position elements. Fixed entry metadata, `Arc` control headers, `Vec`
spare capacity, and allocator overhead are outside that payload count. A
temporary shared handle can keep an evicted entry's arrays alive until the
corresponding glyph emission finishes.

The accepted archive-only raw-reuse prototype used frozen source manifest
`4F4CE0C6092301D159EDD33C0ADAFF436E563B012D30062EAA22F6F1C32CDA44` and
recorded complete ordered glyph and eager geometry parity for its 1,000-row
fixture. Its five-sample fast-dev owner medians were 373.0338 ms with reuse
disabled and 227.9875 ms with reuse enabled; the enabled owner recorded 3,046
hits and 1,001 misses among 4,047 face runs. These measurements describe the
instrumented archive prototype only. They do not establish production timing,
warm, release, or native acceptance. The live private tests cover LRU and
payload bounds, exact hit/miss output, buffer allocation recycling, unknown
buffer and custom-policy bypass, and variation-instance bypass at Bahnschrift
`wght` 300 and 700.

The accepted archive-only earlier-hit comparison passed complete reference
glyph and eager-geometry parity. Its five-sample cold owner medians were
227.9514 ms before and 217.8085 ms after moving the raw hit ahead of unused
shaper and plan setup. That diagnostic also skipped existing plan-lookup
observer work on hits, so the measurements do not quantify production savings.
The full evidence and limitations were accepted at checkpoint `75a611d0` and
are recorded in
`../../docs/plans/egui-modernization/receipts/m2a-earlier-raw-hit-setup-candidate.md`.

The accepted archive-only prototype and its limits are documented in
`docs/plans/egui-modernization/receipts/m2a-plan-reuse-prototype.md` at
checkpoint `b44af80f`. Its complete source manifest receipt has SHA-256
`e4513341f834f44f4387be9951c9350cf42dabeb258dfa0a8310fade35b9d3ca`; the
preserved prototype owner binary has SHA-256
`943b3973896f7ed391d3d94eb47a485f158bded77167c17cc0277f4cd0833029`, and its
raw stderr log has SHA-256
`c66f7e265cf127c5c8de4050b23f05d15fbedc0966489cdc3d35a995eaa4af3b`. That
prototype established the bounded cold-cost result and default-font parity; it
did not establish variable-font glyph parity, which the focused live-package
test now checks at both supported `wght` extrema.

Remove this override after an upstream epaint release provides equivalent
face-owned bounded plan and raw-result reuse, and the locked egui stack can use
that release with the focused glyph and geometry parity tests passing.

## Released text-stack alignment

The production bounds intentionally move HarfRust from 0.12.0 to 0.13.3 and
Skrifa from 0.44.0 to 0.46.2 without changing either crate's enabled features.
HarfRust retains its default std feature; Skrifa retains std and
autohint_shaping with default features disabled. This alignment brings
read-fonts 0.43.3 into the epaint closure while retaining font-types
0.12.6 and once_cell 1.21.4. Cargo.lock retains Skrifa 0.44.0 and
read-fonts 0.41.0 for the existing glifo to vello_cpu dependency path on
other-platform or optional target paths. On the validated Windows target,
version-qualified reverse-tree queries for those older versions produced no
active dependency output, so they are lockfile entries rather than an active
duplicate Windows closure. These older entries were not upgraded.

Crates.io archive SHA-256 values and declared licenses from the accepted
Windows-target graph are: HarfRust 0.13.3,
948d0741125ba89cd3e1c23e5642415b6ade7e1d29d67ba25fb925b533e989d6
(MIT); Skrifa 0.46.2,
d4febebda507fb889c545a37a135517769d1391941013561292897961d1887d6
(MIT OR Apache-2.0); and read-fonts 0.43.3,
005c8acf251756c478b0bf402885bfd88a1476020c4c7e6060edc1aa68da38ea
(MIT OR Apache-2.0). The retained font-types 0.12.6 archive hash is
6c92d9081c7ab3bd83ef1eecdfad090f3d966e3ce74106779664114cfe1a6274,
and once_cell 1.21.4 is
9f7c3e4beb33f85d45ae3e3a1792185706c8e16d043238c593331cc7cd313b50.

The isolated released-stack candidate recorded at evidence checkpoint
a36547bf matched complete ordered glyph output and ROOT geometry/output
identity. Its five-sample 1,000-row cold owner medians were 390.7041 ms on the
0.12.0 stack and 370.1820 ms on the 0.13.3 stack. This narrow CPU owner result
does not establish warm, release, native, or whole-application parity and does
not complete or accept M2-A.

The local delta consists of the bounded per-face FIFO plan reuse, bounded
raw-result reuse, and these compatible dependency bounds. Remove this patch
only after an upstream epaint release supplies equivalent bounded face-owned
plan and raw-result reuse and the locked egui stack can use it with focused
glyph/geometry parity passing.

## Provisional paragraph glyph capacity hint

`src/text/text_layout.rs::layout_section` reserves additional slots in the
current paragraph glyph vector once for each nonempty newline-delimited
segment, using `segment.chars().count()` as the capacity hint. This is a hint
based on Unicode scalar count, not a limit or a promise of one output glyph per
character; the vector grows normally when shaping emits more glyphs.

This local delta was evaluated in the archived experiment at checkpoint
`829dbac6`; see
`../../docs/plans/egui-modernization/receipts/m2a-paragraph-glyph-capacity-experiment.md`.
The result is provisional: the baseline ran first, the observed ranges overlap,
and timing drift prevents attributing a causal production gain. No quantified
production savings are claimed from that experiment or from this source port.

Remove this local line when an upstream epaint release provides equivalent
per-segment capacity reservation and the locked egui stack passes focused glyph
and geometry parity. The broader local override still follows the removal
condition stated in the preceding sections.
