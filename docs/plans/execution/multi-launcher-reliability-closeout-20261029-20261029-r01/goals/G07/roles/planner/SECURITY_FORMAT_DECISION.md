# G07 security and format decision

Date: 2026-10-30. State: DESIGN_READY, SOURCE_OBSERVED; native behavior NOT_RUN. Parent-supplied G07 tip `eaf8aac2876fdc6efb363b4a444529c3a450e285`, independent from bootstrap `02d0ac64d51655f43d6e958ea551f5bf658ee48d`; actual master `4a1283e6edd2c245f562a07dbe3dcc6a61061eed` unchanged; authoritative integration `92c7076a`. Parent owns Git and the operational ledger.

## Decision and scope

Proceed with one M1-A source/catalog/local-header and native reservation checkpoint before a payload writer. The initial execution subset is flat, ordinary files in a ZIP32 archive, method Stored, no data descriptors, no overwrite, into an already existing admitted local directory. This is a deliberately narrower execution contract than the retained read-only catalog. No archive command, extraction button or production streaming/publication entry point is enabled by M1-A.

The safe writer requirement gates UI, not development of the required safety foundation. No G02 source is imported. Prior G02/G05 failures and GUI automation limitations establish no G07 filesystem result.

## Current owner map

| Current owner | Source-observed fact | Consequence |
|---|---|---|
| `src/archive/catalog.rs::read_catalog` | Reads bounded end records and central metadata through caller-owned `Read + Seek`; deliberately never reads local headers/payloads. | Retain it; catalog success is not extraction authorization. |
| `src/archive/model.rs::{ArchiveRevision, ArchiveEntryId, ArchiveSource, ArchiveEntry}` | Revision is caller-provided; ID is revision plus physical central ordinal. Entries retain raw name, flags, CRC, compressed/expanded sizes, central/local offsets and external attributes. | An owned source/session must bind these values to the same handle. Path or timestamp alone cannot create that binding. |
| `src/archive/model.rs::{SafeRelativePath, ValidatedRelativePath}` | Validates Windows lexical names; both slash forms separate components; refuses traversal, roots, ADS, device aliases, trailing dot/space and bounded path overflow. | Reuse; do not sanitize unsafe names into acceptable output names. |
| `src/archive/plan.rs::build_extraction_plan` | Pure selection/occupancy plan; preserves physical duplicate IDs, rejects normalized collisions and file ancestors; captures `no_overwrite = true` and actual output limits. | Build from the session's own catalog; repeat destination checks at reservation/effect. Vacant preview is not a reservation. |
| `src/archive/mod.rs`, `src/lib.rs` | Library exports catalog and plan only. Whole-source symbol search found no external archive catalog/plan callers; command/GUI search found no archive extraction owner. | Keep library foundation isolated; no legacy extraction route to migrate. Later UI placement is not part of this packet. |
| `src/file_operations.rs::{NativeRenameScanBackend, RootAdmission, RootProof, OpenDirectoryChain, DirectoryOpenAccess}` | Native admission/reopen checks local roots, full ancestor identities, canonical parent chain and reparse status. `open_native_object` retains no-delete-sharing guards but permits write sharing. | Reuse the established owner. The existing write-sharing mode is not by itself proof against in-place reparse conversion. |
| `src/file_operations.rs::open_native_readonly_content_file` | Read-only no-follow file handle, `FILE_SHARE_READ`, native identity/canonical path; currently private and not returned beyond its read boundary. | Add a narrow retained archive read owner; do not use the DFF lease, whose sharing deliberately permits mutation. |
| `src/file_operations.rs::{create_native_replace_stage, rename_open_file_no_replace, delete_open_file}` | CREATE_NEW staging, rename of held file with ReplaceIfExists false, handle-bound deletion. Rename uses absolute canonical parent plus validated leaf while parent handles remain alive, not relative RootDirectory operation. Stage opener also requests replacement-specific security rights. | Reuse primitives behind a narrow archive adapter. Prove containment and rights on the actual host; preserve unrelated callers/defaults. |
| `src/common/atomic_file.rs::save_atomic` | Whole-byte-buffer state replacement; path-based create/replace and best-effort cleanup. | Not an archive output primitive. No changes to it. |

Relevant locations at this baseline: catalog entry fields around lines 168–383; plan policy around 790; read-only native opener 8515; stage opener 8105; no-replace rename 8718; directory opener 9020; existing native fixture pattern `file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture` at 10961. Line numbers are navigation aids, not stable APIs.

## Initial execution support and refusal contract

Catalog/listing behavior and its defaults remain unchanged. New preparation uses a separate, captured execution profile and rejects unsupported selections with typed reasons.

