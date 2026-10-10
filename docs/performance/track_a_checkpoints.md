# Track A checkpoint ledger

## Provenance and execution contract

- Branch: `performance-optimization`.
- Base: `02399cf0848de2b429dfd9fae80a141ce50d972d` (clean `performance` checkout).
- The user explicitly confirmed current HEAD as authoritative and waived the referenced ZIP comparison on October 9, 2026.
- Both companion plans in `docs/plans/` and repository/agent instructions were read. Cited source mechanisms correspond to this checkout; shifted native/Quick Notes line hints are nonmaterial.
- Remote `origin/performance` was confirmed at the base SHA using `git ls-remote`. Feature branch did not exist locally or remotely. Push write access is not inferred from read access.
- User steering on October 9, 2026: keep checkpoint commits local; do not attempt further pushes. This overrides the original push cadence. The first failed push is historical only.
- Track B is excluded. Implementation writers run sequentially. Parent owns commits/pushes and this ledger.
- Results use PASS / FAIL / NOT RUN / NOT MEASURED / SKIPPED. A pending checkpoint is not complete.

| Checkpoint | Objective | State | Tests / evidence | Review | Commit | Push |
| --- | --- | --- | --- | --- | --- | --- |
| M0-A | Bounded opt-in owner metrics | complete | PASS: 3 performance library tests; changed-file rustfmt and diff checks | planner + parent diff review; corrections resolved | 193b300c8fd2704e565a38e0928122201c7ada1b | FAIL: GitHub credentials unavailable |
| M0-B | Deterministic isolated fixtures | complete | PASS: 1 builder test, all 6 small owner harnesses, 1 unprofiled backlink test; rustfmt/diff checks | independent scoped review findings resolved | d003b4a35ef3fafd75438493199cb20463378baf | LOCAL ONLY |
| M0-C | Freeze pre-change baseline | complete | PASS: 6 full owner tests/39 scenarios; 1 repeat owner/2 stable-signature scenarios | parent source/signature/counter audit; limitations explicit | e9695fe4edda98eb12f0e5ca4f8f5b12465e6bcc | LOCAL ONLY |
| M1-A | Consistent note revision publication | complete | PASS: 16 focused unit + 4 notes_plugin integration tests; formatting/diff checks | independent review; fixture cleanup finding resolved with regression | a47461ab7f6f312858ae98a62fe76cf360061645 | LOCAL ONLY |
| M1-B | Cheap note refresh gate | complete | PASS: 21 focused panel tests + 1 small owner; 20 idle checks have zero snapshots/alias hashes/recomputes; rustfmt/diff checks | independent retry/debounce/repaint findings resolved | 9793d385a6165a6032165398c85b1d0b46b4c737 | LOCAL ONLY |
| M1-C | Notes regression and comparison | complete | PASS: 6 full note scenarios, exact signatures; 30 focused library + 4 notes_plugin + 1 save-on-close tests; rustfmt/diff checks | independent findings resolved; slower draft timing retained with unresolved cause | 0ebd6fbce9d7f5a2418dde9b235275484d80ab55 | LOCAL ONLY |
| M2-A | Shared command resolution context | complete | PASS: 7 focused library + 2 routing tests + 1 small owner; 4 signatures match baseline, one catalog/prepare; rustfmt/diff | independent scoped review clear | 34d85982b83f83328e53a272a7efb9dd16991ba5 | LOCAL ONLY |
| M2-B | Bounded matching history candidates | complete | PASS: 13 focused library + 1 small owner; zero full-record copies, common8 resolutions/prepare, exact signatures; rustfmt/diff | independent review clear; parent allocation/benchmark assertions corrected | c396ef3f0b45c7f2354ab5657984378931336866 | LOCAL ONLY |
| M2-C | History parity and comparison | complete | PASS: 12 full scenarios, exact signatures/counters; 2 parity + 5 history integration tests; rustfmt/diff | independent scoped test review clear | 9b7447fdf48f6aec6cde61d23512b1f3c36b5f6f | LOCAL ONLY |
| M3-A | Visible launcher list rows | complete | PASS: 9 focused library + 1 small owner + 6 full scenarios; 10k warm p95 0.9719ms/28 widgets, cold631.98ms; rustfmt/diff | independent geometry/font/isolation/menu findings resolved | 89fe30f9f1295ddb69fe1d5672dd79501bfc3fd6 | LOCAL ONLY |
| M3-B | Complete visible grid rows | complete | PASS: 14 focused library + 1 small owner + 6 full scenarios; 10k grid warm p95 1.5626ms/48 widgets, cold588.15ms; rustfmt/diff | independent geometry/extent/spill/click findings resolved | ed106cd6626240f3cc998bf3191ffafc9c3f837c | LOCAL ONLY |
| M3-C | Lightweight Quick Notes projection | complete | PASS:22 focused +1 lifecycle +1 small owner +6 full scenarios; exact signatures/zero warm snapshots; rustfmt/diff | independent atomic publication/draft/preview findings resolved | 369744ef9c697ea2cfddca3077ce61407b46b6b8 | LOCAL ONLY |
| M3-D | Variable-height Quick Notes virtualization | complete | PASS:25 module +1 small owner +9 full scenarios;5k warm p95 .6256ms/6widgets,cold small627ms;zero warm geometry/snapshots;rustfmt/diff | independent scoped review clear | 5068bfec9f304496613d5291d1fcafc42aeaf09d | LOCAL ONLY |
| M4-A | Actions reload reuses indexed tail | complete | PASS:2 focused+1 strengthened+2 integration+1 small+6 full scenarios;zero scan/exactsignatures;10k changed p95 110.5174ms;rustfmt/diff | independent production review clear | 6257e81795a2c50a0d8d39b128ce1f186f1816e3 | LOCAL ONLY |
| M4-B | Bounded scan worker | complete | PASS:11 coordinator +3 domain indexer;bounded queues/cancel/nonblocking lifecycle;rustfmt/diff | independent lifecycle/metric/wake findings resolved | 3468a51dec84c78298de28722aa1317a72b70992 | LOCAL ONLY |
| M4-C | Generation/config guarded publication | complete | PASS:11 GUI+2 startup+3 domain+1 settings;bincheck/fmt/diff | independent retry/startup/isolation/lifecycle findings resolved | 3c0446e88747820a6a32321df10f734a73f85f93 | LOCAL ONLY |
| M4-D | Watcher race and stall evidence | complete | PASS:10 lib+4 domain+2 watcher+2 small+2 full owners/12scenarios;zero reload scans;10k request p95 .0193ms;fmt/diff | independent scoped review clear | f5560cc3660b8187478623ff65fbe1e87c665166 | LOCAL ONLY |
| M5-A | Tick contract characterization | complete | PASS:6 opt-in serial worker +24 scoped compatibility;API receipts match metrics;fmt/diff | independent scoped review clear | 7696f0dc9faf99adfb4c7a74ab56169bcced8c00 | LOCAL ONLY |
| M5-B | Current-source presentation ordering | complete | PASS:25 opt-in serial scoped tests;smoke-bin check;moving0refresh/1present,stationary1refresh/0present;fmt/diff | independent scoped review clear | ab3eace9723280cbe0a00f625b1d0b3949035aac | LOCAL ONLY |
| M5-C | Native reliability/review/comparison | complete | PASS:16 real Windows API/lifecycle stages;stationary44samples/no new fullrender;zero HWND cleanup;visual/latency/mixedDPI NOT MEASURED | independent native-log review clear | f7049863d6ebe8968b04d4002441d936fc345a27 | LOCAL ONLY |
| M6-A | Sampling/HUD measurement decision | complete | PASS:5 collector tests/bincheck;87.277s real profile,2684samples,zero errors/drops;fresh copy/cleanup | independent source/native evidence clear | 3d3dac5df4e78ea08e191e209f646979a57609e3 | LOCAL ONLY |
| M6-B | Conditional display metadata reuse | SKIPPED (measured) | metadata p95 .3286ms;fresh geometry/invalidation complexity exceeds demonstrated benefit | independent evidence review clear | M6-A decision | LOCAL ONLY |
| M6-C | Conditional retained GDI resources | SKIPPED (measured) | mean12.58us creation/redraw,balanced1917brush/fontpairs,stableHUDGDIcount | independent evidence review clear | M6-A decision | LOCAL ONLY |
| M7-A | Bounded cross-surface regression | complete | PASS:17 library+2 startup+3 integration;M5/M6 evidence reused;diffcheck | independent cumulative review clear | this checkpoint | LOCAL ONLY |
| M7-B | Final comparable report | in_progress | — | — | — | — |
