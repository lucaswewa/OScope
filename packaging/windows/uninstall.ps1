# Removes OScope's service and program (P54, ADR-0033). From an
# administrator's PowerShell:
#
#   powershell -ExecutionPolicy Bypass -File uninstall.ps1
#
# Its configuration and data, in C:\ProgramData\OScope, stay.
$ErrorActionPreference = "Stop"
$program = Join-Path $env:ProgramFiles "OScope"
$server = Join-Path $program "oscope-server.exe"
& $server service uninstall
if ($LASTEXITCODE -ne 0) { throw "oscope-server service uninstall: exit code $LASTEXITCODE" }
Remove-Item -Recurse -Force $program
"OScope's service and program are removed."
"Its configuration and data stay in $(Join-Path $env:ProgramData 'OScope')."
