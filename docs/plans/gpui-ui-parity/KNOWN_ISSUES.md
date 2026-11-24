# Known issues and parity gaps

No new application defect was reproduced by this planning task. The following are **known migration hazards from source inspection**, not claims of current baseline failure:

| Hazard | Owner | Required disposition |
| --- | --- | --- |
| LauncherApp and event outcomes carry egui dependencies | G02 | Extract semantic contracts incrementally and preserve egui |
| Default no-op command-host methods can conceal missing UI routes | G02, G25 | Explicit command/intent coverage and actual interaction |
| Existing AlreadyRunning return can ignore an explicit backend request | G03 | Defined observable existing-instance/restart behavior |
| eframe worker-thread topology may not fit GPUI | G01 | Prove correct selected event-loop/thread ownership |
| Plugin and dashboard settings are egui callbacks outside normal dialog inventory | G06 + feature owners, G23 | Inventory/port every specialized control and child |
| Large render.rs was not completely retrieved in preparation | G00 | Inspect locally; do not infer empty source |
| Prior accepted GUI evidence can become stale after shared changes | Every owner, G25/G26 | Selective revalidation with applicability record |

During execution classify issues as confirmed baseline defect, introduced migration regression, parity gap, environment limitation or optional observation. Record evidence, affected criterion, severity, owner, next action and disposition. Do not call a failure “pre-existing” without provenance, or expand unrelated repairs into this program.

## Observed validation limitation

The Windows capture tool timed out when requesting imagery of the G01-A and G01-B GPUI windows. The same owned windows exposed UI Automation content. G01-B handled native keyboard input and retained focus and text. Preserve those receipts and keep visual evidence pending. Retry visual capture from the release candidate or another working display session. See [validation log](VALIDATION_LOG.md).

## Confirmed utility Settings data loss

The scoped G04/G06 utility write defect is resolved. Both shared writers patch the affected canonical utility field under the existing Settings lock. Twenty-two focused tests and separate native Pop Out and resize operations confirmed preservation of opaque Settings values. The broader HASH-08 lifecycle and mixed-DPI criteria remain open. [Reproduction](VALIDATION_LOG.md#g04-and-g06-utility-settings-preservation-baseline) · [Correction evidence](VALIDATION_LOG.md#g04-and-g06-utility-settings-preservation)
