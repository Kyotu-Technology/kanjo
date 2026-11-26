#!/bin/sh
# Tests scripts/gen-homebrew-formula.sh, scripts/gen-winget-manifests.sh and
# scripts/release-packages.sh, which the release workflow runs, against a
# synthetic SHA256SUMS: every URL must sit in the right platform
# block and be paired with the checksum of its own archive, the three WinGet
# manifests must agree on identifier, version and schema, and a missing,
# repeated or malformed entry must stop the generator without output.
#
# Usage: scripts/test-packaging.sh
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
BREW="$REPO/scripts/gen-homebrew-formula.sh"
WINGET="$REPO/scripts/gen-winget-manifests.sh"
VERSION=9.9.9
URL=https://github.com/Kyotu-Technology/kanjo
BASE="$URL/releases/download/v$VERSION"
WINZIP="kanjo-$VERSION-x86_64-pc-windows-msvc.zip"
ID=Kyotu.Kanjo
SCHEMA=1.28.0

WORK=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-packaging-test.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

failures=0
checks=0
fail() {
	failures=$((failures + 1))
	printf 'FAIL: %s\n' "$*"
}
check() {
	checks=$((checks + 1))
	"$@"
}
fake_hash() {
	if command -v sha256sum >/dev/null 2>&1; then
		printf '%s' "$1" | sha256sum | awk '{ print $1 }'
	else
		printf '%s' "$1" | shasum -a 256 | awk '{ print $1 }'
	fi
}
upper() { printf '%s' "$1" | tr 'a-f' 'A-F'; }

for f in "kanjo-$VERSION-aarch64-apple-darwin.tar.gz" "kanjo-$VERSION-x86_64-apple-darwin.tar.gz" \
	"kanjo-$VERSION-aarch64-unknown-linux-musl.tar.gz" "kanjo-$VERSION-x86_64-unknown-linux-musl.tar.gz" \
	"$WINZIP" "kanjo-$VERSION-x86_64-unknown-linux-musl.cdx.json" install.sh install.ps1; do
	printf '%s  %s\n' "$(fake_hash "$f")" "$f"
done >"$WORK/SHA256SUMS"

has() { grep -q -F -x -- "$2" "$1" || fail "${1##*/} lacks the line: $2"; }
ascii() { if LC_ALL=C grep -n '[^ -~]' "$1" >/dev/null; then fail "${1##*/} has a tab or a non-ASCII character"; fi; }

# Homebrew formula
"$BREW" "$VERSION" "$WORK/SHA256SUMS" >"$WORK/kanjo.rb"
F="$WORK/kanjo.rb"
check has "$F" "class Kanjo < Formula"
check has "$F" "  license \"Apache-2.0\""
check has "$F" "    bin.install \"kanjo\""
check has "$F" "    assert_match version.to_s, shell_output(\"#{bin}/kanjo --version\")"
check ascii "$F"
# Prints "<os> <cpu> <target> <sha256>" for every url line, from the blocks around it.
awk '
	/^  on_macos do$/ { os = "macos" }
	/^  on_linux do$/ { os = "linux" }
	/^    on_arm do$/ { cpu = "arm" }
	/^    on_intel do$/ { cpu = "intel" }
	/^      url "/ { split($2, p, "/"); file = p[length(p)]; sub(/"$/, "", file); want = 1; next }
	want && /^      sha256 "/ { h = $2; gsub(/"/, "", h); print os, cpu, file, h; want = 0 }
' "$F" >"$WORK/formula.map"
for row in macos/arm/aarch64-apple-darwin macos/intel/x86_64-apple-darwin \
	linux/arm/aarch64-unknown-linux-musl linux/intel/x86_64-unknown-linux-musl; do
	os=${row%%/*}
	cpu=${row#*/}
	cpu=${cpu%%/*}
	file="kanjo-$VERSION-${row##*/}.tar.gz"
	check has "$WORK/formula.map" "$os $cpu $file $(fake_hash "$file")"
	check has "$F" "      url \"$BASE/$file\""
