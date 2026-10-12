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
 schema='1';stage='cold_grid_geometry_characterization';installation='eager_grid_geometry'
 fixture='synthetic_root_grid_geometry';owner='RootListGeometryCache::ensure_grid';measurement_scope='installation+egui_Context_run+LauncherApp::render_root_frame'
 source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode.ToLowerInvariant()
 telemetry_enabled='true';observer_enabled='true';warmups='5';samples='20';exclusions='0'
}
$numeric=@('elapsed_ns','geometry_ns','cell_loop_ns','geometry_residual_ns','owner_residual_ns','geometry_rebuilds','geometry_measured_rows','geometry_measured_cells','painted_cells','presentation_prefix_len')
$identities=@('full_output_state_ids','action_output_ids','presentation_ids','geometry_projection_ids','viewport_receipts')
$tuples=@('transition_receipts','storage_receipts')
$timings=@('elapsed_ns','geometry_ns','cell_loop_ns','geometry_residual_ns','owner_residual_ns')
$arrays=$numeric+$identities+$tuples
$variable=@('case','count','columns','fixture_signature')
$required=@($fixed.Keys)+$variable+$arrays
$counts=if($ExpectedMode -eq 'Small'){@(100)}else{@(100,1000,10000)}
# Independent fixture bindings inserted here; never learned from measurements.

