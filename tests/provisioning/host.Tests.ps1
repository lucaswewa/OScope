#requires -Version 7.4
# Dependency-free contract tests. Never imports WSL or registers a task.
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot '../../scripts/provision-host.psm1') -Force
$script:checks = 0
function Assert($Condition, [string]$Message) { if (!$Condition) { throw $Message }; $script:checks += 1 }
function Reject([scriptblock]$Work, [string]$Message) {
    $rejected = $false
    try { & $Work | Out-Null } catch { $rejected = $true }
    Assert $rejected $Message
}
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$work = Join-Path $repo "artifacts/r10/ps-$([guid]::NewGuid().ToString('N'))"
$null = [IO.Directory]::CreateDirectory($work)
$id = [guid]::NewGuid().ToString()
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$distro = 'OScope-' + ([guid]$id).ToString('N')
[xml]$xml = New-OScopeTaskXml $distro $sid 'C:\Windows\System32\wsl.exe' $id
Assert ($xml.Task.Triggers.BootTrigger.Enabled -eq 'true') 'Task must trigger at boot'
Assert ($null -eq $xml.Task.Triggers.LogonTrigger) 'An interactive-login trigger cannot qualify startup'
Assert ($xml.Task.Principals.Principal.UserId -eq $sid) 'Task must use the WSL owner'
Assert ($xml.Task.Principals.Principal.LogonType -eq 'Password') 'S4U/interactive login is not this bootstrap policy'
Assert ($xml.Task.Settings.ExecutionTimeLimit -eq 'PT0S') 'Foreground WSL keepalive must not expire'
Assert ($xml.Task.Settings.RestartOnFailure.Count -eq '3') 'Task failure retries must be bounded'
Assert ($xml.Task.Actions.Exec.Arguments -like '* supervise *--engineering') 'Task must keep the Linux supervisor foreground'
Reject { New-OScopeTaskXml $distro 'S-1-5-18' 'C:\Windows\System32\wsl.exe' $id } 'SYSTEM must not adopt the user distro'
$tasksBefore = @(Get-ScheduledTask | Where-Object TaskName -Like 'OScope-*' | ForEach-Object { $_.TaskPath + $_.TaskName } | Sort-Object) -join ','
$scheduler = New-Object -ComObject 'Schedule.Service'
try {
    $scheduler.Connect()
    $definition = $scheduler.NewTask(0)
    $definition.XmlText = $xml.OuterXml
    Assert ($definition.Triggers.Count -eq 1 -and $definition.Triggers.Item(1).Type -eq 8) 'Native task API must accept exactly one boot trigger'
    Assert ($definition.Principal.LogonType -eq 1) 'Native task API must retain password logon'
} finally { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($scheduler) }
$tasksAfter = @(Get-ScheduledTask | Where-Object TaskName -Like 'OScope-*' | ForEach-Object { $_.TaskPath + $_.TaskName } | Sort-Object) -join ','
Assert ($tasksBefore -eq $tasksAfter) 'Task XML validation must not register or change a task'

$state = @{ completed = @(); current_step = $null }
$statePath = Join-Path $work 'state.json'
$observed = @{}
$events = [Collections.Generic.List[string]]::new()
$inject = @{ fail = $true }
$driver = {
    param($step, $verify)
    if ($verify) { return $observed.ContainsKey($step) }
    $events.Add($step)
    if ($step -eq 'payload' -and $inject.fail) { throw 'Injected interruption after import' }
    $observed[$step] = $true
}
Reject { Invoke-OScopeSteps $state $statePath $driver } 'Interrupted payload must fail'
$saved = Get-Content $statePath -Raw | ConvertFrom-Json
Assert (@($saved.completed).Count -eq 1 -and $saved.completed[0] -eq 'import') 'Failed step must not advance receipt'
$inject.fail = $false
Invoke-OScopeSteps $state $statePath $driver
Assert ($state.completed.Count -eq 4) 'Resume must complete remaining steps'
Assert (@($events | Where-Object { $_ -eq 'import' }).Count -eq 1) 'Resume must not reimport owned distro'
$before = $events.Count
Invoke-OScopeSteps $state $statePath $driver
Assert ($events.Count -eq $before) 'Repeated apply must verify without repeating completed mutations'

