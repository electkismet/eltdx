# Run only after the implementation-complete checkpoint. Authoring this script
# is not evidence that its commands have run or that any matrix cell passed.
[CmdletBinding()]
param(
    [ValidateSet('features', 'platform')][string]$Mode = 'platform',
    [string]$Toolchain = '1.89.0',
    [string]$Output = 'artifacts/rust-sdk',
    [switch]$ImplementationComplete
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $ImplementationComplete) { throw 'Finish all T01-T30 implementation sources before running this verifier.' }
$taskRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$taskOutput = [System.IO.Path]::GetFullPath($Output, $taskRoot)
[System.IO.Directory]::CreateDirectory($taskOutput) | Out-Null
$taskEncoding = [System.Text.UTF8Encoding]::new($false)
$taskCommands = [System.Collections.Generic.List[object]]::new()
$taskReport = [ordered]@{
    status = 'running'; mode = $Mode; toolchain = $Toolchain
    system = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    source_sha = $null; worktree_status = @(); lock_sha256 = $null; commands = $taskCommands
}
function Invoke-SdkCargo([string[]]$Arguments, [switch]$Capture) {
    $taskId = '{0:D3}' -f $taskCommands.Count
    $taskLog = Join-Path $taskOutput "$taskId.log"
    $taskStarted = [DateTime]::UtcNow
    $taskLines = @(& cargo "+$Toolchain" @Arguments 2>&1)
    $taskExit = $LASTEXITCODE
    $taskText = ($taskLines | ForEach-Object { $_.ToString() }) -join "`n"
    [System.IO.File]::WriteAllText($taskLog, $taskText + "`n", $taskEncoding)
    $taskCommands.Add([ordered]@{
        executable = 'cargo'; arguments = @("+$Toolchain") + $Arguments; exit_code = $taskExit
        rustdocflags = [System.Environment]::GetEnvironmentVariable('RUSTDOCFLAGS', 'Process')
        started_utc = $taskStarted.ToString('o'); duration_ms = ([DateTime]::UtcNow - $taskStarted).TotalMilliseconds
        log = [System.IO.Path]::GetFileName($taskLog)
        log_sha256 = (Get-FileHash -LiteralPath $taskLog -Algorithm SHA256).Hash.ToLowerInvariant()
    })
    if ($taskExit -ne 0) { throw "cargo failed ($taskExit); see $taskLog" }
    if ($Capture) { return $taskText }
    Write-Host $taskText
}
function Assert-SdkClosure([string[]]$Features) {
    $taskTree = Invoke-SdkCargo -Arguments (@('tree', '-p', 'eltdx', '--locked', '--no-default-features', '--edges', 'normal,build') + $Features) -Capture
    if ($taskTree -match '(?m)(eltdx-python|pyo3(?:-ffi|-build-config)?|maturin)\s+v') {
        throw 'Rust production normal/build closure includes Python discovery or bindings.'
    }
}
Push-Location -LiteralPath $taskRoot
try {
    $taskReport.source_sha = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine candidate SHA.' }
    $taskReport.worktree_status = @(& git status --porcelain)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine worktree status.' }
    $taskReport.lock_sha256 = (Get-FileHash -LiteralPath 'Cargo.lock' -Algorithm SHA256).Hash.ToLowerInvariant()
    Invoke-SdkCargo -Arguments @('--version', '--verbose')
    $taskReport.rustc = @(& rustc "+$Toolchain" -Vv)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine selected rustc version.' }
    if ($Mode -eq 'features') {
        $taskNames = @('serde', 'offline', 'f10', 'ex', 'mac')
        foreach ($taskMask in 0..31) {
            $taskSelected = @()
            foreach ($taskBit in 0..4) {
                if ($taskMask -band (1 -shl $taskBit)) { $taskSelected += $taskNames[$taskBit] }
            }
            $taskFlags = @('--no-default-features')
            if ($taskSelected.Count -gt 0) { $taskFlags += @('--features', ($taskSelected -join ',')) }
            Write-Host "Checking explicit feature subset $taskMask : $($taskSelected -join ',')"
            Invoke-SdkCargo -Arguments (@('check', '-p', 'eltdx', '--all-targets', '--locked') + $taskFlags)
        }
        Invoke-SdkCargo -Arguments @('check', '-p', 'eltdx', '--all-targets', '--locked')
        Invoke-SdkCargo -Arguments @('check', '-p', 'eltdx', '--all-targets', '--locked', '--no-default-features', '--features', 'full')
        Assert-SdkClosure -Features @()
        Assert-SdkClosure -Features @('--features', 'full')
    } else {
        Invoke-SdkCargo -Arguments @('test', '-p', 'eltdx-protocol', '-p', 'eltdx-runtime', '--locked')
        Invoke-SdkCargo -Arguments @('test', '-p', 'eltdx', '--locked', '--no-default-features')
        Invoke-SdkCargo -Arguments @('test', '-p', 'eltdx', '--locked')
        Invoke-SdkCargo -Arguments @('test', '-p', 'eltdx', '--all-targets', '--locked', '--no-default-features', '--features', 'full')
        Invoke-SdkCargo -Arguments @('test', '-p', 'eltdx', '--test', 'serde_contract', '--locked', '--no-default-features', '--features', 'serde')
        $taskPreviousRustdocFlags = [System.Environment]::GetEnvironmentVariable('RUSTDOCFLAGS', 'Process')
        try {
            $env:RUSTDOCFLAGS = (@($taskPreviousRustdocFlags, '-D warnings') | Where-Object { $_ }) -join ' '
            foreach ($taskDocFeatures in @('minimal', 'default', 'serde', 'f10', 'full')) {
                $taskDocFlags = @()
                if ($taskDocFeatures -eq 'minimal') { $taskDocFlags = @('--no-default-features') }
                if ($taskDocFeatures -eq 'full') { $taskDocFlags = @('--no-default-features', '--features', 'full') }
                if ($taskDocFeatures -in @('serde', 'f10')) { $taskDocFlags = @('--no-default-features', '--features', $taskDocFeatures) }
                Invoke-SdkCargo -Arguments (@('test', '-p', 'eltdx', '--doc', '--locked') + $taskDocFlags)
                Invoke-SdkCargo -Arguments (@('doc', '-p', 'eltdx', '--no-deps', '--locked') + $taskDocFlags)
            }
        } finally {
            [System.Environment]::SetEnvironmentVariable('RUSTDOCFLAGS', $taskPreviousRustdocFlags, 'Process')
        }
        Invoke-SdkCargo -Arguments @('clippy', '-p', 'eltdx', '--all-targets', '--locked', '--no-default-features', '--features', 'full', '--', '-D', 'warnings')
        Invoke-SdkCargo -Arguments @('fmt', '--all', '--check')
        Assert-SdkClosure -Features @()
        Assert-SdkClosure -Features @('--features', 'full')
    }
    if ($Mode -eq 'platform') {
        $taskExternal = Join-Path ([System.IO.Path]::GetTempPath()) ('eltdx-rust-consumer-' + [Guid]::NewGuid().ToString('N'))
        [System.IO.Directory]::CreateDirectory($taskExternal) | Out-Null
        Copy-Item -LiteralPath 'tests/consumers/rust-sdk/src' -Destination (Join-Path $taskExternal 'src') -Recurse
        $taskConsumer = Join-Path $taskExternal 'Cargo.toml'
        $taskCandidate = (Join-Path $taskRoot 'crates/eltdx').Replace('\', '/')
        $taskManifest = [System.IO.File]::ReadAllText((Join-Path $taskRoot 'tests/consumers/rust-sdk/Cargo.toml'))
        [System.IO.File]::WriteAllText($taskConsumer, $taskManifest.Replace('../../../crates/eltdx', $taskCandidate), $taskEncoding)
        $taskReport.external_consumer = $taskExternal
        Invoke-SdkCargo -Arguments @('generate-lockfile', '--manifest-path', $taskConsumer)
        Invoke-SdkCargo -Arguments @('run', '--manifest-path', $taskConsumer, '--locked', '--no-default-features', '--', 'lazy')
        Invoke-SdkCargo -Arguments @('run', '--manifest-path', $taskConsumer, '--locked', '--no-default-features', '--', 'loopback')
        foreach ($taskConsumerMode in @('lazy', 'loopback', 'offline', 'financial', 'f10', 'ex', 'mac', 'mac-ex')) {
            Invoke-SdkCargo -Arguments @('run', '--manifest-path', $taskConsumer, '--locked', '--no-default-features', '--features', 'sdk-full', '--', $taskConsumerMode)
        }
        $taskReport.consumer_lock_sha256 = (Get-FileHash -LiteralPath (Join-Path $taskExternal 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $taskReport.status = 'passed'
} catch {
    $taskReport.status = 'failed'
    $taskReport.error = $_.Exception.Message
    throw
} finally {
    [System.IO.File]::WriteAllText((Join-Path $taskOutput 'matrix.json'), ($taskReport | ConvertTo-Json -Depth 12) + "`n", $taskEncoding)
    Pop-Location
}
