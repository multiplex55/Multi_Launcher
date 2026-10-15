# Windows functional qualification — current modernization

Authority: PERFORMANCE_ACCEPTANCE_ADDENDUM.md. This records available functional evidence and exact remaining manual QA; unavailable desktop observations are not invented passes or automatic blockers to safe theming. Accepted synthetic performance debt is not reopened.

## Chosen source and available checks

Current integrated runtime follows6527ddc1/79653eca; M2-A checkpoint71a41051 adds only explicit headless texture-output disposal in one strict radial lifecycle test. Actual Windows fast-dev compilation and all-target check passed. Both ClipboardModify contracts and the corrected radial closed-frame/real-callback contract passed; see receipts/m2a-live-correctness-closeout.md. Integrated vendor layout/preparation sources still match the accepted source796 reference after known text EOL normalization only.

Existing M1-C evidence covers once-only query/action/history dispatch, hidden logic without stale key/drop replay, hidden macro/parameter child bootstrap without root activation, geometry/selection/absolute widget IDs, bounded list/grid virtualization and note popup ownership. M1-D covers Markdown/cache/link/checkbox semantics, local image formats/DDS admission, QR texture reuse/PNG, macro image preview, crop, toast hover/lifetime and file-drop boundaries. See receipts/m1c-verification.md and m1d-verification.md. These are source/headless/integration results, not physical input/focus proof.

The broad stabilization run retained6463run/6242pass/221fail/45skip. Scoped correction runs closed the failures without rewriting the original result; final corrections005 and006 each passed17/17, including strict body-size restoration with font/line-spacing variants. Global formatting and all-target Clippy005 passed; final lib Clippy006 passed. No benchmark owner was activated. See receipts/m2d-functional-stabilization.md for actual counts/source-bound logs and remaining qualification limits.

## Functional coverage and manual follow-up

| Behavior | Available evidence | Native/manual observation still required |
|---|---|---|
| Hotkey show/hide, tap/hold and radial coexistence | Migrated input/visibility/lifecycle owners and existing focused tests; current broad gate/scoped corrections include applicable automated tests | Physical launcher chord while hidden/focused, tap/hold/release and repeated invocation. Not freshly observed; no synthetic Win-key substitution |
| Query typing, navigation, Enter/Escape, once-only execution | M1-C root action/history and event-modifier/focus tests; list/grid stable identity oracles | Current native typing/IME/arrows/Enter/Escape under real focus and hook transport |
| List/grid/scroll and large catalogs | Existing eager-layout oracles, warm cache/revision rules, absolute IDs and bounded visible-row tests; retained CPU results | Actual scrolling/toggle/selection/anchoring at representative catalog size and available DPI |
| Notes and representative plugins | M1-C/M1-D note/Markdown/image/QR/toast/file-drop and editor tests; previous native framebuffer visuals documented in ledger | Current native note editing and representative dialog open/close/scroll with focus restoration |
| Hidden-root child/prompt create/dismiss | Existing hidden macro prompt/parameter notice bootstrap, stale-input and root-unpainted tests; strict radial real retained callback close now passes actual live source | Real hidden-root child visibility/closing, no root flash or focus theft; native callback/window transport |
| Window placement/focus | Native bridge/lifecycle source and tested no-root-move/activation command semantics | Actual monitor/negative-origin/mixed-DPI placement, prior foreground restoration and root parking |
| True idle vs active timers/effects | Event-driven architecture and existing repaint/deadline tests; no theming effect/animation added yet | Bounded native hidden/visible idle observation with pointer still, timers/previews/tooltips distinguished; sustained busy work must be fixed if observed |
| Persistence and data integrity | Existing settings/model/editor and failed-save preservation tests; broad gate and scoped corrections documented | Isolated-fixture restart/save/Cancel and recovered settings; no personal-data migration or destructive action |

## Environment and decision

At M2-D closeout, CUA exposed browser surfaces only and no callable Sky connector had been identified. During M3-B orchestration, deferred-tool discovery found `mcp__node_repl__js`; the Computer Use skill initialized `@oai/sky` successfully and `list_windows()` returned native windows. No launcher window was running in that inventory. This restores a supported route to attempt bounded later native review; it does not verify capture, input, focus or any launcher behavior yet. Earlier capture/elevation/profiling failures remain retained. Avoid unchanged retries, new profilers or unsupported input injection.

