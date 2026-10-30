# G07 baseline

2026-10-30: fetched master/origin/master4a1283e6edd2c245f562a07dbe3dcc6a61061eed unchanged. One clean worktree, no active Git operation. codex/closeout-g07-zip-extraction-20261029-r01 from bootstrap02d0ac64d51655f43d6e958ea551f5bf658ee48d. No unaccepted G02 source imported; accepted cumulative integration92c7076a separate. Revalidate current archive catalog/model/plan and native existing primitives; no historical patch import.

## M1-A tested donor state — UNACCEPTED

HEAD remained `3f6408d672fc975279aa9cc75654af6baf456363`; no implementer Git mutation. Final run used these exact Git blobs, also captured in [run receipt](roles/implementer/verification/run-native-isolation.receipt.json):

| File | Git blob |
|---|---|
| `src/archive/mod.rs` | `473747815bafa25633a53654282463beb0734618` |
| `src/archive/preparation.rs` (new) | `170bea7c95ac9ff9c51c8c939c5f8e5875f70038` |
| `src/file_operations.rs` | `7c5112cb72494ab4e241e1e4fb96a75a9ddc8f2e` |
| unchanged `src/archive/model.rs` | `dac974cf472245266d7796cdc93f1b59ebfc3c97` |
| unchanged `src/archive/catalog.rs` | `9981d9ccadb53c31c189dca10c33dfae877a6139` |
| unchanged `src/archive/plan.rs` | `9a166aa3ebc2fe8dcf854fbd674b63fca2b3e60c` |
| unchanged `Cargo.toml` | `994a55fea039ca79614fbb019ac1b05cfab7381b` |
| unchanged `Cargo.lock` | `648e143ad2ca902e04ae9b7d4cf57899a2db8380` |

Owner inspection corroborated catalog physical IDs, the pure planner and existing native helpers. New source is limited to a private preparation module and archive-specific adapter within the existing native owner. `open_native_object` keeps prior defaults; its archive path additionally requests directory-list access and denies write sharing. Replacement stages keep their original security rights/sharing; archive stages use minimal rights and exclusive sharing. Existing FFI implements creation, handle deletion and no-replace rename. No copied FFI, dependency, parser framework, normal publication caller or UI change. The native failures make **all source nonintegrable**, despite passing read-only controls. Parent/planner/orchestrator records remain separate from these implementer edits.
Parent preservation checkpointe4e74ec8675c070e165849477fd33d9f490b63d2 committed and normally pushed ALLUNACCEPTED three-file prototype plus exact receipts. Original3f6408d6 is the preimplementation packet HEAD, not accepted source. No G07 source imported; acceptedintegration sourceb9f5fa4b unchanged.
