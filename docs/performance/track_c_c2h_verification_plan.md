# C2-H focused acceptance plan — LOCAL ONLY

Accepted starting source: `6340c6b50200ab3a89fb8f182c98a10671973409`.
C2-G is closed; 18 of 29 checkpoints are closed. C2-H is in progress.

One source milestone isolates five legacy Notes tests using the existing owned
workspace and inert app fixture. It removes the obsolete test constructor and HOME
mutation, preserves initial empty-note save and all assertions, and restores the
failed-save override before committed-note checks. Workspace lifetime must contain
app/dialog/worker lifetime; lock order remains TEST_MUTEX then workspace lock.
The only render edit is dashboard refresh suppression immediately after the direct
constructor in the original Track A warm owner. No production or metric change.

Independent fixture review precedes execution. Parent runs all 28 ordinary Notes
tests plus four exact render contracts: bounded list IDs/selection, bounded grid
IDs/click targets, tall-cell spill, and retained grid popup owner. Ignored owners are
separate. Use serial fast-dev Nextest and process-scoped clipboard suppression.
Reuse unchanged accepted C2-G provider/fallback/history/radial/lifecycle evidence;
do not repeat unrelated tests or historical candidate campaigns.

The comparison milestone uses five original owners: shared search, list geometry,
grid geometry, Quick Notes geometry, and original root query CPU frame. Small has
32 records/640 samples; Full has 96 records/1920 samples, including 39 cold/warm
geometry pairs and six horizontal-width receipt arrays. Preserve frozen fixture
identities, font/viewport/settings, five warmups, twenty samples and zero exclusions.
Run the retained C2-C owner separately (Small3/Full9 records). Run the original
Track A warm owner once in Full after its fixture review (six records), labelled
absolute fast-dev evidence: historical debug-test results are not comparable.

Parent freezes exact-source runners, collectors and subset joins before measurement.
Full requires clean committed source. Original owner collector is reusable, but the
105-record baseline inventory validator is not the correct subset gate. Produce
`track_c_search_g1.json` (15 search plus three original query records) and
`track_c_geometry_g1.json` (78 geometry records). Join against immutable G0 by exact
fixture/recipe/settings/viewport/font identities. Keep complete output, structural
output and viewport identities separate; preserve their revision semantics.

Disclose the owned root fixture's dashboard suppression versus historical G0 and
test observer overhead. Whole measured owners can include eager assertions; their
times are not native interaction latency. Do not attribute every difference to an
optimization. Retain unfavorable p50/p95/max, work counts, rejected candidate
decisions, and remaining cold stalls. Native/GPU/allocator/RSS remain unmeasured.

Done when the fixture migration and 32 ordinary checks pass, exact-source Small and
Full collections and subset identity/work comparisons pass, independent reviewer
clears C2 and conditional dispositions, and a coherent local acceptance commit is
made. No broad suite, release build, native probe or user-data operation.

Fixture source/selection review CLEAR and32ordinary tests PASS5.679s, Nextest
31e8a7de-2b63-4095-8bc1-ff817d1c4119 (compile107s). All42referencehelpers unchanged.
Companion comparison validator independently CLEAR24syntheticcontrols; warm legacy
collector CLEAR15syntheticcontrols. Findings corrected before measurement: strict
numeric types before key/parity checks, a rehashed Notes progression negative, exact
legacy summary/geometry/metric fields and nested aggregate timing maxima. These
synthetic controls are not benchmark evidence. Exact-owner execution safety review
and protocol freeze precede local fixture commit and measurement.

Independent premeasurement execution review CLEAR. protocol_freeze.json records
all source/method/reference/runner hashes and exact inventories before any owner
measurement. Reviewed fixture+protocol commit is permitted; Small/Full output
acceptance remains pending.
