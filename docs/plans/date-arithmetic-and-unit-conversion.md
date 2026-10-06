\# Multi Launcher — Goal B: Date Arithmetic + Unit Conversion Expansion



\*\*Suggested path:\*\* `docs/plans/date-arithmetic-and-unit-conversion.md`



\*\*Status:\*\* Ready for implementation  

\*\*Source of truth:\*\* Current checked-out Multi Launcher repository. The reviewed baseline was `multi\_launcher(20261005-221056).zip`.



\---



\# 1. Goal



Expand Multi Launcher into a substantially more capable \*\*local/offline unit-conversion and date-arithmetic utility\*\* while preserving the behavior of the existing calculator, timestamp commands, base conversion, Convert panel, plugin enablement, and persisted settings.



This goal contains two related capabilities:



1\. \*\*Unit Conversion Expansion\*\*

&#x20;  - Expand the unit catalog.

&#x20;  - Accept more natural and useful conversion syntax.

&#x20;  - Support fractions and common compound measurements.

&#x20;  - Correctly distinguish data/storage semantics, US/Imperial volume, and other ambiguous unit families.

&#x20;  - Replace fixed four-decimal output with smart formatting.

&#x20;  - Make the inline converter and Convert panel use the same physical-unit conversion source of truth.



2\. \*\*Date Arithmetic\*\*

&#x20;  - Introduce a new explicit `date` launcher command.

&#x20;  - Support relative dates, calendar-aware month/year arithmetic, weekday anchors, common local date formats, and date differences.

&#x20;  - Keep `date` independent from the existing `ts` / `tsm` timestamp commands.



Everything in this goal must work \*\*locally\*\*.



Do not use:



\- remote APIs;

\- web services;

\- cloud computation;

\- online conversion services;

\- online holiday services;

\- external exchange-rate services.



\---



\# 2. Required Commit Policy



\*\*Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.\*\*



\## Active Checkpoint Commit Cadence



Use an active, milestone-based commit cadence throughout this task.



Break larger milestones into coherent implementation checkpoints such as

`M1-A`, `M1-B`, `M2-A`, etc., and commit after each meaningful subsection is

complete rather than waiting for an entire large milestone or feature to finish.



I want Git history to show visible progress and make it easy to understand what

was implemented at each stage.



Use judgment on commit size:



\- Do NOT commit every tiny edit or individual line.

\- Do NOT create meaningless WIP/checkpoint commits.

\- Do NOT let several substantial, independently understandable changes

&#x20; accumulate into one very large commit.

\- Before beginning a materially different subsection, prefer committing the

&#x20; previous coherent subsection.



Use descriptive commit messages with the plan-stage identifier:



`<type>(<scope>): \[M#-X] <clear description>`



For example:



`refactor(settings): \[M2-B] organize launcher window and appearance controls`



When useful, include a short commit body explaining what changed, why, and what

behavior was intentionally preserved.



Do not run expensive full verification before every commit. Use small/local

checks where useful, commit coherent checkpoints, and perform the plan's

substantive targeted verification at the appropriate verification milestone.



If later testing or review finds a defect, prefer a clearly described follow-up

remediation commit rather than silently folding unrelated fixes into an earlier

checkpoint.



Do not squash or rewrite the checkpoint history unless I explicitly request it.



When creating the implementation plan, explicitly identify natural commit

boundaries and suggested stage IDs/commit subjects.



\---



\# 3. Current-State Findings



The implementation must begin from the actual current repository rather than assumptions from this plan.



The following current-state observations were verified against the reviewed source and should be rechecked briefly before editing.



\## 3.1 Inline unit conversion



Current owner:



\- `src/plugins/unit\_convert.rs`



Current behavior includes:



\- `conv <value> <unit> to <unit>`

\- `convert <value> <unit> to <unit>`



Existing categories already include portions of:



\- length;

\- mass;

\- temperature;

\- volume;

\- area;

\- speed;

\- pressure;

\- energy;

\- power;

\- digital storage;

\- time/duration;

\- fuel economy;

\- angle.



Current limitations include:



\- unit normalization is primarily lowercase/string matching;

\- the parser expects a rigid simple structure;

\- compound values such as `6 ft 2 in` are unsupported;

\- fractions such as `1/2 cup` are unsupported;

\- multi-token unit names are poorly suited to the current parser;

\- results are forced to four decimal places;

\- the clipboard action currently copies only the number;

\- several useful categories/units are absent;

\- some conventional case-sensitive data-unit distinctions cannot be represented correctly by unconditional lowercasing.



\## 3.2 Convert panel



Current owner:



\- `src/gui/convert\_panel.rs`

\- `src/plugins/convert\_panel.rs`



The panel currently maintains its \*\*own physical-unit catalog and conversion functions\*\* instead of using `UnitConvertPlugin`'s definitions.



Its categories currently include:



\- Distance

\- Mass

\- Temperature

\- Volume

\- Base



This creates two physical-conversion sources of truth.



There are already visible differences between them. For example, the panel includes some units that the inline converter does not.



The physical-unit portions of the panel should be migrated to a shared conversion domain.



The panel's \*\*Base\*\* category is conceptually different and must remain functional.



\## 3.3 Base conversion



Current owner:



\- `src/plugins/base\_convert.rs`



It also uses the `conv` / `convert` prefixes.



It supports conversions such as:



\- binary;

\- hexadecimal;

\- octal;

\- decimal;

\- text representations.



Do \*\*not\*\* fold base conversion into the physical-unit engine merely for architectural neatness.



The expanded unit converter must continue coexisting correctly with this plugin.



Queries such as:



`conv ff hex to dec`



must continue being handled by `BaseConvertPlugin` without the physical-unit converter producing a competing misleading error.



\## 3.4 Timestamp behavior



Current owner:



\- `src/plugins/timestamp.rs`



Existing commands include:



\- `ts`

\- `tsm`



These commands have existing tests under:



\- `tests/plugin\_cases/timestamp\_plugin.rs`



Do not overload these commands with date arithmetic.



`date` is a new independent command.



\## 3.5 Existing conversion tests



Relevant current test files include:



\- `tests/plugin\_cases/unit\_convert\_plugin.rs`

\- `tests/plugin\_cases/base\_convert\_plugin.rs`

\- `tests/plugin\_cases/convert\_panel\_plugin.rs`

\- `tests/plugin\_cases/timestamp\_plugin.rs`



They are collected by:



\- `tests/suites/plugin\_queries.rs`



and exposed through the `plugin\_queries` Cargo test target.



Extend these tests rather than establishing an unrelated test architecture unless domain-level unit tests need to live next to new shared modules.



\---



\# 4. Product Requirements



\## 4.1 Invocation



Preserve:



\- `conv`

\- `convert`



A bare:



`conv`



or:



`convert`



must continue making the interactive Convert panel available.



Complete queries continue using explicit conversion prefixes:



\- `conv 10 km to mi`

\- `convert 12 oz to g`



Do not globally interpret arbitrary launcher queries as conversions.



\---



\# 5. Unit Conversion Requirements



\## 5.1 Required physical categories



Support a coherent catalog covering:



\### Length / distance



Include common forms of:



\- millimeter

\- centimeter

\- meter

\- kilometer

\- inch

\- foot

\- yard

\- mile

\- nautical mile



\### Area



Include common forms of:



\- mm²

\- cm²

\- m²

\- km²

\- in²

\- ft²

\- yd²

\- mi²

\- acre

