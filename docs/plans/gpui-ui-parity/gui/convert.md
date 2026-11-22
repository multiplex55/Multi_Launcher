# Convert Panel

G16 owns `convert`. `conv` and `convert` emit `convert:panel` and open `DialogCommand::Convert`. `ConvertPanel` owns the resizable root surface and current draft. The existing unit catalog owns the physical conversion domain.

| Criterion | Source-backed behavior |
| --- | --- |
| CONVERT-01 | Preserve 17 physical categories, canonical units, independent source/target filters and favorites, Swap, up to six targets and per-target validation. Keep exact displayed unit, rounding/approximation and Copy value. |
| CONVERT-02 | Base supports signed 64-bit decimal, hexadecimal, binary and octal with the existing formatting. Preserve input text, selected category, filters and targets across in-process close and reopen. |
| CONVERT-03 | Explicit Save Preferences writes the canonical versioned favorites, format and decimal precision only. Preserve bound limits, byte-snapshot conflict, atomic write and failure feedback. Opening and editing a conversion submit no preference write. |

Select `physical_panel_calculation_matches_inline_domain_and_smart_formatting`, `base_category_keeps_its_existing_radix_conversion_behavior`, `swapping_and_multiple_targets_keep_the_input_and_respect_the_target_cap`, `explicit_save_preferences_round_trip_without_persisting_input_or_results` and `copy_effect_contains_the_displayed_target_unit_and_discarded_save_has_no_file_effect`. Native and GPUI evidence are pending.
