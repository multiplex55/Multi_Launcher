# C2-F Quick Notes prefix candidate execution

State: candidate verification IN_PROGRESS; runtime acceptance PENDING. LOCAL ONLY.
The frozen contract is `track_c_quick_notes_prefix_gate.md` and its JSON. The
reviewed characterization source is804d221cdb4c6f0145213ac52f2ef7b2cb4b77c6;
baseline/contract authorization isfe9f2f885699e85f853b6fbc0727869c0c619c22.
The original baseline collector and collected baseline JSON remain unchanged.

## Implementation and source review

Only `src/gui/notes_dialog.rs` owns the runtime experiment. One optional private
proof binds predecessor geometry, accepted destination generations and the complete
unchanged metadata prefix. All snapshot/index/metadata preparation remains complete.
The comparison runs before the final revision recheck. A shared geometry builder
copies verified prefix rows, measures the suffix and constructs the complete map
and extent. The predecessor remains intact for existing anchor reconciliation.
Environment, atlas, original-index/identity and exact f32 recurrence checks reject
ineligible reuse. Unrendered installs cannot chain proofs; edit, reindex, metadata
settings and missing-geometry paths clear or consume authority.

Independent source/method review CLEAR for scoped execution. All18 protected C0 and
C2-F reference helper bodies remain identical. Review corrected a test-only foreign
font-atlas setup to run one inert egui frame before reading fonts. Parent review
moved proof comparison before the canonical revision publication check.

Candidate reporting is explicitly schema2, stage `quick_notes_prefix_candidate`:
51 arrays and20 scalars. Actual compared/reused/newly-measured row counts are
separate; historical complete projected-row metric meaning remains unchanged.
The companion collector validates source/profile/fixtures, all receipts, counters
and phase arithmetic. The comparison checker accepts only collected Full output
and compares all32 frozen gates using integer nanoseconds. Same-engine PowerShell
7.6.5 synthetic parser positive/negative cases and checker rejection cases passed;
synthetic records are not performance evidence.

## Executed verification

- First ordinary attempt: compile101, zero tests, preserved raw log. Rust rejected
  a cfg attribute on a counter assignment expression. A cfg block corrected it;
  two new unused test bindings were removed. Existing render warnings were retained.
- Exact serial fast-dev ordinary retry:11/11 PASS, compile2m03s, tests4.553s.
  Includes existing editor/save/retry/anchor/popup/eager checks, recipe contract,
  and new prefix lifecycle contract. All use audited owned synthetic fixtures.
- Non-test `cargo check --lib --profile fast-dev`:PASS24.63s.
- Candidate Small owner:1/1 PASS32.240s;10 records/200 retained samples, five warmups
  and20 samples each, zero exclusions. Strict companion collection PASS with the
  actual proof-size receipt152. Independent collected-output review CLEAR:510 arrays/10,200 entries,200 scalars,
  100 timing triplets,370 baseline common arrays and1,600 identity hashes checked.

The ordinary receipt `size_of::<Option<NotesGeometryPrefixProof>>()` is152 bytes on
this build. This is inline field storage, not the containing dialog size delta or
whole-owner peak memory. Named FontFamily values use shared `Arc<str>`; cloning
the key shares text without copying it. Shared name payload/control blocks, other
derived/geometry overlap, temporaries, allocator traffic and RSS are excluded.
Default fixture fonts have no named-family payload.

## Small diagnostic observations

These uncommitted-source measurements are diagnostics; they do not decide the
frozen Full gates. For100 notes, final-note refresh owner p50/p95 decreased from
16.6087/17.6571ms to4.9761/5.6924ms. Contrary controls remain visible: warm640 p95
0.7028→0.9440ms; cold180 max14.3993→46.7155ms; width transition p95
11.7031→14.0046ms; sparse-search p95 2.5020→2.6625ms; cold180 p95
11.9608→14.4391ms. All10 distributions remain in the owned raw/collected evidence.
No exclusions, threshold changes or performance reruns are
authorized to obtain acceptance.

## Remaining gate

After independent Small review, commit the reviewed candidate locally and execute
Full30 records/600 samples on that exact clean SHA. All32 frozen numeric gates,
complete semantic/eager/work parity, bounded ownership and independent review
must pass. Otherwise restore the exact accepted804d source while preserving the
rejected candidate history and evidence. Native display, production frequency,
allocator traffic and process RAM are NOT MEASURED.