\- hectare



Accept practical spelling forms such as:



\- `m2`

\- `m^2`

\- `m²`

\- `square meters`



where reasonable.



\### Mass / weight



Include:



\- mg

\- g

\- kg

\- oz

\- lb

\- stone

\- metric ton / tonne

\- US short ton



Use explicit aliases where ambiguity matters.



\### Volume



Include:



\- ml

\- cl

\- l

\- cm³

\- m³

\- teaspoon

\- tablespoon

\- cup

\- fluid ounce

\- pint

\- quart

\- gallon



\### Temperature



Preserve and support:



\- Celsius

\- Fahrenheit

\- Kelvin



including common symbol forms where practical.



\### Speed



Include:



\- m/s

\- km/h

\- mph

\- ft/s

\- knot



\### Pressure



Include:



\- Pa

\- kPa

\- MPa

\- bar

\- atm

\- psi

\- torr / mmHg



\### Energy



Include:



\- J

\- kJ

\- MJ

\- Wh

\- kWh

\- cal

\- kcal

\- BTU

\- ft-lb

\- eV



\### Power



Include:



\- W

\- kW

\- MW

\- horsepower



Preserve legacy aliases that currently work even if newer conventional capitalization is added.



\### Time / duration



Preserve:



\- ns

\- µs/us

\- ms

\- seconds

\- minutes

\- hours

\- days

\- weeks

\- months

\- years



For the ordinary `conv` system:



\- month remains an approximate 30-day duration;

\- year remains an approximate 365-day duration.



This behavior is deliberately separate from calendar-aware `date` arithmetic.



\### Angles



Include:



\- degree

\- radian

\- gradian

\- arcminute

\- arcsecond

\- revolution / turn



\### Fuel economy



Preserve and support:



\- km/L

\- L/100 km

\- US MPG

\- Imperial MPG



\### Digital storage



Support:



\- bit

\- byte

\- KB / MB / GB / TB

\- KiB / MiB / GiB / TiB

\- corresponding bit forms



\### Digital data rates



Add rate conversions such as:



\- kbps

\- Mbps

\- Gbps

\- KB/s

\- MB/s

\- GB/s

\- Kib/s

\- Mib/s

\- Gib/s



\### Force



Include:



\- N

\- kN

\- lbf



\### Torque



Include:



\- N·m

\- lb-ft

\- lb-in



\### Frequency



Include:



\- Hz

\- kHz

\- MHz

\- GHz



\---



\# 6. Unit Semantics



\## 6.1 US versus Imperial



Unqualified common cooking/volume units use US customary definitions:



\- gallon

\- pint

\- quart

\- cup

\- fluid ounce



Add explicit Imperial aliases such as:



\- `imperial gallon`

\- `imp gal`

\- `imperial pint`

\- `imperial fl oz`



US and Imperial quantities must never be silently treated as equivalent.



\## 6.2 Ounces



Preserve bare:



`oz`



as mass/weight behavior.



Fluid volume must use explicit fluid-ounce forms such as:



\- `fl oz`

\- `fluid ounce`



Do not make plain `oz` contextually change from mass to volume.



\## 6.3 Tons



Use explicit distinctions:



\- `tonne`

\- `metric ton`



for metric tonnes.



Use:



\- `US ton`

\- `short ton`



for US customary short tons.



If retaining `ton` as a convenience alias, it should mean US short ton consistently and be documented.



\## 6.4 Data units



Do not lowercase all data-unit abbreviations before interpreting them.



Correctly support conventional distinctions such as:



\- `MB` = megabytes

\- `Mb` = megabits

\- `GB` = gigabytes

\- `Gb` = gigabits



Also provide explicit word aliases:



\- megabytes

\- megabits



Binary prefixes remain distinct:



\- MiB

\- Mib

\- GiB

\- Gib



Preserve previously valid legacy spellings where practical.



Backward compatibility takes priority over using capitalization to unexpectedly reinterpret a previously valid query.



If a legacy lowercase alias conflicts with a newly supported conventional abbreviation, preserve the legacy form and add the conventional exact-case form rather than silently changing old results.



The same compatibility principle applies to existing power aliases such as the current megawatt/milliwatt spellings.



\---



\# 7. Unit Input Grammar



\## 7.1 Simple values



Continue supporting:



\- integers;

\- decimals;

\- negative values where dimensionally meaningful.



Examples:



\- `conv 10 km to mi`

\- `conv -40 c to f`



\## 7.2 Fractions



Support:



\- `1/2`

\- `3/4`

\- mixed forms such as `1 1/2`



Examples:



\- `conv 1/2 cup to ml`

\- `conv 1 1/2 cups to ml`

\- `conv 3/4 in to mm`



Support common Unicode fractions where practical:



\- ½

\- ¼

\- ¾



The parser must reject zero denominators and malformed fractional forms cleanly.



\## 7.3 Compound measurements



Support common additive compound sources where all components belong to the same compatible linear dimension.



Required examples:



\- `conv 6 ft 2 in to cm`

\- `conv 5 lb 8 oz to kg`



The destination remains a single unit.



Compound input should be generalized enough that compatible expressions such as:



\- `1 cup 2 tbsp to ml`



can work if naturally supported by the parser.



Do not turn this into arbitrary dimensional algebra.



Reject nonsensical compound expressions rather than guessing.



Examples intentionally out of scope:



\- symbolic `kg\*m/s^2`

\- dimensional equation solving

\- arbitrary products or powers of unrelated units



\## 7.4 Unit aliases



Support:



\- singular names;

\- plural names;

\- common abbreviations;

\- common punctuation variants;

\- harmless whitespace variation.



Examples:



\- `kilometer`

\- `kilometers`

\- `km`



\- `foot`

\- `feet`

\- `ft`



\- `m2`

\- `m^2`

\- `m²`



Use longest-valid-alias matching where multi-word units require it.



Do not let unit aliases become an unstructured series of ad hoc parser exceptions.



\---



\# 8. Conversion Result Formatting



Replace fixed four-decimal formatting with one centralized deterministic smart-number formatter.



Required behavior:



\- integer-like results omit unnecessary decimals;

\- trailing zeroes are removed;

\- normal conversions retain useful precision;

\- floating-point noise is not exposed;

\- scientific notation may be used for extremely large/small magnitudes when it improves readability.



Examples:



Prefer:



`100 cm = 1 m`



over:



`100 cm = 1.0000 m`



Prefer something like:



`10 km = 6.21371 mi`



rather than forcing exactly four decimal places.



Do not introduce different formatting policies between:



\- inline conversion;

\- Convert panel;

\- date-difference numeric output.



Shared numeric formatting may be reused where appropriate.



\---



\# 9. Conversion Clipboard Behavior



Executing an inline unit-conversion result must copy:



`<formatted value> <destination unit>`



Example:



`6.21371 mi`



rather than only:



`6.21371`



The visible launcher result should still show the complete equation.



Example:



`10 km = 6.21371 mi`



Approximate month/year duration conversions should make their approximate nature understandable where appropriate, for example through result description or an approximation marker.



\---



\# 10. Shared Unit-Conversion Architecture



Physical/unit conversion must have a \*\*single domain owner\*\* outside the GUI and outside an individual plugin adapter.



Recommended architectural shape:



`src/unit\_conversion/`



or an equivalent clearly named non-GUI domain module.



The exact internal file split is left to implementation judgment, but ownership should conceptually cover:



\- unit/category definitions;

\- aliases;

