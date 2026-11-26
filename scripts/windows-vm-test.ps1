# Runs inside a fresh Windows virtual machine started by scripts/windows-vm-test.sh.
# Installs the packaged zip with install.ps1, connects a fixture Claude Code
# directory with "kanjo up" (logon service, Windows credential store), checks the
# dashboard, the status line, live file watching and the reports, and uninstalls.
# The zip is the only one in the shared dist folder, whatever its name and
# layout. When the host shared a winget folder with a local manifest, it then
# installs Kanjo with winget (non-elevated first, elevated when that is
# refused), runs "kanjo up" with that copy and records the executable path it
# wrote into settings.json and the Run key.
# Everything is written to result.txt on the shared folder.
$ErrorActionPreference = 'Continue'
$share = '\\host.lan\Data'
for ($i = 0; $i -lt 90 -and -not (Test-Path "$share\install.ps1"); $i++) { Start-Sleep 2 }
$log = "$share\result.txt"
function Log($m) { Add-Content -Path $log -Value $m -Encoding UTF8 }
Log "=== start $(Get-Date -Format o) | $([Environment]::OSVersion.VersionString) | PowerShell $($PSVersionTable.PSVersion) | arch $env:PROCESSOR_ARCHITECTURE"
$w = "$env:TEMP\kanjo-test"; New-Item -ItemType Directory -Force $w | Out-Null
Copy-Item "$share\dist\*" $w -Force; Copy-Item "$share\install.ps1" $w -Force

$sw = [Diagnostics.Stopwatch]::StartNew()
$zips = @(Get-ChildItem "$w\*.zip")
Log "archive: $(($zips | ForEach-Object { $_.Name }) -join ', ')"
if ($zips.Count -ne 1) { Log "archive ERR expected exactly one zip in the shared dist folder" }
$out = & powershell -NoProfile -ExecutionPolicy Bypass -File "$w\install.ps1" -Archive $zips[0].FullName -SkipSignature 2>&1 | Out-String
Log "--- 1 install.ps1 ($($sw.ElapsedMilliseconds) ms)"; Log $out
$exe = "$env:LOCALAPPDATA\Programs\Kanjo\kanjo.exe"
Log "exe exists: $(Test-Path $exe); user PATH updated: $([Environment]::GetEnvironmentVariable('Path','User') -like '*Programs\Kanjo*')"
Log "--- 2 version"; Log (& $exe --version 2>&1 | Out-String)

& $exe --home "$w\demo" demo --seed-only 2>&1 | Out-String | ForEach-Object { Log "seed: $_" }
Log "--- 3 status on demo data"; Log (& $exe --home "$w\demo" --lang en status 2>&1 | Out-String)

