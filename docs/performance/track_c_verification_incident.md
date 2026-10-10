# Track C C0-B verification incident

During C0-B verification, the existing integration target `recycle_plugin` was selected together with `watchers`. All five tests reported PASS, including `command_returns_immediately_and_cleans`. The orchestrator failed to inspect this test's native effect before executing it.

Read-only source review subsequently established this call chain: `tests/recycle_plugin.rs::command_returns_immediately_and_cleans` calls `launch_action` with `recycle:clean`, dispatched through `commands/headless.rs` to `actions/system.rs::recycle_clean`, which spawns a background call to `launcher.rs::clean_recycle_bin` and `SHEmptyRecycleBinW(None, None, NOCONFIRMATION | NOPROGRESSUI | NOSOUND)`. There is no test-only guard on the production call. This attempted to empty the actual Windows Recycle Bin. Whether any items were deleted is UNKNOWN. The success notification does not wait for cleanup or reflect its result, and the cleanup result is ignored; its PASS cannot establish either success or failure of deletion. No user data inspection or recovery operation was performed.

This is a verification execution mistake involving a pre-existing test, not a runtime behavior introduced by Track C. The user was informed immediately when source review identified it. The no-unexpected-native-action acceptance statement is not satisfied by that run and must not be reported as PASS. Native visible responsiveness validation remains NOT RUN; the accidental API action is separately disclosed here.

Mitigation implemented: preserve the test body/assertions but mark this one destructive test explicitly ignored/manual-only, requiring an authorized disposable native environment. Do not rerun it in this campaign. Verify only compilation/test listing and the safe search case. This is a legitimate restriction on a destructive native test, not removal of a failing behavioral assertion. Future owner selections must inspect irreversible/native execution paths before running; no broad or ignored native test selection is authorized.

Raw verification log: ignored `target/performance/track-c/c0-b/nextest-watchers-recycle.log`. The report contains no user data or contents of the Recycle Bin. Outcome remains UNKNOWN; mitigation does not undo any prior effect.

Mitigation verification: cargo nextest list --cargo-profile fast-dev --test recycle_plugin succeeded (5.56s compile/list); default listing includes only search_returns_action. No tests or native operations executed in this verification. Destructive test body/assertions retained unchanged; independent source/mitigation review PASS with the verification execution exception retained.

The extra clipboard_modifiers.json was independently confirmed byte-for-byte equal to serialized built-in defaults and traced to the same test constructor using relative settings.json. It was preserved under ignored C0-B evidence, not committed or deleted. No user contents were probed.
