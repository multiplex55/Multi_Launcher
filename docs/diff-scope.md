# Diff scope and safety boundary

## Current scope

Diff currently provides:

- two-way text comparison;
- two-way binary/hex comparison;
- recursive two-way folder comparison; and
- controlled, previewed folder reconciliation.

The hex view is a read-only **hex comparator**, not a hex editor. Binary views
provide difference navigation, but no editing or save command.

Folder reconciliation is deliberately controlled: the user captures a
selection, reviews an immutable copy or deletion plan, and confirms it before
execution. A GUI deletion means moving the selected item to the Recycle Bin;
the GUI has no permanent-delete command. Completion causes a refresh while
preserving any selection which still exists.

## Planned scope

- three-way **text** comparison and merge.

This is a deferred initiative, not a capability of the current UI. Its design
and acceptance gates are in [diff-three-way-initiative.md](diff-three-way-initiative.md).

## Explicit exclusions

Diff does not provide, and its GUI must not imply:

- automatic synchronization or mirroring;
- three-way folders;
- archive virtual folders or archive editing;
- image/media comparison;
- cloud, FTP, or SFTP access;
- Git-specific UI; or
- binary editing.

`.zip` and other archives are ordinary binary files. They may be compared by
the binary/hex comparator, but are never mounted, expanded, or traversed, and
their members cannot be edited through Diff. The planned three-way route is
text-only and must reject folders, binary files, and archives.

## Principal folder workflow

The supported workflow is: assign two directory paths using either picker;
open the comparison; progressively ingest generation-checked scan events;
filter and navigate the results; open a modified text or binary child; navigate
its visible differences; return with **Back** to the retained folder state;
capture a selection; preview and confirm a copy or Recycle deletion; process
the execution report; then refresh and restore the surviving selection.

State/controller tests exercise this workflow with injected scan events,
filesystem fixtures, operation reports/executors, and picker-assignment helpers.
They intentionally do not automate egui.