# Independently reconstructed before any C2-E benchmark evidence.
$bindings=@{
 'full_geometry_cold:100:3'=@('da104120fad83bc1','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'shared_prefix_5_percent_growth:100:3'=@('77e67587e20c1b48','c3745052c65797f2','c3745052c65797f2','5942fd5866696c37','0d50b2c9c841f532','c3578f264109c20f','5','11215','10989')
 'shared_prefix_20_percent_shrink:100:3'=@('12448b89406e6296','c3745052c65797f2','31384c47c48bcd73','cbfd79be25527b18','7f46a882d995ac55','fd2ad140815d50b0','20','13257','9355')
 'warm_repeat:100:3'=@('7a52b5157331c9e4','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'tall_cell_spill_cold:100:3'=@('19cb892bbfdc9a92','c3745052c65797f2','bffd2147ce71d20c','bffd2147ce71d20c','01a380bc17d6755c','01a380bc17d6755c','100','14332','14332')
 'full_geometry_cold:100:1'=@('45285c79b9bf706f','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'full_geometry_cold:100:2'=@('d951edf2ef3991fc','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'full_geometry_cold:100:5'=@('128be8fb7398ff5b','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'full_geometry_cold:100:6'=@('071ba795a9b33f08','c3745052c65797f2','c3745052c65797f2','c3745052c65797f2','0d50b2c9c841f532','0d50b2c9c841f532','100','11215','11215')
 'full_geometry_cold:1000:3'=@('1570867d820af3f9','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'shared_prefix_5_percent_growth:1000:3'=@('09c7ebe40e3f67d9','9a958b83b7e7345f','9a958b83b7e7345f','25fd47fdfa3acaa8','cadb3adc30a8e158','da1d02589808f4f7','50','113796','93095')
 'shared_prefix_20_percent_shrink:1000:3'=@('f5205747e4a84776','9a958b83b7e7345f','0a1401113e75f9f6','0e813c078f19885f','1559283b6f02a8eb','eaa012d6e5cafc04','200','115838','94666')
 'warm_repeat:1000:3'=@('9a1d588a4ddcdd06','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'tall_cell_spill_cold:1000:3'=@('25fb407864c35d3a','9a958b83b7e7345f','5f7a0e3c32144793','5f7a0e3c32144793','28e660235d8897d8','28e660235d8897d8','1000','116913','116913')
 'full_geometry_cold:1000:1'=@('d32c64afb86df913','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'full_geometry_cold:1000:2'=@('1f1b405da3c9dd2c','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'full_geometry_cold:1000:5'=@('9512f108ed35ff6f','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'full_geometry_cold:1000:6'=@('c50587fbd34a36e8','9a958b83b7e7345f','9a958b83b7e7345f','9a958b83b7e7345f','cadb3adc30a8e158','cadb3adc30a8e158','1000','113796','113796')
 'full_geometry_cold:10000:3'=@('48338505575121ac','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
 'shared_prefix_5_percent_growth:10000:3'=@('a6b441c059219541','7696ea08a6c5e369','7696ea08a6c5e369','ddf61491464330d4','4a1dc19572f6933d','b5885b229885a4ec','500','1142898','917447')
 'shared_prefix_20_percent_shrink:10000:3'=@('1282211a9af76d9f','7696ea08a6c5e369','0774757abfbad898','ea3b61f1e9398694','38ee6bf51ea162de','0111826f28e24f12','2000','1144940','951333')
 'warm_repeat:10000:3'=@('5dc5bbd43fa18f7f','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
 'tall_cell_spill_cold:10000:3'=@('0b4744f162adf743','7696ea08a6c5e369','5bda3f8f16b9f775','5bda3f8f16b9f775','69c15f9789f845ed','69c15f9789f845ed','10000','1146015','1146015')
 'full_geometry_cold:10000:1'=@('8d0b40a3251cfd92','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
 'full_geometry_cold:10000:2'=@('0529638efdeb5411','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
 'full_geometry_cold:10000:5'=@('127ac24886ed21fe','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
 'full_geometry_cold:10000:6'=@('7210cc18629e38fd','7696ea08a6c5e369','7696ea08a6c5e369','7696ea08a6c5e369','4a1dc19572f6933d','4a1dc19572f6933d','10000','1142898','1142898')
}

function Read-U64List([string]$value,[string]$name){
 if($value -notmatch '^\[\d+(?:,\d+){19}\]$'){throw "Invalid 20-value numeric array $name"}
 $items=$value.Substring(1,$value.Length-2).Split(',')
 $values=[System.Collections.Generic.List[uint64]]::new()
 foreach($item in $items){[uint64]$parsed=0;if(-not [uint64]::TryParse($item,[ref]$parsed)){throw "UInt64 range $name"};$values.Add($parsed)}
 return ,$values.ToArray()
}
function Read-IdList([string]$value,[string]$name){
 if($value -notmatch '^\[[0-9a-f]{16}(?:,[0-9a-f]{16}){19}\]$'){throw "Invalid 20-value identity array $name"}
 return ,$value.Substring(1,$value.Length-2).Split(',')
}
function Read-TupleList([string]$value,[string]$name,[int]$width){
 if($value -notmatch '^\[[0-9:,]+\]$'){throw "Invalid tuple syntax $name"}
 $items=$value.Substring(1,$value.Length-2).Split(',');if($items.Count -ne 20){throw "Tuple sample count $name"}
 foreach($item in $items){$fields=$item.Split(':');if($fields.Count -ne $width){throw "Tuple width $name"};foreach($field in $fields){[uint64]$parsed=0;if($field -notmatch '^\d+$' -or -not [uint64]::TryParse($field,[ref]$parsed)){throw "Tuple UInt64 $name"}}}
 return ,$items
}
function Get-Quantiles($values){
 $sorted=@($values|Sort-Object)
 return [ordered]@{p50_ns=$sorted[9];p95_ns=$sorted[18];max_ns=$sorted[19]}
}
function Test-Storage($values,[int]$count,[int]$columns,[bool]$warm){
 [uint64[]]$x=@($values.Split(':')|ForEach-Object {[uint64]$_})
 $rows=[int][Math]::Ceiling($count/[double]$columns);$leaves=1;while($leaves -lt $rows){$leaves*=2}
 [uint64[]]$minimum=@((8*$count),(8*$columns),(8*$rows),(8*$rows),(8*$leaves))
 for($group=0;$group -lt 3;$group++){
  [uint64]$sum=0
  for($component=0;$component -lt 5;$component++){
   [uint64]$capacity=$x[6*$group+$component]
   if($capacity%4 -ne 0 -or $capacity -lt $minimum[$component]){throw 'Storage component bounds'}
   $sum+=$capacity
  }
  if($x[6*$group+5] -ne $sum){throw 'Storage component sum'}
 }
 for($component=0;$component -lt 6;$component++){
  if($warm){if($x[$component] -ne $x[6+$component] -or $x[12+$component] -ne $x[$component]){throw 'Warm storage equality'}}
  elseif($x[12+$component] -ne $x[$component]+$x[6+$component]){throw 'Cold old/new capacity overlap'}
 }
}


function Convert-GridBits([uint64]$value){
 if($value -gt [uint32]::MaxValue){throw 'Float bits exceed UInt32'}
 $number=[BitConverter]::ToSingle([BitConverter]::GetBytes([uint32]$value),0)
 if([float]::IsNaN($number) -or [float]::IsInfinity($number)){throw 'Nonfinite geometry receipt'}
 return $number
}
function Get-GridBits([float]$value){return [uint64][BitConverter]::ToUInt32([BitConverter]::GetBytes($value),0)}
function Test-Transition([string]$value,$record,$binding,[int]$sample){
 [uint64[]]$x=@($value.Split(':')|ForEach-Object {[uint64]$_})
 $n=$record['count'];$columns=$record['columns'];$warm=$record['case'] -ceq 'warm_repeat'
 $delta=if($warm){0}else{1}
 if($x[2] -ne $delta -or $x[1] -ne $x[0]+$delta){throw 'Result-generation transition'}
 if($x[3] -ne $n -or $x[4] -ne $columns){throw 'Transition fixture dimensions'}
 foreach($index in @(5,6,15,16,30,51)){if($x[$index] -ne 1){throw 'Key/grid/atlas presence'}}
 foreach($index in @(11,12,13,14)){if($x[$index] -ne 0){throw 'Wrap or path setting changed'}}
 foreach($pair in @(@(7,8),@(9,10),@(17,18),@(19,20),@(21,22))){if($x[$pair[0]] -ne $x[$pair[1]]){throw 'Layout environment changed'}}
 if($x[9] -ne (Get-GridBits 1) -or $x[17] -ne $columns){throw 'Scale/columns binding'}
 if($x[31] -ne (Get-GridBits 960) -or $x[32] -ne (Get-GridBits 640)){throw 'Raw viewport binding'}
 if($x[28] -ne (Get-GridBits 8) -or $x[29] -ne (Get-GridBits 6)){throw 'Grid spacing binding'}
 $floatIndices=@(7,8,9,10,19,20)+(23..29)+(31..48)
 foreach($index in $floatIndices){$null=Convert-GridBits $x[$index]}
 [float]$width=Convert-GridBits $x[45]
 if($x[45] -ne (Get-GridBits 944) -or $x[8] -ne $x[45]){throw 'Actual available width binding'}
 [float]$nominal=[Math]::Max([float]([float]($width-[float](8*($columns-1)))/[float]$columns),[float]160)
 if($x[20] -ne (Get-GridBits $nominal)){throw 'Nominal cell width formula'}
 if((Convert-GridBits $x[35]) -le (Convert-GridBits $x[33]) -or (Convert-GridBits $x[36]) -le (Convert-GridBits $x[34])){throw 'Viewport bounds'}
 if((Convert-GridBits $x[41]) -lt 0 -or (Convert-GridBits $x[42]) -lt 0 -or (Convert-GridBits $x[43]) -lt 0 -or (Convert-GridBits $x[44]) -lt 0){throw 'Content/offset bounds'}
 $rows=[int][Math]::Ceiling($n/[double]$columns);$leaves=1;while($leaves -lt $rows){$leaves*=2}
 if($x[49] -ne $rows -or $x[50] -ne 2*$leaves){throw 'Rows/spill tree binding'}
 if($x[52] -ne $record['painted_cells'][$sample]){throw 'Paint count receipt binding'}
 if($record['case'] -ceq 'tall_cell_spill_cold' -and (Convert-GridBits $x[44]) -le 0){throw 'Tall spill requires actual scroll'}
}


$raw=Get-Content -LiteralPath $LogPath -Raw -Encoding UTF8
if($raw -notmatch 'Summary\s+\[[^\]]+\]\s+1 test run:\s+1 passed(?:\s|,|$)' -or $raw -match '(?m)^\s*(?:FAIL\s+\[|test result: FAILED)'){throw 'Require terminal owner PASS'}
$prefix='C2E_GRID_RESULT '
$records=[System.Collections.Generic.List[object]]::new()
$seen=[System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
foreach($line in ($raw -split '\r?\n')){
 $position=$line.IndexOf($prefix,[StringComparison]::Ordinal);if($position -lt 0){continue}
 $body=$line.Substring($position+$prefix.Length).Trim()
 $matches=[regex]::Matches($body,'([a-z][a-z0-9_]*)=(\[[^\]]*\]|[^ ]+)')
 $fields=[System.Collections.Generic.Dictionary[string,string]]::new([StringComparer]::Ordinal)
 foreach($match in $matches){if(-not $fields.TryAdd($match.Groups[1].Value,$match.Groups[2].Value)){throw 'Duplicate field'}}
 if(([regex]::Replace($body,'([a-z][a-z0-9_]*)=(\[[^\]]*\]|[^ ]+)','')).Trim().Length -ne 0){throw 'Unparsed record content'}
 if($fields.Count -ne $required.Count){throw 'Field inventory count'}
 foreach($name in $required){if(-not $fields.ContainsKey($name)){throw "Missing field $name"}}
 foreach($name in $fixed.Keys){if($fields[$name] -cne $fixed[$name]){throw "Metadata mismatch $name"}}
 if($fields['count'] -notmatch '^[1-9][0-9]*$' -or $fields['columns'] -notmatch '^[1-9][0-9]*$'){throw 'Integer count/columns'}
 $count=[int]$fields['count'];$columns=[int]$fields['columns'];$case=$fields['case']
 $key=$case+':'+$count+':'+$columns
 if($count -notin $counts -or -not $bindings.ContainsKey($key) -or -not $seen.Add($key)){throw 'Unexpected or duplicate recipe'}
 $binding=$bindings[$key]
 if($fields['fixture_signature'] -cne $binding[0]){throw 'Fixture binding'}
 $record=[ordered]@{}
 foreach($name in (@($fixed.Keys)+$variable)){$record[$name]=$fields[$name]}
 $record['count']=$count;$record['columns']=$columns
 foreach($name in $numeric){$record[$name]=Read-U64List $fields[$name] $name}
 foreach($name in $identities){$record[$name]=Read-IdList $fields[$name] $name}
 $record['transition_receipts']=Read-TupleList $fields['transition_receipts'] 'transition_receipts' 53
 $record['storage_receipts']=Read-TupleList $fields['storage_receipts'] 'storage_receipts' 18
 $warm=$case -ceq 'warm_repeat'
 for($sample=0;$sample -lt 20;$sample++){
  if($record['elapsed_ns'][$sample] -eq 0 -or $record['geometry_ns'][$sample] -gt $record['elapsed_ns'][$sample] -or $record['cell_loop_ns'][$sample] -gt $record['geometry_ns'][$sample]){throw 'Nested timer bounds'}
  if($record['geometry_residual_ns'][$sample] -ne $record['geometry_ns'][$sample]-$record['cell_loop_ns'][$sample] -or $record['owner_residual_ns'][$sample] -ne $record['elapsed_ns'][$sample]-$record['geometry_ns'][$sample]){throw 'Paired timer residual'}
  if($record['geometry_measured_rows'][$sample] -ne 0 -or $record['painted_cells'][$sample] -lt 1 -or $record['painted_cells'][$sample] -gt $count){throw 'Grid work domain'}
  if($warm){if($record['geometry_rebuilds'][$sample] -ne 0 -or $record['geometry_measured_cells'][$sample] -ne 0 -or $record['geometry_ns'][$sample] -ne 0 -or $record['cell_loop_ns'][$sample] -ne 0){throw 'Warm work'}}
  elseif($record['geometry_rebuilds'][$sample] -ne 1 -or $record['geometry_measured_cells'][$sample] -ne $count -or $record['geometry_ns'][$sample] -eq 0 -or $record['cell_loop_ns'][$sample] -eq 0){throw 'Cold work'}
  if($record['presentation_prefix_len'][$sample] -ne [uint64]$binding[6]){throw 'Ordered presentation prefix binding'}
  if($record['action_output_ids'][$sample] -cne $binding[3] -or $record['presentation_ids'][$sample] -cne $binding[5]){throw 'Action/presentation binding'}
  Test-Storage $record['storage_receipts'][$sample] $count $columns $warm
  Test-Transition $record['transition_receipts'][$sample] $record $binding $sample
 }
 $statistics=[ordered]@{};foreach($name in $timings){$statistics[$name]=Get-Quantiles $record[$name]};$record['statistics']=$statistics
 $records.Add($record)
}
$expected=@($bindings.Keys|Where-Object {[int]($_.Split(':')[1]) -in $counts})
if($records.Count -ne $expected.Count){throw 'Incomplete recipe inventory'}
foreach($key in $expected){if(-not $seen.Contains($key)){throw 'Missing recipe'}}
$output=[ordered]@{schema=1;status='C2_E_COLLECTED_PENDING_REVIEW';source=$ExpectedSource;profile='fast-dev';mode=$ExpectedMode;production_optimization=$false;native_display='NOT MEASURED';allocator_and_process_ram='NOT MEASURED';geometry_capacity_scope='partial_nine_grid_f32_vector_capacities_excludes_actions_strings_allocator_rss';records=$records.ToArray()}
$output|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output ('Collected '+$records.Count+' C2E grid recipes; independent review pending')
