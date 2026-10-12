# C2-G accepted-query CPU observation evidence

Exact-source schema2 Full: `c6c0f64a5f10f290e95c43bdf2a7104a2a209e67`, fast-dev, five warmups and twenty retained samples per recipe, zero exclusions. Owner, strict collector and independent final review PASS. All126timing triplets and57step bindings independently reproduced.

These whole-sequence times include interstep eager validation, signature work and both neutral viewport frames. They are not production typing or native input-to-display latency. Per-intent completion ends at the first eligible actual CPU frame before its own post-frame validation; these separate distributions are retained in full_v2.json. The nine-frame viewport controls are not comparable to historical schema1 seven-frame controls.

| N | Recipe | Whole p50 / p95 / max ms |
| ---: | --- | ---: |
| 100 | typed_suffix_sequence | 61.8031 / 67.4023 / 77.4222 |
| 100 | app_provider_supersession | 43.1776 / 45.0516 / 45.1127 |
| 100 | equivalent_filter_sequence | 34.2836 / 36.8225 / 37.2144 |
| 100 | viewport_controls | 149.8123 / 181.5663 / 195.6061 |
| 1000 | typed_suffix_sequence | 246.8538 / 257.8521 / 283.3576 |
| 1000 | app_provider_supersession | 164.2932 / 178.0524 / 182.5855 |
| 1000 | equivalent_filter_sequence | 214.6304 / 223.5654 / 229.8617 |
| 1000 | viewport_controls | 818.6603 / 833.7279 / 837.4540 |
| 10000 | typed_suffix_sequence | 1604.0983 / 1768.6790 / 1906.0662 |
| 10000 | app_provider_supersession | 1482.4563 / 1596.2419 / 1812.2029 |
| 10000 | equivalent_filter_sequence | 2095.4850 / 2182.0998 / 2210.3151 |
| 10000 | viewport_controls | 8091.1817 / 8669.6674 / 8763.8767 |

At10k the first typed intent CPU completion is551.4688/622.6444/635.6009ms p50/p95/max. Later typed suffix completions are13.4903/17.7176/19.6086,7.9670/9.6408/9.7690 and7.5004/8.4674/10.6892ms. Cold geometry remains expensive; observation is not an optimization. All full distributions and adverse tails are retained.

Full accounting:12records/240sequences/600intents/1140frames,540completed/60superseded/0cancelled. Per-intent bound8 is separate from nine no-query control frames. Prior failed Full/oracle attempts are preserved; partial runs are not accepted. Schema1 collector/bindings/Small are immutable. Production animation and all42 frozen eager helpers remain unchanged.

Native/hotkey-to-visible/GPU/allocator/RSS and production workload frequency remain NOT RUN/NOT MEASURED. LOCAL ONLY.
