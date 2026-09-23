# Multi Launcher — Automated Radial Acceptance Matrix

**Initial state:** NOT RUN. This file is a runner/ledger target, not evidence of success.

Use only an isolated deterministic profile or a temporary copy of a supplied test profile. Never run destructive acceptance against the original profile.

## Candidate record

```text
source/commit identity:
working diff identity:
launcher path:
launcher SHA-256:
runner path:
runner SHA-256:
Windows version:
launcher PID:
temp data root:
settings SHA-256:
radial SHA-256:
acceptance chord:
hold threshold:
monitor/work-area/DPI summary:
child log:
trace:
report JSON:
report text:
```

## Native cases

| ID | Scenario | Expected | Initial |
|---|---|---|---|
| H0 | ROOT focused + safe injected short tap | ROOT hides; one short tap; radial unchanged | NOT RUN |
| H1 | ROOT hidden + tap | ROOT shows per current placement/focus policy | NOT RUN |
| H2 | ROOT visible, acceptance runner/other owned window focused + tap | global tap toggles ROOT | NOT RUN |
| H3 | Designer focused/open + tap | ROOT toggles only; Designer remains open/interactable | NOT RUN |
| H4 | safe injected hold | runtime radial toggles/open; ROOT unchanged | NOT RUN |
| H5 | release after hold | no grid co-fire | NOT RUN |
| H6 | second full hold | runtime radial closes/toggles; release inert | NOT RUN |
| D0 | Open Edit Radial Menus | one Designer HWND; body reaches Enabled | NOT RUN |
| D1 | Native real pointer click on harmless Designer control | target -> DesignerPointer -> Enabled body -> accepted widget -> visible state change | NOT RUN |
| D2 | Real Tab | focus moves to another eligible Designer control | NOT RUN |
| D3 | Hide ROOT while Designer open | Designer continues receiving input/replies | NOT RUN |
| D4 | Edit Radial Skins | same authoritative Designer reaches Skins mode and remains interactive | NOT RUN |
| D5 | Clean close | Designer closes promptly; ROOT/process remain | NOT RUN |
| D6 | Dirty close | existing safe close prompt works; Keep Editing preserves draft | NOT RUN |
| A0 | New Menu | new menu created with stable ID and selected | NOT RUN |
| A1 | Add Ring | proposal/review appears before mutation | NOT RUN |
| A2 | Slots grow/apply | valid proposal applies; existing cell IDs preserved | NOT RUN |
| A3 | Action search | target beyond old first-50 subset can be found/assigned | NOT RUN |
| A4 | Open in Inspector | Inspector opens/selects same item; popup edits handled safely | NOT RUN |
| A5 | Skin/style edit | draft/preview changes | NOT RUN |
| A6 | Save -> close -> reopen | intended typed state persisted | NOT RUN |
| A7 | Undo/redo | representative edit reverses/reapplies | NOT RUN |
| A8 | Design safety | no real leaf execution/history side effect | NOT RUN |
| G0 | Outer-ring geometry proposal | valid preview, explicit Apply | NOT RUN |
| G1 | Populated shrink cancel/resolution | no silent loss | NOT RUN |
| G2 | Compact Designer structure | essential controls/canvas have sane nonoverlapping bounds | NOT RUN |
| R0 | Machine-readable report | valid JSON/text and identities | NOT RUN |
| R1 | Failure artifact helper | trace/log/screenshot/report retained for controlled harness failure | NOT RUN |
| R2 | Cleanup | child/owned windows gone; temp profile removed on success | NOT RUN |

## Deterministic/headless cases

- Exact `Shift+Alt+Win+End` tap/hold/release table with fake time.
- Active radial + short tap = grid only.
- Active radial + hold = radial only.
- repeat/full-release/fresh-cycle ownership.
- emergency/Screen Draw priority.
- production Designer semantic click via AccessKit bounds + RawInput across retained frames.
- initial snapshot -> enabled -> click path.
- Tab/focus movement in production Designer.
- report/profile-builder pure tests.

## Allowed statuses

Use only:

```text
PASSED
FAILED
SKIPPED — explicit supported reason
UNSUPPORTED — technical boundary and evidence
NOT RUN
```

Do not convert missing native evidence into PASSED because unit tests are green.
