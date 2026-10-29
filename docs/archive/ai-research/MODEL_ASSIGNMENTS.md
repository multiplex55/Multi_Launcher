# Model Assignments — Quality-First Research, Sequential Roles

**Prepared and checked:** 2026-10-28  
**Applies to:** the two research goals in this package, not a permanent replacement of every repository workflow.

## 1. Recommended setup for this request

The user is prioritizing substantial reasoning and detailed engineering output over local build execution. My quality-oriented recommendation is:

| Role | Preferred model ID | Effort | Responsibility |
|---|---|---|---|
| Parent / orchestrator | `gpt-6-astra` | `xhigh` | Maintain evidence/state, make cross-goal judgments, consolidate recommendations, recover from unavailable child roles |
| Planner | `gpt-6-astra` | `max` | Deep architecture analysis, alternatives, constraints, source-backed milestone handoffs |
| Research implementer / author | `gpt-6-astra` | `xhigh` | Write substantive designs and follow-on packages; implement the isolated prototype |
| Reviewer | `gpt-6-astra` | `max` | Challenge reasoning, evidence, safety, benchmark validity, migration and future acceptance |

These are task-specific recommendations, not a measured comparison on Multi Launcher or a guarantee that the highest setting is always better. Use separate role contexts for review. The same model family can perform a separate review, but neither same-model agreement nor a role label proves independence from shared assumptions.

For the hardest bounded decisions—Windows file identity/cleanup semantics, input authority, ranking precedence, synthetic-evaluation leakage—max effort is worth considering. It is not necessary to use max for formatting an index or copying a completed role report. Higher effort is not a substitute for reading the correct source.

OpenAI's current model pages list Astra for demanding reasoning/research/coding and Sol for complex coding/professional work; both document high, xhigh and max effort support. Availability and the installed client's supported controls must still be checked at execution. [S1], [S2]

## 2. A deliberate lower-cost alternative

Keep **Planner = Astra max** and **Reviewer = Astra max**, but use **Research Implementer = GPT-6.1 Sol xhigh**, and optionally **Orchestrator = GPT-6.1 Sol high**.

That combination is my practical alternative for structured source inventory, cross-referencing, dataset preparation, Python prototype code, and turning already settled decisions into detailed documents. Escalate ambiguous ownership/safety/design judgments to an Astra role if available. Do not use the lower-cost model as an excuse to omit evidence or produce vague follow-ons.

For this user's requested thought-heavy work, I would not choose Luna as the main research author simply because it is already configured. Luna can still be useful for a narrowly constrained formatting or inventory transformation, but that is not the central task here. This is a recommendation about task fit, not a claim that Luna cannot perform useful analysis.

## 3. Actual repository configuration inspected during preparation

The following is a snapshot of fetched `master` configuration, not a promise about the user's future post-merge branch or the model used by any actual session:

| Location | Observed model/effort |
|---|---|
| `.codex/config.toml` parent/default | `gpt-6.1-sol`, `medium`; plan mode `high` |
| `.codex/agents/planner.toml` | `gpt-6.1-sol`, `high` |
| `.codex/agents/implementer.toml` | `gpt-6-luna`, `max` |
| `.codex/agents/reviewer.toml` | `gpt-6.1-sol`, `high`; commented Astra xhigh alternative |
| Agent concurrency setting | `max_concurrent_threads_per_session = 4` |

The fetched files' Git blob identities were:

```text
.codex/config.toml              e7d117afa9107f6eeff70105cc460a4701f6a520
.codex/agents/planner.toml       76688afbebca7923c8ad100b595282503c9cf840
.codex/agents/implementer.toml   462656b3033ba4df26fd2e69c67645379993e321
.codex/agents/reviewer.toml      840237e56c0fbb0780148445171016da290759f8
```

Source references: [R1], [R2], [R3], [R4]. Re-read actual files when executing. A configured value is a requested setting, not evidence of the resolved model used by an invocation.

## 4. Configuration guidance without silently changing the repository

No configuration files are changed by this package. Prefer supported task-scoped role assignments or task-specific custom agents over rewriting personal/global defaults.

Current official subagent documentation explains that model/effort may be supplied through parent settings, subagent defaults, explicit dispatch, and custom agent files; a pinned custom-agent value can override the previously resolved choice. Therefore, saying “use Astra” in a task prompt is not sufficient when the selected role file pins a different model. Inspect actual precedence and installed client support once. [S3], [S4]

If the user elects to adjust the existing role files for this research run, these are the recommended field values—not full replacement TOML files:

```toml
# planner role: retain its existing name, instructions and applicable restrictions
model = "gpt-6-astra"
model_reasoning_effort = "max"
```

```toml
# research implementer role
model = "gpt-6-astra"
model_reasoning_effort = "xhigh"
# Lower-cost alternative: model = "gpt-6.1-sol" with effort = "xhigh".
```

```toml
# reviewer role
model = "gpt-6-astra"
model_reasoning_effort = "max"
```

A task-scoped parent setup, where supported, can use:

```toml
model = "gpt-6-astra"
model_reasoning_effort = "xhigh"

[agents]
enabled = true
max_concurrent_threads_per_session = 1
```

