# Duplicate Finder

G10 owns `duplicate-finder`. The built-in `duplicate_finder` plugin matches `dff` and emits the payload-free `dff:open` action. `src/gui/duplicate_finder_dialog.rs` owns root drafts, the retained host and projection. `src/duplicate_finder/session.rs` owns worker and cached result lifetime. The content comparison engine keeps exact root, alias and native file proofs.

| Criterion | Source-backed behavior |
| --- | --- |
| DUPLICATE-01 | Open with no implicit scan. Add or remove explicit draft roots through the inline path or owned native folder picker. Verify both picker admission and the accepted current host before applying its result. Removing a root edits the draft. Source files remain read-only. |
| DUPLICATE-02 | Explicit Start Scan uses the current roots. Show traversal, hashing, comparison, bounded diagnostics and Complete/Partial/Cancelled/Failed state. An accepted Cancel withdraws result authority and Start remains unavailable until the worker drains. A true close retires scan and cached rows. Reopen preserves the root draft. |
| DUPLICATE-03 | Render bounded groups and exact member paths, per-size and status data. Keep verified and unverified identity distinctions and report distinct files and logical duplicate bytes. Results expose member Open and Reveal and group Copy Paths under current action proof. |
| DUPLICATE-04 | Revalidate the exact selected alias, owner and current metadata before member actions. Copy Paths revalidates each verified member and preserves size bounds. Unknown identity rows can be displayed. Action availability follows current proof. |
| DUPLICATE-05 | Preserve one session through Dock/Pop Out, saved presentation and geometry, hidden-root work and pinned close ownership. Retained passes, current epoch, host and target generation gate interactions. Close or discarded rendering cannot replay an action. |

`G10-EGUI-DUPLICATE-VERIFIED-GROUP` records the actual launcher, explicit inline root and three-file read-only fixture. It exposed one verified group, two four-byte identical files and four logical duplicate bytes. A native popout opened. The controller exposed its title bar and did not expose the retained result content. Use `dff_host_gated_scan_survives_moves_cancel_drain_restart_and_close`, `dff_detached_folder_and_close_reject_discarded_root_and_child_passes`, `dff_dialog_action_proof_uses_the_exact_verified_alias_index` and the affected core content tests. GPUI and remaining native criteria are pending.
