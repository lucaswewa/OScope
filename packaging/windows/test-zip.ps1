# The zip's check (P54), without installing the service, which needs an
# administrator: unpacked in a folder of its own, it has its files, its
# server says its version, and serves with the zip's configuration.
#
#   powershell -ExecutionPolicy Bypass -File packaging\windows\test-zip.ps1 ZIP
#
# It ends with "test-zip: ok".
param([Parameter(Mandatory = $true)][string]$Zip)
$ErrorActionPreference = "Stop"
$folder = Join-Path ([IO.Path]::GetTempPath()) ("oscope-zip-" + [Guid]::NewGuid())
Expand-Archive $Zip $folder
try {
    $root = (Get-ChildItem $folder -Directory)[0].FullName
    $wanted = "oscope-server.exe", "oscope.toml", "install.ps1", "uninstall.ps1", "README.txt", "LICENSE"
    foreach ($file in $wanted) {
        if (-not (Test-Path (Join-Path $root $file))) { throw "the zip has no $file" }
    }
    $server = Join-Path $root "oscope-server.exe"
    & $server --version
    # With the zip's configuration, on a port the system picks: it answers.
    $config = Join-Path $root "oscope.toml"
    $log = Join-Path $folder "server.log"
    $process = Start-Process $server -ArgumentList "--config", "`"$config`"", "--port", "0" `
        -PassThru -NoNewWindow -RedirectStandardOutput $log
    try {
        $deadline = (Get-Date).AddSeconds(60)
        $base = $null
        while (-not $base) {
            if ($process.HasExited) { throw "the server exited: $($process.ExitCode)" }
            if ((Get-Date) -gt $deadline) { throw "the server didn't say where it serves in 60 s" }
            $line = if (Test-Path $log) { Get-Content $log -TotalCount 1 } else { $null }
            if ($line -match "serving on (http://\S+)") { $base = $Matches[1] } else { Start-Sleep -Milliseconds 100 }
        }
        $status = (Invoke-WebRequest "$($base)health" -UseBasicParsing).StatusCode
        if ($status -ne 200) { throw "/health: $status" }
        if (-not (Test-Path (Join-Path $root "data"))) { throw "no data folder beside the configuration" }
    } finally {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit()
    }
} finally {
    Remove-Item -Recurse -Force $folder
}
"test-zip: ok"
