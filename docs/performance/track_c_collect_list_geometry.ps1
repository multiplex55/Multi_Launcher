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
 schema='1';stage='cold_list_geometry_characterization';installation='eager_list_geometry'
 fixture='synthetic_root_list_geometry';owner='RootListGeometryCache::ensure';measurement_scope='installation+egui_Context_run+LauncherApp::render_root_frame'
 source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant()
 telemetry_enabled='true';observer_enabled='false';warmups='5';samples='20';exclusions='0'
}
$numeric=@('elapsed_ns','geometry_rebuilds','geometry_measured_rows','geometry_measured_cells','geometry_ns','painted_rows','presentation_prefix_len','geometry_capacity_before_bytes','geometry_capacity_after_bytes')
$identities=@('full_output_state_ids','action_output_ids','presentation_ids','geometry_projection_ids','viewport_receipts')
$arrays=$numeric+$identities+@('transition_receipts')
$variable=@('case','count','fixture_signature')
$required=@($fixed.Keys)+$variable+$arrays
$cases=@('fresh_context_first_open','shared_prefix_5_percent','shared_prefix_20_percent','identical_presentation_new_generation','last_row_text_edit','first_row_width_expansion_no_wrap','full_paths_toggle','wrap_change','font_atlas_replacement','pixels_per_point_change','viewport_width_change','warm_repeat')
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,10000)}
# Pure independent fixture reconstruction, before any emitted benchmark data.
# Frozen catalog1k control9a958b83b7e7345f; bindings never learned from output.
$bindings=@{
 'fresh_context_first_open:100'=@('bced459d451b8ce6','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_5_percent:100'=@('8489a2c83fcf50e4','c3745052c65797f2','3948b6374a90077a','d10d47015e04d833','5','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_20_percent:100'=@('ae25df481aa482bd','c3745052c65797f2','5a78f8ddbda6bd22','1c910b4435b1775b','20','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'identical_presentation_new_generation:100'=@('e62514284e471897','c3745052c65797f2','2b8e3608e783fe92','6b205379e46addc3','100','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'last_row_text_edit:100'=@('38d546f3f5abdf84','c3745052c65797f2','844b5d723624d276','d689a1e6b3b87cdf','99','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'first_row_width_expansion_no_wrap:100'=@('cb9b68532347453d','c3745052c65797f2','05bb50478d1220e4','6a14480dd982a2fd','0','1148190720','1148190720','1142947840','1065353216','1065353216','0','0','0','0')
 'full_paths_toggle:100'=@('5c4e39b2a7031699','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','1')
 'wrap_change:100'=@('1319e51db475a4ab','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','100','1148190720','1148190720','1142947840','1065353216','1065353216','1','0','0','0')
 'font_atlas_replacement:100'=@('feaa2b8dbaad978c','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','100','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'pixels_per_point_change:100'=@('82879df90bbcefae','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','100','1148190720','1148190720','1142947840','1065353216','1067450368','1','1','0','0')
 'viewport_width_change:100'=@('d8e3e98dc76a92af','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','100','1148190720','1144258560','1142947840','1065353216','1065353216','1','1','0','0')
 'warm_repeat:100'=@('f30fda942e7da52a','c3745052c65797f2','c3745052c65797f2','6b205379e46addc3','100','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'fresh_context_first_open:1000'=@('6da1289ea54d2442','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_5_percent:1000'=@('4d58138c7cadbd1d','9a958b83b7e7345f','d9494bd3cacf9867','b6be0043704b3fb7','50','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_20_percent:1000'=@('cd9569c8f7d7d572','9a958b83b7e7345f','3e9c42dec1b84dab','30d6cd6f2ed2eb73','200','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'identical_presentation_new_generation:1000'=@('bb12e745e9b84ac3','9a958b83b7e7345f','328ca4137905506a','8b4d019af38e44b7','1000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'last_row_text_edit:1000'=@('20c063a18a5e0b4c','9a958b83b7e7345f','e9848370d3817767','83b20cf86670bd1b','999','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'first_row_width_expansion_no_wrap:1000'=@('eace74e6469fe455','9a958b83b7e7345f','7c7900fd97ed4475','5067bcc1c101f6b5','0','1148190720','1148190720','1142947840','1065353216','1065353216','0','0','0','0')
 'full_paths_toggle:1000'=@('077ebd54debe747d','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','1')
 'wrap_change:1000'=@('e7781772a3b1a59c','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','1000','1148190720','1148190720','1142947840','1065353216','1065353216','1','0','0','0')
 'font_atlas_replacement:1000'=@('f538548c36a2fe2d','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','1000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'pixels_per_point_change:1000'=@('ddc1521a3d1edf7d','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','1000','1148190720','1148190720','1142947840','1065353216','1067450368','1','1','0','0')
 'viewport_width_change:1000'=@('fa3f9a933b730bbc','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','1000','1148190720','1144258560','1142947840','1065353216','1065353216','1','1','0','0')
 'warm_repeat:1000'=@('79ff2f8966bd669d','9a958b83b7e7345f','9a958b83b7e7345f','8b4d019af38e44b7','1000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'fresh_context_first_open:10000'=@('958e2ccaaa453a3b','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_5_percent:10000'=@('cfcfa3812af25a46','7696ea08a6c5e369','3ba42f91a3e13029','68e7b5af7bb7e332','500','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'shared_prefix_20_percent:10000'=@('0ad68dae892e1d68','7696ea08a6c5e369','f15545c4505f332d','a82a28f86ae1cbaa','2000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'identical_presentation_new_generation:10000'=@('caab4097992e94ad','7696ea08a6c5e369','944967ee032be2f9','e40906134ec0e16e','10000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'last_row_text_edit:10000'=@('27e5e0bd9cacccc1','7696ea08a6c5e369','b33fe7a3e31f30d7','76b53a34c750a240','9999','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'first_row_width_expansion_no_wrap:10000'=@('e725c52a4df84f33','7696ea08a6c5e369','a436c30ce594f8cb','3361991a6bc363c8','0','1148190720','1148190720','1142947840','1065353216','1065353216','0','0','0','0')
 'full_paths_toggle:10000'=@('3489c23b5753e634','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','0','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','1')
 'wrap_change:10000'=@('b79d44e142406d75','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','10000','1148190720','1148190720','1142947840','1065353216','1065353216','1','0','0','0')
 'font_atlas_replacement:10000'=@('b8381e2932e04d64','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','10000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
 'pixels_per_point_change:10000'=@('809492418a439864','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','10000','1148190720','1148190720','1142947840','1065353216','1067450368','1','1','0','0')
 'viewport_width_change:10000'=@('aff2c1e84b36f275','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','10000','1148190720','1144258560','1142947840','1065353216','1065353216','1','1','0','0')
 'warm_repeat:10000'=@('2c5baca41aed63fc','7696ea08a6c5e369','7696ea08a6c5e369','e40906134ec0e16e','10000','1148190720','1148190720','1142947840','1065353216','1065353216','1','1','0','0')
}

function UInt([string]$value){if($value -cnotmatch '^(?:0|[1-9][0-9]*)$'){throw 'Invalid unsigned integer'};[uint64]::Parse($value,[Globalization.CultureInfo]::InvariantCulture)}
function HexUInt([string]$value){[uint64]::Parse($value,[Globalization.NumberStyles]::HexNumber,[Globalization.CultureInfo]::InvariantCulture)}
function PositiveFloatBits([uint64]$value){if($value -gt [uint32]::MaxValue){throw 'Float bits overflow'};$decoded=[BitConverter]::ToSingle([BitConverter]::GetBytes([uint32]$value),0);if([single]::IsNaN($decoded) -or [single]::IsInfinity($decoded) -or $decoded -le 0){throw 'Invalid positive finite float bits'}}
function Stats($values){$sorted=@($values|Sort-Object);[ordered]@{p50_ns=$sorted[9];p95_ns=$sorted[18];max_ns=$sorted[19]}}
$records=[Collections.Generic.List[object]]::new();$seen=[Collections.Generic.HashSet[string]]::new()
foreach($line in Get-Content -LiteralPath $LogPath){
 if($line -notmatch 'C2D_LIST_RESULT '){continue}
 $body=$line.Substring($line.IndexOf('C2D_LIST_RESULT ')+16).Trim();$fields=@{}
 foreach($token in $body.Split(' ',[StringSplitOptions]::RemoveEmptyEntries)){
  if($token -cnotmatch '^([a-z_]+)=(\S+)$'){throw 'Malformed field'}
  if($fields.ContainsKey($Matches[1])){throw 'Duplicate field'};$fields[$Matches[1]]=$Matches[2]
 }
 if(@($fields.Keys).Count -ne $required.Count){throw 'Wrong field inventory'}
 foreach($key in $required){if(-not $fields.ContainsKey($key)){throw "Missing $key"}}
 foreach($key in $fixed.Keys){if($fields[$key] -cne $fixed[$key]){throw "Protocol mismatch $key"}}
 $n=UInt $fields.count;$case=$fields.case;$id="${case}:$n"
 if($n -notin $counts -or $case -cnotin $cases -or -not $seen.Add($id)){throw 'Unexpected or duplicate case'}
 $binding=$bindings[$id]
 if($fields.fixture_signature -cne $binding[0]){throw 'Independent fixture signature mismatch'}
 $record=[ordered]@{};foreach($key in @($fixed.Keys)+$variable){$record[$key]=$fields[$key]};$record.count=$n
 foreach($key in $arrays){
  if($fields[$key] -cnotmatch '^\[([^\[\] ]+)\]$'){throw 'Malformed array'}
  $items=$Matches[1].Split(',');if($items.Count -ne 20){throw 'Wrong sample count'}
  if($key -cin $numeric){$record[$key]=@($items|ForEach-Object {UInt $_})}
  elseif($key -cin $identities){if(@($items|Where-Object {$_ -cnotmatch '^[0-9a-f]{16}$'}).Count){throw 'Malformed identity'};$record[$key]=$items}
  else{foreach($item in $items){$parts=$item.Split(':');if($parts.Count -ne 24){throw 'Wrong receipt arity'};foreach($part in $parts){$null=UInt $part}};$record[$key]=$items}
 }
 $fresh=$case -ceq 'fresh_context_first_open';$warm=$case -ceq 'warm_repeat';$font=$case -ceq 'font_atlas_replacement'
 $replace=$case -cin @('shared_prefix_5_percent','shared_prefix_20_percent','identical_presentation_new_generation','last_row_text_edit','first_row_width_expansion_no_wrap')
 $cold=if($warm){0}else{1};$delta=if($replace){1}else{0}
 for($i=0;$i -lt 20;$i++){
  $expected=@{geometry_rebuilds=$cold;geometry_measured_rows=($cold*$n);geometry_measured_cells=0;presentation_prefix_len=(UInt $binding[4])}
  foreach($key in $expected.Keys){if($record[$key][$i] -ne $expected[$key]){throw "Work mismatch $key"}}
  if($record.elapsed_ns[$i] -eq 0 -or $record.geometry_ns[$i] -gt $record.elapsed_ns[$i]){throw 'Invalid owner/geometry interval'}
  if(($warm -and $record.geometry_ns[$i] -ne 0) -or (-not $warm -and $record.geometry_ns[$i] -eq 0)){throw 'Missing or stale geometry timing'}
  if($record.painted_rows[$i] -eq 0 -or $record.painted_rows[$i] -gt $n){throw 'Invalid painted count'}
  foreach($key in @('geometry_capacity_before_bytes','geometry_capacity_after_bytes')){if($record[$key][$i] % 4 -ne 0){throw 'Invalid f32 geometry capacity'}}
  if($record.geometry_capacity_after_bytes[$i] -lt (24*$n)){throw 'Missing complete geometry capacity'}
  if($fresh -and $record.geometry_capacity_before_bytes[$i] -ne 0){throw 'First-open geometry was prepared'}
  if(-not $fresh -and $record.geometry_capacity_before_bytes[$i] -lt (24*$n)){throw 'Missing settled old geometry capacity'}
  if($warm -and $record.geometry_capacity_before_bytes[$i] -ne $record.geometry_capacity_after_bytes[$i]){throw 'Warm geometry capacity changed'}
  if($record.action_output_ids[$i] -cne $binding[2] -or $record.presentation_ids[$i] -cne $binding[3]){throw 'Independent Action/presentation mismatch'}
  $receipt=@($record.transition_receipts[$i].Split(':')|ForEach-Object {UInt $_})
  if($receipt[0] -ne (HexUInt $binding[1]) -or $receipt[1] -ne (HexUInt $binding[2])){throw 'Transition Action mismatch'}
  if(([decimal]$receipt[3]-[decimal]$receipt[2]) -ne $delta){throw 'Result generation mismatch'}
  $oldPresent=if($fresh){0}else{1};$keyChanged=if($warm -or $font){0}else{1}
  $flags=@{4=$oldPresent;5=1;6=$keyChanged;7=$oldPresent;8=1}
  foreach($slot in $flags.Keys){if($receipt[$slot] -ne $flags[$slot]){throw 'Invalid key/atlas presence/change'}}
  if($receipt[9] -gt 1 -or (($fresh -or $font) -and $receipt[9] -ne 1) -or ($warm -and $receipt[9] -ne 0)){throw 'Invalid actual atlas transition'}
  if(-not $fresh -and -not $font -and $case -cne 'pixels_per_point_change' -and $receipt[9] -ne 0){throw 'Unexpected atlas replacement in settled-font recipe'}
  PositiveFloatBits $receipt[11];if(-not $fresh){PositiveFloatBits $receipt[10]}
  if($fresh){foreach($slot in @(10,12,14,16,18)){if($receipt[$slot] -ne 0){throw 'Absent old key has nonzero field'}}}
  elseif($receipt[10] -eq 0 -or $receipt[12] -ne (UInt $binding[8]) -or $receipt[16] -ne (UInt $binding[10]) -or $receipt[18] -ne (UInt $binding[12])){throw 'Old effective environment mismatch'}
  if($receipt[11] -eq 0 -or $receipt[13] -ne (UInt $binding[9]) -or $receipt[17] -ne (UInt $binding[11]) -or $receipt[19] -ne (UInt $binding[13])){throw 'New effective environment mismatch'}
  if($receipt[15] -eq 0 -or (-not $fresh -and $receipt[14] -ne $receipt[15])){throw 'Unexpected effective button font'}
  if($receipt[20] -ne (UInt $binding[5]) -or $receipt[21] -ne (UInt $binding[6]) -or $receipt[22] -ne (UInt $binding[7]) -or $receipt[23] -ne (UInt $binding[7])){throw 'Screen control mismatch'}
  # Pinned egui0.27.2 begin_frame_mut rescales prior screen on pending zoom.
  # Default CentralPanel margins are8pt each:944 normal,752 zoom,704 resized.
  $expectedOldWidth=if($fresh){0}else{[BitConverter]::ToUInt32([BitConverter]::GetBytes([single]944),0)}
  $effectiveWidth=if($case -ceq 'pixels_per_point_change'){[single](960/1.25-16)}elseif($case -ceq 'viewport_width_change'){[single](720-16)}else{[single](960-16)}
  $expectedNewWidth=[BitConverter]::ToUInt32([BitConverter]::GetBytes($effectiveWidth),0)
  if($receipt[10] -ne $expectedOldWidth -or $receipt[11] -ne $expectedNewWidth){throw 'Pinned effective available-width mismatch'}
  if($case -cin @('viewport_width_change','pixels_per_point_change')){if($receipt[10] -eq $receipt[11]){throw 'Viewport setter did not change actual width'}}
  elseif(-not $fresh -and $receipt[10] -ne $receipt[11]){throw 'Unexpected actual width transition'}
 }
 foreach($key in $identities){if(@($record[$key]|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled output identity'}}
 if(@($record.transition_receipts|Select-Object -Unique).Count -ne 1){throw 'Unstable controlled transition receipt'}
 $record.statistics=[ordered]@{elapsed_ns=(Stats $record.elapsed_ns);geometry_ns=(Stats $record.geometry_ns)}
 $records.Add($record)
}
if($records.Count -ne ($counts.Count*12)){throw 'Incomplete recipe inventory'}
foreach($n in $counts){foreach($case in $cases){if(-not $seen.Contains("${case}:$n")){throw 'Missing recipe'}}}
$bundle=[ordered]@{schema=1;status='COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$false;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';geometry_capacity_scope='six_list_f32_vector_capacities_only';records=@($records.ToArray())}
[IO.File]::WriteAllText($ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputPath),(ConvertTo-Json -InputObject $bundle -Depth 20))
Write-Output "Collected $($records.Count) C2D recipes; independent review pending"
