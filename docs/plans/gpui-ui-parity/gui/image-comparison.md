# Image Comparison

G18 owns `image-comparison`, runtime navigation `image-compare`. Exact `imgdiff` emits `imgdiff:open` and opens the typed dialog. `ImageCompareDialog` owns host, mode and camera. The existing compare session, decoder, budget and pixel engine own input snapshots and results.

| Criterion | Source-backed behavior |
| --- | --- |
| IMAGE-DIFF-01 | Open A/B uses the exact current owned HWND and file/input revisions. Explicit Paste A/B uses supported Windows DIB and bounded validation. Passive open/render makes no clipboard request. Keep the labeled last accepted pair during pending or failed replacement. |
| IMAGE-DIFF-02 | A/B, Wipe including endpoints, and Difference use aligned top-left source coordinates and exact RGBA including hidden channels. Preserve contrast legend, zoom/pan and categorical/numerical results for same, changed, one-side and neither extents. Scope explicit text copy to retained owner. |
| IMAGE-DIFF-03 | Preserve encoded/decode and combined session budgets, accepted pair/worker/upload identity, actual paint/resource retirement and retained camera across Dock/Pop Out. True close retires effects and releases current resources. |

Select `imgdiff_actual_file_and_paste_responses_reach_the_shared_domain`, `imgdiff_host_moves_retain_inflight_pair_worker_and_camera_with_root_hidden`, `imgdiff_detached_source_actions_and_close_reject_discarded_root_and_child_passes`, `imgdiff_numeric_copy_waits_for_root_and_native_owner_retention`, `image_compare_session_failed_replacement_keeps_last_good_pair_and_result`, `image_compare_domain_compares_every_raw_rgba_channel_including_invisible_rgb`, `imgdiff_retired_upload_keeps_budget_until_cpu_and_actual_paint_acks` and `imgdiff_wipe_clips_are_complementary_and_endpoints_show_one_source`. `G18-EGUI-CROP-IMAGE-PICKER` also verified actual compare opening, A picker cancellation and native popout chrome. Owned filename entry, pair evaluation and GPUI remain pending.
