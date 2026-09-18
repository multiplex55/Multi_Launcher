# Radial Stabilization — Acceptance and Evidence Checklist

**Initial status: NOT RUN.** This is a checklist to execute against the implemented candidate, not a report of successful tests. Use harmless fixture actions and non-sensitive content. Never run real automation that could type, delete, send, or capture sensitive material merely to test UI behavior.

## Record the candidate

```text
Branch:
Immutable feature baseline (existing record):
STABILIZATION_START_HEAD and initial working-tree fingerprint:
Candidate source SHA / uncommitted source fingerprint:
Built executable identity and build profile:
Windows version / session type:
Monitor work areas and DPI/scaling:
Grid initially visible/focused:
Designer initially open/dirty:
Native desktop preview state:
Runtime radial state:
Actual configured chord / threshold:
Observed chord producer, if known (otherwise unknown):
Evidence directory:
```

Use the user's configured `Shift+Alt+Win+End` mapped to a single key action when testing their environment. All constituent keys are released between invocations. Do not claim a manually typed alternative chord proves the mapped key path unless both were exercised.

## A. Focused tap and hold matrix

For each valid case, perform several deliberate short taps and holds without a mouse-wiggle or focus-away workaround. Record press/release/threshold, chosen intent, target viewport, and actual outcome when a trace is enabled.

| ID | Starting state | Expected | Status / evidence |
|---|---|---|---|
| H0 | Designer closed; native preview stopped; grid focused; runtime closed | Tap hides grid on release; next tap shows it | NOT RUN |
| H1 | Designer open and idle; preview stopped; grid focused | Same responsive grid-only toggle; Designer remains | NOT RUN |
| H2 | Designer focused; preview stopped | Tap toggles grid only, not Designer | NOT RUN |
| H3 | Designer open; native preview active; grid focused | Grid-only tap still works; no incidental viewport closure | NOT RUN |
| H4 | Designer/preview active; external app focused | Tap reaches correct root owner; approved focus semantics preserved | NOT RUN |
| H5 | Runtime radial open; Designer closed | Short tap changes grid only; hold closes runtime only | NOT RUN |
| H6 | Runtime radial open; Designer open, with supported preview state | Hold closes runtime, not Designer; key release does not reopen | NOT RUN |
| H7 | Grid hidden/offscreen; Designer open or closed | Hold opens radial without grid flash or mouse activity | NOT RUN |
| H8 | Shared tap/hold disabled | Established legacy launcher toggle works | NOT RUN |
| H9 | Screen Draw recovery/exclusive capture active | Existing immediate recovery/emergency ownership wins | NOT RUN |

Preview-only with Designer closed is not a required steady state if the current product deliberately stops preview on Designer close. Record that as **not applicable with reason**, not “passed.” Do not create unsupported coexistence just to fill a row.

Failures must be classified: event absent, decision absent/wrong, wrong target viewport/HWND, command undone by later restore, native effect failed, or unlocalized. Do not conclude “missing repaint” solely because moving a mouse makes the symptom change.

## B. Designer and runtime close

| ID | Operation | Expected | Status / evidence |
|---|---|---|---|
| C1 | Clean, idle Designer X | Promptly closes only Designer and owned disposable preview | NOT RUN |
| C2 | Dirty Designer X repeatedly | One save/discard/keep-editing decision; no duplicate prompt | NOT RUN |
| C3 | Keep Editing after dirty close | Draft intact, normal editing resumes, close latch cleared | NOT RUN |
| C4 | X during font/catalog/snapshot/embedded preview preparation | Cancels/invalidates nonessential work; late replies do not revive window | NOT RUN |
| C5 | X while native preview start is pending | Stop supersedes start; no orphan native surface | NOT RUN |
| C6 | X after durable Save/Apply/import accepted | Visible operation state; safe finish then requested close | NOT RUN |
| C7 | Controlled request-send failure/missing service | Exact pending request fails; error visible; controls/close remain usable | NOT RUN |
| C8 | Durable operation fails or disconnects with uncertain result | No invented success; reconcile actual outcome and preserve data | NOT RUN |
| C9 | Existing Cancel/Apply/Save flows | Current confirmed semantics unchanged, including any guarded revert | NOT RUN |
| C10 | Runtime hold-close while pointer hovers/capture is ending | Actual wheel hides/releases safely; late tooltip/Present cannot flash it back | NOT RUN |

Inject failures through test adapters or a development harness; do not corrupt the user's real store. Record actual close latency for idle versus durable-pending cases rather than claiming all closes are instant.

## C. Compact visual Designer

Use a source-matched real Designer at approximately 900 × 650 logical client units, its supported smaller size, and a wider window. Record OS scale and actual client size. Capture sanitized before/after images when possible.

