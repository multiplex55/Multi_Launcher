param(
 [Parameter(Mandatory=$true)][string]$LogPath,
 [Parameter(Mandatory=$true)][ValidateSet('OriginalLib','Startup','EqualReuse')][string]$Stage,
 [Parameter(Mandatory=$true)][ValidatePattern('^[0-9a-f]{40}$')][string]$Source,
 [Parameter(Mandatory=$true)][string]$SourceManifest
)
$ErrorActionPreference='Stop'
if((git rev-parse HEAD).Trim() -cne $Source -or @(git status --porcelain).Count){throw 'Exact clean committed source required'}
if(Get-Process -Name cargo,cargo-nextest,rustc -ErrorAction SilentlyContinue){throw 'Cargo active'}
if(Test-Path -LiteralPath $LogPath){throw 'Refuse existing log'}
$trackFinalManifest=Get-Content -LiteralPath $SourceManifest -Raw | ConvertFrom-Json
if($trackFinalManifest.source -cne $Source){throw 'Manifest source mismatch'}
foreach($trackFinalEntry in $trackFinalManifest.files.PSObject.Properties){if((Get-FileHash -LiteralPath $trackFinalEntry.Name -Algorithm SHA256).Hash.ToLowerInvariant() -cne $trackFinalEntry.Value){throw 'Pre-run source mismatch'}}
$trackFinalTests=switch($Stage){
 'OriginalLib' {@('gui::note_panel::tests::track_c_benchmark_note_panel_heavy_refresh_owner','gui::search::tests::track_c_benchmark_search_shared_owner','gui::render::tests::track_c_benchmark_root_list_frame_geometry','gui::render::tests::track_c_benchmark_root_grid_frame_geometry','gui::render::tests::track_c_benchmark_root_query_to_usable_cpu_frame','gui::notes_dialog::tests::track_c_benchmark_quick_notes_dialog_geometry','gui::watch::tests::track_c_benchmark_actions_reload_owner','gui::watch::tests::track_c_benchmark_watch_event_finite_burst_owner','gui::watch::tests::track_c_benchmark_watch_event_concurrent_offered_load_owner')}
 'Startup' {@('tests::track_c_benchmark_startup_action_catalog_owner')}
 'EqualReuse' {@('gui::render::tests::track_c2c_benchmark_equal_refresh_reuse')}
}
$trackFinalFilter=($trackFinalTests | ForEach-Object {'test(='+$_+')'}) -join ' | '
$trackFinalTarget=if($Stage -eq 'Startup'){@('--bin','multi_launcher')}else{@('--lib')}
$trackFinalNames=@('MULTI_LAUNCHER_PERF','ML_SKIP_CLIPBOARD_SYNC','ML_TRACK_C_PROFILE','ML_TRACK_C_BENCH_MODE','ML_TRACK_C_SOURCE_SHA');$trackFinalSaved=@{}
foreach($trackFinalName in $trackFinalNames){$trackFinalSaved[$trackFinalName]=[Environment]::GetEnvironmentVariable($trackFinalName,'Process')}
try{
 $env:MULTI_LAUNCHER_PERF='1';$env:ML_SKIP_CLIPBOARD_SYNC='1';$env:ML_TRACK_C_PROFILE='fast-dev';$env:ML_TRACK_C_BENCH_MODE='full';$env:ML_TRACK_C_SOURCE_SHA=$Source
 & cargo nextest run @trackFinalTarget --cargo-profile fast-dev --test-threads 1 --no-fail-fast --run-ignored only --success-output immediate -E $trackFinalFilter *> $LogPath
 if($LASTEXITCODE -ne 0){throw 'Final owner failed; preserve raw log'}
 if((git rev-parse HEAD).Trim() -cne $Source -or @(git status --porcelain).Count){throw 'Worktree changed during execution'}
 foreach($trackFinalEntry in $trackFinalManifest.files.PSObject.Properties){if((Get-FileHash -LiteralPath $trackFinalEntry.Name -Algorithm SHA256).Hash.ToLowerInvariant() -cne $trackFinalEntry.Value){throw 'Post-run source mismatch'}}
 Write-Output "Final $Stage Full PASS; raw log retained; exact counts require collection review"
}finally{foreach($trackFinalName in $trackFinalNames){[Environment]::SetEnvironmentVariable($trackFinalName,$trackFinalSaved[$trackFinalName],'Process')}}
