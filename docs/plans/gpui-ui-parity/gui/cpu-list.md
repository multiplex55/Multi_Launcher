# CPU Usage List

G24 owns `cpu-list`, the read-only CPU Usage utility reached with a supported `info cpu list` count. The utility's geometry key is `cpu-usage`.

| Criterion | Source-backed behavior |
| --- | --- |
| CPU-LIST-01 | Sample current CPU and process use while open at the supported interval. Sort descending, limit to the requested count and preserve the per-logical-CPU display, scroll and Close. Reject malformed routes without dispatch. |

Select `search_cpu_list_returns_action` and `malformed_cpu_list_is_claimed_without_opening_or_generic_execution`. The separate process launcher results have their own `ps`, `pss` and `psk` query/action semantics. Native and GPUI remain pending.