Existing native framebuffer screenshots prove their specific rendered output; they do not establish OS composition, physical chords, hotkey-to-visible latency or focus ownership. Broader physical/hardware/DPI/RDP observations remain explicit release-qualification gaps. Use available supported checks and continue safe implementation; correct any demonstrated critical defect before claiming its functionality stable.

Renderer decision: actual Cargo features and NativeOptions explicitly select Glow. Existing renderer retained; comparative performance study deferred to POST_MODERNIZATION_PERFORMANCE_BACKLOG.md. No new backend variant or renderer change.

M2-B/M2-D acceptance uses actual automated results, supported observations, documented accepted debt and these limits. It must not claim complete native verification or unconditional release readiness. After the functional migration checkpoint, immediately continue M3-A through the full visual rollout.

## M3-D bounded normal-build observation

Normal fast-dev themed executable C53EE78341816314196B833C91191F55E5368EC964E81B4078841DF4D0C40F64 built successfully; source/log bindings in m3d/resources-002/build. Isolated synthetic run native-themed/run-m3d-001 launchedPID35608. Supported Sky returned the actual window; capture failed FrameArrived and one refreshed foreground retry failed window-capture timeout. Stop unchanged retries. No screenshot/visual claim.

Accessibility-only checks observed focused query typing, Ctrl+A, local `app M0-B fixture action 00000` result, and Return changing the query to `fixture-enter-00000`. Escape removed exposed launcher content. Indexed Settings click failed because coordinate geometry is unavailable; Shift+Tab retained query focus. Synthetic Ctrl+Alt+F10 did not restore exposed content in two observations; physical hotkey restoration remains unverified and requires manual QA. Do not reinterpret it as a verified hotkey pass. No current native dialogs/theme preview/grid scrolling/IME/DPI/monitor proof.

One10.039s post-Escape sample recorded CPUdelta0.0625s, working set128643072→128643072bytes, Responding=true. This bounded sample showed no sustained busy behavior/growth; it is not comprehensive true-idle/GPU/native latency qualification. Normal Alt+F4 closed the fixture and subsequent process inventory confirmed exit. Raw process/provenance/observation/idle/log receipts are retained under target/egui-modernization/native-themed/run-m3d-001. No performance/instrumentation flags were enabled.

## M4-B rendered framebuffer coverage

Existing opt-in QA feature produced verified ROOT Dark/Light852x460 images, visually inspected at sourcea4fbebed; see receipts/m4b-root-visual-review.md. This restores rendered-pixel coverage for the new root surfaces. It does not remove physical chord/focus/composition/DPI/IME/dialog interaction gaps or the normal-build synthetic hotkey-restoration limitation. Same executable reused; all owned fixtures closed. Malformed fixture/default400x220 image preserved separately, not counted as intended theme acceptance; planned recovery-surface review should inspect its constrained banner layout.

## Current desktop policy and screenshot route

User permanently prohibits Computer Use for this goal. Prior Computer Use observations remain historical evidence only; no future permission is implied. Use existing app-native framebuffer requests for screenshots and normal process/build/test tooling, with no synthetic desktop mouse/keyboard/window automation. Notes002 and Theme001 atda0ca08c captured successfully withoutCU; receipts/m5b1-notes-presentation.md. These qualify rendered pixels only. Physical hotkey/focus/placement/normal-close observations remain unverified manualQA where not previously established. Owned fixtures are retired by identity-checked process stop afterSaved; that does not prove normal app close lifecycle.

M5-B2 app-native History002 confirms populated persisted synthetic history on pinned startup after transition-only initialization;001 empty-view evidence retained. The15scoped contracts include repeated-open draft preservation and existing pinned-close behavior. Capture is rendered client framebuffer only, not native input/focus/normal-close proof; owned processes stopped after capture. Clipboard Modify native visual/clipboard/focus behavior remains an explicit manual gap, not a passing check. Computer Use remains forbidden.
