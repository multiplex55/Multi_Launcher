# AUD-F002 M1-A — hand-reviewed identity transitions

Status: **STATIC_WITNESS_ONLY** on 2026-10-29. Source baseline `59156d96adcc73939c7171487a0cb582e3286587`. No mutation, native fixture, executable model, test or application was run. This analysis demonstrates consequences of explicitly stated observations and operations; it does not prove that a particular Windows filesystem admits an interleaving or that a user suffered data loss.

Owners and source/API IDs are defined in [contract and evidence](CONTRACT_AND_EVIDENCE.md), [source inventory](SOURCE_AND_TEST_INVENTORY.md) and [API references](API_REFERENCES.md). The tables were reviewed by the author; independent M2 review is pending.

## 1. State and assumptions

- `Approved(path,A,m)` is plan/preview approval of object A at a path with compared metadata m. It is not an open handle.
- `Validate(path,m)` reads the current path and accepts when the actual current guard predicates match the captured observations.
- `ExternalReplace(path,B,m)` models an external writer making the path name another object B while retaining the chosen compared metadata. Whether and when this succeeds with actual handles/permissions is a native obligation.
- `Mutate(path)` is a path-based read/write/recycle operation resolving the current path when its underlying API does so.
- `MutateHeld(A)` means an effect uses a still-held, correctly opened handle to A. It is a hypothetical stronger boundary for the comparison, not an operation that Diff currently performs or a general Windows copy/recycle API.

`A != B` means different filesystem objects, not merely different names. `bytes(A) != bytes(B)` is a separate optional property. Windows Diff comparison is `mW=(kind,len,modified: Option<SystemTime>)`; Unix comparison is `mU=(kind,len,modified,device,inode)`. Equal Windows tuples need not identify the same object or bytes. A Unix replacement with a different inode/device does **not** preserve all compared fields and must be rejected before use. No inode reuse or persistent-ID collision is assumed.

In every admitted copy witness below, source and preexisting destination tuples are unequal when planning runs. For example, source A has four bytes and approved destination D has nine bytes. Thus `add_file` does not skip the item as metadata-identical. The source-replacement witness may give B four different bytes with A's timestamp; the destination-replacement witness may give E nine different bytes with D's timestamp. No real timestamps or files were changed. A planned absent destination uses `None`, never a fictional approved destination object. [S1:30–65,430–479]

Keep captured root/ancestors stable and contained, use an ordinary supported file, no fatal planning errors, empty dirty snapshot, and no cancellation for the positive traces. These conditions isolate the identity boundary without bypassing a real guard. A native effect may still fail for OS reasons; the model does not force success.

## 2. Witness W1 — replacement before validation

| Step | Source substitution, Windows | Approved-overwrite destination substitution, Windows | Recycle substitution, Windows |
|---|---|---|---|
| 1: approval | `Approved(s,A,mA)`; destination D has `mD != mA`, or is absent | `Approved(d,D,mD)` plus stable source A with `mA != mD`; `overwrite=true` | `Approved(p,A,mA)` in a valid Recycle plan |
| 2: replacement | `ExternalReplace(s,B,mA)`, B differs from A | `ExternalReplace(d,E,mD)`, E differs from D | `ExternalReplace(p,B,mA)`, before Diff's item validation |
| 3: observation | `Validate(s,mA)` accepts B's equal compared tuple; other guards hold | Destination reads `Some(mD)` and equals the captured expectation; other guards hold | Root/containment/dirty/path and `FileIdentity` predicates pass for B |
| 4: path use | `fs::copy(s,d)` may read B, not approved A | `fs::copy(s,d)` may overwrite E, not approved D | `trash::delete(p)` subsequently constructs its Shell item from B's current path |
| Consequence in the abstract trace | Source approval does not imply chosen source object or bytes | Overwrite consent for captured D is not enforced as authority over only D | GUI Recycle-only and no-fallback admission do not establish approved-object binding |

This is a representation counterexample for Windows/non-Unix Diff fields. On Unix with changed device/inode, step 3 fails. Replacing a source with a hard link to the **same** object is not the different-object counterexample. A native ID read at validation could reject W1 when IDs differ, but without retention it would not address W2.

