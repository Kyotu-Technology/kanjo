# Install and first run

Kanjo is an independent open-source project published by Kyotu Technology, not a product of Anthropic. "Claude" and "Claude Code" are Anthropic's names for its products.

Kanjo is a single binary with no runtime dependencies. You can install a release with a script or with Homebrew, or build it from source; then `kanjo up` connects it to Claude Code.

## With the install script

On **Linux and macOS**, run:

```sh
curl -fsSL https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.sh | sh
```

The script detects the platform (Linux x86_64 or aarch64, using a static musl build that runs on any distribution; macOS on Intel or Apple silicon, including a native build when the shell runs under Rosetta), downloads the archive and checks it against the release's `SHA256SUMS`. When cosign is installed, it also checks that `SHA256SUMS` was signed by this repository's release workflow for exactly that version, and stops if not. Without cosign it says that no signature was checked and relies on the checksum alone. It installs `kanjo` into `~/.local/bin`, needs no sudo and changes nothing outside the install directory. If that directory is not on your `PATH`, it says how to add it.

The script is safe to pipe: its whole body is a function called on the last line, so a download that is cut off part way runs nothing. Options go after `sh -s --`, for example `curl -fsSL .../install.sh | sh -s -- --version 0.1.0 --dir ~/bin`. The options are `--version`, `--dir`, `--archive` (install an archive you downloaded, offline), `--sums`, `--no-verify`, `--skip-signature` and `--help`. If you would rather read the script before running it, download `install.sh` from the release and run `sh install.sh --version 0.1.0`.

On **Windows**, run in PowerShell:

```powershell
irm https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.ps1 | iex
```

It performs the same checks, installs `kanjo.exe` into `%LOCALAPPDATA%\Programs\Kanjo`, adds that directory to your user `PATH` and to the `PATH` of the current session, and needs no administrator rights. It leaves no variables, functions or preference settings behind in your session, never calls `exit`, and reports a failed check as a normal error without closing the window. `iex` cannot pass parameters, so to choose a version or a directory use the script block form:

```powershell
& ([scriptblock]::Create((irm https://github.com/Kyotu-Technology/kanjo/releases/latest/download/install.ps1))) -Version 0.1.0
```

The parameters are `-Version`, `-Dir`, `-Archive`, `-Sums`, `-NoVerify`, `-SkipSignature` and `-NoPathUpdate`; switches take no value (`-NoPathUpdate`, not `-NoPathUpdate:$true`). A downloaded copy runs with `powershell -ExecutionPolicy Bypass -File install.ps1 -Version 0.1.0`. Both scripts are tested in CI in the piped and in the downloaded form.