- ZIP32 only, Stored method 0 only; selected entries must be ordinary files with one validated path component. Do not flatten nested paths. Directory/subtree creation is unsupported in this subset.
- Selected central and local flags may contain only the UTF-8 flag (bit 11). ASCII without that flag remains valid. Invalid UTF-8, non-ASCII legacy encoding, encryption, Deflate, unknown methods/flags and descriptor bit 3 refuse before any stage is created.
- Selected local and central extra fields must be empty in this initial subset. ZIP64, ZIP64 sentinel values, unsupported extraction-version requirements and conflicting local/central fields refuse. Read central-only fields that the current model does not retain from that physical central record on the same held source; do not enlarge public model fields solely to avoid this bounded check. Initial recognized extraction versions are 1.0 and 2.0 (10 and 20).
- Permit creator hosts already interpreted as DOS/NTFS/Unix by this bounded decision: host 0, 10, 3 or 19. For Unix, only absent file-type bits or regular-file type are accepted. Existing unsafe-kind checks remain mandatory. Reject directory, symlink, reparse, special-file and unknown-host interpretations. Do not restore ACLs, ownership, executable bits, alternate streams, timestamps or archive attributes; output is new ordinary file data.
- Preserve raw local/central filename equality. Never look an entry up by name, apply a lossy name, resolve a duplicate by last-name-wins, or reconstruct an ID from the destination text.
- Reuse existing Windows ordinal collision checks. Same-path source/output, existing output of any kind, output alias of the source, unsafe leaf, occupied stage name, and unprovable parent/source identity fail closed. No rename-over, replace-existing, target deletion, overwrite-consent shortcut or automatic suffixing of final filenames.
- Local supported filesystem roots only; reject UNC/device roots, reparse/cloud-placeholder source or ancestors, unsupported file identity and directory-chain changes. Retain the same admitted source through catalog, plan, local validation and later streaming. No pathname reopen handoff.

The first captured execution ceilings are 512 MiB source archive, 128 selected files, 64 MiB per declared/actual file and 256 MiB declared/actual total output. Use the minimum of these ceilings and tighter caller/catalog limits; never raise a captured limit. Keep existing bounded catalog metadata/path/work budgets. Stored requires compressed size equal expanded size; execution expansion ratio is at most 1. Streaming later uses a fixed buffer no larger than 64 KiB. There is no decompressor/window allocation in the supported subset. Deflate requires its own accepted decoder/window/input-consumption proof before support can change.

M1-A proves byte ranges and reservations, not payload CRC. M1-B must enforce actual bytes, exact input EOF/range, output size, CRC and cancellation before any publication. No CRC promise is inferred from the current fixture builder: `catalog::tests::make_zip` writes zero CRC fields because its tests intentionally do not validate payloads.

## Ownership and native proof contract

Archive owns interpretation, revision, selection, resource accounting and terminal receipts. `file_operations` owns Windows handles, ancestor guards, CREATE_NEW/no-clobber and handle cleanup. A narrow archive-specific guard/access policy is authorized there; do not copy its FFI into archive, globally change sharing/access defaults, or route archives through Hot Folder actions/journals or search/replace whole-buffer staging.

Retain all ancestors through every relevant open/create/rename/delete. If archive containment needs stronger directory sharing than the current read/write-sharing mode, add it only to the archive admission/reopen path. Likewise permit a narrow stage-rights option if the existing WRITE_DAC/WRITE_OWNER request is unnecessary for new ordinary archive data. Preserve existing behavior and prove the new choice; access failures are errors, not permission to fall back to path-only writes.

The native probe must show that a competing source writer/deleter and ancestor replacement/reparse conversion cannot redirect an admitted operation. A prior reparse fixture must be refused before effects. A fresh ordinary fixture must also succeed: blanket refusal is not native acceptance. If the platform cannot construct the required adversarial fixture, record that exact proof as unrun; do not replace it with a mock or an assumed PASS.

Output reservation means held destination identity plus an exclusively created, identity-owned staging file, with captured final leaf/policy. It does not promise the final name stays vacant: an unrelated creator may win before publication. The later no-replace effect must then fail without touching that winner. Never create an empty final-name placeholder and describe it as extracted success.

Cleanup targets only the held created file. Explicit abort returns cleanup/retained-output evidence; cancellation cannot suppress cleanup. RAII fallback is useful but cannot erase a cleanup error or serve as the sole success receipt. A temporary file still visible because of another handle is not yet removed. No recursive cleanup based on an archive-supplied pathname. In M1-A there is no batch publication/crash-recovery claim. Before later writer/UI acceptance, recovery/partial-publication receipts must identify created outputs and never claim atomic batch rollback.

Current `ScanCancellation` is cooperative; synchronous read return and worker retirement must precede terminal cancellation. This contract does not promise in-flight kernel-I/O interruption or a hard I/O deadline. Later asynchronous UI ownership must retain its worker and handles until terminal cleanup, never detach them or publish after cancellation.

## Primary references and proof limits

- [Microsoft CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew): sharing conflicts are checked against open handles and persist until close; CREATE_NEW refuses an existing name. This supports the proposed handle contract, not a host test result.
- [Microsoft FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info): ReplaceIfExists false reports an existing target as an error. Absolute versus directory-relative addressing matters; current code uses the former with retained ancestors.
- [Microsoft reparse operations](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-operations) and [FSCTL_SET_REPARSE_POINT](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_set_reparse_point): in-place reparse changes are a distinct operation to account for, not equivalent to directory rename. Native refusal/protection remains to be demonstrated.
- [PKWARE APPNOTE 6.3.10](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT), sections 4.3.7–4.3.12: local records, payload ranges, descriptor and central records are separate structures. The selected subset refuses descriptors/extra formats rather than treating central metadata as proof of local bytes.

High confidence in current owner and reachability facts; native success, exact source holding and no-redirection guarantees are NOT_RUN. Counterevidence to premature deferral: all basic required native operations already exist in the accepted source, and a repository-owned native filesystem test pattern exists. Counterevidence to premature acceptance: current ancestor sharing permits writes; private stage creation requests extra security rights; no current archive-native owner/probe or payload integrity check exists.

No distinct additional initiative is recommended: NO_ACTION. These proof obligations and intentionally unsupported formats are active G07 scope. If the bounded M1-A proof fails, the discovering implementation/review role must preserve the actual counterexample and author the required complete continuation pair; this design does not invent a blocker before that attempt.
