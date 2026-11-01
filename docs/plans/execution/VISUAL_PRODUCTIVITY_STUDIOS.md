# Visual Productivity Studios execution

Binding scope: [Goal](../CODEX_GOAL_VISUAL_PRODUCTIVITY_STUDIOS.md) and [Kickstart](../CODEX_KICKSTART_VISUAL_PRODUCTIVITY_STUDIOS.md).

## Bootstrap / C01

- Single existing checkout: `G:\Repos\rust\Multi_Launcher`; clean before transition.
- `parent_branch`: `chore/native-acceptance-closeout-20261101-r01`.
- `parent_tip_sha`: `e929b0fc801c436f295ea74416969e9ba22c5dfa` (local/upstream equal; closing C05/M5-C commit matches predecessor handoff).
- Category branch: `feat/visual-studios-01-quick-tools` created directly at that SHA.
- The previous checkout was master `26602cd09c50c15c48cf587dba200989b200888a`. Its application sources match the predecessor. Only approved Goal/Kickstart copied into this baseline; later documentation relocation is not imported.
- This commit is C01/M0-A; actual baseline SHA, direct-parent verification and initial push result recorded after commit.
- No application edits/build/tests before bootstrap. No worktree, auxiliary clone, merge, rebase, cherry-pick or competing writer.

## Status

| Category | State | Accepted / remaining |
| --- | --- | --- |
| C01 Quick Tools | in_progress | M0-A bootstrap; settings and responsive UI pending |
| C02 Calculation Pad / Converter | pending | All checkpoints pending |
| C03 Date & Time | pending | All checkpoints pending |
| C04 Diagram / Mermaid | pending | All checkpoints pending |
| C05 Regex | pending | All checkpoints pending |
| C06 Markdown / Notes | pending | All checkpoints pending; Mermaid depends on accepted C04 |

## Execution rules

Follow Goal F checkpoints sequentially, with one writer, focused Nextest verification (100-second per-test execution bound), scoped GUI builds and real Windows interaction evidence. Native gates stay unverified until observed. Parent owns Git and command sequencing. Record checkpoint SHAs, checks, push outcomes and exact category final tips here; each following branch and baseline directly descend from that final tip. No automatic merge.
