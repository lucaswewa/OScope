#requires -Version 7.4
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-OScopeJson {
    param([string]$Path, $Value)
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    try {
        [IO.File]::WriteAllText($temporary, (($Value | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
        [IO.File]::Move($temporary, $Path, $true)
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
}

function Invoke-OScopeNative {
    param([string]$File, [string[]]$Arguments, [string]$InputText, [string]$InputFile,
          [int]$TimeoutSeconds = 120, [switch]$UnicodeOutput)
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $File
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.RedirectStandardInput = $true
    $start.StandardInputEncoding = [Text.UTF8Encoding]::new($false)
    if ($UnicodeOutput) { $start.StandardOutputEncoding = [Text.Encoding]::Unicode }
    foreach ($argument in $Arguments) { $start.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $started = $false
    try {
        $null = $process.Start()
        $started = $true
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        if ($InputFile) {
            $stream = [IO.File]::OpenRead($InputFile)
            try {
                $copy = $stream.CopyToAsync($process.StandardInput.BaseStream)
                if (!$copy.Wait($TimeoutSeconds * 1000)) { throw 'Artifact transfer timed out' }
                $copy.GetAwaiter().GetResult()
            } finally { $stream.Dispose() }
        } elseif ($InputText) { $process.StandardInput.Write($InputText) }
        $process.StandardInput.Close()
        if (!$process.WaitForExit($TimeoutSeconds * 1000)) {
            $process.Kill($true)
            throw 'Provisioning child timed out; installation receipt was not advanced'
        }
        if ($process.ExitCode -ne 0) { throw "Provisioning child exited $($process.ExitCode); receipt was not advanced" }
        # Child stderr may contain authentication information. Never journal/echo it.
        $null = $errors.GetAwaiter().GetResult()
        return $output.GetAwaiter().GetResult().Trim()
    } finally {
        if ($started -and !$process.HasExited) { $process.Kill($true) }
        $process.Dispose()
    }
}

function Test-OScopeRoot {
    param([string]$Path)
    if (!$IsWindows -or $Path -notmatch '^[A-Za-z]:\\' -or $Path -match '[\r\n"]') { throw 'Use an absolute local Windows install directory' }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\')
    if ($full.Length -le 3) { throw 'A drive root is not an installation directory' }
    $parent = $full
    while ($parent) {
        if (Test-Path -LiteralPath $parent) {
            if ((Get-Item -LiteralPath $parent -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'Install root must not traverse a reparse point'
            }
        }
        $parent = [IO.Path]::GetDirectoryName($parent)
    }
    return $full
}

function Get-OScopePlan {
    param([string]$BaselinePath, [string]$BundleDirectory, [string]$InstallRoot,
          [string]$InstrumentSerial, [string]$PcSerial, [int]$Port = 8210, [hashtable]$Observed)
    $root = Test-OScopeRoot $InstallRoot
    foreach ($value in @($InstrumentSerial, $PcSerial)) {
        if ($value -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$') { throw 'Invalid scanned instrument/PC serial' }
    }
    if ($Port -lt 1024 -or $Port -gt 65535) { throw 'Invalid localhost port' }
    $baseline = Get-Content -LiteralPath $BaselinePath -Raw | ConvertFrom-Json -AsHashtable
    foreach ($table in @{
        root = @('schema_version', 'baseline_id', 'qualification', 'minimum_free_gib', 'windows', 'linux', 'artifacts')
        windows = @('build', 'wsl_version', 'usbipd_version', 'gpu_driver')
        linux = @('ubuntu_release', 'kernel_release', 'docker_version', 'compose_version', 'nvidia_ctk_version')
        artifacts = @('rootfs', 'backend')
    }.GetEnumerator()) {
        $actual = if ($table.Key -eq 'root') { $baseline } else { $baseline[$table.Key] }
        if ($actual -isnot [hashtable] -or (($actual.Keys | Sort-Object) -join ',') -ne (($table.Value | Sort-Object) -join ',')) {
            throw "Baseline schema differs: $($table.Key)"
        }
    }
    if (($baseline.schema_version -isnot [long] -and $baseline.schema_version -isnot [int]) -or $baseline.schema_version -ne 1 -or $baseline.qualification -ne 'pending' -or
        $baseline.baseline_id -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$') {
        throw 'R10 supports explicitly unqualified engineering candidate baselines only'
    }
    $issues = [Collections.Generic.List[string]]::new()
    foreach ($table in @('windows', 'linux')) {
        foreach ($key in $baseline[$table].Keys) {
            if (!$baseline[$table][$key]) { $issues.Add("Missing pin: $table.$key") }
        }
    }
    $files = @{}
    foreach ($name in @('rootfs', 'backend')) {
        $artifact = $baseline.artifacts[$name]
        if ($artifact.file -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') { throw 'Artifact paths must be local leaf filenames' }
        $path = Join-Path ([IO.Path]::GetFullPath($BundleDirectory)) $artifact.file
        $files[$name] = $path
        if ($artifact.sha256 -notmatch '^[0-9a-f]{64}$' -or !(Test-Path -LiteralPath $path -PathType Leaf)) {
            $issues.Add("Missing artifact/pin: $name")
        } elseif ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $artifact.sha256) {
            $issues.Add("Artifact hash mismatch: $name")
        }
    }
    $audit = $baseline.artifacts.rootfs
    if ($audit.audit_file -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') { throw 'Audit path must be a leaf filename' }
    $auditPath = Join-Path ([IO.Path]::GetFullPath($BundleDirectory)) $audit.audit_file
    if ($audit.audit_sha256 -notmatch '^[0-9a-f]{64}$' -or !(Test-Path -LiteralPath $auditPath -PathType Leaf)) {
        $issues.Add('Missing template guard report/pin')
    } elseif ((Get-FileHash -LiteralPath $auditPath).Hash.ToLowerInvariant() -ne $audit.audit_sha256) {
        $issues.Add('Template guard report hash mismatch')
    } else {
        $guard = Get-Content -LiteralPath $auditPath -Raw | ConvertFrom-Json
        if ($guard.ready -isnot [bool] -or !$guard.ready -or $guard.scope -ne 'template_guard_only' -or
            $guard.source_sha256 -ne $audit.sha256 -or $guard.baseline_id -ne $baseline.baseline_id) {
            $issues.Add('Template guard does not match this clean candidate artifact')
        }
    }
    if ($baseline.artifacts.backend.image_id -notmatch '^sha256:[0-9a-f]{64}$') { $issues.Add('Missing immutable backend image ID') }
    if ($baseline.minimum_free_gib -lt 16 -or $baseline.minimum_free_gib -gt 4096) { throw 'Invalid storage requirement' }
    if (!$Observed) {
        $wsl = Invoke-OScopeNative 'wsl.exe' @('--version') -UnicodeOutput
        $usbipd = Get-Command usbipd.exe -ErrorAction SilentlyContinue
        $usbVersion = if ($usbipd) { Invoke-OScopeNative $usbipd.Source @('--version') } else { '' }
        $driver = @(Get-CimInstance Win32_VideoController | Where-Object Name -Like '*NVIDIA*' | Select-Object -ExpandProperty DriverVersion -Unique)
        $Observed = @{
            build = "$([Environment]::OSVersion.Version.Build).$((Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').UBR)"
            wsl_version = [regex]::Match($wsl, '\d+\.\d+\.\d+\.\d+').Value
            usbipd_version = [regex]::Match($usbVersion, '\d+\.\d+\.\d+(?:\.\d+)?').Value
            gpu_driver = $driver -join ','
            free_gib = [math]::Floor(([IO.DriveInfo]::new([IO.Path]::GetPathRoot($root))).AvailableFreeSpace / 1GB)
            host_fingerprint = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes(
                (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Cryptography').MachineGuid + ':' + (Get-CimInstance Win32_BIOS).SerialNumber))).ToLowerInvariant()
        }
    }
    foreach ($key in $baseline.windows.Keys) {
        if (!$Observed.ContainsKey($key) -or $Observed[$key] -ne $baseline.windows[$key]) { $issues.Add("Host version differs: $key") }
    }
    if ($Observed.free_gib -lt $baseline.minimum_free_gib) { $issues.Add('Insufficient physical disk space') }
    $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    if ($sid -notmatch '^S-1-5-21-[0-9-]+$') { throw 'Use the dedicated local/domain Windows account, not SYSTEM/service/AAD registration' }
    $statePath = Join-Path $root 'install-state.json'
    $baselineHash = (Get-FileHash -LiteralPath $BaselinePath).Hash.ToLowerInvariant()
    $runtimeHash = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'provision-runtime.py')).Hash.ToLowerInvariant()
    if (Test-Path -LiteralPath $statePath) {
        $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
        if ($state.owner_sid -ne $sid -or $state.instrument_serial -ne $InstrumentSerial -or
            $state.pc_serial -ne $PcSerial -or $state.baseline_sha256 -ne $baselineHash -or
            $state.runtime_sha256 -ne $runtimeHash -or $state.host_fingerprint -ne $Observed.host_fingerprint) { throw 'Existing installation belongs to another identity, PC, baseline or tool revision' }
        $installation = $state.installation_id
    } else {
        if ((Test-Path -LiteralPath $root) -and @(Get-ChildItem -LiteralPath $root -Force).Count) { throw 'Unowned nonempty install root' }
        $installation = [guid]::NewGuid().ToString()
    }
    return @{
        schema_version = 1; qualification = 'pending'; scope = 'engineering_provisioning'; ready = ($issues.Count -eq 0)
        issues = @($issues.ToArray()); installation_id = $installation; owner_sid = $sid; root = $root
        instrument_serial = $InstrumentSerial; pc_serial = $PcSerial; baseline = $baseline; files = $files
        baseline_sha256 = $baselineHash; runtime_sha256 = $runtimeHash; port = $Port; observed = $Observed
        host_fingerprint = $Observed.host_fingerprint
        distro = 'OScope-' + ([guid]$installation).ToString('N')
        steps = @('import', 'payload', 'runtime', 'startup')
    }
}

function New-OScopeTaskXml {
    param([string]$Distro, [string]$OwnerSid, [string]$WslPath, [string]$InstallationId)
    if ($Distro -notmatch '^OScope-[0-9a-f]{32}$' -or $OwnerSid -notmatch '^S-1-5-21-[0-9-]+$') { throw 'Invalid startup task ownership' }
    if (([guid]$InstallationId).ToString() -cne $InstallationId -or $WslPath -notmatch '^[A-Za-z]:\\.*\\wsl\.exe$') { throw 'Invalid task installation/executable' }
    $command = [Security.SecurityElement]::Escape($WslPath)
    $arguments = "--distribution $Distro --user root --exec /usr/bin/python3 /opt/oscope-host/provision-runtime.py supervise --root /var/lib/oscope-host --engineering"
    return @"
<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>OScope engineering installation $InstallationId</Description></RegistrationInfo>
  <Triggers><BootTrigger><Enabled>true</Enabled><Delay>PT30S</Delay></BootTrigger></Triggers>
  <Principals><Principal id="Owner"><UserId>$OwnerSid</UserId><LogonType>Password</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>true</StartWhenAvailable><ExecutionTimeLimit>PT0S</ExecutionTimeLimit><RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure></Settings>
  <Actions Context="Owner"><Exec><Command>$command</Command><Arguments>$arguments</Arguments></Exec></Actions>
</Task>
"@
}

function Invoke-OScopeSteps {
    param([hashtable]$State, [string]$StatePath, [scriptblock]$Driver)
    foreach ($step in @('import', 'payload', 'runtime', 'startup')) {
        if (!(& $Driver $step $true)) {
            $State.current_step = $step
            Write-OScopeJson $StatePath $State
            $null = & $Driver $step $false
            if (!(& $Driver $step $true)) { throw "Read-back verification failed: $step" }
        }
        if ($step -notin $State.completed) { $State.completed = @($State.completed) + $step }
        $State.current_step = $null
        Write-OScopeJson $StatePath $State
    }
}

function New-OScopeOperatorCredential {
    param([string]$Path)
    if (!$IsWindows) { throw 'Credential handoff requires Windows DPAPI; plaintext cross-platform serialization is not allowed' }
    if (!(Test-Path -LiteralPath $Path)) {
        $secret = [Convert]::ToBase64String([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))
        [pscredential]::new('operator', (ConvertTo-SecureString $secret -AsPlainText -Force)) | Export-Clixml -LiteralPath $Path -NoClobber
        $secret = $null
    }
    $credential = Import-Clixml -LiteralPath $Path
    if ($credential -isnot [pscredential] -or $credential.UserName -ne 'operator') { throw 'Protected bootstrap credential does not match' }
    return $credential
}

function Invoke-OScopeProvision {
    param([hashtable]$Plan, [pscredential]$Credential, [switch]$Engineering)
    if (!$Engineering -or !$Plan.ready) { throw 'Explicit engineering mode and a fully pinned, matching candidate are required' }
    if (([Security.Principal.NTAccount]::new($Credential.UserName)).Translate([Security.Principal.SecurityIdentifier]).Value -ne $Plan.owner_sid) {
        throw 'Task credentials must be for the Windows identity that owns this WSL distribution'
    }
    $key = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Plan.root.ToLowerInvariant())))
    $mutex = [Threading.Mutex]::new($false, "Local\OScopeProvision-$key")
    if (!$mutex.WaitOne(0)) { $mutex.Dispose(); throw 'This installation is already being provisioned' }
    try {
    $null = Test-OScopeRoot $Plan.root
    $statePath = Join-Path $Plan.root 'install-state.json'
    if ((Test-Path -LiteralPath $Plan.root) -and !(Test-Path -LiteralPath $statePath) -and @(Get-ChildItem -LiteralPath $Plan.root -Force).Count) {
        throw 'Install directory became nonempty after planning; refusing to adopt it'
    }
    $null = [IO.Directory]::CreateDirectory($Plan.root)
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($sid in @($Plan.owner_sid, 'S-1-5-18', 'S-1-5-32-544')) {
        $rule = [Security.AccessControl.FileSystemAccessRule]::new([Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Plan.root -AclObject $acl
    $statePath = Join-Path $Plan.root 'install-state.json'
    if (Test-Path -LiteralPath $statePath) {
        $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json -AsHashtable
        foreach ($key in @('installation_id', 'owner_sid', 'instrument_serial', 'pc_serial', 'baseline_sha256', 'runtime_sha256', 'host_fingerprint')) {
            if ($state[$key] -ne $Plan[$key]) { throw 'Installation changed after planning; refusing to replace it' }
        }
    }
    else {
        $state = @{ installation_id = $Plan.installation_id; owner_sid = $Plan.owner_sid; instrument_serial = $Plan.instrument_serial;
                    pc_serial = $Plan.pc_serial; baseline_sha256 = $Plan.baseline_sha256; runtime_sha256 = $Plan.runtime_sha256; host_fingerprint = $Plan.host_fingerprint;
                    completed = @(); current_step = $null }
        Write-OScopeJson $statePath $state
    }
    $handoff = Join-Path $Plan.root 'operator.credential.xml'
    $operator = New-OScopeOperatorCredential $handoff
    $identity = @{ installation_id = $Plan.installation_id; instrument_serial = $Plan.instrument_serial; pc_serial = $Plan.pc_serial;
                   baseline_id = $Plan.baseline.baseline_id; image_id = $Plan.baseline.artifacts.backend.image_id; port = $Plan.port; compute = 'cuda'; linux_versions = $Plan.baseline.linux }
    $identityText = $identity | ConvertTo-Json -Compress
    $wsl = (Get-Command wsl.exe -CommandType Application).Source
    $taskName = 'OScope-' + ([guid]$Plan.installation_id).ToString('N')
    $runtimePath = Join-Path $PSScriptRoot 'provision-runtime.py'
    $driver = {
        param($step, $Verify)
        $exec = @('--distribution', $Plan.distro, '--user', 'root', '--exec')
        switch ($step) {
            import {
                $names = Invoke-OScopeNative $wsl @('--list', '--quiet') -UnicodeOutput
                $exists = $Plan.distro -in @($names -split '\r?\n' | ForEach-Object Trim)
                if ($Verify) {
                    if ($exists -and !(Test-Path -LiteralPath (Join-Path $Plan.root 'wsl/ext4.vhdx'))) { throw 'Distro name collision outside the owned install directory' }
                    return $exists
                }
                if ($exists) { throw 'Refusing to replace an existing distribution' }
                if ((Get-FileHash -LiteralPath $Plan.files.rootfs).Hash.ToLowerInvariant() -ne $Plan.baseline.artifacts.rootfs.sha256) { throw 'Template artifact changed after planning' }
                $null = Invoke-OScopeNative $wsl @('--import', $Plan.distro, (Join-Path $Plan.root 'wsl'), $Plan.files.rootfs, '--version', '2') -TimeoutSeconds 600
            }
            payload {
                if ($Verify) {
                    if ('payload' -notin $state.completed) { return $false }
                    $probe = 'import hashlib,json,pathlib,platform,re,subprocess; run=lambda a:subprocess.check_output(a,text=True).strip(); osr=pathlib.Path("/etc/os-release").read_text(); print(json.dumps({"ubuntu_release":re.search(r"(?m)^VERSION_ID=\"?([^\"\n]+)",osr)[1],"kernel_release":platform.release(),"docker_version":run(["docker","version","--format","{{.Server.Version}}"]),"compose_version":run(["docker","compose","version","--short"]),"nvidia_ctk_version":re.search(r"\d+\.\d+\.\d+",run(["nvidia-ctk","--version"]))[0],"runtime_sha256":hashlib.sha256(pathlib.Path("/opt/oscope-host/provision-runtime.py").read_bytes()).hexdigest()}))'
                    $actual = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/python3', '-c', $probe)) | ConvertFrom-Json -AsHashtable
                    foreach ($key in $Plan.baseline.linux.Keys) {
                        if ($actual[$key] -ne $Plan.baseline.linux[$key]) { throw "Imported Linux baseline differs: $key" }
                    }
                    if ($actual.runtime_sha256 -ne $Plan.runtime_sha256) { throw 'Imported runtime tool differs' }
                    return $true
                }
                if ((Get-FileHash -LiteralPath $runtimePath).Hash.ToLowerInvariant() -ne $Plan.runtime_sha256) { throw 'Runtime tool changed after planning' }
                if ((Get-FileHash -LiteralPath $Plan.files.backend).Hash.ToLowerInvariant() -ne $Plan.baseline.artifacts.backend.sha256) { throw 'Backend artifact changed after planning' }
                $script = [Convert]::ToBase64String([IO.File]::ReadAllBytes($runtimePath))
                $copy = 'import base64,pathlib,sys; p=pathlib.Path("/opt/oscope-host/provision-runtime.py"); p.parent.mkdir(parents=True,exist_ok=True); p.write_bytes(base64.b64decode(sys.stdin.read())); p.chmod(0o755)'
                $null = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/python3', '-c', $copy)) -InputText $script
                $null = Invoke-OScopeNative $wsl ($exec + @('/bin/systemctl', 'start', 'docker'))
                $null = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/docker', 'load')) -InputFile $Plan.files.backend -TimeoutSeconds 600
                $copyIdentity = 'import pathlib,sys; p=pathlib.Path("/opt/oscope-host/identity.json"); p.write_text(sys.stdin.read(),encoding="utf-8"); p.chmod(0o600)'
                $null = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/python3', '-c', $copyIdentity)) -InputText $identityText
                $state.completed = @($state.completed) + 'payload'
            }
            runtime {
                if ($Verify) {
                    if ('runtime' -notin $state.completed) { return $false }
                    $null = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/python3', '/opt/oscope-host/provision-runtime.py', 'supervise', '--root', '/var/lib/oscope-host', '--engineering', '--cycles', '1'))
                    return $true
                }
                $null = Invoke-OScopeNative $wsl ($exec + @('/usr/bin/python3', '/opt/oscope-host/provision-runtime.py', 'init', '--root', '/var/lib/oscope-host', '--identity', '/opt/oscope-host/identity.json', '--engineering')) -InputText ($operator.GetNetworkCredential().Password + "`n") -TimeoutSeconds 180
                $state.completed = @($state.completed) + 'runtime'
            }
            startup {
                $existing = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
                if ($existing) {
                    $taskSid = [string]$existing.Principal.UserId
                    if ($taskSid -notmatch '^S-1-') { $taskSid = ([Security.Principal.NTAccount]::new($taskSid)).Translate([Security.Principal.SecurityIdentifier]).Value }
                    $expectedArgs = "--distribution $($Plan.distro) --user root --exec /usr/bin/python3 /opt/oscope-host/provision-runtime.py supervise --root /var/lib/oscope-host --engineering"
                    if ($taskSid -ne $Plan.owner_sid -or $existing.Description -ne "OScope engineering installation $($Plan.installation_id)" -or
                        $existing.Actions.Count -ne 1 -or $existing.Actions[0].Execute -ne $wsl -or $existing.Actions[0].Arguments -ne $expectedArgs -or
                        [string]$existing.Principal.LogonType -ne 'Password' -or $existing.Triggers.Count -ne 1 -or
                        $existing.Triggers[0].CimClass.CimClassName -ne 'MSFT_TaskBootTrigger' -or !$existing.Triggers[0].Enabled -or
                        !$existing.Settings.Enabled -or $existing.Settings.ExecutionTimeLimit -ne 'PT0S' -or $existing.Settings.RestartCount -ne 3 -or
                        $existing.Settings.RestartInterval -ne 'PT1M' -or [string]$existing.Settings.MultipleInstances -ne 'IgnoreNew') { throw 'Foreign or modified startup task; refusing replacement' }
                    return $true
                }
                if ($Verify) { return $false }
                $xml = New-OScopeTaskXml $Plan.distro $Plan.owner_sid $wsl $Plan.installation_id
                $null = Register-ScheduledTask -TaskName $taskName -Xml $xml -User $Credential.UserName -Password $Credential.GetNetworkCredential().Password
                Start-ScheduledTask -TaskName $taskName
            }
        }
    }
    Invoke-OScopeSteps $state $statePath $driver
    return @{ installation_id = $Plan.installation_id; state = 'engineering_installed'; qualification = 'pending'; distro = $Plan.distro; task = $taskName }
    } finally { $mutex.ReleaseMutex(); $mutex.Dispose() }
}

Export-ModuleMember -Function Get-OScopePlan, New-OScopeTaskXml, Invoke-OScopeSteps, Invoke-OScopeProvision, Write-OScopeJson, New-OScopeOperatorCredential
