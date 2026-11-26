#!/bin/sh
# Checks that both installers pin the repository that signs releases.
# Usage: scripts/installer-repo-url.sh <install.sh> <install.ps1> <expected url>
# The installers verify the cosign signature against this URL, so a release
# built from another repository would fail every signature check at install time.
set -eu

[ $# -eq 3 ] || {
	echo "usage: $0 <install.sh> <install.ps1> <expected url>" >&2
	exit 2
}
sh_url=$(sed -n 's/^REPO_URL="\(.*\)"$/\1/p' "$1")
ps_url=$(sed -n "s/^ *\$RepoUrl = '\(.*\)'\r*$/\1/p" "$2")
if [ "$sh_url" != "$3" ] || [ "$ps_url" != "$3" ]; then
	echo "installer-repo-url: installers pin '$sh_url' and '$ps_url', expected '$3'" >&2
	exit 1
fi
