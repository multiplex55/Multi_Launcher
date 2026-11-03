# Studio and Launcher Usability execution

Contract: `docs/plans/CODEX_GOAL_STUDIO_LAUNCHER_USABILITY.md` and companion Kickstart.

## Ancestry and initial state

- Initial checkout: clean `master`, `1439676a2164e8e8ff4bd6bb9d9f2aee07515c25`; no user-dirty paths.
- Parent branch: `feat/visual-studios-06-markdown-studio`.
- Parent tip: `3b544af7507474f31131dea98bb8dea0756a4c4e`.
- Authority: complete predecessor handoff `target/visual-studios/program-final-handoff.json`, matching local branch and recorded upstream tip. Predecessor committed and pushed.
- Task branch: `feat/studio-launcher-usability`, initial HEAD verified equal to parent.
- Master differs only in documentation archival and the two new task contracts. Retain the task contracts on this exact-parent branch; do not import archival reorganization or merge master.
- One checkout; no worktrees, auxiliary clones, agent branches, or intermediate merges.
- Git branch creation initially failed because sandbox denies `.git/index.lock`; authorized escalation succeeded without changing user work.

## Checkpoints

| Checkpoint | State | Evidence |
| --- | --- | --- |
| M0-A | in_progress | Exact parent verified; baseline and initial push next |
| M1-A / M1-B | pending | Shared query focus / execution |
| M2-A | pending | Diagram presentation |
| M2-B / M2-C / M2-D | pending | Offline renderer / templates / viewer |
| M3-A / M3-B / M3-C | pending | Calculator grammar / host / recovery |
| M4-A / M4-B | pending | Note tables / safe close |
| M5-A | pending | Dashboard folds / save feedback |
| M6-A | pending | Scoped integration / native smoke / lint / independent review |

Use Goal section F checkpoint subjects and acceptance criteria. Planner supplies bounded packets; one implementer writes at a time; parent owns Git. Use cached fast-dev, nonempty focused Nextest selections with no-fail-fast, and new app fixtures disable both Paste Sequences shortcuts. One cumulative independent review and final lint pass. Native smoke is bounded to Goal H3. No full-suite campaign.
