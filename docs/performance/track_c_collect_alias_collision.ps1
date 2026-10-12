param([Parameter(Mandatory=$true)][string]$LogPath,[Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,[Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,[Parameter(Mandatory=$true)][string]$OutputPath)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing output'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$fixed=[ordered]@{schema='1';stage='alias_collision_characterization';owner='NotePanel_alias_collision_warning';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant();telemetry_enabled='true';observer_enabled='true';fixture='synthetic_alias_collision';warmups='5';samples='20';exclusions='0';owner_scope='lookup_and_message_only';invocation_scope='accepted_Add_or_Rename_not_keystrokes';estimate_scope='selected_logical_payload_not_allocator_or_RSS';snapshot_hold_scope='existing_PERF1_estimate_plus_clone';legacy_error_scope='empty_snapshot_defaults_not_coherence_proof'}
$variable=@('fixture_signature','warning_signature','case','count')
$arrays=@('owner_ns','lookup_calls','format_calls','format_ns','alias_map_snapshot_calls','alias_map_snapshot_key_entries','alias_map_snapshot_slug_entries','alias_map_selected_clone_bytes','note_snapshot_calls','note_snapshot_entries','note_snapshot_estimated_clone_bytes','note_snapshot_lock_wait_ns','note_snapshot_lock_hold_ns','warning_output_bytes')
$timings=@('owner_ns','format_ns','note_snapshot_lock_wait_ns','note_snapshot_lock_hold_ns');$required=@($fixed.Keys)+$variable+$arrays
function UInt([string]$s){if($s -notmatch '^[0-9]+$'){throw 'Unsigned decimal required'};[uint64]::Parse($s,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$a,[double]$p){$sorted=@($a|Sort-Object);$sorted[[int][Math]::Ceiling($p*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $start=$line.IndexOf('C1D_ALIAS_RESULT schema=');if($start -lt 0){continue};$f=@{}
 foreach($token in $line.Substring($start+'C1D_ALIAS_RESULT '.Length).Split(' ')){
  if(-not $token){continue};$pair=$token.Split(@('='),2)
  if($pair.Length -ne 2 -or $f.ContainsKey($pair[0]) -or $pair[0] -notin $required){throw 'Malformed/unknown/duplicate field'};$f[$pair[0]]=$pair[1]
 }
 foreach($k in $required){if(-not $f.ContainsKey($k)){throw "Missing $k"}}
 foreach($k in $fixed.Keys){if($f[$k] -cne $fixed[$k]){throw "Protocol mismatch $k"}}
 foreach($k in @('fixture_signature','warning_signature')){if($f[$k] -notmatch '^[0-9a-f]{16}$'){throw 'Invalid hash'}}
 if($f.case -notin @('unique','duplicate','ambiguous')){throw 'Invalid case'}
 $n=UInt $f.count;$key=$f.case+':'+$n;if(-not $seen.Add($key)){throw 'Duplicate scenario'}
 $r=[ordered]@{};foreach($k in @($fixed.Keys)+$variable){$r[$k]=$f[$k]};$r['count']=$n
 foreach($k in $arrays){
  if($f[$k] -notmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'};$items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Expected20samples'}
  $r[$k]=@($items|ForEach-Object {UInt $_})
 }
 for($i=0;$i -lt 20;$i++){
  if($r.lookup_calls[$i] -ne 1 -or $r.alias_map_snapshot_calls[$i] -ne 1){throw 'Invalid lookup/map calls'}
  if($r.alias_map_snapshot_key_entries[$i] -eq 0 -or $r.alias_map_snapshot_slug_entries[$i] -lt $r.alias_map_snapshot_key_entries[$i] -or $r.alias_map_selected_clone_bytes[$i] -eq 0){throw 'Invalid nonempty alias-map receipt'}
  if($r.owner_ns[$i] -eq 0 -or $r.format_ns[$i] -gt $r.owner_ns[$i] -or ([decimal]$r.note_snapshot_lock_wait_ns[$i]+[decimal]$r.note_snapshot_lock_hold_ns[$i]) -gt $r.owner_ns[$i]){throw 'Invalid timing bounds'}
  if($f.case -eq 'unique'){
   foreach($k in @('format_calls','format_ns','note_snapshot_calls','note_snapshot_entries','note_snapshot_estimated_clone_bytes','note_snapshot_lock_wait_ns','note_snapshot_lock_hold_ns','warning_output_bytes')){if($r[$k][$i] -ne 0){throw 'Unique branch performed conflict work'}}
  }else{
   if($r.format_calls[$i] -ne 1 -or $r.note_snapshot_calls[$i] -ne 1 -or $r.note_snapshot_entries[$i] -ne $n -or $r.note_snapshot_estimated_clone_bytes[$i] -eq 0 -or $r.warning_output_bytes[$i] -eq 0){throw 'Conflict branch receipt mismatch'}
  }
 }
 foreach($k in @('lookup_calls','format_calls','alias_map_snapshot_calls','alias_map_snapshot_key_entries','alias_map_snapshot_slug_entries','alias_map_selected_clone_bytes','note_snapshot_calls','note_snapshot_entries','note_snapshot_estimated_clone_bytes','warning_output_bytes')){if(@($r[$k]|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled work receipt'}}
 $stats=[ordered]@{};foreach($k in $timings){$a=$r[$k];$stats[$k]=[ordered]@{p50_ns=(Rank $a 0.5);p95_ns=(Rank $a 0.95);max_ns=(@($a|Sort-Object)[-1])}};$r.statistics=$stats;$records.Add($r)
}
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,5000)}
$expected=@(foreach($n in $counts){foreach($c in @('unique','duplicate','ambiguous')){"${c}:$n"}})
if($records.Count -ne $expected.Count){throw 'Incomplete inventory'};foreach($key in $expected){if(-not $seen.Contains($key)){throw "Missing $key"}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;integrated_query=$false;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText((Join-Path (Get-Location) $OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C1D records; review pending"