param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][ValidateSet('eager','deferred')][string]$ExpectedMaterialization,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing output'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$deferred=$ExpectedMaterialization -ceq 'deferred'
$expectedRecordBytes=if($deferred){24}else{104}
$fixed=[ordered]@{
 schema='1';stage='search_materialization';materialization=$ExpectedMaterialization;owner='search_read_only_outcome_with_plugin_snapshot'
 fixture='synthetic_action_catalog_captured_provider';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant()
 telemetry_enabled='true';observer_enabled='true';warmups='5';samples='20';exclusions='0'
 provider_scope='captured_snapshot_no_provider_invocation'
 materialization_scope='catalog_hit_copies_once_staged_provider_owned_moves_browse_separate'
 empty_browse_scope='command_cache_and_catalog_actions_materialized_provider_input_discarded'
 payload_scope='logical_utf8_and_string_values_not_allocator_or_rss'
 outcome_scope='noresults_0_results_1_pending_2'
}
$variable=@('case','count','fixture_signature','provider_count','rank_record_bytes')
$timings=@('elapsed_ns','sort_ns')
$work=@('local_catalog_visits','local_filter_passes','local_score_attempts','local_hits','local_action_clones','local_clone_logical_bytes','local_clone_strings','provider_input_actions','provider_filter_rejections','provider_actions_retained_moved','provider_projection_strings','provider_projection_bytes','command_fallback_actions_visited','command_fallback_hits_cloned','command_fallback_clone_logical_bytes','command_fallback_clone_strings','sort_input_len','final_scored_entries_moved','final_local_action_clones','empty_browse_command_cache_cloned','empty_browse_catalog_materialized','empty_browse_logical_bytes','empty_browse_strings','final_output_count','outcome_state')
$work+=@('final_local_clone_logical_bytes','final_local_clone_strings','hit_action_clone_total','hit_clone_logical_bytes_total','hit_clone_strings_total')
$identities=@('complete_output_ids','structural_output_ids')
$arrays=$timings+$work+$identities;$required=@($fixed.Keys)+$variable+$arrays
$cases=@('sparse-exact-alias-tie','dense-exact-empty-term','dense-fuzzy-usage-weighted','empty-query-browse','sparse-exact-pending-provider')
# Frozen C0 signatures, in the case order above. Never regenerate from candidate output.
$fixtureIds=@{
 '100'=@('29fdcc8d4ba5e3f0','91b473398cf22409','7ee9c49442ecfaf3','66c45cf13a34a906','aa2665ffc623e4f4')
 '1000'=@('6db9e8a64838bb48','886970aaa9a689a4','4259e326fb923849','4eb0dfae7f49eb3a','c65cdea9d22254ec')
 '10000'=@('fe09032883dd710f','b3cb627767174d56','5ee43cc5cbf7e560','ef8d695268b6e9d7','e71d2e7f4b7a5023')
}
# Independently reconstructed UTF-8 lengths from frozen action_fixture, outside timing.
# Columns: full catalog copied text, last-three sparse text, app-prefixed browse text.
$payloadBytes=@{'100'=@(13496,290,13896);'1000'=@(136657,276,140657);'10000'=@(1372759,276,1412759)}
function UInt([string]$s){if($s -cnotmatch '^[0-9]+$'){throw 'Unsigned decimal required'};[uint64]::Parse($s,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$a,[double]$p){$sorted=@($a|Sort-Object);$sorted[[int][Math]::Ceiling($p*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $prefix='C2B_SEARCH_RESULT ';$start=$line.IndexOf($prefix+'schema=');if($start -lt 0){continue};$f=@{}
 foreach($token in $line.Substring($start+$prefix.Length).Split(' ')){
  if(-not $token){continue};$pair=$token.Split(@('='),2)
  if($pair.Length -ne 2 -or $f.ContainsKey($pair[0]) -or $pair[0] -cnotin $required){throw 'Malformed/unknown/duplicate field'}
  $f[$pair[0]]=$pair[1]
 }
 foreach($k in $required){if(-not $f.ContainsKey($k)){throw "Missing $k"}}
 foreach($k in $fixed.Keys){if($f[$k] -cne $fixed[$k]){throw "Protocol mismatch $k"}}
 if($f.fixture_signature -cnotmatch '^[0-9a-f]{16}$'){throw 'Invalid fixture signature'}
 $heavy=$f.case -ceq 'plugin-heavy-dense'
 if(-not $heavy -and $f.case -cnotin $cases){throw 'Invalid case'}
 $n=UInt $f.count;$key=$f.case+':'+$n;if(-not $seen.Add($key)){throw 'Duplicate scenario'}
 if($heavy){
  if($n -ne 1000 -or $f.fixture_signature -cne 'd5a1cd254a205e24'){throw 'Heavy fixture mismatch'}
 }elseif(-not $fixtureIds.ContainsKey([string]$n) -or $f.fixture_signature -cne $fixtureIds[[string]$n][[array]::IndexOf($cases,$f.case)]){throw 'Frozen fixture mismatch'}
 $providers=if($heavy){128}else{1}
 if((UInt $f.provider_count) -ne $providers -or (UInt $f.rank_record_bytes) -ne $expectedRecordBytes){throw 'Provider count or materialization rank record size mismatch'}
 $r=[ordered]@{};foreach($k in @($fixed.Keys)+$variable){$r[$k]=$f[$k]};$r['count']=$n
 foreach($k in $arrays){
  if($f[$k] -cnotmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'}
  $items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Expected 20 samples'}
  if($k -in $identities){
   foreach($item in $items){if($item -cnotmatch '^[0-9a-f]{16}$'){throw 'Invalid identity'}}
   $r[$k]=@($items)
  }else{$r[$k]=@($items|ForEach-Object {UInt $_})}
 }
 $empty=$f.case -eq 'empty-query-browse'
 $hits=if($empty){0}elseif($f.case -in @('sparse-exact-alias-tie','sparse-exact-pending-provider')){3}else{$n}
 $visits=if($empty){0}else{$n}
 $attempts=if($empty -or $heavy -or $f.case -eq 'dense-exact-empty-term'){0}else{$n}
 $merged=if($empty){0}else{$providers}
 $out=if($empty){$n}else{$hits+$providers}
 $outcome=if($f.case -eq 'sparse-exact-pending-provider'){2}else{1}
 $allStrings=3*$n+[int][Math]::Floor(($n+4)/8)
 $hitStrings=if($hits -eq $n){$allStrings}elseif($hits -eq 3){if($n -eq 100){10}else{9}}else{0}
 $hitBytes=if($hits -eq $n){$payloadBytes[[string]$n][0]}elseif($hits -eq 3){$payloadBytes[[string]$n][1]}else{0}
 $expectedWork=@{
  local_catalog_visits=$visits;local_filter_passes=$visits;local_score_attempts=$attempts
  local_hits=$hits;local_action_clones=$(if($deferred){0}else{$hits});provider_input_actions=$providers;provider_filter_rejections=0
  provider_actions_retained_moved=$merged;sort_input_len=($hits+$merged);final_scored_entries_moved=$(if($deferred){$merged}else{$hits+$merged})
  final_local_action_clones=$(if($deferred){$hits}else{0});final_local_clone_logical_bytes=$(if($deferred){$hitBytes}else{0});final_local_clone_strings=$(if($deferred){$hitStrings}else{0});hit_action_clone_total=$hits;hit_clone_logical_bytes_total=$hitBytes;hit_clone_strings_total=$hitStrings;empty_browse_command_cache_cloned=0
  empty_browse_catalog_materialized=$(if($empty){$n}else{0});final_output_count=$out;outcome_state=$outcome
  command_fallback_actions_visited=0;command_fallback_hits_cloned=0;command_fallback_clone_logical_bytes=0;command_fallback_clone_strings=0
 }
 for($i=0;$i -lt 20;$i++){
  if($r.elapsed_ns[$i] -eq 0 -or $r.sort_ns[$i] -gt $r.elapsed_ns[$i]){throw 'Invalid owner/sort timing'}
  foreach($k in $expectedWork.Keys){if($r[$k][$i] -ne $expectedWork[$k]){throw "Work mismatch $key $k"}}
  if($empty){
   foreach($k in @('sort_ns','local_clone_logical_bytes','local_clone_strings')){if($r[$k][$i] -ne 0){throw 'Empty browse performed scored work'}}
   if($r.empty_browse_logical_bytes[$i] -ne $payloadBytes[[string]$n][2] -or $r.empty_browse_strings[$i] -ne $allStrings){throw 'Missing empty browse materialization'}
  }else{
   foreach($k in @('empty_browse_logical_bytes','empty_browse_strings')){if($r[$k][$i] -ne 0){throw 'Nonempty performed browse work'}}
   if($r.local_clone_logical_bytes[$i] -ne $(if($deferred){0}else{$hitBytes}) -or $r.local_clone_strings[$i] -ne $(if($deferred){0}else{$hitStrings})){throw 'Missing local clone payload'}
  }
  $projectionStrings=if($heavy){384}else{3}
  $projectionBytes=if($heavy){14976}else{79}
  if($r.provider_projection_strings[$i] -ne $projectionStrings -or $r.provider_projection_bytes[$i] -ne $projectionBytes){throw 'Provider projection mismatch'}
 }
 foreach($k in $work+$identities){if(@($r[$k]|Select-Object -Unique).Count -ne 1){throw "Unstable controlled receipt $k"}}
 $stats=[ordered]@{};foreach($k in $timings){$a=$r[$k];$stats[$k]=[ordered]@{p50_ns=(Rank $a 0.5);p95_ns=(Rank $a 0.95);max_ns=(@($a|Sort-Object)[-1])}}
 $r.statistics=$stats;$records.Add($r)
}
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,10000)}
$expected=@(foreach($n in $counts){foreach($case in $cases){"${case}:$n"}})
$expected+=@('plugin-heavy-dense:1000')
if($records.Count -ne $expected.Count){throw 'Incomplete inventory'}
foreach($key in $expected){if(-not $seen.Contains($key)){throw "Missing $key"}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$deferred;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C2B records; review pending"
