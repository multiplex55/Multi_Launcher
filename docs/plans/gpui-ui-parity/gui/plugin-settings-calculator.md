# Calculator Plugin Settings

G15 owns `plugin-settings-calculator` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| CALCULATOR-SETTINGS-01 | Preserve the disabled-by-default save-on-enter checkbox, immediate provider behavior and durable profile save. Preserve history semantics for scalar calculations, including deferred recording when enabled. Calculation Pad routing remains independent. |

Select `calc_pad_is_an_exact_opener_and_inline_calculator_behavior_is_preserved` and `missing_initializes_but_malformed_rejects_mutations_unchanged`. Use a synthetic history store.

Native and GPUI remain pending.
