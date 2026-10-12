param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refusing existing output'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full evidence requires exact committed source'}
$scalar=@('schema','stage','owner','source','profile','mode','telemetry_enabled','observer_enabled','fixture','fixture_signature','transition','count','todo_count','viewport','settings','cold_type','warmups','samples','exclusions','frame_timer','attributed_owner','frame_p50_ns','frame_p95_ns','frame_max_ns')
$identities=@('complete_output_state_ids','structural_output_ids')
$numbers=@('frame_elapsed_ns','note_revisions','todo_revisions','work_units','refresh_calls','refresh_successes','failed_snapshots','snapshot_calls','snapshot_successes','todo_load_calls','label_map_visits','fence_calls','fence_input_bytes','fence_output_bytes','refresh_ns','snapshot_ns','todo_load_ns','label_map_build_ns','derived_install_ns','snapshot_metric_calls','snapshot_metric_estimated_clone_bytes','snapshot_metric_elapsed_ns','snapshot_metric_lock_wait_ns','snapshot_metric_completed','snapshot_metric_errors','snapshot_metric_abandoned')
$numbers+=@('fused_calls','fused_todo_visits','fused_note_visits','fused_ns')
foreach($class in @('needles','fenced','snippet')){foreach($suffix in @('count','len_bytes','capacity_bytes')){$numbers+="string_${class}_$suffix"}}
$required=$scalar+$identities+$numbers+@('work_counters')
function Array-Body([string]$value){if(-not $value.StartsWith('[') -or -not $value.EndsWith(']')){throw 'Malformed array'};return $value.Substring(1,$value.Length-2)}
function UInt-Value([string]$value){if($value -notmatch '^[0-9]+$'){throw 'Unsigned decimal required'};return [uint64]::Parse($value,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$values,[double]$fraction){$sorted=@($values|Sort-Object);return $sorted[[int][Math]::Ceiling($fraction*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $start=$line.IndexOf('C1B_NOTE_RESULT schema=');if($start -lt 0){continue}
 $fields=@{};foreach($token in $line.Substring($start+'C1B_NOTE_RESULT '.Length).Split(' ')){
  if($token.Length -eq 0){continue};$pair=$token.Split(@('='),2)
  if($pair.Length -ne 2 -or $fields.ContainsKey($pair[0]) -or $pair[0] -notin $required){throw 'Duplicate/unknown/malformed field'};$fields[$pair[0]]=$pair[1]
 }
 foreach($key in $required){if(-not $fields.ContainsKey($key)){throw "Missing field $key"}}
 foreach($pair in @(@('schema','1'),@('stage','fused'),@('owner','egui_Context_run+NotePanel_ui'),@('source',$ExpectedSource),@('profile','fast-dev'),@('mode',$ExpectedMode),@('telemetry_enabled','true'),@('observer_enabled','true'),@('fixture','synthetic_note_panel_refresh'),@('todo_count','12'),@('viewport','960x640'),@('settings','backlinks_enabled_inert'),@('warmups','5'),@('samples','20'),@('exclusions','0'),@('frame_timer','Context_run+NotePanel_ui'),@('attributed_owner','NotePanel_refresh_heavy_derived'))){if($fields[$pair[0]] -cne $pair[1]){throw "Protocol mismatch $($pair[0])"}}
 if($fields.fixture_signature -notmatch '^[0-9a-f]{16}$'){throw 'Invalid fixture identity'}
 $count=UInt-Value $fields.count;$transition=$fields.transition;$cold=@{'local-draft-debounce'='debounce-due-refresh';'external-note-revision'='external-note-revision-bypass';'persisted-todo-revision'='todo-revision-bypass'}
 if(-not $cold.ContainsKey($transition) -or $fields.cold_type -cne $cold[$transition]){throw 'Unknown transition/cold pair'}
 if(-not $seen.Add("$transition|$count")){throw 'Duplicate scenario'}
 $record=[ordered]@{};foreach($key in $scalar){$record[$key]=$fields[$key]};$record.Remove('count');$record.item_count=$count;foreach($numericKey in @('schema','todo_count','warmups','samples','exclusions','frame_p50_ns','frame_p95_ns','frame_max_ns')){$record[$numericKey]=UInt-Value $fields[$numericKey]}
 foreach($key in $identities){$array=@((Array-Body $fields[$key]).Split(','));if($array.Count -ne 20 -or @($array|Where-Object {$_ -notmatch '^[0-9a-f]{16}$'}).Count){throw 'Invalid identity array'};$record[$key]=$array}
 foreach($key in $numbers){$array=@((Array-Body $fields[$key]).Split(',')|ForEach-Object {UInt-Value $_});if($array.Count -ne 20){throw 'Wrong numeric sample count'};$record[$key]=$array}
 $work=@((Array-Body $fields.work_counters).Split(';')|ForEach-Object {$row=@($_.Split(',')|ForEach-Object {UInt-Value $_});if($row.Count -ne 6){throw 'Wrong work counter width'};,$row});if($work.Count -ne 20){throw 'Wrong work counter count'};$record.work_counters=$work
 $frame=$record.frame_elapsed_ns
 if((UInt-Value $fields.frame_p50_ns) -ne (Rank $frame .5) -or (UInt-Value $fields.frame_p95_ns) -ne (Rank $frame .95) -or (UInt-Value $fields.frame_max_ns) -ne (@($frame|Sort-Object)[-1])){throw 'Frame quantile mismatch'}
 for($sample=0;$sample -lt 20;$sample++){
  foreach($key in @('refresh_calls','refresh_successes','snapshot_calls','snapshot_successes','todo_load_calls','snapshot_metric_calls')){if($record[$key][$sample] -ne 1){throw "Owner did not execute once: $key"}}
  foreach($key in @('failed_snapshots','snapshot_metric_completed','snapshot_metric_errors','snapshot_metric_abandoned')){if($record[$key][$sample] -ne 0){throw 'Failed/abandoned owner'}}
  if($record.label_map_visits[$sample] -ne 12 -or $record.fence_calls[$sample] -ne $count -or $record.work_units[$sample] -ne ($count+12)){throw 'Actual work accounting mismatch'}
  if(($work[$sample] -join ',') -ne "1,1,1,1,12,$(($count+12))"){throw 'Work receipt mismatch'}
  if($record.fused_calls[$sample] -ne 1 -or $record.fused_note_visits[$sample] -ne $count -or $record.fused_todo_visits[$sample] -ne 12){throw 'Fused visit mismatch'};$passTotal=$record.fused_ns[$sample]
  if($record.refresh_ns[$sample] -gt $frame[$sample] -or $passTotal -gt $record.refresh_ns[$sample]){throw 'Timer containment mismatch'}
  foreach($key in @('snapshot_ns','todo_load_ns','label_map_build_ns','derived_install_ns')){if($record[$key][$sample] -gt $record.refresh_ns[$sample]){throw 'Child timer containment mismatch'}}
  # Existing NoteSnapshot lock wait and hold are disjoint intervals inside the wrapper.
  # Subtract only after bounding hold, avoiding unsigned overflow on corrupt input.
  if($record.snapshot_metric_elapsed_ns[$sample] -gt $record.snapshot_ns[$sample] -or $record.snapshot_metric_lock_wait_ns[$sample] -gt ($record.snapshot_ns[$sample]-$record.snapshot_metric_elapsed_ns[$sample])){throw 'Snapshot metric containment mismatch'}
  foreach($class in @('needles','fenced','snippet')){if($record["string_${class}_capacity_bytes"][$sample] -lt $record["string_${class}_len_bytes"][$sample]){throw 'Construction capacity smaller than length'}}
  if($record.fence_calls[$sample] -ne $record.string_fenced_count[$sample] -or $record.fence_output_bytes[$sample] -ne $record.string_fenced_len_bytes[$sample]){throw 'Fence construction receipt mismatch'}
 }
 $stats=[ordered]@{};foreach($key in @($numbers|Where-Object {$_ -match '_ns$'})){$values=$record[$key];$stats[$key]=[ordered]@{p50_ns=(Rank $values .5);p95_ns=(Rank $values .95);max_ns=@($values|Sort-Object)[-1]}};$record.timing_quantiles=$stats
 $records.Add($record)
}
$expected=@('local-draft-debounce|100','external-note-revision|100','persisted-todo-revision|100')
if($ExpectedMode -eq 'Full'){$expected=@('local-draft-debounce|100','local-draft-debounce|1000','local-draft-debounce|5000','external-note-revision|1000','persisted-todo-revision|1000')}
if($records.Count -ne $expected.Count){throw 'Incomplete scenario inventory'};foreach($identity in $expected){if(-not $seen.Contains($identity)){throw 'Missing required scenario'}}
[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;observer_enabled=$true;native_display='NOT MEASURED';records=$records.ToArray()}|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $OutputPath -Encoding UTF8
Write-Output "Collected $($records.Count) C1-B scenarios; independent evidence review required."
