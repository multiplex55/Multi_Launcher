# Jump Mode Execution

- Authority: CODEX_GOAL_JUMP_MODE.md (approved M00–M15), CODEX_KICKSTART_JUMP_MODE.md, current AGENTS.md.
- Source branch: master; starting SHA: 8f94b003a12853735d1cb4689c6ec4e53c4ace8e.
- Task branch: feat/jump-mode-2026-10-19-c7e42 (created from checked-out HEAD).
- Initial working tree: clean, including tracked task-input documents. No user changes copied/discarded.
- Environment: Windows PowerShell; rustc/cargo 1.99.0; Nextest 0.9.135; egui/eframe 0.36.2 with local vendor patches.
- Git branch creation needed sandbox escalation because .git is read-only in the normal sandbox; authorized creation succeeded.
- Current milestone: M00 ownership inspection; M01 bounded implementation delegated sequentially.
- Latest evidence: pending baseline run.
- Push status: M00 checkpoint ready for commit and one normal push.

| Milestone | State | Checkpoint / verification | Evidence or dependency |
|---|---|---|---|
| M00 | complete | Branch from recorded HEAD; verified source map | Clean initial tree; existing fixture isolation seam |
| M01 | in_progress | Isolated fixture runner | Single implementer owns source writes |
| M02 | pending | Persisted default-off gate | M00 |
| M03 | pending | Pure hint engine | M02 |
| M04 | pending | Registry and safe focus intents | M03 |
| M05 | pending | Input lifecycle and overlay | M02–M04 |
| M06 | pending | Grid integration | M05 |
| M07 | pending | Dashboard integration | M05 |
| M08 | pending | Native keyboard acceptance | M01, M06–M07 |
| M09 | pending | Phase 1 review | M02–M08 |
| M10 | pending | Modal ownership and Settings | M09 |
| M11 | pending | Notes/Todo/Timer/Paste Sequence | M10 |
| M12 | pending | Complex controls and MkMacro | M10–M11 |
| M13 | pending | Extended native evidence | M08, M10–M12 |
| M14 | pending | Focused reliability and review | M09, M13 |
| M15 | pending | Final smoke and handoff | All prior outcomes |

## Verified source ownership

- `src/main.rs::main`: derives `AppDataRoot` from cwd-relative settings.json and acquires `SingleInstanceGuard` before persistent reads. Existing cwd isolation can preserve production identity semantics without adding a bypass.
- `src/main.rs::spawn_gui`: creates `LauncherApp`, installs optional QA framebuffer request and startup services, runs native Glow renderer.
- `src/platform/app_data.rs::AppDataRoot`: records existing cwd-relative resolution; normalizes directory identity for single-instance locking.
- `src/gui/mod.rs::LauncherApp`: owns query, logical selected result and `handle_key`; current action/navigation tests live alongside it.
- `src/gui/render.rs`: owns root frame phases, query editor/autofocus, keyboard routing and result painting. True query focus gates several existing keyboard flows; Jump Mode must account for this explicitly.
- `src/gui/render.rs`: multipass rendering guards side effects with `first_ui_pass`; existing action sheet, OCR and snippet prompt have higher input ownership. Capture leader before query rendering while respecting those owners. Settings/confirmation use ordinary egui windows; explicit surface ownership is needed, not panel-stack inference alone.
- `src/actions/mod.rs::Action`: label/description/action/args. `ActionIdLookup` is tied to its immutable catalog; IDs use action strings. A stale result index alone is insufficient for deferred targeting.
- `src/dashboard/dashboard.rs::DashboardContext`, `WidgetActivation`; `src/dashboard/widgets/mod.rs::Widget`, `WidgetAction`: widgets return existing action payloads, dashboard owns slot identity. Registration must preserve this route.
- `src/settings/model.rs::Settings`: serde defaults and explicit Rust Default; existing `settings_editor/render.rs` controls need a small opt-in setting.
- `src/gui/qa_framebuffer.rs`: existing gated ROOT capture, startup/F11 triggers, maximum 16 captures per request, exclusive fresh output directory. ROOT captures CPU framebuffer completion before GPU presentation; no claim of presented pixels.
- `dev/run_owned_process.ps1`: verified PID/creation-time tree tracking and bounded cleanup. Runner currently lacks cwd/env overrides, so reuse its ownership pattern or extend narrowly.
- Notes fixture must explicitly use `ML_NOTES_DIR`; safe enabled-plugin allowlist and empty index paths prevent loading live user sources.

## Execution boundaries

Parent owns branch, ledger, commits and one best-effort push per checkpoint. Planner is read-only. One implementer owns each bounded source milestone; reviewer is read-only at meaningful integration boundaries. No PR, merge, force push, unrelated refactor or live-data testing.

## Next action

Complete source map, checkpoint M00, then verify M01 baseline fixture/launch/capture/cleanup. Continue M02–M15 automatically; preserve honest environment blockers and progress independently where safe.