\- conversion factors/strategies;

\- parsing;

\- conversion evaluation;

\- output formatting;

\- typed conversion errors.



Possible domain concepts include:



\- `UnitCategory` / `UnitDimension`

\- `UnitId`

\- `UnitDefinition`

\- `ConversionRequest`

\- `ConversionOutcome`

\- `ConversionError`



Do not introduce types merely for ceremony. Use only enough structure to prevent duplicated catalogs and stringly typed cross-category logic.



The following must consume the same physical-unit definitions:



\- `UnitConvertPlugin`

\- physical categories in `ConvertPanel`



The following does \*\*not\*\* need to be migrated into this physical-unit model:



\- `BaseConvertPlugin`

\- Convert panel's Base category



\---



\# 11. Date Arithmetic Command



Introduce:



`date`



Examples:



\- `date 30 days from today`

\- `date 2 weeks from tomorrow`

\- `date 3 months after 2026-10-05`

\- `date 10 days before Christmas`

\- `date today + 10 days`

\- `date Friday - 3 weeks`

\- `date days between 2026-10-05 and 2026-12-25`



Do not require a dialog for ordinary date arithmetic.



Do not interpret random launcher text as date arithmetic without the `date` prefix.



\---



\# 12. Date Input Formats



Support:



\## ISO



\- `2026-10-05`



\## US numeric



\- `10/5/2026`

\- `10/05/2026`



Numeric slash dates use US month/day/year semantics.



Do not guess unsupported ambiguous international numeric forms.



\## Written dates



Support common forms such as:



\- `October 5 2026`

\- `October 5, 2026`

\- `Oct 5 2026`

\- `5 October 2026`



Case should not matter for month names.



Invalid calendar dates must be rejected.



Example:



`2026-02-30`



must not normalize into another date.



\---



\# 13. Relative Date Anchors



Support:



\- `today`

\- `tomorrow`

\- `yesterday`

\- `now`



These use the machine's local clock.



No location lookup or network time service is needed.



\---



\# 14. Weekday Semantics



Support weekday names and:



\- `next Friday`

\- `last Monday`

\- `this Wednesday`



Use deterministic semantics:



\### Bare weekday



A bare weekday such as:



`Friday`



means the next occurrence of that weekday \*\*on or after\*\* the reference date.



If the reference day itself is Friday, bare `Friday` means today.



\### `next <weekday>`



Always means the strictly future occurrence.



If today is Friday:



`next Friday`



means seven days later.



\### `last <weekday>`



Always means the strictly previous occurrence.



\### `this <weekday>`



Means that weekday within the reference date's current Monday-through-Sunday calendar week.



Document these semantics through tests so future parser changes cannot silently alter them.



\---



\# 15. Named Fixed Dates



Keep named-date support deliberately small.



Required local deterministic names:



\- Christmas

\- New Year's Day



A named holiday without an explicit year should resolve using the reference/current calendar year.



Where practical, support an explicit year:



\- `Christmas 2027`



Do not build a general holiday database.



Do not add country-specific holiday logic.



\---



\# 16. Date Arithmetic Units



Required:



\- days

\- weeks

\- months

\- years



For date-time expressions also support:



\- hours

\- minutes



Sub-day units should require a meaningful time anchor such as:



\- `now`

\- an explicit date-time



Examples:



\- `date 3 hours from now`

\- `date 90 minutes after 2026-10-05 14:30`



Avoid silently inventing a time for a date-only expression when that would be ambiguous.



\---



\# 17. Relative Date Grammar



Support both readable word forms and simple operator forms.



Examples:



\- `10 days from today`

\- `10 days after today`

\- `10 days before today`



and:



\- `today + 10 days`

\- `Friday - 3 weeks`



Singular/plural variants should work:



\- `1 day`

\- `2 days`



Do not attempt unrestricted natural-language understanding.



Implement a bounded deterministic grammar that handles the agreed forms well.



\---



\# 18. Calendar-Aware Month and Year Arithmetic



This is a critical distinction from ordinary duration conversion.



For `date` arithmetic:



\- one month means a calendar month;

\- one year means a calendar year.



Do not implement:



`1 month = 30 days`



inside date arithmetic.



Required examples:



`date 1 month after 2026-01-15`



→ `2026-02-15`



When the original day does not exist in the destination month, clamp to the last valid day.



Example:



`date 1 month after 2026-01-31`



→ `2026-02-28`



In a leap year:



`date 1 month after 2024-01-31`



→ `2024-02-29`



Year arithmetic must also handle leap-day transitions sensibly.



Example:



`2024-02-29 + 1 year`



→ a valid end-of-February date in 2025.



The exact helper implementation may use suitable `chrono` functionality or a small explicit calendar helper, but the behavior must be covered directly by tests.



\---



\# 19. Date Differences



Support at least:



\- `days between <date> and <date>`

\- `weeks between <date> and <date>`



Examples:



`date days between 2026-10-05 and 2026-12-25`



`date weeks between Oct 5 2026 and Jan 1 2027`



Use normal mathematical difference semantics:



`second - first`



Therefore reversed input may produce a negative result.



For date-only day differences, use calendar-date arithmetic rather than local-time/DST-sensitive timestamp subtraction.



For weeks:



\- derive consistently from the underlying day difference;

\- exact whole-week values should display cleanly;

\- fractional weeks may use the shared smart-number formatting policy.



Calendar-aware months/years \*\*difference reporting\*\* is not required as an acceptance gate for this goal. Do not broaden the implementation merely to support every possible difference unit.



Months/years are required for arithmetic, not necessarily for `between` output.



\---



\# 20. Date Result Formatting



For date results, show:



1\. a human-readable form;

2\. an unambiguous ISO form.



Example:



`Wednesday, November 4, 2026 — 2026-11-04`



Executing the result copies:



`2026-11-04`



For date-time results, include time:



`Monday, October 5, 2026 17:30 — 2026-10-05 17:30`



and copy the ISO-like local date-time form.



For date-difference results, copy the useful value with its unit, e.g.:



`81 days`



Do not write date arithmetic into calculator history.



\---



\# 21. Deterministic Date Testing



Date-domain logic must not directly depend on `Local::now()` internally in a way that makes tests time-dependent.



Provide an evaluation boundary such as:



`evaluate(expression, reference\_now)`



or an equivalent context object.



The launcher plugin may obtain `Local::now()` once and pass that reference into the date evaluator.



Tests must be able to supply a fixed reference date/time.



Do not introduce a large service abstraction solely for clock injection if a simple explicit evaluation context is sufficient.



\---



\# 22. Error Handling



Because `date` has a unique explicit prefix, malformed date expressions may return a concise non-executing informational result.



Examples:



\- `Invalid date: 2026-02-30`

\- `Unknown date expression`

\- `Hours require a date-time or 'now' anchor`



Use the existing no-op action convention where appropriate rather than inventing an unrelated error-action protocol.



For unit conversion, error handling must account for the shared `conv` prefix.



The unit converter must \*\*not\*\* emit misleading errors for valid base-conversion queries such as:



`conv ff hex to dec`



A useful policy is:



\- if the expression clearly resolves as physical-unit intent, return relevant unit errors;

\- if it contains recognized physical-unit context and the opposite side is invalid, show the useful error;

\- if it does not appear to be physical-unit intent, remain silent and allow other `conv` providers to handle it.



Do not let improved error messages create result noise across unrelated conversion plugins.



\---



\# 23. Out of Scope



