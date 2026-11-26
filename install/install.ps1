<#
.SYNOPSIS
Installs kanjo on Windows.

.DESCRIPTION
Downloads the kanjo release archive for Windows from GitHub, verifies it
against SHA256SUMS, installs kanjo.exe into %LOCALAPPDATA%\Programs\Kanjo and
adds that directory to the user PATH. No administrator rights are needed.
SHA256SUMS itself is checked against its Sigstore signature when cosign is
installed (the certificate must name this repository's release workflow and
tag), and against its minisign signature when minisign is installed and a
public key is set below. A failed check stops the installer; a skipped one is
reported with the reason.
Run it straight from the latest release of the repository in $RepoUrl below:

  irm https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.ps1 | iex

To pass parameters in that form, turn the downloaded text into a script block:

  & ([scriptblock]::Create((irm https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.ps1))) -Version 0.1.0

Give switches without a value there (-NoPathUpdate, not -NoPathUpdate:$true),
because PowerShell does not pass the value of a switch on to the script block
that holds the parameters.

Or download it, read it, and run:

  powershell -ExecutionPolicy Bypass -File install.ps1 -Version 0.1.0

Everything runs in a script block of its own, so the installer leaves no
variables, functions or preference settings behind in the session that runs
it, and an error is reported without closing that session.

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

.PARAMETER SkipSignature
Do not run cosign or minisign (SHA-256 is still checked), for machines where
cosign is installed but cannot reach Sigstore.