Merge settings thoughtfully; do not paste a second `[agents]` table into an existing TOML file or erase unrelated keys. The concurrency key is a cap on spawned-agent threads; the parent remains responsible for explicitly scheduling sequential roles. A setting alone does not establish that no other task is using the machine.

Do not automatically change `.codex/`, AGENTS.md, sandbox/approval mode, account settings, installed tools, or subscription configuration during these docs-only research goals. If the preferred model cannot be selected under current permissions, record the fallback and continue with an available authorized model. No trial loop repeatedly invoking unavailable models.

A preference for max reasoning does not require “Ultra” or automatic parallel delegation. This program explicitly wants serial execution, so do not select a mode merely because it may launch more agents.

## 5. Role contract for research

### Planner

Ask the planner to reason through the actual current owners, alternatives, invariants, failure modes, and research questions. It returns a checkpoint packet that an author can execute without broad rediscovery. It does not modify production files or run tests.

The current repository planner is read-only by instruction. It can still author Markdown in its response, including complete future GOAL/KICKSTART content. The parent persists the content and attributes it correctly. Do not loosen permissions just to make every role physically write its own folder.

### Research implementer

Its deliverable is documentation, structured inventories, and the small isolated prototype. Explicitly tell it that “implementation” does not mean editing `src/`, Cargo manifests or application tests. The milestone's verification is static/document/prototype-only; generic habits about running Cargo do not expand this task.

### Reviewer

Use a separate role context when available. Give it the exact goal, source baseline, artifacts and evaluation protocol. Ask it to look for unsupported claims, overlooked guards, unsafe migration, evaluation leakage, ambiguous goals, and unnecessary scope. Its result must distinguish critical factual problems from optional polish.

No source writes, test campaigns or automatic redesign. A new actionable review recommendation must include a full proposed follow-on pair or a link to the canonical existing package with a precise amendment.

### Parent fallback

The parent can continue research and write its own critique if dispatch fails. Record `review_mode: parent_self_review`, not independent review. Do not claim a particular child model ran when invocation metadata is unavailable. This limitation can be resolved later without losing the useful research artifacts.

## 6. Provenance fields for every role

Store in each role's `ROLE_RECORD.md`:

```text
role:
checkpoint:
requested_model:
requested_effort:
resolved_model: observed value or unverified
resolved_effort: observed value or unverified
role_or_thread_id: when exposed
execution_mode: child / parent_fallback
authored_by:
persisted_by:
source_baseline:
input_artifacts:
output_artifacts:
review_independence: separate_context / parent_self_review / not_applicable
limitations:
```

These are automatically filled by the executing orchestrator; the user need not fill them before launching. Preserve actual transitions when a model changes between checkpoints.

## 7. Models inside generated follow-on packages

Every future recommendation gets its own assignment rationale. Do not blindly copy the research program's most expensive setup into a trivial code task.

| Future work type | Suggested planner | Suggested implementer | Suggested reviewer |
|---|---|---|---|
| Ambiguous architecture / file safety / input ownership | Astra max | Astra xhigh | Astra max |
| Well-scoped pure ranking core or typed metadata | Astra xhigh | Sol xhigh | Astra xhigh |
| Narrow, evidence-backed bug fix | Sol high/xhigh | Sol high/xhigh | Astra xhigh or Sol xhigh |
| Research spike with competing platform semantics | Astra max | Astra xhigh | Astra max |
| Documentation-only mapping or catalog cleanup | Sol high | Sol high | Sol high; escalate substantive claims |

These are recommendations to review at future execution. Model availability, actual scope, and evidence can change. High effort does not authorize wider edits, more tests, a bigger refactor, or extra repair attempts.

## 8. References

External references checked 2026-10-28; verify again if selecting models substantially later.

- [S1 — GPT-6 Astra model](https://developers.openai.com/api/docs/models/gpt-6-astra)
- [S2 — GPT-6.1 Sol model](https://developers.openai.com/api/docs/models/gpt-6.1-sol)
- [S3 — Official subagent model/configuration guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents)
- [S4 — Configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference)
- [S5 — Reasoning effort guidance](https://developers.openai.com/api/docs/guides/reasoning)
- [R1 — Repository parent/default configuration](https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/config.toml)
- [R2 — Repository planner role](https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/planner.toml)
- [R3 — Repository implementer role](https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/implementer.toml)
- [R4 — Repository reviewer role](https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/reviewer.toml)

Repository links above are moving navigation links; the blob IDs document the preparation snapshot. Runtime research must cite its own pinned source commit.

[S1]: https://developers.openai.com/api/docs/models/gpt-6-astra
[S2]: https://developers.openai.com/api/docs/models/gpt-6.1-sol
[S3]: https://learn.chatgpt.com/docs/agent-configuration/subagents
[S4]: https://learn.chatgpt.com/docs/config-file/config-reference
[S5]: https://developers.openai.com/api/docs/guides/reasoning
[R1]: https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/config.toml
[R2]: https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/planner.toml
[R3]: https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/implementer.toml
[R4]: https://github.com/multiplex55/Multi_Launcher/blob/master/.codex/agents/reviewer.toml