Do not implement any of the following as part of this goal:



\- currency conversion;

\- live exchange rates;

\- currency APIs;

\- timezone conversion;

\- timezone database UI;

\- holiday/business-calendar database;

\- country-specific holiday arithmetic;

\- ingredient-density conversion such as cups of flour to grams;

\- arbitrary dimensional algebra;

\- symbolic unit equation solving;

\- scientific expression simplification;

\- a new calculator UI;

\- a Date Arithmetic dialog;

\- conversion/date history;

\- global bare-query conversion detection;

\- major launcher UI redesign;

\- unrelated calculator refactors;

\- unrelated timestamp refactors.



\---



\# 24. Backward-Compatibility Invariants



The following are explicit invariants:



1\. Existing valid `conv` unit queries continue working.

2\. Existing valid `convert` unit queries continue working.

3\. Existing base-conversion queries continue working.

4\. `conv` / `convert` continue exposing the Convert panel.

5\. Existing `=` calculator behavior continues working.

6\. Existing calculator history behavior continues working.

7\. Existing `ts` commands continue working.

8\. Existing `tsm` commands continue working.

9\. The Convert panel remains available.

10\. The Convert panel Base category remains functional.

11\. Plugin enable/disable behavior remains intact.

12\. No migration of user settings/data is required unless current code proves one genuinely necessary.

13\. No external network requirement is introduced.



\---



\# 25. Pre-Implementation Check



This is read-only work and does not require a commit.



Before M1-A:



\- read `AGENTS.md`;

\- read this plan;

\- inspect the current branch;

\- run `git status`;

\- identify pre-existing user changes;

\- confirm the relevant current files still match the architecture described above;

\- inspect the current relevant tests;

\- do not discard or absorb unrelated working-tree changes.



At minimum recheck:



\- `src/plugins/unit\_convert.rs`

\- `src/plugins/base\_convert.rs`

\- `src/plugins/convert\_panel.rs`

\- `src/gui/convert\_panel.rs`

\- `src/plugins/timestamp.rs`

\- `src/plugin.rs`

\- `src/plugins/mod.rs`

\- `src/lib.rs`

\- `tests/plugin\_cases/unit\_convert\_plugin.rs`

\- `tests/plugin\_cases/base\_convert\_plugin.rs`

\- `tests/plugin\_cases/convert\_panel\_plugin.rs`

\- `tests/plugin\_cases/timestamp\_plugin.rs`

\- `tests/suites/plugin\_queries.rs`



If the repository has materially changed from this baseline, adapt implementation details while preserving the approved behavior.



Do not re-plan the feature merely because local helper names differ.



\---



\# 26. Milestone M1 — Establish the Shared Unit-Conversion Domain



\## M1-A — Centralize Existing Physical Conversion Ownership



\### Objective



Create one reusable physical/unit conversion domain and migrate the current inline unit converter onto it \*\*without yet trying to add every new syntax feature\*\*.



This checkpoint establishes architectural ownership.



\### Architectural intent



The GUI must not own physical-conversion math.



`UnitConvertPlugin` should not remain the only owner of physical-unit definitions if the Convert panel also needs them.



Create a reusable non-GUI conversion module.



Recommended location:



\- `src/unit\_conversion/`



or an equivalent clearly named domain location.



Expose it from `src/lib.rs` as needed.



\### Required changes



1\. Introduce a typed representation for conversion category/dimension.

2\. Introduce a central unit definition/catalog.

3\. Move current physical conversion factors and nonlinear conversion strategies out of `src/plugins/unit\_convert.rs`.

4\. Preserve:

&#x20;  - current supported unit values;

&#x20;  - current aliases;

&#x20;  - temperature conversion;

&#x20;  - fuel-economy conversion;

&#x20;  - approximate month/year duration semantics.

5\. Centralize incompatible-dimension detection.

6\. Keep the existing simple plugin parser functional during this checkpoint if that reduces migration risk.

7\. Remove old duplicated physical math from `UnitConvertPlugin` once the new domain is authoritative.

8\. Do not touch the Convert panel's duplicate implementation yet except where compilation requires it.

9\. Do not move `BaseConvertPlugin` into this module.



\### Tests



Add domain tests for representative conversions in each currently supported category.



Keep or adapt existing plugin tests so they establish that the migration has not changed current supported answers unexpectedly.



Important compatibility cases include:



\- km → mi

\- F → C

\- cm → in

\- L → gal

\- kWh → J

\- kW → W

\- bit → byte

\- hour → minute

\- MPG → km/L

\- degree → radian

\- kg → lb

\- square meter → square foot

\- km/h → mph

\- bar → psi



Use approximate numeric assertions in domain tests rather than formatting assertions.



Formatting remains a presentation concern.



\### Verification



Use narrow checks appropriate to the modified module/tests.



Do not run the entire repository suite.



\### Done criteria



\- one domain owner exists for physical conversion math;

\- `UnitConvertPlugin` consumes it;

\- old factor/conversion duplication inside the plugin is removed;

\- existing representative conversions still evaluate correctly;

\- base conversion is untouched;

\- targeted tests pass.



\### Commit boundary



Commit before beginning catalog expansion.



Suggested subject:



`refactor(convert): \[M1-A] centralize physical unit conversion`



\---



\## M1-B — Expand the Unit Catalog and Semantics



\### Objective



Expand the shared domain to the approved categories and settle ambiguous-unit semantics before implementing the more flexible expression parser.



\### Required changes



Add the required units for:



\- length;

\- area;

\- mass;

\- volume;

\- temperature;

\- speed;

\- pressure;

\- energy;

\- power;

\- time/duration;

\- angles;

\- fuel economy;

\- digital storage;

\- digital rates;

\- force;

\- torque;

\- frequency.



Use one catalog as the source for:



\- canonical unit identity;

\- display abbreviation;

\- aliases;

\- dimension/category;

\- conversion strategy/factor;

\- metadata needed for US/Imperial and approximate duration behavior.



\### Compatibility-sensitive requirements



Explicitly handle:



\- existing `oz` mass behavior;

\- US unqualified gallons;

\- existing month/year approximate conversion;

\- existing lowercase aliases;

\- case-aware data units;

\- legacy forms such as current lowercase storage aliases;

\- existing power aliases.



Do not “correct” a previously valid legacy spelling by silently changing its meaning.



\### US / Imperial



Add explicit definitions for US and Imperial volume units.



Unqualified common cooking volume is US customary.



\### Data storage and rates



Introduce sufficient normalization logic to support meaningful capitalization without globally lowercasing the token before interpretation.



Test both:



\- conventional forms;

\- backwards-compatible legacy forms.



\### Tests



Cover at least one representative conversion for every new category.



Add explicit tests for:



\- US gallon vs Imperial gallon;

\- US fluid ounce;

\- Imperial fluid ounce;

\- `MB` vs `Mb`;

\- `MiB` vs decimal MB;

\- byte/s vs bit/s;

\- force;

\- torque;

\- frequency;

\- stone;

\- tonne;

\- US short ton;

\- knot;

\- torr/mmHg;

\- cubic and square units.



\### Done criteria



\- approved catalog exists;

\- ambiguous semantics are deterministic;

\- legacy valid aliases remain supported;

\- new categories convert correctly;

\- no separate new catalog is introduced elsewhere.



\### Commit boundary



Suggested subject:



`feat(convert): \[M1-B] expand unit catalog and conversion semantics`



\---