The scripts contact GitHub to download the release. Kanjo itself never does; with `--archive` (`-Archive`) the scripts work offline too. How to verify a download by hand is described in [release.md](release.md#verifying-a-download).

The binaries are built for Linux x86_64 and arm64 (static musl), macOS 11 or newer on Apple silicon and Intel, and Windows x86_64. Windows on ARM64 runs the x86_64 build under emulation. The macOS binaries are not notarized by Apple and the Windows binary is not Authenticode-signed; [release.md](release.md#targets-and-toolchains) says what that means when you first start them.

## With Homebrew

On **macOS and Linux**, Kanjo can also be installed from the Kyotu Technology tap, [Kyotu-Technology/homebrew-tap](https://github.com/Kyotu-Technology/homebrew-tap):

```sh
brew install kyotu-technology/tap/kanjo
kanjo up
```

The formula installs the prebuilt binary of the latest Kanjo release for your platform, the same archive the install script downloads, pinned to its SHA-256; nothing is built from source. How the tap takes the formula from a release is described in [release.md](release.md#install-channels). A formula install is not quarantined by macOS, so the binary starts although it is not notarized. `kanjo up` writes the path of Homebrew's link, `<prefix>/bin/kanjo` (`/opt/homebrew/bin/kanjo` on Apple silicon, `/usr/local/bin/kanjo` on an Intel Mac, `/home/linuxbrew/.linuxbrew/bin/kanjo` on Linux), into `settings.json` and the background service, not the versioned path under `Cellar` that `brew upgrade` removes.

To upgrade:

```sh
brew upgrade kanjo
kanjo service install
```

`brew upgrade` replaces the binary but not the server that is already running; `kanjo service install` restarts the background service on the new version. The status line and the hooks run the new binary straight away. To remove Kanjo, run `kanjo uninstall` first, while the binary is still there, then `brew uninstall kanjo`, and `brew untap kyotu-technology/tap` if you no longer want the tap. The formula prints these steps as caveats when it is installed. If Homebrew refuses to use a third-party tap by the formula's short name, the [tap's README](https://github.com/Kyotu-Technology/homebrew-tap#install-upgrade-and-remove) says what to run.

Under Homebrew, the tap's workflow has installed the formula and run `brew test` on GitHub's macOS and Linux runners; `kanjo up`, the LaunchAgent and `brew upgrade` have not been run under Homebrew on a Mac. [status.md](status.md) has the details.

## From source

You need Rust 1.91 or newer (the `rust-version` in `Cargo.toml`), a C compiler and Perl, because the database layer compiles SQLCipher and OpenSSL from source; on Windows that means the MSVC build tools and Strawberry Perl. The web UI needs Node.js and npm (CI uses Node 24).

```sh
git clone https://github.com/Kyotu-Technology/kanjo
cd kanjo
(cd web && npm ci && npm run build)
cargo build --release -p kanjo
./target/release/kanjo demo
```

The last command opens the dashboard with 30 days of invented data, marked "Demo data" in the top bar and the sidebar; it needs no account and no configuration. The first build compiles the C code and takes a few minutes. Building downloads crates and npm packages; the binary it produces makes no network connections.

## Connecting Claude Code: `kanjo up`

`kanjo up` finds Claude Code's directory (`~/.claude`, or `CLAUDE_CONFIG_DIR`), lists what it is about to do, and asks before changing anything. Running it again is safe. It makes three kinds of change.

**Claude Code's `settings.json`.** Kanjo adds a `statusLine` entry that runs `kanjo statusline` and three hooks, `SessionStart`, `Stop` and `SessionEnd`, that run `kanjo hook` to note when sessions start and end. The commands use the full path of the binary (and `--home DIR` if you use a non-default data directory). If you already have a status line, Kanjo puts itself in front of it and remembers your command in its own `config.toml`; your status line keeps showing. Everything else in the file stays as it was, including key order and indentation. Before changing the file Kanjo copies it to `settings.json.kanjo-backup-<UTC timestamp>` next to it, and the new version is written atomically. If the file is not valid JSON, Kanjo refuses to touch it.

**A per-user background service** that runs `kanjo serve` when you log in, so data is collected even when the dashboard is closed. No administrator rights are involved:

| Platform | What is installed |
| --- | --- |
| Linux | systemd user unit `~/.config/systemd/user/kanjo.service`, enabled and started with `systemctl --user` |
| macOS | LaunchAgent `~/Library/LaunchAgents/io.kyotu.kanjo.plist`, loaded with `launchctl` |
| Windows | a value named `Kanjo` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` that starts a windowless launcher script, `%LOCALAPPDATA%\Kyotu\kanjo\service\kanjo-service.vbs`, at logon (a logon entry, not a Windows service) |

**Kanjo's data directory** (see [Configuration](configuration.md)), with the encrypted database, `config.toml`, the access token and backups. On first start Kanjo imports the transcript history that is still on disk and then follows new requests as they are written.

Finally it opens the dashboard in your browser and signs you in. `--no-hooks` leaves `settings.json` alone, `--no-service` runs the dashboard in the terminal instead of installing a service, `--no-open` skips the browser, and `--yes` skips the question. The **System and privacy** page of the dashboard shows the state of each piece and can connect or disconnect them too.

## Undoing it: `kanjo uninstall`

`kanjo uninstall` removes Kanjo's status line from `settings.json` (putting back your previous status line if Kanjo was chained in front of it), removes its three hooks, and stops and removes the background service. Your Kanjo data stays unless you add `--purge-data`, which deletes the data directory; it refuses while a Kanjo server is still running and refuses a directory that does not look like Kanjo's. Three things are left for you to remove by hand if you want: the `settings.json.kanjo-backup-*` copies, the binary itself (for a Homebrew install, `brew uninstall kanjo`; on Windows also the `PATH` entry the installer added), and, if the database key was kept in the system keyring, its `kanjo` entry.
