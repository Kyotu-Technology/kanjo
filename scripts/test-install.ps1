# Tests install/install.ps1 in offline mode with stub cosign and minisign
# commands, in a temporary directory, with -NoPathUpdate so the user PATH is
# never changed. Also runs it the way "irm ... | iex" and the script block form
# do, with stub download commands, in a child session that checks that nothing
# of the installer is left behind in it and that it keeps running after an
# error. Runs the installer in the same PowerShell edition as this script, so
# run it under both powershell.exe (5.1) and pwsh.
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
$url = 'https://github.com/Kyotu-Technology/kanjo'
$identity = "$url/.github/workflows/release.yml@refs/tags/v$version"
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

# Runs the installer text inside this session, as "irm | iex" (-Mode iex) or as
# "& ([scriptblock]::Create((irm ...))) <parameters>" (-Mode scriptblock), with
# Invoke-WebRequest and Invoke-RestMethod replaced by functions that serve the
# files of the test release, then reports what the installer left behind.
$driver = Join-Path $work 'piped.ps1'
Set-Content -LiteralPath $driver -Encoding ascii -Value @'
param([string]$Mode, [string]$Installer, [string]$Dir, [switch]$NoPathUpdate, [string]$Version = '')
$text = Get-Content -Raw -LiteralPath $Installer
function Invoke-WebRequest {
    param([string]$Uri, [string]$OutFile, [switch]$UseBasicParsing)
    Add-Content -LiteralPath (Join-Path $env:STUB_LOG 'web') -Value $Uri
    $src = Join-Path $env:STUB_REL ($Uri -replace '^.*/', '')
    if (-not $Uri.StartsWith($env:STUB_BASE + '/') -or -not (Test-Path -LiteralPath $src)) { throw "stub: 404 for $Uri" }
    Copy-Item -LiteralPath $src -Destination $OutFile
}
function Invoke-RestMethod {
    param([string]$Uri, [switch]$UseBasicParsing, [hashtable]$Headers)
    Add-Content -LiteralPath (Join-Path $env:STUB_LOG 'web') -Value $Uri
    [pscustomobject]@{ tag_name = $env:STUB_LATEST_TAG }
}
if ($Mode -eq 'iex') { $env:LOCALAPPDATA = $null }
$probe = @('Version', 'Dir', 'Archive', 'Sums', 'tmp', 'zip', 'name', 'Status', 'RepoUrl', 'Target')
foreach ($p in $probe) { if (-not (Get-Variable -Name $p -ErrorAction SilentlyContinue)) { Set-Variable -Name $p -Value 'caller' } }
$expected = @{}
foreach ($p in $probe) { $expected[$p] = [string](Get-Variable -Name $p).Value }
$preferences = "$ErrorActionPreference|$ProgressPreference|$VerbosePreference"
$protocol = [Net.ServicePointManager]::SecurityProtocol
$automatic = @('?', '^', '$', '_', 'PSItem', 'Error', 'LASTEXITCODE', 'Matches', 'StackTrace', 'args', 'input', 'foreach', 'switch', 'this')
$caught = $null
$leaks = @()
$v = $null
$f = $null
$l = $null
$functionsBefore = @(Get-ChildItem function: | ForEach-Object { $_.Name })
$variablesBefore = @(Get-Variable | ForEach-Object { $_.Name }) + 'variablesBefore'
try {
    if ($Mode -eq 'iex') {
        $text | Invoke-Expression
    } else {
        if ($NoPathUpdate) {
            & ([scriptblock]::Create($text)) -Dir $Dir -Version $Version -NoPathUpdate
        } else {
            & ([scriptblock]::Create($text)) -Dir $Dir -Version $Version
        }
    }
} catch {
    $caught = $_
}
if ($caught) { Write-Host "piped: error: $($caught.Exception.Message)" }
foreach ($v in @(Get-Variable)) { if ($variablesBefore -notcontains $v.Name -and $automatic -notcontains $v.Name) { $leaks += "variable $($v.Name)" } }
foreach ($f in @(Get-ChildItem function:)) { if ($functionsBefore -notcontains $f.Name -and -not $f.ModuleName) { $leaks += "function $($f.Name)" } }
foreach ($p in $probe) { if ([string](Get-Variable -Name $p).Value -ne $expected[$p]) { $leaks += "the value of $p" } }
if ("$ErrorActionPreference|$ProgressPreference|$VerbosePreference" -ne $preferences) { $leaks += 'preference settings' }
if ([Net.ServicePointManager]::SecurityProtocol -ne $protocol) { $leaks += 'the SecurityProtocol setting' }
try { $null = $undefinedProbe } catch { $leaks += 'strict mode' }
foreach ($l in $leaks) { Write-Host "piped: leaked $l" }
Write-Host 'piped: the calling session is still running'
'@

