param([Parameter(Mandatory=$true)][string]$LogPath,[Parameter(Mandatory=$true)][ValidatePattern('^(?:[0-9a-f]{40}|UNCOMMITTED_SOURCE)$')][string]$ExpectedSource,[Parameter(Mandatory=$true)][ValidateSet('Small','Full')][string]$ExpectedMode,[Parameter(Mandatory=$true)][string]$OutputPath)
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputPath){throw 'Refuse existing output'}
if($ExpectedMode -eq 'Full' -and $ExpectedSource -eq 'UNCOMMITTED_SOURCE'){throw 'Full requires committed source'}
$static=@('schema','stage','owner','source','profile','mode','telemetry_enabled','observer_enabled','fixture','fixture_signature','transition','count','todo_count','viewport','settings','cold_type','warmups','samples','exclusions','repeat_k','publication_model','integrated_owner','break_even_scope','memory_scope','allocator_and_process_ram','publication_totals_applicability','repeat_scope')
$footprints=@('text_logical_bytes','text_capacity_bytes','metadata_logical_bytes','metadata_string_capacity_bytes','source_slot_bytes','posting_key_logical_bytes','posting_key_capacity_bytes','posting_positions','posting_position_capacity','equal_projection_retained_logical_bytes','equal_projection_retained_capacity_bytes','changed_old_projection_logical_bytes','changed_old_projection_capacity_bytes','changed_two_generation_peak_logical_bytes','changed_two_generation_peak_capacity_bytes')
$timings=@('current_owner_ns','projection_owner_ns','projection_prepare_ns','current_repeat5_ns','projection_repeat5_ns','unchanged_gate_ns','equal_current_publish_ns','equal_projection_publish_ns','changed_current_publish_ns','changed_projection_publish_ns','equal_current_plus1_ns','equal_projection_plus1_ns','equal_current_plus5_ns','equal_projection_plus5_ns','changed_current_plus1_ns','changed_projection_plus1_ns','changed_current_plus5_ns','changed_projection_plus5_ns')
$work=@('equal_break_even_k','equal_break_even_finite','changed_break_even_k','changed_break_even_finite','canonical_posting_entries','typed_posting_entries','fallback_visits','fallback_input_bytes','linked_todo_rows','related_note_rows','mention_rows','snippet_calls','metadata_reads','note_revisions','todo_revisions')
$ids=@('complete_output_ids','structural_output_ids');$required=$static+$footprints+$timings+$work+$ids
function UInt([string]$s){if($s -notmatch '^[0-9]+$'){throw 'Unsigned decimal required'};return [uint64]::Parse($s,[Globalization.CultureInfo]::InvariantCulture)}
function Rank([object[]]$a,[double]$p){$sorted=@($a|Sort-Object);return $sorted[[int][Math]::Ceiling($p*$sorted.Count)-1]}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 $start=$line.IndexOf('C1C_NOTE_RESULT schema=');if($start -lt 0){continue};$f=@{}
 foreach($token in $line.Substring($start+'C1C_NOTE_RESULT '.Length).Split(' ')){if(-not $token){continue};$pair=$token.Split(@('='),2);if($pair.Length -ne 2 -or $f.ContainsKey($pair[0]) -or $pair[0] -notin $required){throw 'Malformed/unknown/duplicate field'};$f[$pair[0]]=$pair[1]}
 foreach($k in $required){if(-not $f.ContainsKey($k)){throw "Missing $k"}}
 foreach($pair in @(@('schema','1'),@('stage','prepared_projection'),@('owner','NotePanel_refresh_heavy_derived_vs_prepared_projection_replay'),@('source',$ExpectedSource),@('profile','fast-dev'),@('mode',$ExpectedMode.ToLowerInvariant()),@('telemetry_enabled','true'),@('observer_enabled','false'),@('fixture','synthetic_note_panel_refresh'),@('todo_count','12'),@('viewport','960x640'),@('settings','backlinks_enabled_inert'),@('warmups','5'),@('samples','20'),@('exclusions','0'),@('repeat_k','5'),@('publication_model','local_cache_simulation_without_global_lock_or_revision'),@('integrated_owner','false'),@('break_even_scope','local_simulation'),@('memory_scope','projection_only_lossy_utf8_paths_map_buckets_excluded'),@('allocator_and_process_ram','not_measured'),@('publication_totals_applicability','standalone_model_not_transition_latency'),@('repeat_scope','forced_invalidated_refresh_not_idle'))){if($f[$pair[0]] -cne $pair[1]){throw "Protocol mismatch $($pair[0])"}}
 if($f.fixture_signature -notmatch '^[0-9a-f]{16}$'){throw 'Invalid fixture hash'}
 $count=UInt $f.count;$cold=@{'local-draft-debounce'='debounce-due-refresh';'external-note-revision'='external-note-revision-bypass';'persisted-todo-revision'='todo-revision-bypass'}
 if(-not $cold.ContainsKey($f.transition) -or $f.cold_type -cne $cold[$f.transition]){throw 'Unknown transition/cold pair'}
 $key=$f.transition+':'+$count;if(-not $seen.Add($key)){throw 'Duplicate scenario'}
 $r=[ordered]@{};foreach($k in $static){$r[$k]=$f[$k]};$r['count']=$count
 foreach($k in $footprints){$r[$k]=UInt $f[$k]}
 if($r.text_capacity_bytes -lt $r.text_logical_bytes -or $r.posting_key_capacity_bytes -lt $r.posting_key_logical_bytes -or $r.posting_position_capacity -lt $r.posting_positions){throw 'Invalid capacity receipt'}
 $logical=[decimal]$r.text_logical_bytes+[decimal]$r.metadata_logical_bytes+[decimal]$r.source_slot_bytes+[decimal]$r.posting_key_logical_bytes+8*[decimal]$r.posting_positions
 $capacity=[decimal]$r.text_capacity_bytes+[decimal]$r.metadata_string_capacity_bytes+[decimal]$r.source_slot_bytes+[decimal]$r.posting_key_capacity_bytes+8*[decimal]$r.posting_position_capacity
 if($r.equal_projection_retained_logical_bytes -ne $logical -or $r.equal_projection_retained_capacity_bytes -ne $capacity){throw 'Retained projection sum mismatch'}
 if([decimal]$r.changed_two_generation_peak_logical_bytes -ne [decimal]$r.changed_old_projection_logical_bytes+$logical -or [decimal]$r.changed_two_generation_peak_capacity_bytes -ne [decimal]$r.changed_old_projection_capacity_bytes+$capacity){throw 'Generation peak mismatch'}
 foreach($k in $timings+$work+$ids){$v=$f[$k];if($v -notmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'};$items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Expected twenty samples'}
  if($k -in $ids){foreach($item in $items){if($item -notmatch '^[0-9a-f]{16}$'){throw 'Invalid output identity'}};$r[$k]=@($items)}else{$r[$k]=@($items|ForEach-Object {UInt $_})}
 }
 for($i=0;$i -lt 20;$i++){
  foreach($kind in @('equal','changed')){foreach($side in @('current','projection')){foreach($n in @(1,5)){$pub=$r["${kind}_${side}_publish_ns"][$i];$owner=if($n -eq 1){$r["${side}_owner_ns"][$i]}else{$r["${side}_repeat5_ns"][$i]};if([decimal]$r["${kind}_${side}_plus${n}_ns"][$i] -ne ([decimal]$pub+[decimal]$owner)){throw 'Paired total mismatch'}}}
   $saving=[decimal]$r.current_owner_ns[$i]-[decimal]$r.projection_owner_ns[$i];$extra=[decimal]$r["${kind}_projection_publish_ns"][$i]-[decimal]$r["${kind}_current_publish_ns"][$i];$finite=[uint64]0;$breakeven=[uint64]0
   if($saving -gt 0){$finite=1;if($extra -gt 0){$breakeven=[uint64][Math]::Ceiling($extra/$saving)}}
   if($r["${kind}_break_even_finite"][$i] -ne $finite -or $r["${kind}_break_even_k"][$i] -ne $breakeven){throw 'Break-even mismatch'}
  }
  $todo=$r.linked_todo_rows[$i];$related=$r.related_note_rows[$i];$mentions=$r.mention_rows[$i]
  if($todo -gt 12 -or $mentions -lt $todo -or $r.fallback_visits[$i] -gt $count-1){throw 'Invalid work receipt'}
  $notes=[decimal]$related+[decimal]$mentions-[decimal]$todo
  if($r.metadata_reads[$i] -ne $notes -or $r.snippet_calls[$i] -ne $notes+$todo){throw 'Output/work mismatch'}
  if($notes -gt $count-1 -or $r.fallback_input_bytes[$i] -gt $r.text_logical_bytes){throw 'Invalid work density/bytes'}
 }
 $stats=[ordered]@{};foreach($k in $timings){$a=$r[$k];$stats[$k]=[ordered]@{p50_ns=(Rank $a 0.5);p95_ns=(Rank $a 0.95);max_ns=(@($a|Sort-Object)[-1])}}
 $r.statistics=$stats;$records.Add($r)
}
$expected=if($ExpectedMode -eq 'Small'){@('local-draft-debounce:100','external-note-revision:100','persisted-todo-revision:100')}else{@('local-draft-debounce:100','local-draft-debounce:1000','local-draft-debounce:5000','external-note-revision:1000','persisted-todo-revision:1000')}
if($records.Count -ne $expected.Count){throw 'Incomplete inventory'};foreach($key in $expected){if(-not $seen.Contains($key)){throw "Missing scenario $key"}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;integrated_owner=$false;allocator_and_process_ram='NOT MEASURED';records=@($records.ToArray())}
[IO.File]::WriteAllText((Join-Path (Get-Location) $OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C1C experimental records; review pending"