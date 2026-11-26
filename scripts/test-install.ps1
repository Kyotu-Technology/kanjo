# Tests install/install.ps1 in offline mode with stub cosign and minisign
# commands, in a temporary directory, with -NoPathUpdate so the user PATH is
# never changed. Runs the installer in the same PowerShell edition as this
# script, so run it under both powershell.exe (5.1) and pwsh.
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$installer = Join-Path (Split-Path -Parent $PSScriptRoot) 'install\install.ps1'
$installer = [IO.Path]::GetFullPath($installer.Replace('\', [IO.Path]::DirectorySeparatorChar))
$shell = (Get-Process -Id $PID).Path
$onWindows = [IO.Path]::DirectorySeparatorChar -eq '\'
$sep = [IO.Path]::PathSeparator
$version = '9.9.9'
$name = "kanjo-$version-x86_64-pc-windows-msvc"
$identity = "https://github.com/Kyotu-Technology/kanjo/.github/workflows/release.yml@refs/tags/v$version"
$issuer = 'https://token.actions.githubusercontent.com'
if (-not $env:PROCESSOR_ARCHITECTURE) { $env:PROCESSOR_ARCHITECTURE = 'AMD64' }

$work = Join-Path ([IO.Path]::GetTempPath()) ('kanjo-install-test-' + [guid]::NewGuid().ToString('N'))
$rel = Join-Path $work 'rel'
$stubs = Join-Path $work 'stubs'
New-Item -ItemType Directory -Path (Join-Path $rel $name), $stubs | Out-Null

Set-Content -LiteralPath (Join-Path (Join-Path $rel $name) 'kanjo.exe') -Value 'not a real binary'
$zip = Join-Path $rel "$name.zip"
Compress-Archive -Path (Join-Path $rel $name) -DestinationPath $zip
Remove-Item -LiteralPath (Join-Path $rel $name) -Recurse
$hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
$sums = Join-Path $rel 'SHA256SUMS'
[IO.File]::WriteAllText($sums, "$hash  $name.zip" + [char]10)
Set-Content -LiteralPath "$sums.sigstore.json" -Value '{"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json"}'
Set-Content -LiteralPath "$sums.minisig" -Value 'untrusted comment: test'

if ($onWindows) {
    Set-Content -LiteralPath (Join-Path $stubs 'cosign.cmd') -Encoding ascii -Value @(
        '@echo off'
        'if "%~1"=="version" ('
        '  echo GitVersion:    %STUB_COSIGN_VERSION%'
        '  exit /b 0'
        ')'
        'echo %*>>"%STUB_LOG%\cosign"'
        'if "%STUB_COSIGN_EXIT%"=="0" (echo Verified OK 1>&2) else (echo Error: none of the expected identities matched 1>&2)'
        'exit /b %STUB_COSIGN_EXIT%'
    )
    Set-Content -LiteralPath (Join-Path $stubs 'minisign.cmd') -Encoding ascii -Value @(
        '@echo off'
        'echo %*>>"%STUB_LOG%\minisign"'
        'exit /b %STUB_MINISIGN_EXIT%'
    )
} else {
    Set-Content -LiteralPath (Join-Path $stubs 'cosign') -Encoding ascii -Value @(
        '#!/bin/sh'
        'if [ "$1" = version ]; then printf "GitVersion:    %s\n" "$STUB_COSIGN_VERSION"; exit 0; fi'
        'printf "%s\n" "$*" >>"$STUB_LOG/cosign"'
        'if [ "$STUB_COSIGN_EXIT" = 0 ]; then echo "Verified OK" >&2; else echo "Error: none of the expected identities matched" >&2; fi'
        'exit "$STUB_COSIGN_EXIT"'
    )
    Set-Content -LiteralPath (Join-Path $stubs 'minisign') -Encoding ascii -Value @(
        '#!/bin/sh'
        'printf "%s\n" "$*" >>"$STUB_LOG/minisign"'
        'exit "$STUB_MINISIGN_EXIT"'
    )
    & chmod 0755 (Join-Path $stubs 'cosign') (Join-Path $stubs 'minisign')
}

$script:failures = 0
$script:n = 0
$realPath = $env:PATH

# Runs the installer. $Tools names the stub commands to put on PATH. Returns
# the combined output, the exit code, the install directory and the log directory.
function Invoke-Case([string]$Label, [string[]]$Tools, [hashtable]$Env, [string[]]$Arguments, [switch]$DirOnPath) {
    $script:n++
    $case = Join-Path $work "case$script:n"
    $dir = Join-Path $case 'bin'
    $path = Join-Path $case 'path'
    $log = Join-Path $case 'log'
    New-Item -ItemType Directory -Path $path, $log | Out-Null
    foreach ($s in $Tools) {
        Get-ChildItem -LiteralPath $stubs -Filter "$s*" | Copy-Item -Destination $path
    }
    if (-not $onWindows) { Get-ChildItem -LiteralPath $path | ForEach-Object { & chmod 0755 $_.FullName } }
    $vars = @{ STUB_LOG = $log; STUB_COSIGN_EXIT = '0'; STUB_COSIGN_VERSION = 'v3.1.5'; STUB_MINISIGN_EXIT = '0'; KANJO_MINISIGN_PUBKEY = '' }
    if ($Env) { foreach ($k in $Env.Keys) { $vars[$k] = $Env[$k] } }
    foreach ($k in $vars.Keys) { [Environment]::SetEnvironmentVariable($k, $vars[$k]) }
    $env:PATH = $path + $sep + $realPath
    if ($DirOnPath) { $env:PATH = $dir + $sep + $env:PATH }
    $ErrorActionPreference = 'Continue'
    $out = & $shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $installer -Dir $dir -NoPathUpdate @Arguments 2>&1 | ForEach-Object { "$_" }
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    $env:PATH = $realPath
    foreach ($k in $vars.Keys) { [Environment]::SetEnvironmentVariable($k, $null) }
    return @{ Label = $Label; Out = (@($out) -join [char]10); Code = $code; Dir = $dir; Log = $log }
}

function Fail-Check($R, [string]$Message) {
    $script:failures++
    Write-Host "FAIL [$($R.Label)]: $Message"
    Write-Host "--- output ---"
    Write-Host $R.Out
    Write-Host "--------------"
}
function Expect-Exit($R, [int]$Code) { if ($R.Code -ne $Code) { Fail-Check $R "exit $($R.Code), expected $Code" } }
# PowerShell 7 colours and wraps the text of a thrown error to the console
# width, with "|" at the start of each continuation line, so compare without
# escape sequences, whitespace and bars.
function Compress-Space([string]$Text) { return (($Text -replace '\x1b\[[0-9;]*[A-Za-z]', '') -replace '[\s|]', '') }
function Expect-Out($R, [string]$Text) { if (-not (Compress-Space $R.Out).Contains((Compress-Space $Text))) { Fail-Check $R "output lacks: $Text" } }
function Expect-NoOut($R, [string]$Text) { if ((Compress-Space $R.Out).Contains((Compress-Space $Text))) { Fail-Check $R "output contains: $Text" } }
function Expect-Installed($R) { if (-not (Test-Path -LiteralPath (Join-Path $R.Dir 'kanjo.exe'))) { Fail-Check $R 'kanjo.exe was not installed' } }
function Expect-NotInstalled($R) { if (Test-Path -LiteralPath (Join-Path $R.Dir 'kanjo.exe')) { Fail-Check $R 'kanjo.exe was installed although verification failed' } }
function Get-Log($R, [string]$Tool) {
    $f = Join-Path $R.Log $Tool
    if (Test-Path -LiteralPath $f) { return (Get-Content -LiteralPath $f -Raw) } else { return '' }
}
function Expect-Log($R, [string]$Tool, [string]$Text) { if (-not (Get-Log $R $Tool).Contains($Text)) { Fail-Check $R "$Tool was not called with: $Text" } }
function Expect-NoLog($R, [string]$Tool) { if ((Get-Log $R $Tool) -ne '') { Fail-Check $R "$Tool was called" } }

try {
    if (Get-Command cosign -ErrorAction SilentlyContinue) {
        Write-Host 'test-install: a real cosign is on PATH; skipping the case without cosign'
    } else {
        $r = Invoke-Case 'no cosign' @() $null @('-Archive', $zip)
        Expect-Exit $r 0
        Expect-Installed $r
        Expect-Out $r 'SHA-256 checksum: verified'
        Expect-Out $r 'cosign signature: skipped (cosign is not installed)'
        Expect-Out $r 'WARNING: no signature was verified'
    }

    $r = Invoke-Case 'cosign verifies' @('cosign') $null @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Installed $r
    Expect-Out $r 'cosign signature: verified'
    Expect-NoOut $r 'WARNING: no signature was verified'
    Expect-Log $r 'cosign' 'verify-blob'
    Expect-Log $r 'cosign' "--certificate-identity $identity"
    Expect-Log $r 'cosign' "--certificate-oidc-issuer $issuer"
    Expect-Log $r 'cosign' '--bundle'
    Expect-Log $r 'cosign' 'SHA256SUMS.sigstore.json'

    $r = Invoke-Case 'cosign rejects' @('cosign') @{ STUB_COSIGN_EXIT = '1' } @('-Archive', $zip)
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r 'cosign signature check failed'
    Expect-Out $r 'none of the expected identities matched'

    $nobundle = Join-Path $work 'nobundle'
    New-Item -ItemType Directory -Path $nobundle | Out-Null
    Copy-Item -LiteralPath $zip, $sums -Destination $nobundle
    $r = Invoke-Case 'cosign but no bundle' @('cosign') $null @('-Archive', (Join-Path $nobundle "$name.zip"))
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r 'SHA256SUMS.sigstore.json'

    $r = Invoke-Case '-SkipSignature' @('cosign') @{ STUB_COSIGN_EXIT = '1' } @('-Archive', $zip, '-SkipSignature')
    Expect-Exit $r 0
    Expect-Installed $r
    Expect-NoLog $r 'cosign'
    Expect-Out $r 'cosign signature: skipped (-SkipSignature)'
    Expect-Out $r 'WARNING: no signature was verified'

    $r = Invoke-Case 'cosign 2' @('cosign') @{ STUB_COSIGN_VERSION = 'v2.6.5' } @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Log $r 'cosign' '--new-bundle-format'

    $r = Invoke-Case 'cosign 2 without v' @('cosign') @{ STUB_COSIGN_VERSION = '2.6.5' } @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Log $r 'cosign' '--new-bundle-format'

    $r = Invoke-Case 'cosign 1' @('cosign') @{ STUB_COSIGN_VERSION = 'v1.13.1' } @('-Archive', $zip)
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r "cannot read the release's Sigstore bundle"

    $r = Invoke-Case 'cosign 3' @('cosign') @{ STUB_COSIGN_VERSION = 'v3.1.5' } @('-Archive', $zip)
    Expect-Exit $r 0
    if ((Get-Log $r 'cosign').Contains('--new-bundle-format')) { Fail-Check $r 'cosign 3 got --new-bundle-format' }

    $renamed = Join-Path $work 'renamed'
    New-Item -ItemType Directory -Path $renamed | Out-Null
    Copy-Item -LiteralPath $zip -Destination (Join-Path $renamed 'kanjo-local.zip')
    $r = Invoke-Case '-NoVerify with a renamed archive' @('cosign') $null @('-Archive', (Join-Path $renamed 'kanjo-local.zip'), '-NoVerify')
    Expect-Exit $r 0
    Expect-Installed $r
    Expect-NoLog $r 'cosign'

    $r = Invoke-Case 'minisign verifies' @('minisign') @{ KANJO_MINISIGN_PUBKEY = 'RWtestkey' } @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Out $r 'minisign signature: verified'
    Expect-Log $r 'minisign' '-P RWtestkey'

    $r = Invoke-Case 'minisign rejects' @('minisign') @{ KANJO_MINISIGN_PUBKEY = 'RWtestkey'; STUB_MINISIGN_EXIT = '1' } @('-Archive', $zip)
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r 'minisign signature check failed'

    $r = Invoke-Case 'no minisign key' @('minisign') $null @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Out $r 'minisign signature: skipped (no minisign public key is configured'
    Expect-NoLog $r 'minisign'

    $tampered = Join-Path $work 'tampered'
    New-Item -ItemType Directory -Path $tampered | Out-Null
    Copy-Item -LiteralPath $zip, $sums, "$sums.sigstore.json" -Destination $tampered
    Add-Content -LiteralPath (Join-Path $tampered "$name.zip") -Value 'x'
    $r = Invoke-Case 'tampered archive' @('cosign') $null @('-Archive', (Join-Path $tampered "$name.zip"))
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r 'checksum mismatch'

    $r = Invoke-Case 'install directory not on PATH' @() $null @('-Archive', $zip)
    Expect-Exit $r 0
    Expect-Out $r "next step: run & '$(Join-Path $r.Dir 'kanjo.exe')' up"
    Expect-Out $r 'is not on the PATH of this terminal'
    Expect-Out $r 'PowerShell:'
    Expect-NoOut $r 'next step: run "kanjo up"'

    $r = Invoke-Case 'install directory on PATH' @() $null @('-Archive', $zip) -DirOnPath
    Expect-Exit $r 0
    Expect-Out $r 'next step: run "kanjo up"'
    Expect-NoOut $r 'is not on the PATH of this terminal'
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($script:failures -gt 0) {
    Write-Host "test-install: $script:failures failed check(s) in $script:n case(s)"
    exit 1
}
Write-Host "test-install: all $script:n cases passed"
