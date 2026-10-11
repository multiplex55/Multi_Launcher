param(
 [Parameter(Mandatory=$true)][string[]]$LogPaths,
 [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$ExpectedSource,
 [Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,
 [Parameter(Mandatory=$true)][string]$OutputPath
)
$ErrorActionPreference='Stop'
if (Test-Path -LiteralPath $OutputPath) { throw 'Refusing to overwrite existing collection output' }
$records=[System.Collections.Generic.List[object]]::new()
$widthReceipts=[System.Collections.Generic.List[object]]::new()
$eventAgeRows=[System.Collections.Generic.List[object]]::new()
$identities=[System.Collections.Generic.HashSet[string]]::new()
function Read-Array([string]$value,[string]$separator=',') {
 if (-not $value.StartsWith('[') -or -not $value.EndsWith(']')) { throw 'Invalid sample array' }
 $body=$value.Substring(1,$value.Length-2)
 return $body.Split([string[]]@($separator),[System.StringSplitOptions]::None)
}
foreach($logPath in $LogPaths) {
 foreach($line in Get-Content -LiteralPath $logPath) {
  $ageMarker=$line.IndexOf('TRACK_C_EVENT_AGES schema=')
  if($ageMarker -ge 0) {
   $ageFields=@{}
   foreach($token in $line.Substring($ageMarker+'TRACK_C_EVENT_AGES '.Length).Split(' ')) {
    if($token.Length -eq 0) { continue }
    $pair=$token.Split(@('='),2)
    if($pair.Length -ne 2 -or $ageFields.ContainsKey($pair[0])) { throw 'Malformed event age field' }
    $ageFields[$pair[0]]=$pair[1]
   }
   foreach($required in @('schema','owner','source','profile','mode','telemetry_enabled','fixture','fixture_signature','count','cold_type','recorder_capacity','sample','events','age_ns','age_p50_ns','age_p95_ns','age_max_ns','classes','origins','complete_output_state_id','structural_output_id','drain_calls','drain_ns','handler_receipts','overflow','abandoned')) {
    if(-not $ageFields.ContainsKey($required)) { throw "Missing event age field: $required" }
   }
   if($ageFields.schema -ne '1' -or $ageFields.source -ne $ExpectedSource -or $ageFields.profile -ne 'fast-dev' -or $ageFields.mode -cne $ExpectedMode -or $ageFields.telemetry_enabled -ne 'true' -or $ageFields.overflow -ne '0' -or $ageFields.abandoned -ne '0') { throw 'Event age source/profile/protocol mismatch or incomplete delivery' }
   $sampleIndex=[int]$ageFields.sample; $eventCount=[int]$ageFields.events; $recorderCapacity=[int]$ageFields.recorder_capacity
   if($sampleIndex -lt 0 -or $sampleIndex -ge 20 -or $eventCount -le 0 -or $eventCount -gt $recorderCapacity -or $recorderCapacity -gt 4096 -or [uint64]$ageFields.drain_calls -lt 1) { throw 'Invalid bounded event age counts' }
   $ages=@(Read-Array $ageFields.age_ns | ForEach-Object { [uint64]$_ })
   $classes=@(Read-Array $ageFields.classes); $origins=@(Read-Array $ageFields.origins)
   if($ages.Count -ne $eventCount -or $classes.Count -ne $eventCount -or $origins.Count -ne $eventCount) { throw 'Event age array length mismatch' }
   $sortedAges=@($ages | Sort-Object); $medianIndex=[int][Math]::Ceiling($eventCount*0.50)-1; $tailIndex=[int][Math]::Ceiling($eventCount*0.95)-1
   if([uint64]$ageFields.age_p50_ns -ne $sortedAges[$medianIndex] -or [uint64]$ageFields.age_p95_ns -ne $sortedAges[$tailIndex] -or [uint64]$ageFields.age_max_ns -ne $sortedAges[-1]) { throw 'Invalid per-event age quantiles' }
   $expectedPairs=@{}
   for($eventIndex=0;$eventIndex -lt $eventCount;$eventIndex++) {
    if($classes[$eventIndex] -notin @('FileReload','IndexCompletion','Clipboard') -or $origins[$eventIndex] -notin @('FileWatcher','IndexCoordinator','Registry')) { throw 'Unexpected queue fixture class/origin' }
    $pairKey=$classes[$eventIndex]+'.'+$origins[$eventIndex]
    if($pairKey -notin @('FileReload.FileWatcher','IndexCompletion.IndexCoordinator','Clipboard.Registry')) { throw 'Unapproved queue class/origin pair' }
    if(-not $expectedPairs.ContainsKey($pairKey)) { $expectedPairs[$pairKey]=0 }
    $expectedPairs[$pairKey]++
   }
   $handlers=[System.Collections.Generic.List[object]]::new(); $handlerKeys=[System.Collections.Generic.HashSet[string]]::new(); $totalHandlerNanos=[uint64]0
   foreach($handler in @(Read-Array $ageFields.handler_receipts)) {
    if($handler -notmatch '^([A-Za-z]+\.[A-Za-z]+):([0-9]+):([0-9]+):([0-9]+)$') { throw 'Invalid handler receipt' }
    $pairKey=$Matches[1]; $calls=[uint64]$Matches[2]; $total=[uint64]$Matches[3]; $maximum=[uint64]$Matches[4]
    if(-not $handlerKeys.Add($pairKey) -or -not $expectedPairs.ContainsKey($pairKey) -or $calls -ne $expectedPairs[$pairKey] -or $maximum -gt $total) { throw 'Handler receipt does not match delivered events' }
    $totalHandlerNanos+=$total; $handlers.Add([ordered]@{class_origin=$pairKey;calls=$calls;total_ns=$total;max_ns=$maximum})
   }
   if($handlerKeys.Count -ne $expectedPairs.Count -or $totalHandlerNanos -gt [uint64]$ageFields.drain_ns) { throw 'Incomplete handler receipts or invalid nested timing' }
   $eventAgeRows.Add([ordered]@{fields=$ageFields;sample=$sampleIndex;events=$eventCount;recorder_capacity=$recorderCapacity;age_ns=$ages;classes=$classes;origins=$origins;handler_receipts=$handlers.ToArray()})
   continue
  }
  $widthMarker=$line.IndexOf('TRACK_C_WIDTH_RECEIPT schema=')
  if($widthMarker -ge 0) {
   $widthFields=@{}
   foreach($token in $line.Substring($widthMarker+'TRACK_C_WIDTH_RECEIPT '.Length).Split(' ')) {
    if($token.Length -eq 0) { continue }
    $pair=$token.Split(@('='),2)
    if($pair.Length -ne 2 -or $widthFields.ContainsKey($pair[0])) { throw 'Malformed width receipt field' }
    $widthFields[$pair[0]]=$pair[1]
   }
   foreach($required in @('schema','owner','source','profile','mode','telemetry_enabled','fixture','fixture_signature','count','viewport','settings','cold_type','scenario','transition','actual_scroll_area_width_milli','samples')) {
    if(-not $widthFields.ContainsKey($required)) { throw "Missing width field: $required" }
   }
   if($widthFields.schema -ne '1' -or $widthFields.source -ne $ExpectedSource -or $widthFields.profile -ne 'fast-dev' -or $widthFields.mode -cne $ExpectedMode -or $widthFields.telemetry_enabled -ne 'true' -or $widthFields.samples -ne '20') { throw 'Width source/profile/protocol mismatch' }
   if($widthFields.scenario -ne 'quick_notes_empty_horizontal_width_transition' -or $widthFields.transition -ne 'screen_960x640_to_300x640' -or $widthFields.viewport -ne 'screen_300x640_transition_960x640') { throw 'Unknown width scenario' }
   $actualWidths=@(Read-Array $widthFields.actual_scroll_area_width_milli | ForEach-Object { [uint64]$_ })
   if($actualWidths.Count -ne 20 -or @($actualWidths | Where-Object { $_ -eq 0 -or $_ -gt 300000 }).Count -ne 0) { throw 'Invalid narrow width array' }
   $widthReceipts.Add([ordered]@{fields=$widthFields;actual_scroll_area_width_milli=$actualWidths})
   continue
  }
  $marker=$line.IndexOf('TRACK_C_RESULT schema=')
  if($marker -lt 0) { continue }
  $fields=@{}
  foreach($token in $line.Substring($marker+'TRACK_C_RESULT '.Length).Split(' ')) {
   if($token.Length -eq 0) { continue }
   $pair=$token.Split(@('='),2)
   if($pair.Length -ne 2 -or $fields.ContainsKey($pair[0])) { throw 'Malformed/duplicate field' }
   $fields[$pair[0]]=$pair[1]
  }
  foreach($required in @('schema','owner','source','profile','mode','telemetry_enabled','fixture','fixture_signature','count','viewport','scale_milli','fonts','settings','cold_type','warmups','samples','elapsed_ns','p50_ns','p95_ns','max_ns','complete_output_state_ids','structural_output_ids','revision_receipts','viewport_receipts','work_units','work_counters','exclusions')) {
   if(-not $fields.ContainsKey($required)) { throw "Missing field: $required" }
  }
  if($fields.schema -ne '1' -or $fields.source -ne $ExpectedSource -or $fields.profile -ne 'fast-dev' -or $fields.mode -cne $ExpectedMode -or $fields.telemetry_enabled -ne 'true' -or $fields.warmups -ne '5' -or $fields.samples -ne '20' -or $fields.exclusions -ne '0') { throw 'Source/profile/protocol mismatch' }
  foreach($label in @('owner','fixture','viewport','fonts','settings','cold_type')) {
   if($fields[$label] -notmatch '^[A-Za-z0-9_.:+,-]+$') { throw "Unexpected metadata syntax: $label" }
  }
  if($fields.fixture_signature -notmatch '^[0-9a-f]{16}$') { throw 'Invalid fixture signature' }
  $elapsed=@(Read-Array $fields.elapsed_ns | ForEach-Object { [uint64]$_ })
  $complete=@(Read-Array $fields.complete_output_state_ids)
  $structural=@(Read-Array $fields.structural_output_ids)
  $revisions=@(Read-Array $fields.revision_receipts)
  $viewport=@(Read-Array $fields.viewport_receipts)
  $work=@(Read-Array $fields.work_units | ForEach-Object { [uint64]$_ })
  $counters=@(Read-Array $fields.work_counters '|')
  foreach($array in @($elapsed,$complete,$structural,$revisions,$viewport,$work,$counters)) {
   if($array.Count -ne 20) { throw 'Expected twenty samples in every array' }
  }
  foreach($id in @($complete)+@($structural)) {
   if($id -notmatch '^[0-9a-f]{16}$') { throw 'Invalid output identity' }
  }
  foreach($id in $viewport) {
   if($id -ne 'none' -and $id -notmatch '^[0-9a-f]{16}$') { throw 'Invalid viewport receipt' }
  }
  foreach($receipt in @($revisions)+@($counters)) {
   if($receipt -notmatch '^(?:[0-9]+(?::[0-9]+)*)?$') { throw 'Invalid numeric receipt' }
  }
  $ordered=@($elapsed | Sort-Object)
  if([uint64]$fields.p50_ns -ne $ordered[9] -or [uint64]$fields.p95_ns -ne $ordered[18] -or [uint64]$fields.max_ns -ne $ordered[19]) { throw 'Quantiles differ from raw nearest-rank samples' }
  $identity=@($fields.owner,$fields.fixture_signature,$fields.viewport,$fields.settings,$fields.cold_type)-join '|'
  if(-not $identities.Add($identity)) { throw 'Duplicate scenario; retain reruns separately' }
  $records.Add([ordered]@{
   owner=$fields.owner; source=$fields.source; profile=$fields.profile; mode=$fields.mode
   telemetry_enabled=$true; fixture=$fields.fixture; fixture_signature=$fields.fixture_signature
   item_count=[uint64]$fields.count; viewport=$fields.viewport; scale_milli=[uint64]$fields.scale_milli
   fonts=$fields.fonts; settings=$fields.settings; cold_type=$fields.cold_type
   warmups=5; samples=20; exclusions=0; elapsed_ns=$elapsed
   p50_ns=$ordered[9]; p95_ns=$ordered[18]; max_ns=$ordered[19]
   complete_output_state_ids=$complete; structural_output_ids=$structural
   revision_receipts=$revisions; viewport_receipts=$viewport; work_units=$work; work_counters=$counters
  })
 }
}
$widthKeys=[System.Collections.Generic.HashSet[string]]::new()
foreach($receipt in $widthReceipts) {
 $f=$receipt.fields
 $key=@($f.owner,$f.fixture_signature,$f.viewport,$f.settings,$f.cold_type)-join '|'
 if(-not $widthKeys.Add($key)) { throw 'Duplicate width receipt' }
 $matching=@($records | Where-Object { $_.owner -eq $f.owner -and $_.fixture -eq $f.fixture -and $_.fixture_signature -eq $f.fixture_signature -and $_.viewport -eq $f.viewport -and $_.settings -eq $f.settings -and $_.cold_type -eq $f.cold_type })
 if($matching.Count -ne 1 -or $matching[0].item_count -ne [uint64]$f.count) { throw 'Width receipt does not join exactly one owner record' }
 $matching[0].Add('width_receipt',[ordered]@{scenario=$f.scenario;transition=$f.transition;actual_scroll_area_width_milli=$receipt.actual_scroll_area_width_milli})
}
foreach($record in $records) {
 if($record.viewport -eq 'screen_300x640_transition_960x640' -and -not $record.Contains('width_receipt')) { throw 'Missing required width receipt' }
}
$ageKeys=[System.Collections.Generic.HashSet[string]]::new()
foreach($row in $eventAgeRows) {
 $f=$row.fields
 $key=@($f.owner,$f.fixture_signature,$f.cold_type,$f.sample)-join '|'
 if(-not $ageKeys.Add($key)) { throw 'Duplicate event age sample' }
 $matching=@($records | Where-Object { $_.owner -eq $f.owner -and $_.fixture -eq $f.fixture -and $_.fixture_signature -eq $f.fixture_signature -and $_.cold_type -eq $f.cold_type -and $_.item_count -eq [uint64]$f.count })
 if($matching.Count -ne 1) { throw 'Event age receipt does not join exactly one owner' }
 $record=$matching[0]; $sampleIndex=$row.sample
 if($record.owner -eq 'process_watch_events_concurrent_offered_load' -and [uint64]$f.drain_ns -ne $record.elapsed_ns[$sampleIndex]) { throw 'Concurrent reducer elapsed must equal its drain receipt' }
 if($record.complete_output_state_ids[$sampleIndex] -ne $f.complete_output_state_id -or $record.structural_output_ids[$sampleIndex] -ne $f.structural_output_id -or $record.work_units[$sampleIndex] -ne $row.events -or [uint64]$f.drain_ns -gt $record.elapsed_ns[$sampleIndex]) { throw 'Event age identity/work/timing mismatch' }
 $queueCounters=$record.work_counters[$sampleIndex].Split(':')
 if($queueCounters.Count -lt 4) { throw 'Missing queue event counters' }
 $pairCalls=@{'FileReload.FileWatcher'=[uint64]0;'IndexCompletion.IndexCoordinator'=[uint64]0;'Clipboard.Registry'=[uint64]0}
 foreach($handler in $row.handler_receipts) { $pairCalls[$handler.class_origin]=[uint64]$handler.calls }
 if([uint64]$queueCounters[0] -ne $row.events -or [uint64]$queueCounters[1] -ne $pairCalls['FileReload.FileWatcher'] -or [uint64]$queueCounters[2] -ne $pairCalls['IndexCompletion.IndexCoordinator'] -or [uint64]$queueCounters[3] -ne $pairCalls['Clipboard.Registry']) { throw 'Queue event counters do not match delivered handler receipts' }
 if(-not $record.Contains('event_age_samples')) { $record.Add('event_age_samples',[System.Collections.Generic.List[object]]::new()) }
 $record.event_age_samples.Add([ordered]@{sample=$sampleIndex;events=$row.events;recorder_capacity=$row.recorder_capacity;age_ns=$row.age_ns;age_p50_ns=[uint64]$f.age_p50_ns;age_p95_ns=[uint64]$f.age_p95_ns;age_max_ns=[uint64]$f.age_max_ns;classes=$row.classes;origins=$row.origins;drain_calls=[uint64]$f.drain_calls;drain_ns=[uint64]$f.drain_ns;handler_receipts=$row.handler_receipts;overflow=0;abandoned=0})
}
foreach($record in $records) {
 if($record.owner -in @('process_watch_events_finite_burst','process_watch_events_concurrent_offered_load')) {
  if(-not $record.Contains('event_age_samples') -or $record.event_age_samples.Count -ne 20) { throw 'Missing required twenty event age samples' }
  $record.event_age_samples=@($record.event_age_samples | Sort-Object { [int]$_['sample'] })
  for($sampleIndex=0;$sampleIndex -lt 20;$sampleIndex++) {
   if([int]$record.event_age_samples[$sampleIndex]['sample'] -ne $sampleIndex) { throw 'Event age samples must be emitted in numeric sample order' }
  }
 }
}
if($records.Count -eq 0) { throw 'No owner records; no baseline produced' }
[ordered]@{
 schema=1; source=$ExpectedSource; profile='fast-dev'; mode=$ExpectedMode
 status='COLLECTED_PENDING_REVIEW'; native_display='NOT MEASURED'; records=$records.ToArray()
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
Write-Output "Collected $($records.Count) owner scenarios; review and inventory still required."
