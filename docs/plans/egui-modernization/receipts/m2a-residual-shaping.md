# M2-A retained-plan shaping attribution

Independent review accepted the archive-only probe as attribution evidence, with no findings. Live production remains checkpoint87e08b32. M2-A performance acceptance remains open.

Copied stock HarfRust0.12.0 package SHA256 `c03d949a14aa089bbb282f7dd76a498a7f684428e4257202efc119ec010376f9` into the existing ignored actual-layout archive. Both modes use the accepted epaint retained-plan path; only aggregate HarfRust clocks switch off/on. Complete-call, setup, default substitution, planned substitution and positioning clocks preserve shaping order. Lazy charmap/metrics timers run only inside actual initialization closures and nest within those phases. No per-glyph clocks or production instrumentation.

One exact ignored Nextest test passed, native0,30.894s command/28.101s test. Five samples per mode,two warmups each,only1,000 cold rows,real nine-pass preparation outside timing. All ten samples retained4,047 runs/plan hits,zero misses/builds,103,830 glyphs,one rebuild/1,000 measured/25 painted rows/one pass and output identity bd4860c5d2427fa2. Untimed complete ordered glyph and eager geometry parity passed. Parent independently checked work/partition arithmetic; reviewer verified all phase arrays,source and binary bindings.

| Clock-on phase | p50 ms |
|---|---:|
| Complete shape_with_plan |214.0174|
| Scale setup |0.2364|
| Font dispatch setup |0.5147|
| Default substitution |13.0248|
| Planned substitution |26.9168|
| Position |162.6545|
| Explicit remaining call time |10.7752|
| Nested FontRef charmap initialization |0.3144|
| Nested FontRef metrics initialization |0.9438|

Medians are separate statistics and must not be added as one sample. Every sample's coarse phases plus explicit remainder reconcile exactly. FontInstance lazy initializers recorded zero work. Position dominates; this does not identify a specific GPOS operation. Owner off/on medians394.7156/396.1604ms; epaint-shape213.7221/215.6612ms. Five samples do not establish negligible observer overhead or original parity.

Evidence under target/egui-modernization/m2a/actual-layout-probe/receipts: final96-row source manifest003 SHA256 `af3c93c543a42d02032650ca2ec67c1aa10071525e3c41c46e9a6363975bc13f`; raw residual-shaping-004.stderr.log SHA256 `02e08bfc603b92cbb90e2b9205b489f0415821e29672894e3e59fa1674ee4d8b`; preserved138,918,912-byte binary SHA256 `117b082174ca804c922bb6ef2b79dcc7e523f02ab2703fb91ed7137f50a3dceb`. Build native0/171.933s; Cargo JSON selects actual binary. Original receipt presentation errors and an incomplete intermediate source manifest are retained with corrected003 source and command/result002 addenda. Start wall timestamp unavailable; completion2026-10-13T16:57:17.9495472-04:00. Accepted previous source/binary preserved before modifications.

Next bounded lead: isolated aligned released HarfRust0.13.3/Skrifa0.46.2/read-fonts0.43.3 candidate, preserving production and accepted archive. Planner verified compatible reader bounds and retained public APIs. Newer0.14.0 requires a larger API migration and is outside this candidate. Released0.13.3 still reparses PairPosFormat2; issue256 is not evidence that this specific cost is fixed. Cross-version complete canonical glyph snapshots and eager parity must pass before a matched1,000-cold comparison. Candidate timing does not automatically authorize production adoption. See implementation-handoffs.md.
