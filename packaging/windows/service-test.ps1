# Tries OScope's server as a Windows service, from an administrator's
# PowerShell: installs it under a test name, starts it, restarts it through
# the system Thing (exit code 75, which the service manager's recovery
# restarts), stops it, and removes it.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\service-test.ps1
#
# It uses port 8229, the service oscope-test, and C:\ProgramData\oscope-test,
# and removes both. It writes what it did to %TEMP%\oscope-service-test.log.
#Requires -RunAsAdministrator
param(
    [string]$Server = "$PSScriptRoot\..\..\target\debug\oscope-server.exe",
    [string]$Name = "oscope-test",
    [int]$Port = 8229,
    [string]$Log = "$env:TEMP\oscope-service-test.log"
)
$ErrorActionPreference = "Stop"
Start-Transcript -Path $Log -Force | Out-Null
$root = Join-Path $env:ProgramData $Name
$url = "http://127.0.0.1:$Port"
$server = (Resolve-Path $Server).Path

function Wait-Until([string]$what, [scriptblock]$check) {
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        try { if (& $check) { return } } catch {}
        Start-Sleep -Milliseconds 200
    }
    throw "timed out: $what"
}
function Get-ServicePid { (Get-CimInstance Win32_Service -Filter "Name='$Name'").ProcessId }

$ok = $false
try {
    New-Item -ItemType Directory -Force $root | Out-Null
    $config = Join-Path $root "oscope.toml"
    $text = "[server]`nport = $Port`n`n[storage]`ndata_root = `"data`"`nscratch_root = `"scratch`"`n`n" +
        "[devices]`ncamera = `"fake`"`nstage = `"fake`"`nillumination = `"fake`"`nobjective = `"fake`"`n"
    [IO.File]::WriteAllText($config, $text)

    & $server --config $config service install --name $Name
    if ($LASTEXITCODE -ne 0) { throw "install exited with $LASTEXITCODE" }
    Start-Service $Name
    Wait-Until "the service answers" { (Invoke-RestMethod "$url/health").status -eq "ok" }
    $first = Get-ServicePid
    "started: process $first answers on $url"

    Invoke-RestMethod -Method Post "$url/api/v1/system/restart" -ContentType "application/json" -Body "{}" | Out-Null
    Wait-Until "the service manager starts it again" { $now = Get-ServicePid; $now -ne 0 -and $now -ne $first }
    Wait-Until "the new process answers" { (Invoke-RestMethod "$url/health").status -eq "ok" }
    "restarted: process $first exited with 75, and the service manager started $(Get-ServicePid)"

    Stop-Service $Name
    $service = Get-CimInstance Win32_Service -Filter "Name='$Name'"
    if ($service.State -ne "Stopped" -or $service.ExitCode -ne 0) {
        throw "stopped as $($service.State), exit code $($service.ExitCode)"
    }
    "stopped: exit code 0"
    $ok = $true
}
finally {
    & $server service uninstall --name $Name 2>$null | Out-Null
    # The service, as LocalSystem, made its local token readable by itself
    # alone: an administrator takes the folder over first.
    if (Test-Path $root) {
        takeown /f $root /a /r /d Y 2>$null | Out-Null
        icacls $root /grant "*S-1-5-32-544:F" /t /c 2>$null | Out-Null
        Remove-Item -Recurse -Force $root
    }
    if ($ok) { "service-test: ok" } else { "service-test: FAILED" }
    Stop-Transcript | Out-Null
}
