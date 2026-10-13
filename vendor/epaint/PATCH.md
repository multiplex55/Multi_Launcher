# Local epaint shaping-plan patch

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
mutably to that owner and still emits glyphs only after shaping returns its
`GlyphBuffer`. Shaping flags, segmentation, variation ordering, buffer reuse,
and glyph output remain unchanged.

Private tests cover each key component, face isolation and recreation, the
bounded FIFO behavior, multilingual default-font glyph parity, and parity at
the supported minimum and maximum `wght` coordinates of the available local
Bahnschrift variable font. The test reads the system font only when it exists;
it does not bundle or require that asset on other hosts.

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
face-owned bounded plan reuse, and the locked egui stack can use that release
with the focused parity tests passing.

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

The local delta consists of the bounded per-face FIFO plan reuse above and
these compatible dependency bounds. Remove this patch only after an upstream
epaint release supplies equivalent bounded face-owned plan reuse and the
locked egui stack can use it with focused glyph/geometry parity passing.