For an expected-absent destination, prevalidation arrival of a readable file yields `Some(identity) != None` and is **refused**; do not apply the overwrite W1 row to absence. Separately, `fresh` converts destination read errors with `.ok()`, so not every `None` proves absence. Neither observation establishes exclusive creation later. [S1:507–529]

## 3. Witness W2 — replacement after validation, before path use

| Step | Stable facts/transition | Current authority | Held-object comparison and its limits |
|---|---|---|---|
| 1 | `Approved(s,A,mA)`, `Approved(d,D,mD)` or `Absent(d)`; all current guards hold | Captured plan, metadata and names only | A future stronger owner would need separate source, parent and destination/absence obligations |
| 2 | `Validate(s,mA)` and, for overwrite, `Validate(d,mD)` succeed | No approved native handle retained by Diff | Opening/checking a retained A could establish source identity, subject to actual rights/share/reparse rules |
| 3a | `ExternalReplace(s,B,mB)` before `fs::copy`; mB need not equal mA | No further `fresh` call between substitution and copy | `MutateHeld(A)` would still address A **if** the held handle and API support the required read/effect; content immutability remains separate |
| 3b | Independently replace d with E after successful overwrite validation | `Mutate(d)` may resolve E | A source handle alone leaves E unprotected; an approved destination handle/guarded publication protocol is needed |
| 3c | For `Absent(d)`, create E after validation | Current `fs::copy` can overwrite E; planned outcome remains `Copied` | A true no-replace create/publication must refuse E at the effect boundary; another prior stat is insufficient |
| 3d | For recycle, replace p after item validation but **before trash's parent resolution/Shell-item construction** | Backend is given the path, not A's held handle or ID | No equivalent `MutateHeld(A)` recycle primitive is established; Shell behavior after item creation is intentionally not assumed |
| 4 | Path effect happens, if the OS permits | Effect can address B/E under the abstract assumptions despite earlier true validation | Holding roots/parents matters independently; a final path string or source-only handle is not complete effect authority |

Unlike W1, W2 does not depend on equality of metadata or identifiers after replacement. Unix identity fields improve the observation at step 2 but do not remove the interval. The trace is not a proof of Windows swap feasibility under any proposed guard; current Diff has no such retained native guard, while Rename does. [S1:507–631,804–876; A1, A3–A8, A11–A14]

## 4. Counterguards and remaining two cases

| Case | Counterexample / boundary | Source-supported result, not an executed assertion |
|---|---|---|
| C1 absent copy | A readable destination appears **before** `fresh` | `Some != None`, refuse. Destination read error mapped to None remains a separate limitation. |
| C2 overwrite/source | Source length or compared timestamp changes before `fresh`; or approved destination metadata changes | Refuse with source-changed/destination-state message. Unix changed device/inode also refuses W1. |
| C2 admitted-plan constraint | Source/destination identities equal during planning | `add_file` skips; it is invalid to claim an overwrite executes from this setup. |
| C3 recursive/creation | Selected `d` and `d/x`; or destination file where directory needed | Normalize nested selection; record conflict rather than duplicate/conflicting planned copy. Planned-directory execution still has containment/path checks, **not** all file `fresh` checks. |
| C3 changed subtree | Directory identity/metadata accepted, but an unrecorded descendant changes later | No per-descendant recycle binding is established. `create_dir_all` can leave parents on error; already-created directories/earlier copied files are not undone. |
| C4 dirty directory | Dirty snapshot contains `root/folder/dirty`, deleting `root/folder` | Dirty descendant prevents backend invocation, as the fake-backend test's assertions show. This is a snapshot, not an ongoing editor lock. |
| C4 mode/backend | Permanent passed to GUI spawn; or Recycle backend returns Err | Spawn refuses Permanent. Recycle error becomes Failed and does not call `permanently_delete`. Shared direct executor's explicit Permanent arm still exists. |
| Containment | Invalid relative path; canonical existing ancestor outside captured root; changed captured path meaning | Relevant validation refuses. No inference that check-time containment guards a later ancestor swap. |
| C5 cancellation | Flag set before next file/item, after an earlier successful effect | Stop at the next check with prior results intact; no cancellation inside current copy/Shell call, no rollback, no full list of unattempted items. |
| C5 GUI receipt | Completed report contains a Failed item or `cancelled=true` | GUI consumes matching generation and rescans; it does not show either field. A report existing is not user-visible failure feedback. |