.PARAMETER NoPathUpdate
Do not add the install directory to the user PATH.
#>
# Rethrowing turns a parameter binding error, which would otherwise let
# "powershell -File install.ps1" end with exit code 0, into one that stops it.
try {
    & {
        [CmdletBinding(PositionalBinding = $false)]
        param(
            [string]$Version = '',
            [string]$Dir = '',
            [string]$Archive = '',
            [string]$Sums = '',
            [switch]$NoVerify,
            [switch]$SkipSignature,
            [switch]$NoPathUpdate
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = 'Stop'
        $ProgressPreference = 'SilentlyContinue'

        $RepoUrl = 'https://github.com/Kyotu-Technology/kanjo'
        # Minisign public key of the release signer (the base64 line of minisign.pub).
        # Empty until the project publishes its key; signature checks are skipped then.
        $MinisignPubKey = if ($env:KANJO_MINISIGN_PUBKEY) { $env:KANJO_MINISIGN_PUBKEY } else { '' }
        # Releases are signed by .github/workflows/release.yml with a GitHub OIDC token.
        $CosignIssuer = 'https://token.actions.githubusercontent.com'
        $Target = 'x86_64-pc-windows-msvc'

        function Say([string]$Message) { Write-Host "kanjo-install: $Message" }
        function Fail([string]$Message) { throw "kanjo-install: error: $Message" }

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
            $Status.Sha = 'verified'
        }

        $Status = @{ Sha = 'skipped'; Cosign = 'skipped'; Minisign = 'skipped'; Signed = $false }

        # Windows PowerShell turns native stderr into errors when it is redirected, and
        # cosign reports on stderr, so run native tools with errors non-terminating.
        function Invoke-Native([string]$Exe, [string[]]$Arguments) {
            $ErrorActionPreference = 'Continue'
            $output = @(& $Exe @Arguments 2>&1 | ForEach-Object { "$_" })
            return @{ Code = $LASTEXITCODE; Output = $output }
        }

        function Test-Cosign([string]$SumsFile, [string]$BundleFile, [string]$ReleaseVersion) {
            if ($SkipSignature) { $Status.Cosign = 'skipped (-SkipSignature)'; return }
            $cosign = Get-Command cosign -ErrorAction SilentlyContinue | Select-Object -First 1
            if (-not $cosign) { $Status.Cosign = 'skipped (cosign is not installed)'; return }
            if (-not (Test-Path -LiteralPath $BundleFile)) {
                Fail 'cosign is installed but there is no SHA256SUMS.sigstore.json next to SHA256SUMS, so the signature cannot be checked; copy it from the same release (or pass -SkipSignature to rely on the checksum alone)'
            }
            if (-not $ReleaseVersion) { Fail "cosign needs the release version, but it cannot be read from the archive name (expected kanjo-<version>-$Target.zip)" }
            $identity = "$RepoUrl/.github/workflows/release.yml@refs/tags/v$ReleaseVersion"
            $cosignVersion = ''
            foreach ($line in (Invoke-Native $cosign.Source @('version')).Output) {
                if ($line -match '^GitVersion:\s+(\S+)') { $cosignVersion = $Matches[1]; break }
            }
            $verifyArgs = @('verify-blob')
            if ($cosignVersion -match '^v?[01]\.') { Fail "cosign $cosignVersion cannot read the release's Sigstore bundle; install cosign 3 or later (or pass -SkipSignature)" }
            if ($cosignVersion -match '^v?2\.') { $verifyArgs += '--new-bundle-format' }
            $verifyArgs += @('--bundle', $BundleFile, '--certificate-oidc-issuer', $CosignIssuer, '--certificate-identity', $identity, $SumsFile)
            $result = Invoke-Native $cosign.Source $verifyArgs
            if ($result.Code -ne 0) {
                foreach ($line in $result.Output) { Write-Host "  cosign: $line" }
                Fail "cosign signature check failed: SHA256SUMS was not signed by $identity; do not install this download. If cosign failed only because this machine cannot reach Sigstore, pass -SkipSignature to rely on the checksum alone"
            }
            $Status.Cosign = "verified (signed by $identity)"
            $Status.Signed = $true
        }

        function Test-Minisign([string]$SumsFile, [string]$SigFile) {
            if ($SkipSignature) { $Status.Minisign = 'skipped (-SkipSignature)'; return }
            if (-not $MinisignPubKey) { $Status.Minisign = 'skipped (no minisign public key is configured in this installer yet)'; return }
            $minisign = Get-Command minisign -ErrorAction SilentlyContinue | Select-Object -First 1
            if (-not $minisign) { $Status.Minisign = 'skipped (minisign is not installed)'; return }
            if (-not (Test-Path -LiteralPath $SigFile)) { Fail 'minisign is installed and a public key is configured, but there is no SHA256SUMS.minisig to check' }
            $result = Invoke-Native $minisign.Source @('-Vm', $SumsFile, '-x', $SigFile, '-P', $MinisignPubKey)
            if ($result.Code -ne 0) {
                foreach ($line in $result.Output) { Write-Host "  minisign: $line" }
                Fail 'minisign signature check failed; do not install this download'
            }
            $Status.Minisign = 'verified'
            $Status.Signed = $true
        }

        function Write-Verification {
            Say 'verification:'
            Say "  SHA-256 checksum: $($Status.Sha)"
            Say "  cosign signature: $($Status.Cosign)"
            Say "  minisign signature: $($Status.Minisign)"
            if (-not $Status.Signed) {
                Say 'WARNING: no signature was verified. The checksum shows that the archive matches SHA256SUMS from the same source, not that the Kanjo release workflow built it. Install cosign and run the installer again, or verify the download by hand as described in docs/release.md.'
            }
        }

        function Test-OnPath([string]$PathValue, [string]$Wanted) {
            foreach ($p in ($PathValue -split [regex]::Escape([string][IO.Path]::PathSeparator))) {
                if ($p -and ($p.TrimEnd('\', '/') -ieq $Wanted.TrimEnd('\', '/'))) { return $true }
            }
            return $false
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
        # The protocol setting is process-wide, and "irm | iex" runs in the user's own
        # PowerShell, so it is put back when the installer ends.
        $securityProtocol = [Net.ServicePointManager]::SecurityProtocol
        try {
            if ($PSVersionTable.PSVersion.Major -lt 6) {
                [Net.ServicePointManager]::SecurityProtocol = $securityProtocol -bor [Net.SecurityProtocolType]::Tls12
            }
            if ($Archive) {
                if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) { Fail "archive not found: $Archive" }
                $name = Split-Path -Leaf $Archive
                if ($name -notlike 'kanjo-*.zip') { Fail "expected an archive named kanjo-<version>-<target>.zip, got $name" }
                $Version = ''
                if ($name -match "^kanjo-(.+)-$([regex]::Escape($Target))\.zip$") {
                    $Version = $Matches[1]
                    if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$') { Fail "not a version number: $Version" }
                }
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
                    $bundle = "$Sums.sigstore.json"
                    if (Test-Path -LiteralPath $bundle) { Copy-Item -LiteralPath $bundle -Destination "$sumsFile.sigstore.json" }
                    Test-Cosign $sumsFile "$sumsFile.sigstore.json" $Version
                    Test-Minisign $sumsFile "$sumsFile.minisig"
                    Test-Sha256 $zip $sumsFile
                    Write-Verification
                } elseif ($NoVerify) {
                    Say 'WARNING: -NoVerify given; installing an archive whose checksum and signature were not verified'
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
                if (-not $SkipSignature -and (Get-Command cosign -ErrorAction SilentlyContinue)) {
                    try {
                        Get-Download "$base/SHA256SUMS.sigstore.json" "$sumsFile.sigstore.json"
                    } catch {
                        Fail 'cosign is installed but SHA256SUMS.sigstore.json could not be downloaded from the release, so the signature cannot be checked'
                    }
                }
                if (-not $SkipSignature -and $MinisignPubKey -and (Get-Command minisign -ErrorAction SilentlyContinue)) {
                    Get-Download "$base/SHA256SUMS.minisig" "$sumsFile.minisig"
                }
                Test-Cosign $sumsFile "$sumsFile.sigstore.json" $Version
                Test-Minisign $sumsFile "$sumsFile.minisig"
                Test-Sha256 $zip $sumsFile
                Write-Verification
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
            if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) { Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue }
            Say "installed $dest"

            $wasOnPath = Test-OnPath $env:PATH $Dir
            if (-not $NoPathUpdate) {
                if (Add-UserPath $Dir) {
                    Say "added $Dir to your user PATH; terminals opened from now on find kanjo"
                }
                if (-not $wasOnPath) { $env:PATH = $env:PATH + [IO.Path]::PathSeparator + $Dir }
            }
            if ($wasOnPath) {
                Say 'next step: run "kanjo up"'
            } elseif (-not $NoPathUpdate) {
                Say 'next step: run "kanjo up" in a new terminal'
                Say "  (this terminal finds kanjo by name only if the installer ran inside it, as with irm | iex; otherwise run & '$dest' up here)"
            } else {
                Say "$Dir is not on the PATH of this terminal, so it does not find kanjo by name yet"
                Say "next step: run & '$dest' up"
                Say "  (in cmd.exe: `"$dest`" up)"
                Say "to put kanjo on your PATH (-NoPathUpdate was given):"
                Say "  PowerShell: [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$Dir', 'User')"
                Say "  or add $Dir to the user variable Path in Settings, System, About, Advanced system settings, Environment Variables"
                Say 'then open a new terminal, and "kanjo up" works from anywhere'
            }
        } finally {
            [Net.ServicePointManager]::SecurityProtocol = $securityProtocol
            Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
        }
    } @args
} catch {
    throw
}