\## M1-C — Add Flexible Unit-Expression Parsing and Smart Formatting



\### Objective



Replace the rigid four-token unit query parser with a bounded conversion-expression parser capable of fractions, multi-word aliases, and compound source quantities.



\### Required parser behavior



Parse the expression \*\*after\*\* `conv` / `convert`.



Recognize a standalone `to` separator.



Support:



\- decimal values;

\- integers;

\- signed values;

\- simple fractions;

\- mixed fractions;

\- common Unicode fractions;

\- multi-word unit aliases;

\- compound source quantities.



\### Numeric parsing



Support examples:



\- `1/2`

\- `3/4`

\- `1 1/2`

\- `½`

\- `¼`

\- `¾`



Reject malformed expressions and zero denominators.



\### Compound quantities



Allow multiple quantity/unit components on the source side when:



\- every component resolves to the same compatible linear dimension;

\- combining them is mathematically additive.



Examples:



\- `6 ft 2 in`

\- `5 lb 8 oz`



Reject incompatible compounds.



Do not allow compound parsing to turn temperature/fuel-economy or another nonlinear dimension into an accidental additive system.



\### Unit resolution



Use catalog-driven alias matching.



Prefer the longest valid matching alias where aliases contain multiple words.



Do not duplicate alias knowledge inside the parser.



\### Destination



The destination is one resolved unit.



Do not implement compound destination formatting in this goal.



\### Smart numeric formatting



Create a reusable deterministic formatter that:



\- removes meaningless zeroes;

\- renders near-integers cleanly;

\- retains useful precision;

\- hides binary floating-point noise;

\- uses scientific notation where appropriate.



\### Approximate duration indication



Month/year duration conversions should remain compatible with the existing 30/365-day assumptions while allowing the result presentation to indicate that they are approximate.



\### Tests



Add dedicated parser tests covering:



\- simple decimal conversion;

\- negative temperature;

\- plural aliases;

\- whitespace variation;

\- `m2`;

\- `m^2`;

\- `m²`;

\- multi-word units;

\- `1/2 cup`;

\- `1 1/2 cups`;

\- Unicode fractions;

\- `6 ft 2 in`;

\- `5 lb 8 oz`;

\- malformed fractions;

\- incompatible compound units;

\- unknown source unit;

\- unknown destination unit;

\- smart output formatting.



\### Done criteria



\- conversion parsing no longer depends on exactly four whitespace-separated tokens;

\- accepted examples parse deterministically;

\- parser uses the catalog rather than its own unit list;

\- formatting is centralized;

\- invalid expressions have typed/structured failure information suitable for plugin error handling.



\### Commit boundary



Suggested subject:



`feat(convert): \[M1-C] support flexible unit expressions`



\---



\# 27. Milestone M2 — Integrate Shared Conversion Behavior Into Launcher Surfaces



\## M2-A — Upgrade Inline `conv` / `convert` Results



\### Objective



Make `UnitConvertPlugin` expose the new domain behavior while preserving coexistence with Base Convert and the Convert panel.



\### Scope



Primary file:



\- `src/plugins/unit\_convert.rs`



Tests:



\- `tests/plugin\_cases/unit\_convert\_plugin.rs`

\- `tests/plugin\_cases/base\_convert\_plugin.rs`



\### Required behavior



Examples such as these should now work:



\- `conv 6 ft 2 in to cm`

\- `conv 1/2 cup to ml`

\- `conv 5 lb 8 oz to kg`

\- `conv 100 m^2 to ft^2`

\- `conv 1 imperial gal to l`

\- `conv 1 MB to Mb`

\- `conv 100 MB/s to Mbps`

\- `conv 1 Nm to lb-ft`

\- `conv 60 hz to khz`



Use actual supported canonical aliases chosen by the catalog; examples should be adjusted only where required for unambiguous syntax.



\### Result presentation



Visible label:



`<source expression> = <formatted value> <destination>`



Clipboard action:



`clipboard:<formatted value> <destination>`



\### Error-routing behavior



Do not make the physical converter claim every malformed `conv` expression.



In particular:



`conv ff hex to dec`



must remain a Base Convert query.



When an expression clearly intends physical unit conversion and one side fails, return a concise non-executing error where useful.



Examples:



\- `Unknown unit: foobar`

\- `Cannot convert length to temperature`



If there is insufficient evidence that the query is physical-unit intent, remain silent.



\### Existing blank prefix behavior



`UnitConvertPlugin` should continue returning no direct conversion for a bare:



`conv`



so the panel provider can offer the Convert panel.



\### Tests



Update old fixed-four-decimal expectations.



Verify clipboard results now include units.



Add cross-provider regression coverage establishing that:



\- Base Convert still resolves its queries;

\- Unit Convert does not add misleading errors to valid base-conversion expressions.



\### Done criteria



\- inline conversion exposes all agreed parser behavior;

\- copied output includes unit;

\- useful errors work without polluting base-conversion queries;

\- existing base-conversion tests remain green.



\### Commit boundary



Suggested subject:



`feat(convert): \[M2-A] expose expanded inline conversions`



\---



\## M2-B — Unify the Convert Panel With the Shared Physical Catalog



\### Objective



Remove the Convert panel's separate physical-conversion source of truth.



\### Scope



Primary:



\- `src/gui/convert\_panel.rs`



Potential supporting code:



\- shared unit-conversion module.



Do not unnecessarily change:



\- window behavior;

\- focus behavior;

\- general panel interaction model.



\### Required changes



1\. Replace static duplicated physical category/unit definitions with data from the shared conversion catalog.

2\. Replace:

&#x20;  - distance factor logic;

&#x20;  - mass factor logic;

&#x20;  - volume factor logic;

&#x20;  - temperature conversion logic;

&#x20;  with the shared engine.

3\. Make the panel expose the expanded physical-unit categories.

4\. Use shared smart-result formatting.

5\. Keep existing filter functionality.

6\. Make unit filtering friendly/case-insensitive for display where appropriate.

7\. Ensure category changes still reset invalid from/to selections safely.

8\. Preserve the panel's Base category.

9\. Continue using the existing Base conversion implementation for that category unless a very small internal helper extraction is needed.

10\. Do not force Base Convert into the physical unit catalog.



\### Consistency requirement



For the same physical value and units:



\- inline conversion;

\- Convert panel



must evaluate through the same engine and produce equivalent numeric results.



\### Testing



Prefer testing the panel's calculation-facing behavior through extracted/state-level helpers rather than screenshot/pixel UI testing.



Do not introduce heavy egui visual regression infrastructure for this task.



Ensure existing:



\- `tests/plugin\_cases/convert\_panel\_plugin.rs`



still passes.



\### Done criteria



\- duplicated physical factor tables are gone from the panel;

\- expanded categories appear in the panel;

\- physical calculations use shared definitions;

\- Base category remains functional;

\- panel opening behavior remains intact.



\### Commit boundary



Suggested subject:



`refactor(convert): \[M2-B] unify convert panel with shared units`



\---



\# 28. Milestone M3 — Build the Date Arithmetic Domain



The date domain should be implemented independently from the launcher plugin adapter so it can be tested deterministically.



Recommended location:



\- `src/date\_arithmetic/`



or an equivalent non-GUI domain module.



Expose it through `src/lib.rs` where needed.



Do not put parsing/calendar arithmetic directly inside `Plugin::search`.



\---



\## M3-A — Add Deterministic Date Parsing and Anchors



\### Objective



