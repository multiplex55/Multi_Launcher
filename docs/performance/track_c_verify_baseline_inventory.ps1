param([Parameter(Mandatory=$true)][string]$CollectionPath)
$ErrorActionPreference='Stop'
$data=Get-Content -LiteralPath $CollectionPath -Raw | ConvertFrom-Json
if($data.source -ne '071243db068835617a054a74fb5734a6dd9f128d' -or $data.profile -ne 'fast-dev' -or $data.mode -ne 'Full' -or $data.status -ne 'COLLECTED_PENDING_REVIEW'){throw 'Wrong collection provenance/state'}
$records=@($data.records)
if($records.Count -ne 105){throw 'Expected exactly105 Full scenario records'}
$families=@(
 @{owner='NotePanel::maybe_refresh_heavy_derived';fixture='synthetic-note-and-todo-relations';sizes=@(100,1000,5000);per_size=1},
 @{owner='LauncherApp::search_read_only_outcome_with_plugin_snapshot';fixture='sparse-exact-alias-tie';sizes=@(100,1000,10000);per_size=1},
 @{owner='LauncherApp::search_read_only_outcome_with_plugin_snapshot';fixture='dense-exact-empty-term';sizes=@(100,1000,10000);per_size=1},
 @{owner='LauncherApp::search_read_only_outcome_with_plugin_snapshot';fixture='dense-fuzzy-usage-weighted';sizes=@(100,1000,10000);per_size=1},
 @{owner='LauncherApp::search_read_only_outcome_with_plugin_snapshot';fixture='empty-query-browse';sizes=@(100,1000,10000);per_size=1},
 @{owner='LauncherApp::search_read_only_outcome_with_plugin_snapshot';fixture='sparse-exact-pending-provider';sizes=@(100,1000,10000);per_size=1},
 @{owner='egui_Context_run+LauncherApp::render_root_frame';fixture='root-list-default-layout';sizes=@(100,1000,10000);per_size=2},
 @{owner='egui_Context_run+LauncherApp::render_root_frame';fixture='root_grid_action_fixture';sizes=@(100,1000,10000);per_size=10},
 @{owner='egui_Context_run+NotesDialog::ui';fixture='quick_notes_note_fixture';sizes=@(100,1000,5000);per_size=14},
 @{owner='query_event+egui_Context_run+LauncherApp::render_root_frame';fixture='root_query_action_fixture';sizes=@(100,1000,10000);per_size=1},
 @{owner='process_watch_events_actions_reload';fixture='custom_prefix_retained_indexed_tail';sizes=@(20000);per_size=1},
 @{owner='process_watch_events_finite_burst';fixture='actions_original_stale_indexready_recycle';sizes=@(20000);per_size=1},
 @{owner='process_watch_events_concurrent_offered_load';fixture='actions_recycle_finite_producer';sizes=@(20000);per_size=1},
 @{owner='startup_action_catalog';fixture='owned_synthetic_index_tree';sizes=@(16,1000,10000);per_size=1}
)
$covered=0
foreach($family in $families){
 $group=@($records | Where-Object {$_.owner -eq $family.owner -and $_.fixture -eq $family.fixture})
 if($group.Count -ne $family.sizes.Count*$family.per_size){throw "Wrong family count $($family.fixture)"}
 foreach($size in $family.sizes){if(@($group | Where-Object {$_.item_count -eq $size}).Count -ne $family.per_size){throw "Missing/duplicate size $($family.fixture):$size"}}
 $covered+=$group.Count
}
if($covered -ne 105){throw 'Unrecognized owner family'}
function Assert-ScenarioSet($fixture,$sizes,$expected) {
 foreach($size in $sizes){
  $actual=@($records|Where-Object {$_.fixture -eq $fixture -and $_.item_count -eq $size}|ForEach-Object {@($_.settings,$_.viewport,$_.cold_type)-join '|'})
  if($actual.Count -ne $expected.Count -or @($actual|Select-Object -Unique).Count -ne $expected.Count){throw "Duplicate/missing scenario $fixture at $size"}
  foreach($key in $expected){if($actual -notcontains $key){throw "Missing exact scenario $fixture at ${size}:$key"}}
 }
}
$listExpected=@('list-wrap-default|960x640|geometry-cold-warm-fonts','list-wrap-default|960x640|geometry-warm-warm-fonts')
Assert-ScenarioSet 'root-list-default-layout' @(100,1000,10000) $listExpected
$gridExpected=@(foreach($cols in @(1,2,3,5,6)){foreach($cold in @('geometry_cold_warm_fonts','geometry_warm_warm_fonts')){"grid_cols_${cols}_default_style|960x640|$cold"}})
Assert-ScenarioSet 'root_grid_action_fixture' @(100,1000,10000) $gridExpected
$notesExpected=@(foreach($filter in @('empty','sparse','long_unicode')){foreach($height in @(640,180)){foreach($cold in @('geometry_cold_warm_fonts','geometry_warm_warm_fonts')){"notes_${filter}_backlinks_on_tasks_on|screen_960x${height}|$cold"}}})
$notesExpected+=@('notes_empty_backlinks_on_tasks_on|screen_300x640_transition_960x640|geometry_cold_warm_fonts','notes_empty_backlinks_on_tasks_on|screen_300x640_transition_960x640|geometry_warm_warm_fonts')
Assert-ScenarioSet 'quick_notes_note_fixture' @(100,1000,5000) $notesExpected
$geometry=@($records|Where-Object {$_.owner -in @('egui_Context_run+LauncherApp::render_root_frame','egui_Context_run+NotesDialog::ui')})
foreach($pair in ($geometry|Group-Object owner,fixture,fixture_signature,item_count,viewport,settings)){
 if($pair.Count -ne 2 -or @($pair.Group|Where-Object {$_.cold_type -in @('geometry_cold_warm_fonts','geometry-cold-warm-fonts')}).Count -ne 1 -or @($pair.Group|Where-Object {$_.cold_type -in @('geometry_warm_warm_fonts','geometry-warm-warm-fonts')}).Count -ne 1){throw 'Incomplete geometry cold/warm pair'}
 if(($pair.Group[0].structural_output_ids -join ',') -ne ($pair.Group[1].structural_output_ids -join ',') -or ($pair.Group[0].viewport_receipts -join ',') -ne ($pair.Group[1].viewport_receipts -join ',')){throw 'Cold/warm structural or viewport mismatch'}
}
$width=@($records|Where-Object {$_.PSObject.Properties.Name -contains 'width_receipt'})
if($width.Count -ne 6){throw 'Missing six horizontal width receipts'}
foreach($size in @(100,1000,5000)){if(@($width|Where-Object {$_.item_count -eq $size}).Count -ne 2){throw 'Missing size width pair'}}
$queues=@($records|Where-Object {$_.owner -in @('process_watch_events_finite_burst','process_watch_events_concurrent_offered_load')})
$ageRows=0;$ages=0;$queueSummary=@()
foreach($queue in $queues){
 $ageRows+=@($queue.event_age_samples).Count
 $values=@($queue.event_age_samples|ForEach-Object {$_.age_ns}|ForEach-Object {[uint64]$_}|Sort-Object)
 $expectedCount=160;if($queue.owner -eq 'process_watch_events_concurrent_offered_load'){$expectedCount=280}
 if($values.Count -ne $expectedCount){throw 'Wrong queue delivered age count'}
 $ages+=$values.Count
 $p50=[int][Math]::Ceiling(0.5*$values.Count)-1;$p95=[int][Math]::Ceiling(0.95*$values.Count)-1
 $queueSummary+= [ordered]@{owner=$queue.owner;delivered_ages=$values.Count;p50_ns=$values[$p50];p95_ns=$values[$p95];max_ns=$values[-1]}
}
if($ageRows -ne 40 -or $ages -ne 440){throw 'Wrong age inventory'}
[ordered]@{status='INVENTORY_VALIDATED_PENDING_REVIEW';source=$data.source;scenario_records=$records.Count;owner_samples=2100;geometry_pairs=($geometry.Count/2);width_receipts=$width.Count;event_age_rows=$ageRows;delivered_ages=$ages;queue_aggregate=$queueSummary}|ConvertTo-Json -Depth 6
