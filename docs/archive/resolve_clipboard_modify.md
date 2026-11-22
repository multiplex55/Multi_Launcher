## Task: Resolve Clipboard Modify Native Read Timeout

**Repository:** `multiplex55/Multi_Launcher`  
**Starting branch:** `fix/nextest-stack-overflows-and-clipboard`  
**Reference HEAD:** `a93675051a4c48380633d98c285f5bb5c7cc7515`

### Problem

Clipboard Modify continues to display:

`Clipboard source error: Permanent("The clipboard did not respond in time.")`

The existing M3 corrections improve retry classification, lock contention, state preservation, and Undo safety, but have not resolved the actual clipboard retrieval failure.

### Required work

Investigate the WinRT `Clipboard::GetContent()` → `DataPackageView::GetTextAsync()` → `wait_text()` path.

Specifically, determine whether COM apartment configuration, synchronous UI-thread execution, or incomplete asynchronous operation handling is preventing ordinary text reads from completing.

Add privacy-safe diagnostics that distinguish an operation timeout from a failing HRESULT. Record the operation stage, elapsed time, and relevant apartment information without logging clipboard contents.

Move potentially blocking clipboard reads away from the GUI thread where appropriate. Prefer a correctly initialized Windows Runtime worker and asynchronous request/completion handling. Ensure both initial dialog loading and Reload Clipboard follow the corrected path.

A timed-out operation must not block the GUI, corrupt a draft, overwrite newer text, invalidate an existing preview unnecessarily, or permit a stale result from an earlier request to replace the current source.

If WinRT retrieval remains unreliable after correcting the execution context, evaluate a native Windows text retrieval alternative. Preserve the existing private-clipboard marker detection and never bypass protected clipboard safeguards.

Distinguish retryable timeouts from genuinely permanent failures. Do not treat increasing the timeout or adding retries as a sufficient fix.

### Verification

Add deterministic tests covering timeout handling, successful recovery, asynchronous completion, stale responses, privacy protection, and preservation of source/Undo state.

Use a synthetic non-sensitive clipboard fixture for native testing when an appropriate Windows session is available.

Native acceptance requires opening Clipboard Modify, loading ordinary text, previewing and committing a transformation, verifying the resulting clipboard text, and successfully using Undo.

Run scoped verification first. Do not expand into the unrelated M4 full-suite milestone.

Preserve the existing branch workflow, commit changes in reviewable checkpoints, and report native acceptance separately from deterministic test success. Do not claim the regression is resolved based solely on mocked tests.