Create the date evaluation model and support concrete/relative anchor parsing without yet layering the complete arithmetic grammar on top.



\### Domain model



Use a representation that can distinguish at least:



\- date-only values;

\- local date-time values.



A simple enum is sufficient.



Avoid adding timezone conversion concepts.



\### Evaluation context



Pass a fixed current local date/time into the evaluator.



The domain must not internally call the wall clock every time it needs an anchor.



\### Concrete date parsing



Support:



\- ISO;

\- US slash format;

\- written month-first format;

\- written day-first format.



Reject invalid dates.



\### Relative anchors



Support:



\- today

\- tomorrow

\- yesterday

\- now



\### Weekdays



Implement and test the approved semantics for:



\- bare weekdays;

\- next weekday;

\- last weekday;

\- this weekday.



\### Fixed named dates



Support:



\- Christmas

\- New Year's Day



Resolve yearless named dates against the reference year.



Support an explicit year where practical.



\### Tests



Use a fixed reference date/time.



Cover:



\- every concrete date format;

\- invalid calendar dates;

\- leap dates;

\- today/tomorrow/yesterday;

\- now;

\- bare weekday;

\- next weekday;

\- last weekday;

\- this weekday;

\- weekday boundary at Sunday/Monday;

\- Christmas;

\- New Year's Day;

\- explicit holiday year.



\### Done criteria



\- deterministic date anchors parse without plugin involvement;

\- no network/timezone dependency exists;

\- tests do not depend on today's actual date.



\### Commit boundary



Suggested subject:



`feat(date): \[M3-A] add deterministic date anchors`



\---



\## M3-B — Add Relative Calendar Arithmetic



\### Objective



Implement the actual relative arithmetic grammar and calendar-aware addition/subtraction.



\### Required grammar



Support forms equivalent to:



\- `<amount> <unit> from <anchor>`

\- `<amount> <unit> after <anchor>`

\- `<amount> <unit> before <anchor>`

\- `<anchor> + <amount> <unit>`

\- `<anchor> - <amount> <unit>`



\### Required units



\- day/days

\- week/weeks

\- month/months

\- year/years

\- hour/hours

\- minute/minutes



\### Day/week arithmetic



Use exact day/week calendar offsets for date values.



\### Month/year arithmetic



Use calendar-aware behavior.



Preserve original day where possible.



Clamp to the final valid day when necessary.



Test:



\- January 31 → February;

\- leap-year February;

\- February 29 + year;

\- month subtraction;

\- year subtraction;

\- crossing December/January.



\### Date-time behavior



Hours/minutes require:



\- `now`;

\- or an explicit date-time anchor.



Keep time-of-day when applying day/week/month/year arithmetic to a date-time anchor.



Do not silently assign arbitrary daytime values to date-only expressions for hour/minute arithmetic.



\### Tests



Include:



\- `30 days from today`

\- `2 weeks from tomorrow`

\- `3 months after 2026-10-05`

\- `10 days before Christmas`

\- `today + 10 days`

\- `Friday - 3 weeks`

\- `3 hours from now`

\- `90 minutes after 2026-10-05 14:30`

\- month-end clamping;

\- leap transitions;

\- negative direction;

\- invalid sub-day/date-only combination.



\### Done criteria



\- required relative forms evaluate;

\- calendar arithmetic is deterministic;

\- month/year behavior is calendar-aware rather than duration-based;

\- date-only and date-time semantics remain explicit.



\### Commit boundary



Suggested subject:



`feat(date): \[M3-B] add calendar-aware relative arithmetic`



\---



\## M3-C — Add Date Differences, Formatting, and Typed Errors



\### Objective



Complete the reusable date domain with difference calculations and presentation-ready results.



\### Date differences



Support:



\- `days between A and B`

\- `weeks between A and B`



Use:



`B - A`



semantics.



Allow negative differences.



For date-only input:



\- use calendar date differences;

\- avoid DST-sensitive local timestamp math.



\### Week differences



Derive from exact day difference.



Use smart numeric formatting when the number of weeks is fractional.



\### Output model



The domain should return enough information for the plugin to distinguish:



\- successful date result;

\- successful date-time result;

\- successful difference result;

\- invalid input/error.



Avoid requiring the plugin to re-parse formatted strings.



\### Human-readable formatting



Date:



`Wednesday, November 4, 2026 — 2026-11-04`



Date-time:



`Monday, October 5, 2026 17:30 — 2026-10-05 17:30`



Difference:



`81 days`



or equivalent concise result.



\### Clipboard payload



Provide separately from display where useful:



\- date → ISO date

\- date-time → ISO-like local date/time

\- difference → value + unit



\### Typed errors



Distinguish useful cases such as:



\- invalid date;

\- unsupported syntax;

\- invalid anchor;

\- invalid arithmetic unit;

\- sub-day arithmetic without time anchor.



Do not over-engineer a giant error hierarchy.



\### Tests



Cover:



\- forward differences;

\- reversed differences;

\- exact weeks;

\- fractional weeks;

\- dates containing written month forms;

\- malformed `between` expressions;

\- display/clipboard formatting.



\### Done criteria



The date domain is independently usable and thoroughly deterministic before being exposed as a plugin.



\### Commit boundary



Suggested subject:



`feat(date): \[M3-C] add date differences and result formatting`



\---



\# 29. Milestone M4 — Integrate Date Arithmetic Into Multi Launcher



\## M4-A — Add and Register `DateArithmeticPlugin`



\### Objective



Expose the completed date domain through a new launcher command.



\### Expected files



Likely:



\- `src/plugins/date\_arithmetic.rs`

\- `src/plugins/mod.rs`

\- `src/plugin.rs`

\- `tests/plugin\_cases/date\_arithmetic\_plugin.rs`

\- `tests/suites/plugin\_queries.rs`



Use current repository conventions if names differ.



\### Plugin behavior



Recognize explicit:



`date`



prefixes.



Examples:



\- `date 30 days from today`

\- `date 1 month after 2026-01-31`

\- `date days between 2026-10-05 and 2026-12-25`



\### Current time



Call the machine's local clock at the plugin boundary.



Pass that reference into the deterministic date evaluator.



Do not let test code depend on the actual clock.



If needed for plugin-level deterministic tests, expose a narrow evaluator helper that accepts a fixed reference.



\### Launcher result



Successful date result:



\- readable label;

\- description identifying Date Arithmetic;

\- clipboard action containing ISO result.



Difference result:



\- useful concise label;

\- clipboard payload containing value and unit.



\### `date` with no expression



Provide lightweight discoverability.



Either:



\- return a concise usage/help result;

\- or remain empty while `commands()` exposes examples.



Do not open a dialog.



\### Errors



Because the prefix is explicit, malformed date input may surface a no-op explanatory result.



Do not crash or silently reinterpret malformed dates.



\### Commands metadata



Expose a discoverable command entry such as:



`date <expression>`



with examples/description consistent with other plugins.



\### No history



Do not write these evaluations into:



\- calculator history;

\- query-specific conversion history.



\### Tests



Add plugin-level tests for:



\- a fixed explicit date expression;

\- ISO input;

\- relative expression using a fixed context helper where needed;

\- date difference;

\- result action copies ISO/value+unit;

\- invalid date error;

\- empty prefix behavior;

\- no interference with `ts` / `tsm`.



\### Done criteria



\- new `date` plugin is registered;

\- it follows normal plugin enablement;

\- it returns launcher actions correctly;

