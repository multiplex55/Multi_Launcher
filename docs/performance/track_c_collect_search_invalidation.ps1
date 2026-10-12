param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing collection'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$fixed=[ordered]@{
 schema='1';stage='forced_refresh_invalidation';installation='eager_reinstall'
 owner='LauncherApp::search+egui_Context_run+LauncherApp::render_root_frame'
 fixture='synthetic_action_catalog_controlled_provider';query='app_empty_term_exact'
 viewport='960x640';fonts='warm_egui_font_atlas';settings='exact_app_prefix_default_list'
 source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant();telemetry_enabled='true'
 warmups='5';samples='20';exclusions='0';notification_scope='inside_owner_before_frame'
 mutation_scope='setup_outside_owner';geometry_scope='existing_test_observation_no_equality_observer'
}
$numeric=@('elapsed_ns','result_count','provider_calls','provider_generation_delta','result_generation_delta','geometry_rebuilds','geometry_measured_rows','geometry_measured_cells','geometry_ns','painted_rows','search_valid','search_pending','provider_deferral','catalog_versions_stable','selected_index','suggestion_count')
$tuples=@('catalog_receipts','provider_revision_receipts')
$identities=@('complete_output_state_ids','action_output_ids','viewport_receipts')
$arrays=$numeric+$tuples+$identities
$variable=@('case','count','fixture_signature')
$required=@($fixed.Keys)+$variable+$arrays
$cases=@('cached_repeat','equal_provider_notification','changed_provider_notification')
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,10000)}
# Independently reconstructed in pure JS BigInt and Python, with frozen C0
# catalog identity control 9a958b83b7e7345f. Never derive from candidate output.
$fixtureIds=@{
 'cached_repeat:100'='83aca5477013e236';'equal_provider_notification:100'='3f27cdd737d29326';'changed_provider_notification:100'='b14c8ea753c6391a'
 'cached_repeat:1000'='dd5e4f19ca871066';'equal_provider_notification:1000'='fd1fa896de805376';'changed_provider_notification:1000'='58af94f66ee7856a'
 'cached_repeat:10000'='04d143fbbcd5d7b9';'equal_provider_notification:10000'='44fdc1e0fbefb7a9';'changed_provider_notification:10000'='9c75a4f1bef638f5'
}
$actionIds=@{
 '100'=@('91f78f1a00909136','e18555614ad4cc6f')
 '1000'=@('acc4b6c39213538b','b7c875f3a5f1bd48')
 '10000'=@('e9e08ad263119091','a1e34d1a09cd8152')
}
$stateIds=@{
 '100'=@('4a2bbb6caf1da39d','3a5b1d6fffd935bc')
 '1000'=@('16f8e44ff5d893fe','dead72363b7692e0')
 '10000'=@('790ee81b13705eee','abd4525a5dde5766')
}
function UInt([string]$value){if($value -cnotmatch '^(?:0|[1-9][0-9]*)$'){throw 'Invalid unsigned integer'};[uint64]::Parse($value,[Globalization.CultureInfo]::InvariantCulture)}
function Stats($values){$sorted=@($values|Sort-Object);[ordered]@{p50_ns=$sorted[9];p95_ns=$sorted[18];max_ns=$sorted[19]}}
$records=[Collections.Generic.List[object]]::new()
$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 if($line -notmatch 'C2C_REFRESH_RESULT '){continue}
 $body=$line.Substring($line.IndexOf('C2C_REFRESH_RESULT ')+19).Trim()
 $fields=@{}
 foreach($token in $body.Split(' ',[StringSplitOptions]::RemoveEmptyEntries)){
  if($token -cnotmatch '^([a-z_]+)=(\S+)$'){throw 'Malformed field'}
  if($fields.ContainsKey($Matches[1])){throw 'Duplicate field'}
  $fields[$Matches[1]]=$Matches[2]
 }
 if(@($fields.Keys).Count -ne $required.Count){throw 'Wrong field inventory'}
 foreach($key in $required){if(-not $fields.ContainsKey($key)){throw "Missing $key"}}
 foreach($key in $fixed.Keys){if($fields[$key] -cne $fixed[$key]){throw "Protocol mismatch $key"}}
 $n=UInt $fields.count;$case=$fields.case;$id="${case}:$n"
 if($n -notin $counts -or $case -cnotin $cases -or -not $seen.Add($id)){throw 'Unexpected or duplicate case'}
 if($fields.fixture_signature -cne $fixtureIds[$id]){throw 'Frozen fixture signature mismatch'}
 $record=[ordered]@{};foreach($key in @($fixed.Keys)+$variable){$record[$key]=$fields[$key]};$record.count=$n
 foreach($key in $arrays){
  if($fields[$key] -cnotmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'}
  $items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Wrong sample count'}
  if($key -cin $numeric){$record[$key]=@($items|ForEach-Object {UInt $_})}
  elseif($key -cin $identities){if(@($items|Where-Object {$_ -cnotmatch '^[0-9a-f]{16}$'}).Count){throw 'Malformed identity'};$record[$key]=$items}
  else{
   $arity=if($key -ceq 'catalog_receipts'){7}else{4}
   foreach($item in $items){$parts=$item.Split(':');if($parts.Count -ne $arity){throw 'Wrong receipt arity'};foreach($part in $parts){$null=UInt $part}}
   $record[$key]=$items
  }
 }
 $notified=$case -cne 'cached_repeat';$delta=if($notified){1}else{0};$measuredRows=if($notified){$n+1}else{0}
 for($i=0;$i -lt 20;$i++){
  $expected=@{result_count=($n+1);provider_calls=$delta;provider_generation_delta=$delta;result_generation_delta=$delta;geometry_rebuilds=$delta;geometry_measured_rows=$measuredRows;geometry_measured_cells=0;search_valid=1;search_pending=0;provider_deferral=0;catalog_versions_stable=1;selected_index=[uint64]::MaxValue;suggestion_count=5}
  foreach($key in $expected.Keys){if($record[$key][$i] -ne $expected[$key]){throw "Work/state mismatch $key"}}
  if($record.elapsed_ns[$i] -eq 0 -or $record.geometry_ns[$i] -gt $record.elapsed_ns[$i]){throw 'Invalid owner/geometry interval'}
  if(($notified -and $record.geometry_ns[$i] -eq 0) -or (-not $notified -and $record.geometry_ns[$i] -ne 0)){throw 'Stale or missing geometry timing'}
  if($record.painted_rows[$i] -eq 0 -or $record.painted_rows[$i] -gt ($n+1)){throw 'Invalid painted row count'}
  $actionId=$actionIds[[string]$n][$(if($case -ceq 'changed_provider_notification'){1}else{0})]
  if($record.action_output_ids[$i] -cne $actionId){throw 'Complete Action output identity mismatch'}
  $stateId=$stateIds[[string]$n][$(if($case -ceq 'changed_provider_notification'){1}else{0})]
  if($record.complete_output_state_ids[$i] -cne $stateId){throw 'Complete semantic output/state identity mismatch'}
  $provider=@($record.provider_revision_receipts[$i].Split(':')|ForEach-Object {UInt $_})
  if($provider[1] -ne ($provider[0]+$delta) -or $provider[2] -ne $provider[1] -or $provider[3] -ne $provider[1]){throw 'Stale provider receipt'}
  $catalog=@($record.catalog_receipts[$i].Split(':')|ForEach-Object {UInt $_})
  if($catalog[0] -ne 1 -or $catalog[1] -ne $catalog[4] -or $catalog[2] -ne $catalog[5] -or $catalog[3] -ne $catalog[6]){throw 'Stale catalog receipt'}
 }
 foreach($key in $identities){if(@($record[$key]|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled output identity'}}
 $record.statistics=[ordered]@{elapsed_ns=(Stats $record.elapsed_ns);geometry_ns=(Stats $record.geometry_ns)}
 $records.Add($record)
}
if($records.Count -ne ($counts.Count*3)){throw 'Incomplete inventory'}
foreach($n in $counts){foreach($case in $cases){if(-not $seen.Contains("${case}:$n")){throw 'Missing case'}}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$false;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C2C cases; independent review pending"