$script:failures = 0
$script:n = 0
$realPath = $env:PATH

# Runs the installer. $Tools names the stub commands to put on PATH. Returns
# the combined output, the exit code, the install directory and the log directory.
function Invoke-Case([string]$Label, [string[]]$Tools, [hashtable]$Env, [string[]]$Arguments, [switch]$DirOnPath, [string]$Script = $installer) {
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
    $vars = @{
        STUB_LOG = $log; STUB_COSIGN_EXIT = '0'; STUB_COSIGN_VERSION = 'v3.1.5'; STUB_MINISIGN_EXIT = '0'; KANJO_MINISIGN_PUBKEY = ''
        STUB_REL = $rel; STUB_BASE = "$url/releases/download/v$version"; STUB_LATEST_TAG = "v$version"
    }
    if ($Env) { foreach ($k in $Env.Keys) { $vars[$k] = $Env[$k] } }
    foreach ($k in $vars.Keys) { [Environment]::SetEnvironmentVariable($k, $vars[$k]) }
    $env:PATH = $path + $sep + $realPath
    if ($DirOnPath) { $env:PATH = $dir + $sep + $env:PATH }
    $ErrorActionPreference = 'Continue'
    $out = & $shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $Script -Dir $dir -NoPathUpdate @Arguments 2>&1 | ForEach-Object { "$_" }
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

    $r = Invoke-Case 'unknown parameter' @() $null @('-Archive', $zip, '-NoSuchOption')
    Expect-Exit $r 1
    Expect-NotInstalled $r
    Expect-Out $r 'NoSuchOption'

    $r = Invoke-Case 'irm | iex, error before any download' @() $null @('-Mode', 'iex', '-Installer', $installer) -Script $driver
    Expect-Exit $r 0
    Expect-Out $r 'piped: error: kanjo-install: error: LOCALAPPDATA is not set'
    Expect-Out $r 'piped: the calling session is still running'
    Expect-NoOut $r 'piped: leaked'
    Expect-NoLog $r 'web'

    $r = Invoke-Case 'script block, latest release, cosign verifies' @('cosign') $null @('-Mode', 'scriptblock', '-Installer', $installer) -Script $driver
    Expect-Exit $r 0
    Expect-Installed $r
    Expect-NoOut $r 'piped: error'
    Expect-Out $r 'cosign signature: verified'
    Expect-Out $r 'piped: the calling session is still running'
    Expect-NoOut $r 'piped: leaked'
    Expect-Log $r 'web' 'https://api.github.com/repos/Kyotu-Technology/kanjo/releases/latest'
    Expect-Log $r 'web' "$url/releases/download/v$version/$name.zip"
    Expect-Log $r 'web' "$url/releases/download/v$version/SHA256SUMS.sigstore.json"
    Expect-Log $r 'cosign' "--certificate-identity $identity"

    $r = Invoke-Case 'script block, -Version, cosign rejects' @('cosign') @{ STUB_COSIGN_EXIT = '1' } @('-Mode', 'scriptblock', '-Installer', $installer, '-Version', $version) -Script $driver
    Expect-Exit $r 0
    Expect-NotInstalled $r
    Expect-Out $r 'piped: error: kanjo-install: error: cosign signature check failed'
    Expect-Out $r 'piped: the calling session is still running'
    Expect-NoOut $r 'piped: leaked'
    if ((Get-Log $r 'web').Contains('releases/latest')) { Fail-Check $r 'asked for the latest release although -Version was given' }
} finally {
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}

if ($script:failures -gt 0) {
    Write-Host "test-install: $script:failures failed check(s) in $script:n case(s)"
    exit 1
}
Write-Host "test-install: all $script:n cases passed"