\- no existing timestamp command changed;

\- plugin query suite recognizes the new test module.



\### Commit boundary



Suggested subject:



`feat(date): \[M4-A] expose date arithmetic launcher command`



\---



\## M4-B — Add Compatibility and Cross-Feature Regression Coverage



\### Objective



Lock down the boundaries between the new behavior and existing conversion/calculator/timestamp features before documentation/final review.



\### Required checks/tests



Verify:



\### Unit conversion compatibility



Existing examples still work.



Update expectations only where behavior intentionally changed:



\- smart formatting;

\- clipboard now includes unit.



\### Base conversion compatibility



Keep existing tests green:



\- binary ↔ hex;

\- decimal ↔ binary/hex/octal;

\- text ↔ binary/hex.



Add a regression test if needed confirming expanded unit error handling does not steal base queries.



\### Convert panel compatibility



Verify:



\- bare `conv` opens panel;

\- bare `convert` opens panel;

\- Base category remains usable;

\- physical categories use shared engine.



\### Timestamp compatibility



Keep existing timestamp tests green.



No new `date` parsing should affect:



\- `ts`

\- `tsm`



\### Calculator compatibility



Do not modify calculator behavior/history.



If no calculator source was changed, do not create a large new calculator regression campaign merely to prove nothing changed.



A focused existing calculator smoke/test is sufficient if needed by the actual diff.



\### Plugin enablement



Ensure registration follows the existing plugin manager pattern and the new plugin behaves like ordinary built-ins.



\### Done criteria



\- affected compatibility suites pass;

\- no shared prefix regression exists;

\- `date`, `conv`, base conversion, and timestamp paths have clear ownership.



\### Commit boundary



If this checkpoint adds meaningful dedicated regression tests, commit them separately.



Suggested subject:



`test(utilities): \[M4-B] cover date and conversion compatibility`



If all required tests naturally landed with their implementation checkpoints and no additional source/test change is needed, do \*\*not\*\* manufacture an empty checkpoint commit.



\---



\# 30. Milestone M5 — Documentation, Review, and Completion



\## M5-A — Update User-Facing Documentation



\### Objective



Document the new capabilities without turning README into an exhaustive unit catalog.



\### Scope



Update relevant README areas:



\- command prefix cheat sheet;

\- conversion cookbook section;

\- add concise Date Arithmetic section.



\### Conversion examples



Include representative examples such as:



\- normal conversion;

\- fraction;

\- compound measurement;

\- data storage/rate;

\- US/Imperial distinction.



\### Date examples



Include:



\- relative day arithmetic;

\- month arithmetic;

\- date difference.



Explicitly distinguish:



\- `conv 1 month to days` as approximate duration behavior;

\- `date 1 month after ...` as calendar-aware behavior.



Mention that these utilities are local/offline if appropriate.



\### Do not



\- document unsupported natural-language phrases;

\- imply currency/timezone support;

\- list hundreds of aliases.



\### Commit boundary



Suggested subject:



`docs(utilities): \[M5-A] document date and conversion commands`



\---



\## M5-B — Focused Review and Remediation



\### Objective



Perform an independent review of the completed goal, concentrating on architectural ownership and compatibility.



Use a reviewer agent when available.



\### Reviewer focus



Inspect:



\- shared physical conversion ownership;

\- parser edge cases;

\- accidental duplicated unit catalogs;

\- case-sensitive data semantics;

\- US/Imperial behavior;

\- base-converter coexistence;

\- date calendar correctness;

\- date parser ambiguity;

\- deterministic tests;

\- plugin integration;

\- accidental calculator/timestamp changes;

\- scope creep.



\### Reviewer should not



\- propose unrelated calculator redesign;

\- demand currency/timezone support;

\- turn this into a global natural-language parser;

\- initiate broad repository cleanup;

\- require full repository testing without a concrete reason.



\### Remediation



If substantive findings exist:



\- fix them;

\- run the directly affected targeted verification;

\- create explicit remediation commit(s).



Example:



`fix(date): \[M5-B] correct end-of-month subtraction`



or:



`fix(convert): \[M5-B] preserve base conversion query routing`



Do not create an empty “review complete” commit.



\---



\# 31. Targeted Verification Strategy



Verification should be proportional to the goal.



Do not run expensive broad verification before every checkpoint.



\## During checkpoints



Use:



\- local unit/domain tests;

\- formatting checks where inexpensive;

\- focused plugin tests;

\- compilation checks only when useful to establish checkpoint correctness.



\## After conversion integration



Run the relevant plugin query suite:



`cargo nextest run --test plugin\_queries`



This suite already contains the existing:



\- unit converter;

\- base converter;

\- Convert panel provider;

\- timestamp



query tests and should also include the new Date Arithmetic plugin tests.



If this target becomes materially expensive, use actual test-name filters during iteration and run the complete `plugin\_queries` target at the substantive integration checkpoint.



\## Final focused verification



At minimum:



1\. formatting:

&#x20;  `cargo fmt --check`



2\. date/unit domain tests using the narrowest useful test-name filter;



3\. plugin query integration:

&#x20;  `cargo nextest run --test plugin\_queries`



4\. compilation:

&#x20;  `cargo check`



Use the repository's normal build environment.



Do \*\*not\*\* automatically run:



`cargo nextest run`



for the entire repository.



Only broaden to full nextest if:



\- the user explicitly requests it;

\- targeted tests reveal collateral failures;

\- a concrete shared-infrastructure issue makes narrower verification insufficient.



Record the concrete reason before broadening verification.



\---



\# 32. Required Test Coverage Summary



The final feature should have direct automated coverage for the following.



\## Conversion parser



\- decimal

\- negative numeric value

\- simple fraction

\- mixed fraction

\- Unicode fraction

\- whitespace

\- singular/plural aliases

\- multi-word unit

\- square notation

\- compound length

\- compound mass

\- malformed fraction

\- incompatible compound



\## Unit catalog



Representative coverage for every approved category.



\## Ambiguous semantics



\- oz vs fl oz

\- US vs Imperial gallon

\- tonne vs US ton

\- MB vs Mb

\- MiB vs MB

\- bit/s vs byte/s

\- legacy lowercase aliases



\## Output



\- trailing-zero removal

\- integer result

\- fractional result

\- very large/small result

\- clipboard includes destination unit



\## Conversion routing



\- physical unit query

\- base conversion query

\- bare `conv`

\- invalid but clearly physical-unit query



\## Date parsing



\- ISO

\- US numeric

\- written month

\- day-first written month

\- invalid calendar date



\## Date anchors



\- today

\- tomorrow

\- yesterday

\- now

\- bare weekday

\- next weekday

\- last weekday

\- this weekday

\- Christmas

\- New Year's Day



\## Date arithmetic



\- days

\- weeks

\- months

\- years

\- hours

\- minutes

\- addition

\- subtraction

\- end-of-month clamp

\- leap year

\- leap-day year arithmetic



\## Date differences



\- days

\- weeks

\- reversed order

\- fractional weeks



\## Compatibility



\- base conversion

\- timestamp

\- Convert panel

\- existing valid unit queries

\- plugin routing



\---



\# 33. Task-Specific Commit Map



The intended history should roughly resemble:



| Stage | Natural boundary | Suggested commit |

|---|---|---|

| M1-A | Shared physical conversion owner + migration of existing math | `refactor(convert): \[M1-A] centralize physical unit conversion` |

