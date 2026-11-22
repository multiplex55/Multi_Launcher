# XML Save Overwrite

G14 owns `xml-toolkit-overwrite`, mapped to runtime `xml-overwrite-confirmation`. The [XML Toolkit](xml-toolkit.md) host presents Confirm XML file overwrite after the exact Save request resolves to an existing target. Preserve the initiating source and owner, path and frozen payload.

| Criterion | Source-backed behavior |
| --- | --- |
| XML-OVERWRITE-01 | Display the exact target and keep conflicting parent actions gated. Replace existing file passes only the reviewed payload to the current save worker. Cancel Save, Escape and title close clear the decision without writing. |
| XML-OVERWRITE-02 | Keep the reviewed path and payload identity through accepted host change, revalidate local target policy, and preserve no-clobber initial save and explicit atomic replacement. Current review approves the path and payload. |

Select `m7f_xml_file_size_and_atomic_save_confirmation_are_enforced`, `host_move_keeps_session_and_frozen_overwrite_identity`, `m7f_xml_overwrite_confirmation_controls_remain_reachable_in_small_viewport` and `xml_toolkit_host_moves_keep_the_session_and_discarded_replace_keeps_the_file`. Native and GPUI evidence are pending.