done
[ "$(wc -l <"$WORK/formula.map" | tr -d ' ')" = 4 ] || fail "the formula has $(wc -l <"$WORK/formula.map") url lines, expected 4"
"$BREW" "v$VERSION" "$WORK/SHA256SUMS" >"$WORK/kanjo-v.rb"
check cmp -s "$F" "$WORK/kanjo-v.rb" || fail "a v prefix on the version changes the formula"
if command -v ruby >/dev/null 2>&1; then
	check ruby -c "$F" >/dev/null || fail "kanjo.rb is not valid Ruby"
else
	echo "test-packaging: ruby is not installed; skipping the Ruby syntax check"
fi

# WinGet manifests
"$WINGET" "$VERSION" "$WORK/SHA256SUMS" "$WORK/winget"
V="$WORK/winget/$ID.yaml"
I="$WORK/winget/$ID.installer.yaml"
L="$WORK/winget/$ID.locale.en-US.yaml"
for m in "$V:version" "$I:installer" "$L:defaultLocale"; do
	file=${m%:*}
	type=${m##*:}
	[ -f "$file" ] || {
		fail "${file##*/} was not written"
		continue
	}
	check has "$file" "# yaml-language-server: \$schema=https://aka.ms/winget-manifest.$type.$SCHEMA.schema.json"
	check has "$file" "PackageIdentifier: $ID"
	check has "$file" "PackageVersion: $VERSION"
	check has "$file" "ManifestType: $type"
	check has "$file" "ManifestVersion: $SCHEMA"
	check ascii "$file"
done
[ "$(find "$WORK/winget" -type f | wc -l | tr -d ' ')" = 3 ] || fail "expected exactly three manifest files"
check has "$V" "DefaultLocale: en-US"
check has "$I" "InstallerType: zip"
check has "$I" "NestedInstallerType: portable"
check has "$I" "- RelativeFilePath: kanjo-$VERSION-x86_64-pc-windows-msvc\\kanjo.exe"
check has "$I" "  PortableCommandAlias: kanjo"
check has "$I" "- Architecture: x64"
check has "$I" "  InstallerUrl: $BASE/$WINZIP"
check has "$I" "  InstallerSha256: $(upper "$(fake_hash "$WINZIP")")"
check has "$L" "PackageLocale: en-US"
check has "$L" "License: Apache-2.0"

KANJO_REPO_URL=https://github.com/example/fork "$WINGET" "$VERSION" "$WORK/SHA256SUMS" "$WORK/fork"
check has "$WORK/fork/$ID.installer.yaml" "  InstallerUrl: https://github.com/example/fork/releases/download/v$VERSION/$WINZIP"
KANJO_REPO_URL=https://github.com/example/fork "$BREW" "$VERSION" "$WORK/SHA256SUMS" >"$WORK/fork.rb"
if grep -q -F "$URL" "$WORK/fork.rb"; then fail "KANJO_REPO_URL does not replace every URL in the formula"; fi

# The release step: formula and manifest zip in the dist directory, byte-identical when repeated.
if command -v zip >/dev/null 2>&1 && command -v unzip >/dev/null 2>&1; then
	for run in 1 2; do
		mkdir -p "$WORK/dist$run"
		cp "$WORK/SHA256SUMS" "$WORK/dist$run/"
		VERSION=$VERSION SOURCE_DATE_EPOCH=1700000000 "$REPO/scripts/release-packages.sh" "$WORK/dist$run" >/dev/null
	done
	Z="winget-manifests-$VERSION.zip"
	check cmp -s "$WORK/dist1/$Z" "$WORK/dist2/$Z" || fail "two runs of release-packages.sh give different zips"
	check cmp -s "$WORK/dist1/kanjo.rb" "$F" || fail "release-packages.sh writes a different kanjo.rb"
	unzip -Z1 "$WORK/dist1/$Z" | grep -v '/$' >"$WORK/zip.list"
	for m in "$ID.yaml" "$ID.installer.yaml" "$ID.locale.en-US.yaml"; do
		check has "$WORK/zip.list" "manifests/k/Kyotu/Kanjo/$VERSION/$m"
	done
	[ "$(wc -l <"$WORK/zip.list" | tr -d ' ')" = 3 ] || fail "the manifest zip holds more than the three manifests"
	mkdir -p "$WORK/unzipped"
	(cd "$WORK/unzipped" && unzip -q "$WORK/dist1/$Z")
	check cmp -s "$WORK/unzipped/manifests/k/Kyotu/Kanjo/$VERSION/$ID.installer.yaml" "$I" || fail "the zip holds a different installer manifest"
else
	echo "test-packaging: zip or unzip is not installed; skipping the release-packages.sh check"
fi

# Bad input must stop both generators, and the formula generator must print nothing.
# $1 = case name, $2 = SHA256SUMS file, $3 = version
expect_refusal() {
	checks=$((checks + 1))
	if "$BREW" "$3" "$2" >"$WORK/bad.rb" 2>"$WORK/bad.err"; then
		fail "[$1] gen-homebrew-formula.sh accepted it"
	elif [ -s "$WORK/bad.rb" ]; then
		fail "[$1] gen-homebrew-formula.sh printed a partial formula"
	fi
	rm -rf "$WORK/bad"
	if "$WINGET" "$3" "$2" "$WORK/bad" 2>"$WORK/bad.err"; then fail "[$1] gen-winget-manifests.sh accepted it"; fi
}
grep -v -F "aarch64-apple-darwin.tar.gz" "$WORK/SHA256SUMS" >"$WORK/missing-mac"
grep -v -F "$WINZIP" "$WORK/SHA256SUMS" >"$WORK/missing-win"
cat "$WORK/missing-mac" >"$WORK/missing-both"
grep -F "aarch64-apple-darwin.tar.gz" "$WORK/SHA256SUMS" >>"$WORK/missing-both"
grep -v -F "$WINZIP" "$WORK/missing-both" >"$WORK/missing-only-win"
cat "$WORK/SHA256SUMS" "$WORK/SHA256SUMS" >"$WORK/repeated"
sed 's/^[0-9a-f]/z/' "$WORK/SHA256SUMS" >"$WORK/malformed"
sed 's/^[0-9a-f]//' "$WORK/SHA256SUMS" >"$WORK/short"
expect_refusal "repeated entries" "$WORK/repeated" "$VERSION"
expect_refusal "malformed checksum" "$WORK/malformed" "$VERSION"
expect_refusal "short checksum" "$WORK/short" "$VERSION"
expect_refusal "no such file" "$WORK/none" "$VERSION"
expect_refusal "incomplete version" "$WORK/SHA256SUMS" "9.9"
expect_refusal "shell characters in the version" "$WORK/SHA256SUMS" "9.9.9;id"
if "$BREW" "$VERSION" "$WORK/missing-mac" >"$WORK/bad.rb" 2>"$WORK/bad.err" || [ -s "$WORK/bad.rb" ]; then
	fail "gen-homebrew-formula.sh accepted a SHA256SUMS without the Apple silicon archive"
fi
grep -q -F "aarch64-apple-darwin.tar.gz" "$WORK/bad.err" || fail "the error does not name the missing archive"
if "$WINGET" "$VERSION" "$WORK/missing-only-win" "$WORK/bad-win" 2>"$WORK/bad.err"; then
	fail "gen-winget-manifests.sh accepted a SHA256SUMS without the Windows archive"
fi
grep -q -F "$WINZIP" "$WORK/bad.err" || fail "the error does not name the missing Windows archive"
checks=$((checks + 2))

if [ "$failures" -gt 0 ]; then
	echo "test-packaging: $failures of $checks checks failed"
	exit 1
fi
echo "test-packaging: all $checks checks passed"