| M1-B | Expanded catalog and semantics | `feat(convert): \[M1-B] expand unit catalog and conversion semantics` |

| M1-C | Flexible parser + smart formatter | `feat(convert): \[M1-C] support flexible unit expressions` |

| M2-A | Inline plugin UX/output/routing | `feat(convert): \[M2-A] expose expanded inline conversions` |

| M2-B | Convert panel migration | `refactor(convert): \[M2-B] unify convert panel with shared units` |

| M3-A | Date anchors and deterministic context | `feat(date): \[M3-A] add deterministic date anchors` |

| M3-B | Calendar-aware relative arithmetic | `feat(date): \[M3-B] add calendar-aware relative arithmetic` |

| M3-C | Differences + formatting/errors | `feat(date): \[M3-C] add date differences and result formatting` |

| M4-A | Date plugin integration | `feat(date): \[M4-A] expose date arithmetic launcher command` |

| M4-B | Additional cross-feature regression coverage if needed | `test(utilities): \[M4-B] cover date and conversion compatibility` |

| M5-A | README/user docs | `docs(utilities): \[M5-A] document date and conversion commands` |

| M5-B | Review remediation only if findings require code changes | task-specific `fix(...)` commit |



This table describes intended boundaries, not a mandate to create empty or meaningless commits.



If two adjacent changes prove inseparable at implementation time, use engineering judgment, but do not allow several independently understandable checkpoints to accumulate into a giant commit.



\---



\# 34. Orchestrator / Agent Guidance



This initiative is appropriate for orchestrated implementation, but write-heavy stages overlap heavily and should remain sequential.



Recommended use:



\## Parent orchestrator



Own:



\- overall plan state;

\- Git state;

\- milestone/checkpoint boundaries;

\- verification scope;

\- commit boundaries;

\- final integration.



\## Planner



The plan already exists.



Use a planner only if:



\- the current repository has materially changed;

\- a checkpoint exposes a genuine architecture ambiguity;

\- implementation reveals a requirement conflict.



Do not have a planner repeatedly re-plan already settled milestones.



\## Implementer



Delegate one explicit checkpoint or tightly related checkpoint packet at a time.



Each packet should contain:



\- objective;

\- exact scope;

\- current architectural owner;

\- required behavior;

\- invariants;

\- non-goals;

\- tests;

\- verification;

\- done criteria.



Do not tell an implementer merely:



“Implement M1.”



Give it the actual M1-A or M1-B contract.



\## Reviewer



Use an independent reviewer after meaningful architectural integration, especially:



\- after the conversion core/panel migration;

\- after the date domain/plugin integration;

\- during M5-B final review.



The reviewer should inspect the diff and relevant nearby architecture rather than redesigning the feature.



\## Parallelism



Do not run overlapping source-writing agents in parallel.



Potential bounded read-only research can run in parallel, but:



\- `UnitConvertPlugin`;

\- shared conversion domain;

\- Convert panel;

\- date plugin registration;

\- common tests



have overlapping ownership and should be integrated sequentially.



\---



\# 35. Completion Criteria



This goal is complete when all of the following are true:



\- \[ ] physical unit conversion has one domain source of truth;

\- \[ ] inline Unit Convert uses it;

\- \[ ] Convert panel physical categories use it;

\- \[ ] Base Convert remains functional and independent;

\- \[ ] approved unit categories are supported;

\- \[ ] US/Imperial semantics are explicit;

\- \[ ] digital storage/rate semantics are correct;

\- \[ ] fractions work;

\- \[ ] compound source measurements work;

\- \[ ] smart formatting replaces fixed four-decimal output;

\- \[ ] inline result execution copies value + unit;

\- \[ ] `date` command exists;

\- \[ ] relative date anchors work;

\- \[ ] weekday semantics are deterministic;

\- \[ ] calendar-aware month/year arithmetic works;

\- \[ ] leap/end-of-month behavior is covered;

\- \[ ] day/week differences work;

\- \[ ] date results copy ISO output;

\- \[ ] calculator history is unchanged;

\- \[ ] `ts` / `tsm` remain unchanged;

\- \[ ] no network dependency was added;

\- \[ ] README is updated;

\- \[ ] targeted verification passes;

\- \[ ] review findings are resolved;

\- \[ ] intended checkpoints are committed using active checkpoint cadence;

\- \[ ] no unrelated user changes are included in commits;

\- \[ ] checkpoint history has not been squashed/re-written.



\---



\# 36. Final Deliverable Report



When implementation finishes, report:



\## Implemented



Briefly summarize:



\- shared conversion architecture;

\- new unit capabilities;

\- parser improvements;

\- Convert panel unification;

\- date arithmetic;

\- date differences.



\## Preserved



Explicitly state the status of:



\- calculator;

\- calculator history;

\- base conversion;

\- Convert panel;

\- `ts`;

\- `tsm`;

\- plugin settings.



\## Verification



List the commands actually run and their outcomes.



Do not claim tests that were not executed.



\## Commits



List each checkpoint commit subject and hash.



\## Remaining Issues



List only genuine remaining issues or deliberately deferred scope.



Do not present out-of-scope items such as currency/timezones as unfinished defects.


## Execution ledger

Current branch: `date-and-unit`. Initial working tree: clean.

| Checkpoint | Status | Verification / notes |
|---|---|---|
| M1-A | complete | Domain 18/18; legacy unit plugin 16/16; diff inspected; checkpoint committed |
| M1-B | complete | Domain 27/27; unit plugin 16/16; exact customary factors and aliases verified; checkpoint committed |
| M1-C | complete | Conversion and number-format filter 36/36; Unicode and zero-temperature regressions covered; checkpoint committed |
| M2-A | complete | Unit/Base plugin filter 37/37; copied units, routing, documented numeric-to-decimal gap verified; checkpoint committed |
| M2-B | complete | Conversion/panel 41/41; plugin_queries 121/121; independent findings resolved; checkpoint committed |
| M3-A | complete | Deterministic date anchors 7/7; bounded grammar; checkpoint committed |
| M3-B | complete | Date domain 11/11; calendar clamping and checked offsets verified; checkpoint committed |
| M3-C | complete | Date domain 15/15; signed differences, typed outcomes and clipboard formatting verified; checkpoint committed |
| M4-A | pending | Date plugin integration |
| M4-B | pending | Compatibility coverage if needed |
| M5-A | pending | User documentation |
| M5-B | pending | Independent review and focused final verification |

Checkpoint commits are recorded in Git with the stage identifier. No full repository test run is required.

### Execution decisions

- M1-B replaced legacy rounded customary factors with exact definitions so gallon/fluid-ounce, mile/yard and pound/ounce relations remain coherent. The prior four-decimal speed result intentionally changed from 62.1373 to 62.1371 mph.
- M2-A added binary/hexadecimal/octal-to-decimal cases because README and plan promised `conv ff hex to dec`, while baseline Base Convert did not implement it. Base conversion remains separate.
- M2-B independent conversion review identified canonical-symbol lookup gaps, missing required micro-sign/spaced fuel aliases, and avoidable intermediate floating-point range failures. Parent review also identified a filter-selection regression. Remediation completed in the panel integration checkpoint; source verification found no remaining issues and focused domain/panel plus plugin query tests passed.

- M3-C follow-up review found accepted leap-second differences lost Chrono accounting. Explicit M5-B remediation uses signed Chrono durations and preserves trimmed fractional output; fresh targeted rebuild passed 15/15 and reviewer confirmed the difference fix.
