---
paths:
  - "install/**"
  - "scripts/**"
---

# Installers and scripts

- `install/install.sh` and `scripts/*.sh` are POSIX `sh`, not bash: no arrays, no `[[ ]]`, no `local`, no process substitution. They must pass `shellcheck -s sh` and run under dash and under the bash 3.2 that macOS ships as `sh`.- `install/install.ps1` and the `.ps1` scripts run under both Windows PowerShell 5.1 and PowerShell 7.
- The installers verify what they download (checksum always, the cosign signature when cosign is present) and stop on a mismatch. Do not add a path that installs without the checksum.
- A change to an installer is covered by `scripts/test-install.sh` or `scripts/test-install.ps1`, which run it against stub tools in a temporary directory. Run `sh scripts/test-install.sh` after the change.
- Scripts start with a comment that says what they do and how to call them, use `set -eu`, and clean up their temporary directories in a trap.
- Development scripts may need the network (`cargo-audit.sh` clones the advisory database). The `kanjo` binary and its build never do.