The seven domain tests and two GUI tests in the inventory corroborate ordinary paths and several counterguards by their source assertions. They were not run and are not identity-race acceptance tests. Nothing in these tables converts their historical names into current passing evidence.

## 5. One conditional later native protocol — NOT_RUN

This protocol is available only if M2 needs stronger native evidence. It is **one future disposable synthetic fixture protocol with exactly two scheduled interleavings**, not an authorization, implementation goal, broad filesystem matrix or experiment executed here. H0 does not need it to establish the source-level limitation.

**Fixture boundary.** Use a newly created, uniquely named local Windows NTFS temporary directory owned solely by the fixture, outside all compared/user directories. Record OS build, filesystem/volume, exact executable source/toolchain/dependency versions and root native ID before mutation. Reject reparse objects, unexpected entries/IDs, unsupported API results or inability to establish exclusive fixture ownership. Prepare only synthetic files A/B (different IDs, equal Windows Diff metadata, different same-length bytes) and a distinct approved destination D. Source/destination initial metadata must admit a real plan. Keep any observation handles' access/share rights explicit; observation handles that permit delete are not protective guards. No application fixture or live Recycle Bin enumeration/emptying is permitted.

**Exactly two interleavings.** Select the load-bearing role before starting; reuse these two schedules for that role, with no extra retries or substitute native campaign:

| Schedule | Controlled pause and action | Expected evidence / refusal |
|---|---|---|
| N1: before validation | Capture/approve A (or approved destination D), then replace its fixture path with a known distinct B (or E) preserving compared Windows metadata; resume validation | Record old/new native IDs and actual compared tuples. Current path owner can accept only if all real guards permit. A proposed exact-object owner must refuse identity mismatch. For expected absence, use a new E and expect current prevalidation refusal instead of fabricating an approved object. |
| N2: after validation | Pause at the actual final validation/effect seam; attempt the same identity-changing path replacement; then resume the exact selected effect | Record whether replacement was prevented by actual retained shares/guards, or succeeded and which native object the effect used. A claimed stronger contract requires prevention, explicit refusal, or effect on the approved held object with independently satisfied destination/parent conditions. Sharing denial is evidence of a guard, not a failed reproduction to be bypassed. |

**Choose only the needed effect.** For a copy claim, observe source/destination IDs and bytes before/after, preserve absence versus overwrite intent, and record partial files and errors. Do not call native Rename a copy proof. For a recycle claim, use the exact locked trash backend and flags; N2's pause must precede Shell item construction unless a later Shell binding claim has separately specified instrumentation. A successful HRESULT is insufficient: retain abort status, actual target ID and evidence of the fixture item's reversible recycle outcome. Inability to establish or clean up the exact synthetic recycle item safely makes that guarantee **unproven**, not a reason to empty the bin or fall back to permanent deletion. No extra third interleave is authorized by this protocol.

**Handle/cleanup ownership.** A future runner owns and records every guard and observation handle and releases it deliberately after result verification. Keep A/B/D/E identities traceable through backup names inside the fixture; no untracked victim path. Cleanup first reopens/verifies root, ancestor and item identities against the fixture receipt; it must refuse any unexpected identity or descendant. Remove only recorded synthetic objects after releasing the runner's handles, using an identity-safe cleanup mechanism; any inability to provide that mechanism blocks the native protocol before it runs. No path-only recursive cleanup of an unverified tree, permanent fallback, or deletion of unrelated recycle entries. Record leftovers rather than improvising cleanup.

Native ID equality is an object observation, not proof of unchanged bytes or a durable identity across arbitrary deletion/reuse. The Windows API and selected Rename guards must be assessed on the chosen environment; no portability claim follows. The protocol's native implementation and safe cleanup are deliberately **not provided or executed** by this documentation milestone.

## 6. Model and proof budget

Executable model: **NOT_NEEDED**. The finite hand-reviewed tables already expose both logical gaps and their counterguards. Executable states explored: **0**; model invocations: **0**; model wall time: **0 seconds**; model failures/repairs: **none**. No proof script or runner was created.

The unused optional allowance remains at most 100 explicit states, one initial run plus one localized repair, at most 30 seconds/run and 60 seconds aggregate with enforced runner bounds; pure memory only, no filesystem/network/subprocess/environment/source imports. It is not transferred to native execution or renewed by M2. Current status remains sufficient static evidence, not a passed native acceptance.
