param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing output'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$fixed=[ordered]@{
 schema='1';stage='id_position_projection';fixture='custom_first_indexed_tail';source=$ExpectedSource
 profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant();telemetry_enabled='true';observer_enabled='true'
 warmups='5';samples='20';exclusions='0';scope='owner_includes_sync_publication';overlap='nested_durations_not_subtracted'
 outcome_scope='typed_load_0_not_called_1_valid_2_malformed_3_missing'
 payload_scope='precomputed_logical_utf8_not_allocator_or_rss';index_tail_scope='cloned_iterator_consumption'
}
$variable=@('owner','fixture_signature','output_signature','case','count','custom_count','indexed_count')
$timings=@('actions_event_ns','typed_load_ns','tail_copy_ns','catalog_assembly_arc_ns','search_entries_ns','filter_metadata_ns','id_map_ns','completion_invalidation_ns','same_query_refresh_ns','search_total_ns','index_ready_owner_ns','index_request_ns','scanner_wait_ns','index_custom_prefix_copy_ns','index_tail_clone_ns')
$calls=@('typed_load_calls','tail_copy_calls','catalog_assembly_arc_calls','search_entries_calls','filter_metadata_calls','id_map_calls','completion_invalidation_calls','same_query_refresh_calls')
$work=@('typed_load_outcome','typed_actions_loaded','tail_actions_cloned','index_custom_actions_cloned','index_tail_actions_cloned','catalog_assembly_actions','search_entries_processed','filter_entries_processed','id_actions_processed','cache_entries','filter_entries','id_entries','query_results','completion_keys','suggestions','actions_version_delta','arc_retained','tail_selected_clone_bytes','index_custom_selected_clone_bytes','index_tail_selected_clone_bytes','id_key_selected_clone_bytes')
$work+=@('id_full_actions_cloned','id_keys_cloned')
$arrays=$timings+$calls+$work;$required=@($fixed.Keys)+$variable+$arrays
$cases=@('custom_changed','custom_equal','custom_malformed','custom_missing','index_accepted_changed')
function UInt([string]$s){if($s -notmatch '^[0-9]+$'){throw 'Unsigned decimal required'};[uint64]::Parse($s,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$a,[double]$p){$sorted=@($a|Sort-Object);$sorted[[int][Math]::Ceiling($p*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $prefix='C1F_ACTIONS_RESULT ';$start=$line.IndexOf($prefix+'schema=');if($start -lt 0){continue};$f=@{}
 foreach($token in $line.Substring($start+$prefix.Length).Split(' ')){
  if(-not $token){continue};$pair=$token.Split(@('='),2)
  if($pair.Length -ne 2 -or $f.ContainsKey($pair[0]) -or $pair[0] -notin $required){throw 'Malformed/unknown/duplicate field'}
  $f[$pair[0]]=$pair[1]
 }
 foreach($k in $required){if(-not $f.ContainsKey($k)){throw "Missing $k"}}
 foreach($k in $fixed.Keys){if($f[$k] -cne $fixed[$k]){throw "Protocol mismatch $k"}}
 foreach($k in @('fixture_signature','output_signature')){if($f[$k] -notmatch '^[0-9a-f]{16}$'){throw 'Invalid hash'}}
 if($f.case -notin $cases){throw 'Invalid case'}
 $isIndex=$f.case -eq 'index_accepted_changed';$publishes=$isIndex -or $f.case -eq 'custom_changed'
 $expectedOwner=if($isIndex){'process_index_ready_accepted'}else{'process_watch_events_actions'}
 if($f.owner -cne $expectedOwner){throw 'Wrong owner'}
 $n=UInt $f.count;$custom=UInt $f.custom_count;$indexed=UInt $f.indexed_count
 if($custom -eq 0 -or $indexed -eq 0 -or ([decimal]$custom+[decimal]$indexed) -ne $n){throw 'Invalid catalog partition'}
 $key=$f.case+':'+$n;if(-not $seen.Add($key)){throw 'Duplicate scenario'}
 $r=[ordered]@{};foreach($k in @($fixed.Keys)+$variable){$r[$k]=$f[$k]};$r['count']=$n;$r['custom_count']=$custom;$r['indexed_count']=$indexed
 foreach($k in $arrays){
  if($f[$k] -notmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'}
  $items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Expected20samples'}
  $r[$k]=@($items|ForEach-Object {UInt $_})
 }
 for($i=0;$i -lt 20;$i++){
  $owner=if($isIndex){$r.index_ready_owner_ns[$i]}else{$r.actions_event_ns[$i]}
  if($owner -eq 0){throw 'Missing owner duration'}
  foreach($k in $timings | Where-Object {$_ -notin @('actions_event_ns','index_ready_owner_ns','index_request_ns','scanner_wait_ns')}){
   if($r[$k][$i] -gt $owner){throw 'Phase exceeds owner'}
  }
  if($r.search_total_ns[$i] -gt $r.same_query_refresh_ns[$i] -or $r.index_tail_clone_ns[$i] -gt $r.catalog_assembly_arc_ns[$i]){throw 'Invalid nested phase bound'}
  $expectedLoad=if($isIndex){0}else{1};if($r.typed_load_calls[$i] -ne $expectedLoad){throw 'Typed load count mismatch'}
  $outcome=switch($f.case){'custom_changed'{1};'custom_equal'{1};'custom_malformed'{2};'custom_missing'{3};'index_accepted_changed'{0}}
  if($r.typed_load_outcome[$i] -ne $outcome){throw 'Typed load outcome mismatch'}
  $loaded=if($f.case -in @('custom_changed','custom_equal')){$custom}else{0}
  if($r.typed_actions_loaded[$i] -ne $loaded){throw 'Typed load work mismatch'}
  $publishCount=if($publishes){1}else{0}
  if($r.id_full_actions_cloned[$i] -ne 0 -or $r.id_keys_cloned[$i] -ne ($publishCount*$n)){throw 'ID projection clone receipt mismatch'}
  foreach($k in $calls | Where-Object {$_ -notin @('typed_load_calls','tail_copy_calls')}){if($r[$k][$i] -ne $publishCount){throw 'Publication phase count mismatch'}}
  foreach($k in @('catalog_assembly_actions','search_entries_processed','filter_entries_processed','id_actions_processed')){if($r[$k][$i] -ne ($publishCount*$n)){throw 'Construction work mismatch'}}
  if($r.actions_version_delta[$i] -ne $publishCount -or $r.arc_retained[$i] -ne (1-$publishCount)){throw 'Publication identity mismatch'}
  if($r.cache_entries[$i] -ne $n -or $r.filter_entries[$i] -ne $n -or $r.id_entries[$i] -ne ($n-1) -or $r.query_results[$i] -eq 0 -or $r.query_results[$i] -gt $n -or $r.completion_keys[$i] -eq 0 -or $r.completion_keys[$i] -gt $n -or $r.suggestions[$i] -gt 5){throw 'Invalid retained consumer cardinality'}
  if($isIndex){
   foreach($k in @('actions_event_ns','typed_load_ns','tail_copy_ns','tail_copy_calls','tail_actions_cloned','tail_selected_clone_bytes')){if($r[$k][$i] -ne 0){throw 'Index case performed custom reload work'}}
   if($r.index_custom_actions_cloned[$i] -ne $custom -or $r.index_tail_actions_cloned[$i] -ne $indexed -or $r.index_custom_selected_clone_bytes[$i] -eq 0 -or $r.index_tail_selected_clone_bytes[$i] -eq 0){throw 'Index clone receipt mismatch'}
  }else{
   foreach($k in @('index_ready_owner_ns','index_request_ns','scanner_wait_ns','index_custom_prefix_copy_ns','index_tail_clone_ns','index_custom_actions_cloned','index_tail_actions_cloned','index_custom_selected_clone_bytes','index_tail_selected_clone_bytes')){if($r[$k][$i] -ne 0){throw 'Custom case performed index work'}}
   if($f.case -eq 'custom_changed'){
    if($r.tail_copy_calls[$i] -ne 1 -or $r.tail_actions_cloned[$i] -ne $indexed -or $r.tail_selected_clone_bytes[$i] -eq 0){throw 'Retained tail receipt mismatch'}
   }else{
    foreach($k in @('tail_copy_ns','tail_copy_calls','tail_actions_cloned','tail_selected_clone_bytes','catalog_assembly_arc_ns','search_entries_ns','filter_metadata_ns','id_map_ns','completion_invalidation_ns','same_query_refresh_ns','search_total_ns','id_key_selected_clone_bytes')){if($r[$k][$i] -ne 0){throw 'No-op or failure performed publication work'}}
   }
  }
  if($publishes -and $r.id_key_selected_clone_bytes[$i] -lt $r.id_keys_cloned[$i]){throw 'Missing ID-map clone estimate'}
  $exclusive=@('typed_load_ns','tail_copy_ns','index_custom_prefix_copy_ns','catalog_assembly_arc_ns','search_entries_ns','filter_metadata_ns','id_map_ns','completion_invalidation_ns','same_query_refresh_ns')
  $sum=[decimal]0;foreach($k in $exclusive){$sum+=[decimal]$r[$k][$i]};if($sum -gt $owner){throw 'Exclusive phase sum exceeds owner'}
 }
 foreach($k in $calls+$work){if(@($r[$k]|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled work receipt'}}
 $stats=[ordered]@{};foreach($k in $timings){$a=$r[$k];$stats[$k]=[ordered]@{p50_ns=(Rank $a 0.5);p95_ns=(Rank $a 0.95);max_ns=(@($a|Sort-Object)[-1])}}
 $r.statistics=$stats;$records.Add($r)
}
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,10000,20000)}
$expected=@(foreach($n in $counts){foreach($case in $cases){"${case}:$n"}})
if($records.Count -ne $expected.Count){throw 'Incomplete inventory'}
foreach($key in $expected){if(-not $seen.Contains($key)){throw "Missing $key"}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$true;candidate_status='PENDING_ACCEPTANCE';deferred_completion_rebuild='OUTSIDE_SERIES';native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText((Join-Path (Get-Location) $OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C1F records; review pending"
