# Runs inside a fresh Windows virtual machine started by scripts/windows-vm-test.sh.
# Installs the packaged zip with install.ps1, connects a fixture Claude Code
# directory with "kanjo up" (logon service, Windows credential store), checks the
# dashboard, the status line, live file watching and the reports, and uninstalls.
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
$out = & powershell -NoProfile -ExecutionPolicy Bypass -File "$w\install.ps1" -Archive "$w\kanjo-0.1.0-x86_64-pc-windows-gnu.zip" -SkipSignature 2>&1 | Out-String
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
Log "=== done $(Get-Date -Format o)"
Set-Content "$share\done.txt" 'done'
