# Installs OScope's server as the Windows service `oscope` (P54, ADR-0033),
# from the folder this script is in. From an administrator's PowerShell:
#
#   powershell -ExecutionPolicy Bypass -File install.ps1
#
# The program goes in C:\Program Files\OScope; the configuration and the data
# in C:\ProgramData\OScope, where a configuration already there is kept.
$ErrorActionPreference = "Stop"
$program = Join-Path $env:ProgramFiles "OScope"
$data = Join-Path $env:ProgramData "OScope"
New-Item -ItemType Directory -Force $program, $data | Out-Null
$files = "oscope-server.exe", "LICENSE", "README.txt", "THIRD-PARTY-NOTICES.html",
    "Roboto-LICENSE.txt", "typst-assets-0.15.1-NOTICE.txt"
foreach ($file in $files) {
    $from = Join-Path $PSScriptRoot $file
    if (Test-Path $from) { Copy-Item $from $program -Force }
}
# The web app, beside the server: the one before goes, its files named afresh.
$web = Join-Path $program "web"
if (Test-Path $web) { Remove-Item -Recurse -Force $web }
Copy-Item -Recurse (Join-Path $PSScriptRoot "web") $web
$config = Join-Path $data "oscope.toml"
if (-not (Test-Path $config)) {
    Copy-Item (Join-Path $PSScriptRoot "oscope.toml") $config
}
$server = Join-Path $program "oscope-server.exe"
& $server --config $config service install
if ($LASTEXITCODE -ne 0) { throw "oscope-server service install: exit code $LASTEXITCODE" }
Start-Service oscope
"OScope is installed: the service oscope, serving on http://127.0.0.1:8210/."
"Its configuration and data are in $data."
