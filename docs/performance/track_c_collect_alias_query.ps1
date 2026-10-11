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
 schema='1';stage='single_lock_conflict_labels';owner='NotePanel_alias_collision_warning';source=$ExpectedSource
 profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant();telemetry_enabled='true';observer_enabled='true'
 fixture='synthetic_alias_collision';warmups='5';samples='20';exclusions='0'
 owner_scope='lookup_join_message_only';invocation_scope='accepted_Add_or_Rename_not_keystrokes'
 estimate_scope='selected_logical_payload_not_allocator_or_RSS';lock_scope='single_CACHE_generation'
 candidate_scope='map_slug_entries_including_current';note_scope='ordered_cache_rows_scanned'
 legacy_error_scope='fallible_query_diagnosed_save_continues'
}
$variable=@('fixture_signature','warning_signature','case','count')
$zero=@('alias_map_snapshot_calls','alias_map_snapshot_key_entries','alias_map_snapshot_slug_entries','alias_map_selected_clone_bytes','note_snapshot_calls','note_snapshot_entries','note_snapshot_estimated_clone_bytes','note_snapshot_lock_wait_ns','note_snapshot_lock_hold_ns')
$arrays=@('owner_ns','query_ns','message_ns','query_calls','query_successes','query_failures','query_lock_wait_ns','query_lock_hold_ns','candidate_entries','candidate_slugs_examined','notes_visited','labels_returned','label_bytes','output_bytes','note_revision_receipts')+$zero
$timings=@('owner_ns','query_ns','message_ns','query_lock_wait_ns','query_lock_hold_ns')
$required=@($fixed.Keys)+$variable+$arrays
function UInt([string]$s){if($s -notmatch '^[0-9]+$'){throw 'Unsigned decimal required'};[uint64]::Parse($s,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$a,[double]$p){$sorted=@($a|Sort-Object);$sorted[[int][Math]::Ceiling($p*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new()
$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $prefix='C1D_ALIAS_QUERY_RESULT ';$start=$line.IndexOf($prefix+'schema=');if($start -lt 0){continue};$f=@{}
 foreach($token in $line.Substring($start+$prefix.Length).Split(' ')){
  if(-not $token){continue};$pair=$token.Split(@('='),2)
  if($pair.Length -ne 2 -or $f.ContainsKey($pair[0]) -or $pair[0] -notin $required){throw 'Malformed/unknown/duplicate field'}
  $f[$pair[0]]=$pair[1]
 }
 foreach($k in $required){if(-not $f.ContainsKey($k)){throw "Missing $k"}}
 foreach($k in $fixed.Keys){if($f[$k] -cne $fixed[$k]){throw "Protocol mismatch $k"}}
 foreach($k in @('fixture_signature','warning_signature')){if($f[$k] -notmatch '^[0-9a-f]{16}$'){throw 'Invalid hash'}}
 if($f.case -notin @('unique','duplicate','ambiguous')){throw 'Invalid case'}
 $n=UInt $f.count;$key=$f.case+':'+$n;if(-not $seen.Add($key)){throw 'Duplicate scenario'}
 $r=[ordered]@{};foreach($k in @($fixed.Keys)+$variable){$r[$k]=$f[$k]};$r['count']=$n
 foreach($k in $arrays){
  if($f[$k] -notmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'}
  $items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Expected20samples'}
  $r[$k]=@($items|ForEach-Object {UInt $_})
 }
 for($i=0;$i -lt 20;$i++){
  if($r.query_calls[$i] -ne 1 -or $r.query_successes[$i] -ne 1 -or $r.query_failures[$i] -ne 0){throw 'Invalid query outcome'}
  foreach($k in $zero){if($r[$k][$i] -ne 0){throw 'Legacy snapshot work performed'}}
  if($r.owner_ns[$i] -eq 0 -or $r.query_ns[$i] -eq 0 -or ([decimal]$r.query_ns[$i]+[decimal]$r.message_ns[$i]) -gt $r.owner_ns[$i]){throw 'Invalid owner timing bounds'}
  if(([decimal]$r.query_lock_wait_ns[$i]+[decimal]$r.query_lock_hold_ns[$i]) -gt $r.query_ns[$i]){throw 'Invalid query timing bounds'}
  if($r.note_revision_receipts[$i] -eq 0){throw 'Missing canonical revision'}
  $expectedCandidates=if($f.case -eq 'ambiguous'){2}else{1}
  if($r.candidate_entries[$i] -ne $expectedCandidates -or $r.candidate_slugs_examined[$i] -ne $expectedCandidates){throw 'Invalid candidate visits'}
  if($f.case -eq 'unique'){
   foreach($k in @('message_ns','notes_visited','labels_returned','label_bytes','output_bytes')){if($r[$k][$i] -ne 0){throw 'Unique branch performed conflict work'}}
  }else{
   if($r.notes_visited[$i] -ne $n -or $r.labels_returned[$i] -ne 2 -or $r.label_bytes[$i] -eq 0 -or $r.output_bytes[$i] -le $r.label_bytes[$i]){throw 'Invalid conflict receipts'}
  }
 }
 foreach($k in @('query_calls','query_successes','query_failures','candidate_entries','candidate_slugs_examined','notes_visited','labels_returned','label_bytes','output_bytes')+$zero){
  if(@($r[$k]|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled work receipt'}
 }
 $stats=[ordered]@{};foreach($k in $timings){$a=$r[$k];$stats[$k]=[ordered]@{p50_ns=(Rank $a 0.5);p95_ns=(Rank $a 0.95);max_ns=(@($a|Sort-Object)[-1])}}
 $r.statistics=$stats;$records.Add($r)
}
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,5000)}
$expected=@(foreach($n in $counts){foreach($c in @('unique','duplicate','ambiguous')){"${c}:$n"}})
if($records.Count -ne $expected.Count){throw 'Incomplete inventory'}
foreach($key in $expected){if(-not $seen.Contains($key)){throw "Missing $key"}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;integrated_query=$true;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText((Join-Path (Get-Location) $OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C1D query records; review pending"
