<#
.SYNOPSIS
Installs kanjo on Windows.

.DESCRIPTION
Downloads the kanjo release archive for Windows from GitHub, verifies it
against SHA256SUMS (and SHA256SUMS.minisig when minisign.exe and a public key
are available), installs kanjo.exe into %LOCALAPPDATA%\Programs\Kanjo and
adds that directory to the user PATH. No administrator rights are needed.
Download it from the latest release of the repository in $RepoUrl below,
read it, and then run:

  powershell -ExecutionPolicy Bypass -File install.ps1

.PARAMETER Version
Install this version (x.y.z) instead of the latest release.

.PARAMETER Dir
Install directory. Default: %LOCALAPPDATA%\Programs\Kanjo.

.PARAMETER Archive
Offline mode: install from a local kanjo-<version>-x86_64-pc-windows-msvc.zip.
A SHA256SUMS file next to it is used for verification when present.

.PARAMETER Sums
SHA256SUMS file to verify an offline archive against.

.PARAMETER NoVerify
Offline mode only: install an archive that has no SHA256SUMS.

.PARAMETER NoPathUpdate
Do not add the install directory to the user PATH.
#>
[CmdletBinding()]
param(
    [string]$Version = '',
    [string]$Dir = '',
    [string]$Archive = '',
    [string]$Sums = '',
    [switch]$NoVerify,
    [switch]$NoPathUpdate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RepoUrl = 'https://github.com/kyotu/kanjo'
# Minisign public key of the release signer (the base64 line of minisign.pub).
# Empty until the project publishes its key; signature checks are skipped then.
$MinisignPubKey = if ($env:KANJO_MINISIGN_PUBKEY) { $env:KANJO_MINISIGN_PUBKEY } else { '' }
$Target = 'x86_64-pc-windows-msvc'

function Say([string]$Message) { Write-Host "kanjo-install: $Message" }
function Fail([string]$Message) { throw "kanjo-install: error: $Message" }

if ($PSVersionTable.PSVersion.Major -lt 6) {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

if (-not $Dir) {
    if (-not $env:LOCALAPPDATA) { Fail 'LOCALAPPDATA is not set; pass -Dir' }
    $Dir = Join-Path $env:LOCALAPPDATA 'Programs\Kanjo'
}
if ($Version) {
    $Version = $Version.TrimStart('v')
    if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$') { Fail "not a version number: $Version" }
}

function Get-Download([string]$Url, [string]$OutFile) {
    Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing
}

function Get-LatestVersion {
    $slug = $RepoUrl -replace '^https://github\.com/', ''
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$slug/releases/latest" -UseBasicParsing -Headers @{ 'User-Agent' = 'kanjo-installer' }
    $v = ([string]$release.tag_name).TrimStart('v')
    if ($v -notmatch '^[0-9]+\.[0-9]+\.[0-9]+') { Fail "could not determine the latest release (got '$v'); pass -Version" }
    return $v
}

function Test-Sha256([string]$File, [string]$SumsFile) {
    $name = Split-Path -Leaf $File
    $entries = @(Get-Content -LiteralPath $SumsFile | ForEach-Object {
        $parts = $_ -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $name) { $parts[0].ToLowerInvariant() }
    })
    if ($entries.Count -ne 1) { Fail "SHA256SUMS has no single entry for $name" }
    $actual = (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entries[0]) { Fail "checksum mismatch for $name (expected $($entries[0]), got $actual)" }
    Say 'SHA-256 verified'
}

function Test-Minisign([string]$SumsFile, [string]$SigFile) {
    $minisign = Get-Command minisign -ErrorAction SilentlyContinue
    if (-not $minisign) { Say 'minisign not found; skipping the signature check (SHA-256 still verified)'; return }
    if (-not $MinisignPubKey) { Say 'no minisign public key configured in this installer; skipping the signature check'; return }
    if (-not (Test-Path -LiteralPath $SigFile)) { Fail 'minisign is available but the release has no SHA256SUMS.minisig' }
    & $minisign.Source -Vm $SumsFile -x $SigFile -P $MinisignPubKey | Out-Null
    if ($LASTEXITCODE -ne 0) { Fail 'minisign signature check failed' }
    Say 'minisign signature verified'
}

function Add-UserPath([string]$PathToAdd) {
    # Read and write the raw registry value so entries such as %USERPROFILE%\bin
    # stay unexpanded; [Environment]::SetEnvironmentVariable would expand them.
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $parts = @($raw -split ';' | Where-Object { $_ -ne '' })
        $wanted = $PathToAdd.TrimEnd('\')
        foreach ($p in $parts) {
            if ([Environment]::ExpandEnvironmentVariables($p).TrimEnd('\') -ieq $wanted) { return $false }
        }
        $key.SetValue('Path', (($parts + $PathToAdd) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
    } finally {
        $key.Close()
    }
    # Setting a user variable through .NET broadcasts WM_SETTINGCHANGE, so new
    # terminals pick up the changed PATH without a logoff.
    [Environment]::SetEnvironmentVariable('KANJO_INSTALL_REFRESH', '1', 'User')
    [Environment]::SetEnvironmentVariable('KANJO_INSTALL_REFRESH', $null, 'User')
    return $true
}

$arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
switch ($arch) {
    'AMD64' { }
    'ARM64' { Say 'no native ARM64 build yet; installing the x86_64 build, which Windows 11 runs under emulation' }
    default { Fail "unsupported architecture: $arch" }
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ('kanjo-install-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    if ($Archive) {
        if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) { Fail "archive not found: $Archive" }
        $name = Split-Path -Leaf $Archive
        if ($name -notlike 'kanjo-*.zip') { Fail "expected an archive named kanjo-<version>-<target>.zip, got $name" }
        $zip = Join-Path $tmp $name
        Copy-Item -LiteralPath $Archive -Destination $zip
        if (-not $Sums) {
            $candidate = Join-Path (Split-Path -Parent (Resolve-Path -LiteralPath $Archive)) 'SHA256SUMS'
            if (Test-Path -LiteralPath $candidate) { $Sums = $candidate }
        }
        if ($Sums) {
            if (-not (Test-Path -LiteralPath $Sums -PathType Leaf)) { Fail "SHA256SUMS not found: $Sums" }
            $sumsFile = Join-Path $tmp 'SHA256SUMS'
            Copy-Item -LiteralPath $Sums -Destination $sumsFile
            $sig = "$Sums.minisig"
            if (Test-Path -LiteralPath $sig) { Copy-Item -LiteralPath $sig -Destination "$sumsFile.minisig" }
            Test-Minisign $sumsFile "$sumsFile.minisig"
            Test-Sha256 $zip $sumsFile
        } elseif ($NoVerify) {
            Say 'WARNING: -NoVerify given; installing an archive whose checksum was not verified'
        } else {
            Fail "no SHA256SUMS next to $Archive; pass -Sums <file>, or -NoVerify to install it unverified"
        }
    } else {
        if (-not $Version) { $Version = Get-LatestVersion }
        $name = "kanjo-$Version-$Target.zip"
        $base = "$RepoUrl/releases/download/v$Version"
        $zip = Join-Path $tmp $name
        $sumsFile = Join-Path $tmp 'SHA256SUMS'
        Say "downloading $name"
        Get-Download "$base/$name" $zip
        Get-Download "$base/SHA256SUMS" $sumsFile
        if ((Get-Command minisign -ErrorAction SilentlyContinue) -and $MinisignPubKey) {
            Get-Download "$base/SHA256SUMS.minisig" "$sumsFile.minisig"
        }
        Test-Minisign $sumsFile "$sumsFile.minisig"
        Test-Sha256 $zip $sumsFile
    }

    $extract = Join-Path $tmp 'x'
    Expand-Archive -LiteralPath $zip -DestinationPath $extract -Force
    $exe = Get-ChildItem -LiteralPath $extract -Recurse -File -Filter 'kanjo.exe' | Select-Object -First 1
    if (-not $exe) { Fail 'the archive does not contain kanjo.exe' }

    New-Item -ItemType Directory -Path $Dir -Force | Out-Null
    $dest = Join-Path $Dir 'kanjo.exe'
    $staged = Join-Path $Dir 'kanjo.exe.new'
    Copy-Item -LiteralPath $exe.FullName -Destination $staged -Force
    try {
        Move-Item -LiteralPath $staged -Destination $dest -Force
    } catch {
        Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
        Fail "could not replace $dest; stop any running kanjo process and run the installer again"
    }
    Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue
    Say "installed $dest"

    if (-not $NoPathUpdate) {
        if (Add-UserPath $Dir) {
            Say "added $Dir to your user PATH; open a new terminal to use it"
        }
        if (-not (($env:Path -split ';') -contains $Dir)) { $env:Path = "$env:Path;$Dir" }
    }
    Say 'next step: run "kanjo up"'
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
