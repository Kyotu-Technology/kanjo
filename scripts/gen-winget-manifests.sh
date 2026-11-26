#!/bin/sh
# Writes the WinGet manifests for package Kyotu.Kanjo (version, installer and
# en-US default locale, manifest schema 1.28.0) for a release into <out dir>.
# The installer is the Windows zip of the release with kanjo.exe inside as a
# portable command named kanjo, pinned to its checksum in SHA256SUMS. The
# release workflow packs the files into winget-manifests-<version>.zip; they
# are submitted to microsoft/winget-pkgs by hand (docs/release.md).
#
# Usage: scripts/gen-winget-manifests.sh <version> <SHA256SUMS> <out dir>
#   KANJO_REPO_URL overrides the repository (default https://github.com/Kyotu-Technology/kanjo).
set -eu

[ $# -eq 3 ] || {
	echo "usage: ${0##*/} <version> <SHA256SUMS> <out dir>" >&2
	exit 2
}
version=${1#v}
sums=$2
out=$3
repo=${KANJO_REPO_URL:-https://github.com/Kyotu-Technology/kanjo}
id=Kyotu.Kanjo
schema=1.28.0

die() {
	printf '%s: %s\n' "${0##*/}" "$*" >&2
	exit 1
}

# $1 = file name; prints its SHA-256 from SHA256SUMS, which must list it exactly once.
sha256_for() {
	awk -v f="$1" '
		$2 == f || $2 == "*" f { n++; h = tolower($1) }
		END {
			if (n != 1 || length(h) != 64 || h ~ /[^0-9a-f]/) exit 1
			print h
		}
	' "$sums" || die "$sums has no single valid SHA-256 entry for $1"
}

case "$version" in
[0-9]*.[0-9]*.[0-9]*) ;;
*) die "not a version number: $version" ;;
esac
case "$version" in
*[!0-9A-Za-z.+-]*) die "not a version number: $version" ;;
esac
[ -f "$sums" ] || die "SHA256SUMS not found: $sums"

target=x86_64-pc-windows-msvc
archive="kanjo-$version-$target.zip"
sha=$(sha256_for "$archive")
sha=$(printf '%s' "$sha" | tr 'a-f' 'A-F')
mkdir -p "$out"

cat >"$out/$id.yaml" <<EOF
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
EOF

cat >"$out/$id.installer.yaml" <<EOF
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: kanjo-$version-$target\\kanjo.exe
  PortableCommandAlias: kanjo
Commands:
- kanjo
Installers:
- Architecture: x64
  InstallerUrl: $repo/releases/download/v$version/$archive
  InstallerSha256: $sha
ManifestType: installer
ManifestVersion: $schema
EOF

cat >"$out/$id.locale.en-US.yaml" <<EOF
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
PackageLocale: en-US
Publisher: Kyotu Technology
PublisherUrl: ${repo%/*}
PublisherSupportUrl: $repo/issues
PackageName: Kanjo
PackageUrl: $repo
License: Apache-2.0
LicenseUrl: $repo/blob/v$version/LICENSE
Copyright: Copyright 2026 Kyotu
ShortDescription: Offline cost and limit monitor for Claude Code.
Description: |-
  Kanjo shows what Claude Code costs and how close you are to its 5-hour and weekly limits.
  One binary contains the collector, a local HTTP API and the web dashboard. It reads Claude Code's files on the same machine and sends nothing over the network.
Moniker: kanjo
Tags:
- claude
- claude-code
- cli
- cost
- dashboard
- monitoring
- offline
- usage
ReleaseNotesUrl: $repo/releases/tag/v$version
ManifestType: defaultLocale
ManifestVersion: $schema
EOF
