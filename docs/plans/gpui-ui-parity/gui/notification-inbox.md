# Notification Inbox

G23 owns `notification-inbox`. The `inbox`, `notifications` and `notification inbox` aliases open the same retained host. The bounded worker owns notification facts and receipts. G26 owns Windows packaging identity.

| Criterion | Source-backed behavior |
| --- | --- |
| INBOX-01 | Preserve current records, selection, search, filter and paging, retained host and exact close retirement. Apply validated age/count and text-retention preferences. System capture, session text and durable retention have separate controls and default off. |
| INBOX-02 | Request Windows access only with committed enabled capture, a current consent state and valid foreground UI ownership. Track asynchronous consent and release disabled listeners. Keep generated local facts separate from protected external text. |

Select `notification_inbox_both_commands_open_the_same_retained_host_without_setup`, `notification_inbox_dialog_close_retires_session`, `notification_inbox_windows_disabled_consent_never_initializes_winrt` and `notification_inbox_store_retains_typed_history_and_keeps_external_session_private`. Native and GPUI remain pending.
