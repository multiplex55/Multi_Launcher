# G02 acceptance matrix — B01 close

| Required gate | Observation | Acceptance |
|---|---|---|
| Exact selected compilation/inventory | corrected list exit0;19nonignored | BUILD_PASSED only |
| Ten existing controls + three resource tests |13/13 passed | scoped deterministic protection |
| Two generic native-named refusal tests |2/2 passed; can refuse early | no downstream native claim |
| Four mandatory publication/native cases |0/4, OS5 at C:\Users\Jay before CREATE_NEW | FAILED_VERIFICATION |
| Successful actual native publication |0 observed | NATIVE_PROOF_DEFERRED |
| M1-B cancellation/cleanup/recovery/session lifecycle |NOT_RUN, requires accepted M1-A | DEFERRED_DEPENDENCY |
| Entire G02 contract |no accepted candidate source | NOT ACCEPTED; G03 blocked |
| Preservation and chain source safety |five candidate paths preserved; production source restored to80455ffa | PASS, docs-only closure |

See [verification](VERIFICATION.md), [preservation](PRESERVATION_RECEIPT.json) and [rollback/cleanup](ROLLBACK_AND_CLEANUP.json). Independent docs-only disposition review ACCEPTED; source remains UNACCEPTED. No feature enablement.