| ID | Check | Expected | Status / evidence |
|---|---|---|---|
| D1 | New visual workspace | One selected menu board dominates; compact selector/breadcrumbs; optional panes initially hidden | NOT RUN |
| D2 | Enable tree and inspector | Readable vertical tree, real bounded panes, internal scrolling, usable canvas | NOT RUN |
| D3 | Long menu names / many rings | No character-per-line collapse; no offscreen essential controls | NOT RUN |
| D4 | Select, warn, refresh, resize | No unsolicited section expansion or whole-window growth | NOT RUN |
| D5 | Close/reopen with changed pane state/zoom/pan | Explicit preferences remembered | NOT RUN |
| D6 | Reset Designer layout with dirty draft | Only UI preferences reset; contents/undo state unchanged | NOT RUN |
| D7 | Single/right/double click | Select, compact properties, submenu edit respectively; no leaf execution | NOT RUN |
| D8 | Center-plus/empty slot create then cancel | Explicit destination; cancel leaves document unchanged | NOT RUN |
| D9 | Drag to empty/occupied/outside slot | Visible destination, no silent overwrite, safe cancellation | NOT RUN |
| D10 | Create submenu / breadcrumb / Back / undo | One coherent transaction and actual visited-path navigation | NOT RUN |
| D11 | Zoom/pan and then hit/drag/right-click | Drawn and interactive coordinates agree | NOT RUN |
| D12 | Skins/advanced properties/import/export discovery | Existing capabilities still reachable and responsive | NOT RUN |
| D13 | Dynamic source/projected cell | Source retained; no implicit conversion to static content | NOT RUN |

Headless test evidence should exercise the actual Designer widget tree and capture rectangles/clip bounds, not only a layout helper or presence of AccessKit labels. Real screenshots complement—not replace—automated assertions.

## D. Tooltips and hover

| ID | Check | Expected | Status / evidence |
|---|---|---|---|
| T1 | Short one-line label | Text-sized near-cell box, roughly one line plus padding; no monitor-height rectangle | NOT RUN |
| T2 | Long original label/custom description | Reasonable wrapping/max width; readable source preserved | NOT RUN |
| T3 | Edges/corners and negative-origin monitor | Local flip/clamp; radial itself does not move | NOT RUN |
| T4 | Hover same cell through deadline | One tooltip; movement within cell does not endlessly delay/recreate it | NOT RUN |
| T5 | Rapid cell transitions, leave, Back, page | Old tooltip disappears; only current identity can show | NOT RUN |
| T6 | Repeated hover show/hide | No ghost menus, old pixels at new positions, input proxy flashing, or focus changes | NOT RUN |
| T7 | Click through actual outside area and tooltip | Tooltip has no action/input interception; existing owned radial gaps remain safe | NOT RUN |
| T8 | High-DPI scale and Unicode labels | Glyphs fit measured boxes; scale applied once; no clipping regression | NOT RUN |
| T9 | Close with timer/presentation pending | No tooltip or radial resurfaces after close | NOT RUN |

Useful evidence: a short screen recording with pointer; structural/layout and visual-backing bounds; native call counts for input Hide/Show/region updates during hover; trace showing stable session and geometry IDs. No requirement to OCR a screen or log private note text.

## E. Overlapping Cascade

| ID | Check | Expected | Status / evidence |
|---|---|---|---|
| K1 | Two menus with different complete skins/backgrounds | Full decorated parent behind full child, not detached ancestor cells | NOT RUN |
| K2 | Root → child → grandchild | Small consistent overlap; obvious active frontmost menu | NOT RUN |
| K3 | Click exposed parent cell/background | Return to that frame only; zero action/history/click-through on same gesture | NOT RUN |
| K4 | Click overlapping region owned by child | Child wins; does not activate parent beneath it | NOT RUN |
| K5 | Repeated Back / reopening / explicit drag | Saved visible centers remain stable; no accumulated drift | NOT RUN |
| K6 | Work-area edges / unequal sizes / mixed DPI | Stack stays local on session monitor or explicit safe existing fallback | NOT RUN |
| K7 | SameCenter and mixed presentation chain | Existing SameCenter behavior preserved; no bulk conversion | NOT RUN |
| K8 | Embedded and native authoring previews | Same layer/navigation result, without execution | NOT RUN |

## F. Automated and resource gates

Record actual commands, filter matches, source identity, logs, true exit codes, and counts. Initial values below are intentionally unfilled.

```text
S1 focused lifecycle/visibility/authoring gate:
S2 focused tooltip/render/native-presentation gate:
S3 production Designer UI/interaction gate:
S4 Cascade layers/navigation gate:
cargo fmt --all --check:
cargo check:
git diff --check:
Complete required cargo nextest run --no-fail-fast:
Repository-required additional checks:
Independent review and remediation:
Final source identity after any remediation:
```

Measure affected response times, repeated open/close handle counts, frame/cache bounds, unchanged grid idle/search behavior, and whether Designer idle or hover causes unnecessary work. Do not claim no regression without comparable observations.

## G. Long job record template

```text
Job purpose / milestone:
Command and cwd:
Profile / features / target:
Source SHA + source-diff fingerprint:
Session ID / PID + start time / process identity:
Durable stdout/stderr log:
Exit-code record:
Launch time:
First observation due: launch + 10 minutes
Second observation due: first check + 15 minutes
Later observations due: previous check + 20 minutes
Completion notification received:
Actual result / all failing tests / next corrective batch:
```

An earlier real completion event can be handled immediately. These are not kill deadlines. Never start a duplicate Cargo process or perform frequent log/status polling because output is quiet.

## Sign-off

Use **Passed**, **Failed**, **Not run**, or **Not applicable — reason** per case. A native evidence gap cannot be counted as a pass. Preserve historical automated success, but do not use it as proof of this candidate's visible behavior. Final report must distinguish implemented code, automated proof, observed Windows behavior, and unresolved limits.
