param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing collection'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$fixed=[ordered]@{schema='1';stage='quick_notes_refresh_characterization';owner='egui_Context_run+NotesDialog::ui';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant();telemetry_enabled='true';observer_enabled='true';warmups='5';samples='20';exclusions='0';fixture='quick_notes_note_fixture'}
$variable=@('recipe','count','screen_width_milli','screen_height_milli','viewport','settings','fixture_signature')
$timings=@('elapsed_ns','snapshot_ns','search_index_ns','metadata_build_ns','filtered_projection_ns','ensure_geometry_ns','measure_geometry_ns','owner_residual_ns')
$work=@('snapshot_calls','snapshot_successes','snapshot_errors','snapshot_note_entries','snapshot_estimated_clone_bytes','snapshot_metric_lock_wait_ns','snapshot_metric_hold_ns','search_index_build_calls','search_index_entries','search_index_utf8_bytes','metadata_build_calls','metadata_entries','metadata_output_utf8_bytes','metadata_backlink_queries','metadata_checkbox_lines_visited','metadata_preview_builds','projection_calls','projection_entries_scanned','projection_indices_materialized','projection_selected_indices','geometry_ensure_calls','geometry_rebuilds','geometry_rows_measured','geometry_identity_entries','painted_rows')
$ids=@('fixture_ids','canonical_ids','search_index_ids','metadata_ids','projection_ids','geometry_ids','viewport_ids','complete_output_ids')
$tuples=[ordered]@{revision_receipts=10;settings_receipts=4;viewport_receipts=8;width_receipts=4;capacity_before=12;capacity_after=12;geometry_overlap=8}
$required=@($fixed.Keys)+$variable+$timings+$work+$ids+@($tuples.Keys)+@('visible_original_indices')
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,5000)}
# Independent source-derived bindings, fixed before any C2-F execution.
# fixture/canonical/index/metadata/projection IDs, index bytes, metadata bytes,
# checkbox lines, projected count, requested screen width milli, final backlinks.
$bindings=@{
 'revision_final_preview:100:640000'=@('b8fd76f3cc89e86b','1ba7b1e44d23bba6','dcfece0adadf696b','e785a15928c860b5','881ad19a48b2c6ad','508814','23370','5836','100','960000','1')
 'metadata_settings_only:100:640000'=@('099cd4f72a25c1d0','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','0692ac29c78ee49b','881ad19a48b2c6ad','508775','21863','5835','100','960000','0')
 'search_sparse:100:640000'=@('619a8c90a66028dd','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','7a02a67d07aa575a','508775','23363','5835','1','960000','1')
 'search_case_only:100:640000'=@('3cb1e7a20dbd1469','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','7a02a67d07aa575a','508775','23363','5835','1','960000','1')
 'geometry_cold:100:640000'=@('16f2855552d20771','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','881ad19a48b2c6ad','508775','23363','5835','100','960000','1')
 'geometry_cold:100:180000'=@('ad988605cce34c80','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','881ad19a48b2c6ad','508775','23363','5835','100','960000','1')
 'warm_repeat:100:640000'=@('91cb78b9bb6d58b5','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','881ad19a48b2c6ad','508775','23363','5835','100','960000','1')
 'warm_repeat:100:180000'=@('9d91c3c5b3a87d54','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','881ad19a48b2c6ad','508775','23363','5835','100','960000','1')
 'early_wide_unicode:100:640000'=@('031dc3f81d42de5f','925fca895ae831d8','698c9c31e1a3173d','2cb65f51589e4c9a','881ad19a48b2c6ad','511385','26154','5833','100','960000','1')
 'width_960_to_300:100:640000'=@('2baaa27c57e5f682','cb61ebc0c9c5a9f3','cb5f4adcce0d6cd4','ce4e69419b896429','881ad19a48b2c6ad','508775','23363','5835','100','300000','1')
 'revision_final_preview:1000:640000'=@('1b29361b68652d78','0567e241f96d6496','cf6ee5314e414647','8ae23eac9f573be4','ae0a16dc2e3dd760','5205256','235031','59418','1000','960000','1')
 'metadata_settings_only:1000:640000'=@('a598e0dd3c77a69a','dc0a76a6abbeef47','ec583194ad825c2c','369f9bb69e3d4c50','ae0a16dc2e3dd760','5205217','220023','59417','1000','960000','0')
 'search_sparse:1000:640000'=@('6d20d19d4a298aed','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','7a02a67d07aa575a','5205217','235024','59417','1','960000','1')
 'search_case_only:1000:640000'=@('f08b190d40fef407','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','7a02a67d07aa575a','5205217','235024','59417','1','960000','1')
 'geometry_cold:1000:640000'=@('67cbda476b2023e5','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','ae0a16dc2e3dd760','5205217','235024','59417','1000','960000','1')
 'geometry_cold:1000:180000'=@('73252970087f9fa4','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','ae0a16dc2e3dd760','5205217','235024','59417','1000','960000','1')
 'warm_repeat:1000:640000'=@('d23571cce672acb5','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','ae0a16dc2e3dd760','5205217','235024','59417','1000','960000','1')
 'warm_repeat:1000:180000'=@('ddfbbcd8deadd154','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','ae0a16dc2e3dd760','5205217','235024','59417','1000','960000','1')
 'early_wide_unicode:1000:640000'=@('d7bc3889e31e75cb','c4167d3d314c1664','7a3f5ef53f02fc29','3dd43bc8c12415f8','ae0a16dc2e3dd760','5207827','237815','59415','1000','960000','1')
 'width_960_to_300:1000:640000'=@('0744d9ad4ee1d970','dc0a76a6abbeef47','ec583194ad825c2c','7c56504940e92173','ae0a16dc2e3dd760','5205217','235024','59417','1000','300000','1')
 'revision_final_preview:5000:640000'=@('deede70aa25961d3','f8c50b1de4f23175','17d62aac8997d906','00d0e3bf19238d95','c6891cc8d6994a50','26083470','1175126','297643','5000','960000','1')
 'metadata_settings_only:5000:640000'=@('4d9c7a321a6a0f4c','8796bffb35e72bb4','d781f640887bc695','19075ed3c0437ada','c6891cc8d6994a50','26083431','1100117','297642','5000','960000','0')
 'search_sparse:5000:640000'=@('ce7758b4f8205d3f','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','7a02a67d07aa575a','26083431','1175119','297642','1','960000','1')
 'search_case_only:5000:640000'=@('e83946727700fef1','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','7a02a67d07aa575a','26083431','1175119','297642','1','960000','1')
 'geometry_cold:5000:640000'=@('815f6081e70376ab','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','c6891cc8d6994a50','26083431','1175119','297642','5000','960000','1')
 'geometry_cold:5000:180000'=@('7d9e1a79e4122b46','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','c6891cc8d6994a50','26083431','1175119','297642','5000','960000','1')
 'warm_repeat:5000:640000'=@('eabf64e796d7705b','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','c6891cc8d6994a50','26083431','1175119','297642','5000','960000','1')
 'warm_repeat:5000:180000'=@('e76b1ac2eec1cdd6','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','c6891cc8d6994a50','26083431','1175119','297642','5000','960000','1')
 'early_wide_unicode:5000:640000'=@('cba301b9e17b71bc','92c59df82bf8d4bf','52972d3d85955954','9f6a330b1e86746c','c6891cc8d6994a50','26086041','1177910','297640','5000','960000','1')
 'width_960_to_300:5000:640000'=@('60487e1c4ca97132','8796bffb35e72bb4','d781f640887bc695','a512be9b67983eb9','c6891cc8d6994a50','26083431','1175119','297642','5000','300000','1')
}
function Read-Numbers([string]$value,[string]$name){
 if($value -notmatch '^\[\d+(?:,\d+){19}\]$'){throw "Invalid numeric array $name"}
 $result=[Collections.Generic.List[uint64]]::new()
 foreach($item in $value.Substring(1,$value.Length-2).Split(',')){[uint64]$x=0;if(-not [uint64]::TryParse($item,[ref]$x)){throw "UInt64 range $name"};$result.Add($x)}
 return ,$result.ToArray()
}
function Read-Ids([string]$value,[string]$name){
 if($value -notmatch '^\[[0-9a-f]{16}(?:,[0-9a-f]{16}){19}\]$'){throw "Invalid identities $name"}
 return ,$value.Substring(1,$value.Length-2).Split(',')
}
function Read-Tuples([string]$value,[string]$name,[int]$width,[bool]$signed){
 if($value -notmatch '^\[-?[0-9:,\-]+\]$'){throw "Invalid tuples $name"}
 $rows=$value.Substring(1,$value.Length-2).Split(',');if($rows.Count -ne 20){throw "Tuple samples $name"}
 foreach($row in $rows){$items=$row.Split(':');if($items.Count -ne $width){throw "Tuple width $name"};foreach($item in $items){
  if($signed){[int64]$x=0;if($item -notmatch '^-?\d+$' -or -not [int64]::TryParse($item,[ref]$x)){throw "Int64 tuple $name"}}
  else{[uint64]$x=0;if($item -notmatch '^\d+$' -or -not [uint64]::TryParse($item,[ref]$x)){throw "UInt64 tuple $name"}}
 }}
 return ,$rows
}
function Read-Visible([string]$value,[int]$count){
 if($value -notmatch '^\[\d+(?::\d+)*(?:;\d+(?::\d+)*){19}\]$'){throw 'Visible index syntax'}
 $rows=$value.Substring(1,$value.Length-2).Split(';')
 foreach($row in $rows){$last=-1;foreach($item in $row.Split(':')){[uint64]$x=0;if(-not [uint64]::TryParse($item,[ref]$x) -or $x -ge $count -or $x -le $last){throw 'Visible index domain/order'};$last=$x}}
 return ,$rows
}
function Float-Width([uint64]$bits){
 if($bits -gt [uint32]::MaxValue){throw 'Float bits range'}
 $value=[BitConverter]::ToSingle([BitConverter]::GetBytes([uint32]$bits),0)
 if([float]::IsNaN($value) -or [float]::IsInfinity($value) -or $value -le 0){throw 'Invalid actual width'}
 return $value
}
function Quantiles($values){$sorted=@($values|Sort-Object);return [ordered]@{p50_ns=$sorted[9];p95_ns=$sorted[18];max_ns=$sorted[19]}}
$raw=Get-Content -LiteralPath $LogPath -Raw -Encoding UTF8
if($raw -notmatch 'Summary\s+\[[^\]]+\]\s+1 test run:\s+1 passed(?:\s|,|$)' -or $raw -match '(?m)^\s*(?:FAIL\s+\[|test result: FAILED)'){throw 'Require terminal owner PASS'}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$prefix='TRACK_C2F_NOTES_RESULT '
foreach($line in ($raw -split '\r?\n')){
 $position=$line.IndexOf($prefix,[StringComparison]::Ordinal);if($position -lt 0){continue}
 $body=$line.Substring($position+$prefix.Length).Trim();$pattern='([a-z][a-z0-9_]*)=(\[[^\]]*\]|[^ ]+)'
 $fields=[Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
 foreach($match in [regex]::Matches($body,$pattern)){if(-not $fields.TryAdd($match.Groups[1].Value,$match.Groups[2].Value)){throw 'Duplicate field'}}
 if(([regex]::Replace($body,$pattern,'')).Trim().Length -ne 0){throw 'Unparsed content'}
 if($fields.Count -ne $required.Count){throw 'Field inventory'}
 foreach($name in $required){if(-not $fields.ContainsKey($name)){throw "Missing field $name"}}
 foreach($name in $fixed.Keys){if($fields[$name] -cne $fixed[$name]){throw "Metadata mismatch $name"}}
 foreach($name in @('count','screen_width_milli','screen_height_milli')){if($fields[$name] -notmatch '^[1-9]\d*$'){throw 'Invalid scalar integer'}}
 $count=[int]$fields['count'];$height=[int]$fields['screen_height_milli'];$recipe=$fields['recipe'];$key=$recipe+':'+$count+':'+$height
 if($count -notin $counts -or -not $bindings.ContainsKey($key) -or -not $seen.Add($key)){throw 'Unexpected/duplicate recipe'}
 $b=$bindings[$key];$projected=[int]$b[8]
 $viewport=if($recipe -ceq 'width_960_to_300'){'screen_300x640_transition_960x640'}elseif($height -eq 180000){'screen_960x180'}else{'screen_960x640'}
 $settings=if($recipe -ceq 'metadata_settings_only'){'backlinks_off_tasks_on'}else{'backlinks_on_tasks_on'}
 if($fields['fixture_signature'] -cne $b[0] -or $fields['screen_width_milli'] -cne $b[9] -or $fields['viewport'] -cne $viewport -or $fields['settings'] -cne $settings){throw 'Fixture/scenario binding'}
 $r=[ordered]@{};foreach($name in (@($fixed.Keys)+$variable)){$r[$name]=$fields[$name]};$r['count']=$count;$r['screen_height_milli']=$height;$r['screen_width_milli']=[int]$b[9]
 foreach($name in ($timings+$work)){$r[$name]=Read-Numbers $fields[$name] $name}
 foreach($name in $ids){$r[$name]=Read-Ids $fields[$name] $name}
 foreach($name in $tuples.Keys){$r[$name]=Read-Tuples $fields[$name] $name $tuples[$name] ($name -ceq 'viewport_receipts')}
 $r['visible_original_indices']=Read-Visible $fields['visible_original_indices'] $count
 $changed=$recipe -in @('revision_final_preview','early_wide_unicode');$metadata=$changed -or $recipe -ceq 'metadata_settings_only';$scan=$changed -or $recipe -ceq 'search_sparse';$cold=$recipe -notin @('warm_repeat','search_case_only')
 for($i=0;$i -lt 20;$i++){
  [uint64]$sum=0;foreach($name in @('snapshot_ns','search_index_ns','metadata_build_ns','filtered_projection_ns','ensure_geometry_ns')){$sum+=$r[$name][$i]}
  if($r['elapsed_ns'][$i] -eq 0 -or $sum -gt $r['elapsed_ns'][$i] -or $r['owner_residual_ns'][$i] -ne $r['elapsed_ns'][$i]-$sum -or $r['measure_geometry_ns'][$i] -gt $r['ensure_geometry_ns'][$i]){throw 'Timer bounds/residual'}
  $expected=@{snapshot_calls=[int]$changed;snapshot_successes=[int]$changed;snapshot_errors=0;snapshot_note_entries=($count*[int]$changed);search_index_build_calls=[int]$changed;search_index_entries=($count*[int]$changed);search_index_utf8_bytes=([uint64]$b[5]*[int]$changed);metadata_build_calls=[int]$metadata;metadata_entries=($count*[int]$metadata);metadata_output_utf8_bytes=([uint64]$b[6]*[int]$metadata);metadata_backlink_queries=($count*[int]$metadata*[int]$b[10]);metadata_checkbox_lines_visited=([uint64]$b[7]*[int]$metadata);metadata_preview_builds=($count*[int]$metadata);projection_calls=2;projection_entries_scanned=($count*[int]$scan);projection_indices_materialized=($projected*[int]$scan);projection_selected_indices=$projected;geometry_ensure_calls=1;geometry_rebuilds=[int]$cold;geometry_rows_measured=($projected*[int]$cold);geometry_identity_entries=($projected*[int]$cold)}
  foreach($name in $expected.Keys){if($r[$name][$i] -ne $expected[$name]){throw "Actual work binding $name"}}
  if($changed){if($r['snapshot_estimated_clone_bytes'][$i] -eq 0 -or $r['snapshot_metric_hold_ns'][$i] -eq 0){throw 'Missing snapshot metric'}}
  else{foreach($name in @('snapshot_estimated_clone_bytes','snapshot_metric_lock_wait_ns','snapshot_metric_hold_ns','snapshot_ns','search_index_ns')){if($r[$name][$i] -ne 0){throw 'Unexpected snapshot/index work/time'}}}
  if(-not $metadata -and $r['metadata_build_ns'][$i] -ne 0){throw 'Unexpected metadata timer'}
  if($r['snapshot_metric_lock_wait_ns'][$i] -gt $r['snapshot_ns'][$i] -or $r['snapshot_metric_hold_ns'][$i] -gt $r['snapshot_ns'][$i]-$r['snapshot_metric_lock_wait_ns'][$i]){throw 'Nested snapshot metric bounds'}
  if(-not $cold -and $r['measure_geometry_ns'][$i] -ne 0){throw 'Unexpected warm geometry measurement'}
  for($domain=0;$domain -lt 5;$domain++){if($r[$ids[$domain]][$i] -cne $b[$domain]){throw 'Independent semantic identity binding'}}
  foreach($name in $ids){if($r[$name][$i] -ceq '0000000000000000'){throw 'Zero identity'}}
  [uint64[]]$rev=@($r['revision_receipts'][$i].Split(':')|ForEach-Object {[uint64]$_})
  if($rev[1] -ne $rev[2] -or $rev[2] -ne $rev[3] -or $rev[4] -ne $rev[7] -or $rev[5] -ne $rev[8] -or $rev[6] -ne $rev[9]){throw 'Revision/generation receipt'}
  if($changed){if($rev[0] -eq $rev[1]){throw 'Missing canonical revision transition'}}elseif($rev[0] -ne $rev[1]){throw 'Unexpected canonical revision transition'}
  if($r['settings_receipts'][$i] -cne ('1:'+ $b[10] +':1:1')){throw 'Metadata settings receipt'}
  [int64[]]$vp=@($r['viewport_receipts'][$i].Split(':')|ForEach-Object {[int64]$_});[uint64[]]$width=@($r['width_receipts'][$i].Split(':')|ForEach-Object {[uint64]$_})
  $null=Float-Width $width[0];$null=Float-Width $width[1]
  if($width[1] -ne $vp[0] -or $width[3] -ne $vp[1] -or $vp[1] -le 0 -or $vp[2] -le 0 -or $vp[3] -lt 0 -or $vp[4] -lt 0 -or $vp[5] -lt 0 -or $vp[6] -lt 0){throw 'Actual viewport/width receipt'}
  if($recipe -ceq 'width_960_to_300'){if($width[1] -eq $width[0] -or $width[3] -ge $width[2]){throw 'Missing actual width transition'}}
  elseif($recipe -in @('warm_repeat','search_case_only','geometry_cold')){if($width[0] -ne $width[1] -or $width[2] -ne $width[3]){throw 'Unchanged control width'}}
  $visible=$r['visible_original_indices'][$i].Split(':');if($visible.Count -ne $r['painted_rows'][$i] -or $vp[7] -ne $visible.Count -or $visible.Count -lt 1 -or $visible.Count -gt $projected){throw 'Painted viewport count'}
  if($projected -eq 1 -and $r['visible_original_indices'][$i] -cne '50'){throw 'Sparse original index'}
  [uint64[]]$before=@($r['capacity_before'][$i].Split(':')|ForEach-Object {[uint64]$_});[uint64[]]$after=@($r['capacity_after'][$i].Split(':')|ForEach-Object {[uint64]$_});[uint64[]]$overlap=@($r['geometry_overlap'][$i].Split(':')|ForEach-Object {[uint64]$_})
  if($after[0] -lt 216*$count -or $after[3] -lt 24*$count -or $after[4] -lt [uint64]$b[5] -or $after[5] -lt 80*$count -or $after[6] -lt [uint64]$b[6] -or $after[7] -lt 8*$projected -or $after[8] -lt 80*$projected -or $after[11] -ne $projected){throw 'Partial capacity bounds'}
  foreach($endpoint in @($before,$after)){foreach($component in @(1,2,9,10)){if($endpoint[$component] -eq 0){throw 'Missing nonempty owned payload capacity'}}}
  if(-not $cold){if($r['capacity_before'][$i] -cne $r['capacity_after'][$i] -or @($overlap|Where-Object {$_ -ne 0}).Count){throw 'Warm endpoint/overlap receipt'}}
  else{for($component=0;$component -lt 4;$component++){
   $old=if($recipe -ceq 'geometry_cold'){0}else{$before[8+$component]}
   if($overlap[$component] -ne $old -or $overlap[4+$component] -ne $after[8+$component]){throw 'Actual geometry replacement overlap'}
  }}
 }
 $stats=[ordered]@{};foreach($name in ($timings+@('snapshot_metric_lock_wait_ns','snapshot_metric_hold_ns'))){$stats[$name]=Quantiles $r[$name]};$r['statistics']=$stats;$records.Add($r)
}
$expected=@($bindings.Keys|Where-Object {[int]($_.Split(':')[1]) -in $counts})
if($records.Count -ne $expected.Count){throw 'Incomplete recipe inventory'};foreach($key in $expected){if(-not $seen.Contains($key)){throw 'Missing recipe'}}
$output=[ordered]@{schema=1;status='C2_F_COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$false;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';storage_scope='partial_NotesDialog_endpoint_capacities_and_actual_geometry_pair_only_excludes_map_buckets_control_blocks_galleys_other_derived_overlap_harness_allocator_RSS';records=$records.ToArray()}
$output|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output ('Collected '+$records.Count+' C2F Quick Notes recipes; independent review pending')