$firstPath = Join-Path $work 'first.credential.xml'
$secondPath = Join-Path $work 'second.credential.xml'
$first = New-OScopeOperatorCredential $firstPath
$second = New-OScopeOperatorCredential $secondPath
Assert ($first.GetNetworkCredential().Password -ne $second.GetNetworkCredential().Password) 'Distinct installs need distinct bootstrap credentials'
Assert (!(Get-Content $firstPath -Raw).Contains($first.GetNetworkCredential().Password)) 'Plaintext credential must not be serialized'
$hash = (Get-FileHash $firstPath).Hash
$again = New-OScopeOperatorCredential $firstPath
Assert ($again.GetNetworkCredential().Password -eq $first.GetNetworkCredential().Password) 'Resume must preserve the bootstrap secret'
Assert ((Get-FileHash $firstPath).Hash -eq $hash) 'Resume must not rewrite credential handoff'

$baselinePath = Join-Path $repo 'configs/host-baseline.example.json'
$candidate = Get-Content $baselinePath -Raw | ConvertFrom-Json -AsHashtable
$observedPins = @{ build='fixture'; wsl_version='fixture'; usbipd_version='fixture'; gpu_driver='fixture'; free_gib=128; host_fingerprint=('a' * 64) }
$installTarget = Join-Path $env:TEMP "oscope-provision-test-$([guid]::NewGuid().ToString('N'))"
$plan = Get-OScopePlan $baselinePath $work $installTarget 'fixture-instrument' 'fixture-pc' 8220 $observedPins
Assert (!$plan.ready -and $plan.issues.Count -gt 0) 'Unconfigured example cannot install'
Assert (!(Test-Path $installTarget)) 'Planning must not create an installation'
Reject { Invoke-OScopeProvision $plan $null -Engineering } 'Incomplete plan must not mutate a host'
Reject { Get-OScopePlan $baselinePath $work $installTarget '../foreign' 'fixture-pc' 8220 $observedPins } 'Reject unsafe scanned identity'
$foreign = Join-Path $env:TEMP "oscope-foreign-test-$([guid]::NewGuid().ToString('N'))"
$null = [IO.Directory]::CreateDirectory($foreign)
[IO.File]::WriteAllText((Join-Path $foreign 'keep.txt'), 'existing data')
Reject { Get-OScopePlan $baselinePath $work $foreign 'fixture' 'pc' 8220 $observedPins } 'Never adopt an unowned directory'
Assert ((Get-Content (Join-Path $foreign 'keep.txt')) -eq 'existing data') 'Foreign data changed'

foreach ($key in @($candidate.windows.Keys)) { $candidate.windows[$key] = 'fixture' }
foreach ($key in @($candidate.linux.Keys)) { if (!$candidate.linux[$key]) { $candidate.linux[$key] = 'fixture' } }
[IO.File]::WriteAllText((Join-Path $work 'host-rootfs.tar'), 'fixture-only archive, never imported')
[IO.File]::WriteAllText((Join-Path $work 'backend-image.tar'), 'fixture-only image, never loaded')
foreach ($key in @('rootfs', 'backend')) {
    $candidate.artifacts[$key].sha256 = (Get-FileHash (Join-Path $work $candidate.artifacts[$key].file)).Hash.ToLowerInvariant()
}
$candidate.artifacts.backend.image_id = 'sha256:' + ('b' * 64)
$guardPath = Join-Path $work 'rootfs.audit.json'
Write-OScopeJson $guardPath @{ ready=$true; scope='template_guard_only'; source_sha256=$candidate.artifacts.rootfs.sha256; baseline_id=$candidate.baseline_id; qualification='pending' }
$candidate.artifacts.rootfs.audit_sha256 = (Get-FileHash $guardPath).Hash.ToLowerInvariant()
$filledPath = Join-Path $work 'candidate.json'
Write-OScopeJson $filledPath $candidate
$complete = Get-OScopePlan $filledPath $work $installTarget 'fixture-instrument' 'fixture-pc' 8220 $observedPins
Assert $complete.ready 'Complete fixture plan should pass metadata checks without executing host actions'
$drift = $observedPins.Clone()
$drift.wsl_version = 'different'
Assert (!(Get-OScopePlan $filledPath $work $installTarget 'fixture-instrument' 'fixture-pc' 8220 $drift).ready) 'Host version drift must block apply'
[IO.File]::AppendAllText((Join-Path $work 'backend-image.tar'), 'changed')
Assert (!(Get-OScopePlan $filledPath $work $installTarget 'fixture-instrument' 'fixture-pc' 8220 $observedPins).ready) 'Changed artifact must block apply'
$candidate.windows = @{}
Write-OScopeJson $filledPath $candidate
Reject { Get-OScopePlan $filledPath $work $installTarget 'fixture-instrument' 'fixture-pc' 8220 $observedPins } 'Missing version schema must not bypass preflight'
Write-Host "Host provisioning contracts passed: $script:checks assertions; boot ownership, bounded retries, resume, credential isolation, invalid/foreign plans."