$claude = "$env:USERPROFILE\.claude"; New-Item -ItemType Directory -Force "$claude\projects" | Out-Null
Copy-Item "$share\projects\*" "$claude\projects" -Recurse -Force
Set-Content "$claude\settings.json" '{"model":"opus"}' -Encoding ASCII
$sw.Restart()
$up = & $exe --lang en up --yes --no-open 2>&1 | Out-String
Log "--- 4 kanjo up with background service ($($sw.ElapsedMilliseconds) ms)"; Log $up
Start-Sleep 8
Log "--- settings.json after up"; Log (Get-Content "$claude\settings.json" -Raw)
Log "backup files: $((Get-ChildItem $claude -Filter 'settings.json.kanjo-backup-*' | Measure-Object).Count)"
Log "Run key: $((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Kanjo)"
Log "--- service status"; Log (& $exe --lang en service status 2>&1 | Out-String)
$data = "$env:APPDATA\Kyotu\kanjo\data"
Log "--- data dir"; Log ((Get-ChildItem $data -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) $($_.Length)" }) -join '; ')
Log "kanjo processes: $((Get-Process kanjo -ErrorAction SilentlyContinue | Measure-Object).Count)"
$sj = $null; try { $sj = Get-Content "$data\server.json" -Raw | ConvertFrom-Json } catch { Log "server.json ERR $_" }
$tok = ''; try { $tok = (Get-Content "$data\access.token" -Raw).Trim() } catch { Log "token ERR $_" }
$h = @{ Authorization = "Bearer $tok" }
$url = if ($sj) { $sj.url } else { 'http://127.0.0.1:4747/' }
try { Log "health: $((Invoke-WebRequest -UseBasicParsing ($url + 'api/v1/health')).Content)" } catch { Log "health ERR $_" }
try { Log "ui: $((Invoke-WebRequest -UseBasicParsing $url).StatusCode)" } catch { Log "ui ERR $_" }
try { $sys = Invoke-RestMethod -Headers $h ($url + 'api/v1/system'); Log "system: encrypted=$($sys.database.encrypted) key_source=$($sys.database.key_source) requests=$($sys.ingest.requests_stored) watcher=$($sys.ingest.watcher_running) statusline=$($sys.hooks.statusline_installed) hooks=$($sys.hooks.session_hooks_installed) service=$($sys.hooks.service_installed)"; Log ("checks: " + (($sys.checks | ForEach-Object { "$($_.id)=$($_.status)" }) -join ' ')) } catch { Log "system ERR $_" }

$sl = Get-Content "$share\payload.json" -Raw | & $exe statusline 2>&1 | Out-String
Log "--- 5 statusline: $sl"
Start-Sleep 6
try { $lim = Invoke-RestMethod -Headers $h ($url + 'api/v1/limits'); Log "limits: five_hour=$($lim.current.five_hour.used_pct) confidence=$($lim.current.five_hour.confidence) seven_day=$($lim.current.seven_day.used_pct)" } catch { Log "limits ERR $_" }

$t = Get-ChildItem "$claude\projects" -Recurse -Filter '*.jsonl' | Where-Object { $_.Directory.Parent.Name -eq 'projects' } | Select-Object -First 1
$before = -1; try { $before = (Invoke-RestMethod -Headers $h ($url + 'api/v1/system')).ingest.requests_stored } catch {}
$ts = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
$line = '{"type":"assistant","uuid":"winvm-uuid-1","sessionId":"' + $t.BaseName + '","timestamp":"' + $ts + '","cwd":"/work/acme-shop","version":"2.1.300","isSidechain":false,"message":{"id":"msg_winvm_appended_1","type":"message","role":"assistant","model":"claude-opus-5-5","content":[{"type":"text","text":"x"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}'
[IO.File]::AppendAllText($t.FullName, $line + "`n")
$after = $before
for ($i = 0; $i -lt 15 -and $after -le $before; $i++) { Start-Sleep 1; try { $after = (Invoke-RestMethod -Headers $h ($url + 'api/v1/system')).ingest.requests_stored } catch {} }
Log "--- 6 watcher: requests before=$before after=$after picked up after about $i s"

Log "--- 7 daily"; Log (& $exe --lang en daily 2>&1 | Out-String)
Log "--- doctor"; Log (& $exe --lang en doctor 2>&1 | Out-String)
Log "--- open --json"; Log (& $exe open --json 2>&1 | Out-String)
Log "--- status (pl)"; Log (& $exe --lang pl status 2>&1 | Out-String)
Log "--- 8 uninstall"; Log (& $exe --lang en uninstall --yes 2>&1 | Out-String)
for ($i = 0; $i -lt 8 -and (Get-Process kanjo -ErrorAction SilentlyContinue); $i++) { Start-Sleep 1 }
Log "kanjo processes after uninstall: $((Get-Process kanjo -ErrorAction SilentlyContinue | Measure-Object).Count) (waited $i s)"
Log "settings.json after uninstall: $(Get-Content "$claude\settings.json" -Raw)"
Log "Run key after: [$((Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Kanjo)]; kanjo processes: $((Get-Process kanjo -ErrorAction SilentlyContinue | Measure-Object).Count)"

if (Test-Path "$share\winget") {
    Log "--- 9 winget"
    $mf = "$w\winget-manifest"; New-Item -ItemType Directory -Force $mf | Out-Null
    Copy-Item "$share\winget\*" $mf -Recurse -Force
    $elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    Log "this script runs elevated: $elevated"
    $wg = $null
    for ($i = 0; $i -lt 12 -and -not $wg; $i++) {
        $c = Get-Command winget -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($c) {
            $v = & $c.Source --version 2>&1 | Out-String
            if ($LASTEXITCODE -eq 0) { $wg = $c.Source; Log "winget: $($v.Trim()) at $wg" }
        }
        if (-not $wg -and $i -eq 0) {
            try { Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe -ErrorAction Stop; Log "App Installer registered for this user" } catch { Log "App Installer registration: $_" }
        }
        if (-not $wg) { Start-Sleep 10 }
    }
    if (-not $wg) {
        Log "winget: not available in this Windows image (no working winget command after registering App Installer and waiting 2 minutes); the WinGet step did not run"
    } else {
        $runner = "$w\winget-run.ps1"
        Set-Content $runner -Encoding UTF8 -Value @'
param([string]$Winget, [string]$Manifest, [string]$Out, [switch]$EnableLocalManifests)
$ErrorActionPreference = 'Continue'
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$lines = @("token elevated: $admin")
if ($EnableLocalManifests) {
    $lines += @(& $Winget settings --enable LocalManifestFiles 2>&1 | ForEach-Object { "$_" })
    $lines += "settings --enable LocalManifestFiles: exit $LASTEXITCODE"
}
$lines += @(& $Winget install --manifest $Manifest --accept-package-agreements --accept-source-agreements --disable-interactivity 2>&1 | ForEach-Object { "$_" })
Set-Content -Path "$Out.tmp" -Encoding UTF8 -Value ($lines + "exit $LASTEXITCODE")
Move-Item -Force "$Out.tmp" $Out
'@
        function Read-WingetRun($file, $label) {
            if (-not (Test-Path $file)) { Log "winget install ${label}: no result within 10 minutes"; return '' }
            $res = @(Get-Content $file)
            Log "winget install ${label}: $($res[-1])"; Log (($res | Select-Object -SkipLast 1) -join "`n")
            if ($res[-1] -ne 'exit 0') { return '' }
            return "$label, $($res[0])"
        }
        function Wait-File($file) { for ($j = 0; $j -lt 600 -and -not (Test-Path $file); $j++) { Start-Sleep 1 } }
        if ($elevated) {
            $en = & $wg settings --enable LocalManifestFiles 2>&1 | Out-String
            Log "winget settings --enable LocalManifestFiles (elevated): exit $LASTEXITCODE $($en.Trim())"
        } else {
            Log "LocalManifestFiles was not enabled before the non-elevated attempt, because enabling it needs elevation; a failure of that attempt can come from the setting"
        }
        $limitedOut = "$w\winget-limited.txt"
        $runArgs = "-NoProfile -ExecutionPolicy Bypass -File `"$runner`" -Winget `"$wg`" -Manifest `"$mf`" -Out `"$limitedOut`""
        if ($elevated) {
            $task = 'kanjo-winget-limited'
            try {
                $principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
                Register-ScheduledTask -TaskName $task -Action (New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $runArgs) -Principal $principal -Force | Out-Null
                Start-ScheduledTask -TaskName $task
                Wait-File $limitedOut
            } catch { Log "non-elevated run through a scheduled task with a limited token failed: $_" }
            Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue
            $how = 'non-elevated (scheduled task with a limited token)'
        } else {
            & powershell -NoProfile -ExecutionPolicy Bypass -File $runner -Winget $wg -Manifest $mf -Out $limitedOut
            $how = 'non-elevated'
        }
        $installed = Read-WingetRun $limitedOut $how
        if (-not $installed) {
            $elevatedOut = "$w\winget-elevated.txt"
            if ($elevated) {
                & powershell -NoProfile -ExecutionPolicy Bypass -File $runner -Winget $wg -Manifest $mf -Out $elevatedOut
            } else {
                try {
                    Start-Process powershell -Verb RunAs -ArgumentList ("-NoProfile -ExecutionPolicy Bypass -File `"$runner`" -Winget `"$wg`" -Manifest `"$mf`" -Out `"$elevatedOut`" -EnableLocalManifests") | Out-Null
                    Wait-File $elevatedOut
                } catch { Log "elevated start refused: $_" }
            }
            $installed = Read-WingetRun $elevatedOut 'elevated'
        }
        if (-not $installed) { Log "winget installed: no" } else {
            Log "winget installed: yes ($installed)"
            $env:Path = (([Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')) -split ';' | Where-Object { $_ -and $_ -notlike '*Programs\Kanjo*' }) -join ';'
            $onPath = @(Get-Command kanjo -All -ErrorAction SilentlyContinue | ForEach-Object { $_.Source })
            Log "kanjo on PATH after winget install: $($onPath -join '; ')"
            $wexe = $onPath | Select-Object -First 1
            if (-not $wexe) { $wexe = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages", "$env:ProgramFiles\WinGet\Packages" -Recurse -File -Filter 'kanjo.exe' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName }
            if (-not $wexe) { Log "winget kanjo exe not found on PATH or under the WinGet package directories" } else {
                Log "winget kanjo exe: $wexe (link target: $((Get-Item $wexe).Target))"
                Log (& $wexe --version 2>&1 | Out-String)
                Set-Content "$claude\settings.json" '{"model":"opus"}' -Encoding ASCII
                $stale = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Kanjo
                if ($stale) { Log "Run key before up with the WinGet copy, removed: $stale"; Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Kanjo -ErrorAction SilentlyContinue }
                Log "--- kanjo up with the WinGet copy"; Log (& $wexe --lang en up --yes --no-open 2>&1 | Out-String)
                Start-Sleep 8
                $settingsAfter = Get-Content "$claude\settings.json" -Raw
                $cmd = ''; try { $cmd = ($settingsAfter | ConvertFrom-Json).statusLine.command } catch { Log "settings.json ERR $_" }
                if (-not $cmd) { $cmd = '(none)' }
                $run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Kanjo
                if (-not $run) { $run = '(none)' }
                Log "winget recorded in settings.json: $cmd"
                Log "winget recorded in Run key: $run"
                Log "--- settings.json after up with the WinGet copy"; Log $settingsAfter
                Log "--- uninstall with the WinGet copy"; Log (& $wexe --lang en uninstall --yes 2>&1 | Out-String)
            }
        }
    }
}
Log "=== done $(Get-Date -Format o)"
Set-Content "$share\done.txt" 'done'
