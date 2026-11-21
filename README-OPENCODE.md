# OpenCode configuration converted from your uploaded `.codex.zip`

This package preserves the developer instructions from all three source TOML agent profiles **verbatim** (except line endings and surrounding blank lines), and adds an optional primary orchestration agent and three slash commands. Source models and reasoning efforts remain explicit.

## Install (Windows PowerShell)

1. Extract this ZIP into the **root of your Multi_Launcher repository**, preserving hidden `.opencode/` folder.
2. Back up any existing `opencode.json` / `opencode.jsonc` and `.opencode/agents/` files **before replacing them**; the package supplies a root `opencode.jsonc`.
3. From the repository root, run `opencode`, then `/connect` -> OpenAI -> Sign in with ChatGPT (or your existing provider).
4. Run `/models`. Confirm the three configured model/variant selections are available to your particular connection:
   - `openai/gpt-6.2-sol#high` (planner and reviewer)
   - `openai/gpt-6-luna#max` (implementer)
   - `openai/gpt-6.2-sol#medium` (primary orchestrator)
5. If a model or its `#variant` is unavailable, **edit the `model:` line in that agent Markdown file** to an exact supported value from `/models`, or delete that `model:` line to inherit the parent model. You may also need to adjust the root `opencode.jsonc` `model` field. OpenCode does not translate Codex model names automatically.
6. Test: `Use the planner subagent to inspect the code relevant to <feature> and propose one milestone without changing files.`
7. Run an actual goal/kickstart: `/kickstart path/to/GOAL.md path/to/KICKSTART.md`.

## Files

- `opencode.jsonc`: chooses `orchestrator` as the default primary agent and mirrors the Codex main Sol model (base ID).
- `.opencode/agents/planner.md`: source planning prompt, subagent, Sol High, non-editing.
- `.opencode/agents/implementer.md`: source implementation prompt, subagent, Luna Max, write-capable.
- `.opencode/agents/reviewer.md`: source review prompt, subagent, Sol High, non-editing.
- `.opencode/agents/orchestrator.md`: **new**, runs planner / implementer / reviewer sequentially.
- `.opencode/commands/kickstart.md`: the `/kickstart` command.
- `.opencode/commands/plan-milestones.md`: the `/plan-milestones` command.
- `.opencode/commands/review-milestone.md`: the `/review-milestone` command.

## Key differences and limitations

- The source `.codex/config.toml` had `max_concurrent_threads_per_session=4` and a default subagent model `gpt-6-astra` at `xhigh`. OpenCode does not document direct equivalents of those particular Codex knobs; the *dedicated* agent model selections are preserved and orchestration limits implementation work to one active milestone at a time.
- OpenCode V2 has `permissions` rule lists; read-only agents deny edits and general shell execution while permitting common code-inspection commands. These agent-level rules are **not an OS-level sandbox**. Review your global permissions and provider plugins as well.
- Subagent sessions are background/asynchronous in current OpenCode. The orchestrator is explicitly instructed to await real completion reports before continuing; automatic long-running execution is dependent on the current OpenCode runtime/client behavior.
- `.codex/` is unchanged. OpenCode loads `.opencode/` and `AGENTS.md` project instructions separately.
- No Git branch or push operations are performed by the package.
- All models used here require that they are visible to the connection in `/models`; OpenCode Console and ChatGPT sign-in are **different billing/authentication paths**.

Docs: https://opencode.ai/docs/agents/ | https://opencode.ai/docs/permissions/ | https://opencode.ai/docs/models/ | https://opencode.ai/docs/commands/ | https://opencode.ai/docs/providers/
