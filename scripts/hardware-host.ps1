#requires -Version 7.4
<# Read-only Windows inventory. No SDK loading, USB attachment or port access. #>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Report,
    [switch]$RequireUsbipd
)
$ErrorActionPreference = 'Stop'

function Invoke-InventoryCommand {
    param([string]$Name, [string[]]$Arguments)
    $command = Get-Command -Name $Name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if (!$command) {
        return [ordered]@{ available = $false; exit_code = $null; timed_out = $false; stdout = ''; stderr = 'Not installed or not on PATH' }
    }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $command.Source
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    # WSL's informational output is UTF-16 when redirected on Windows.
    if ($Name -eq 'wsl.exe') {
        $start.StandardOutputEncoding = [Text.Encoding]::Unicode
        $start.StandardErrorEncoding = [Text.Encoding]::Unicode
    }
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        $null = $process.Start()
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        $timedOut = !$process.WaitForExit(15000)
        if ($timedOut) { $process.Kill($true); $process.WaitForExit() }
        return [ordered]@{
            available = $true; exit_code = $process.ExitCode; timed_out = $timedOut
            stdout = $output.GetAwaiter().GetResult().Trim()
            stderr = $errors.GetAwaiter().GetResult().Trim()
        }
    } catch {
        return [ordered]@{ available = $true; exit_code = $null; timed_out = $false; stdout = ''; stderr = $_.Exception.Message }
    } finally { $process.Dispose() }
}

if (!$IsWindows) { throw 'Run hardware-host.ps1 on Windows; use hardware-linux.py in WSL/Linux.' }
$issues = [Collections.Generic.List[string]]::new()
$devices = @()
$osInfo = $null
$gpus = @()
try {
    $osInfo = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture
    $gpus = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion, PNPDeviceID)
    $devices = @(Get-PnpDevice -PresentOnly -ErrorAction Stop |
        Where-Object Class -In @('USB', 'Ports', 'Camera') |
        Select-Object Class, FriendlyName, InstanceId, Status)
} catch { $issues.Add("Windows device inventory is incomplete: $($_.Exception.Message)") }
$usbipdVersion = Invoke-InventoryCommand 'usbipd.exe' @('--version')
$usbipdList = Invoke-InventoryCommand 'usbipd.exe' @('list')
$wslVersion = Invoke-InventoryCommand 'wsl.exe' @('--version')
$wslList = Invoke-InventoryCommand 'wsl.exe' @('--list', '--verbose')
foreach ($entry in @(
    @{ name = 'usbipd version'; result = $usbipdVersion },
    @{ name = 'usbipd list'; result = $usbipdList },
    @{ name = 'WSL version'; result = $wslVersion },
    @{ name = 'WSL distributions'; result = $wslList }
)) {
    if (!$entry.result.available -or $entry.result.exit_code -ne 0 -or $entry.result.timed_out) {
        $issues.Add("$($entry.name): unavailable, failed or timed out")
    }
}
$result = [ordered]@{
    schema_version = 1; scope = 'inventory_only'; run_id = [guid]::NewGuid().ToString()
    collected_at_utc = [DateTimeOffset]::UtcNow.ToString('o')
    collector_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    environment = $osInfo; powershell_version = $PSVersionTable.PSVersion.ToString()
    gpu = $gpus; devices = $devices
    usbipd = @{ version = $usbipdVersion; list = $usbipdList }
    wsl = @{ version = $wslVersion; distributions = $wslList }
    prerequisite_issues = @($issues.ToArray())
    qualification = @{ status = 'not_run'; reason = 'Inventory does not open or qualify microscope hardware' }
}
$path = [IO.Path]::GetFullPath($Report)
$null = [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
$json = ($result | ConvertTo-Json -Depth 12) + "`n"
$bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
$stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
Write-Host "Collected Windows inventory: $path; hardware qualification NOT RUN"
if ($RequireUsbipd -and (
    !$usbipdVersion.available -or $usbipdVersion.exit_code -ne 0 -or $usbipdVersion.timed_out -or
    !$usbipdList.available -or $usbipdList.exit_code -ne 0 -or $usbipdList.timed_out
)) {
    exit 2
}
