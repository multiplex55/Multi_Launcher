# Platform evidence checked by orchestrator

Accessed 2026-10-28. This is an API-contract reference, not a filesystem experiment.

## MoveFileExW

[Microsoft API reference](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw), page last updated 2023-06-01: REPLACE_EXISTING permits replacement subject to access rights and does not replace an existing directory. WRITE_THROUGH documents flushing for copy/delete moves. The page also distinguishes cross-volume copying and security-descriptor behavior.

[Current atomic-file owner](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/atomic_file.rs#L7-L56) writes a newly created temporary file in the destination directory, flushes it, optionally syncs it, drops the file and calls replacement. [Windows replacement](https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/atomic_file.rs#L113-L150) selects REPLACE_EXISTING with optional WRITE_THROUGH.

**Interpretation:** preserve the distinction between durable and replaceable state. Neither the API name nor this static inspection proves complete power-failure behavior, adversarial identity continuity, or correctness of a multi-file transaction. No current defect is asserted by this reference alone. Any stronger claim needs its own owner-specific evidence and bounded future validation.
