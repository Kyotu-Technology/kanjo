#!/bin/sh
# Generates the package definitions of a release from its SHA256SUMS: the
# Homebrew formula <dist dir>/kanjo.rb and <dist dir>/winget-manifests-<version>.zip,
# which holds the WinGet manifests in the layout of microsoft/winget-pkgs
# (manifests/k/Kyotu/Kanjo/<version>/). The zip has sorted entries and every
# timestamp set to SOURCE_DATE_EPOCH, so it depends only on its contents.
#
# Usage: VERSION=0.1.0 SOURCE_DATE_EPOCH=<unix time> scripts/release-packages.sh <dist dir>
#   <dist dir> must contain SHA256SUMS. KANJO_REPO_URL is passed on to the generators.
set -eu

dist=$1
: "${VERSION:?set VERSION}"
: "${SOURCE_DATE_EPOCH:?set SOURCE_DATE_EPOCH}"

scripts=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

"$scripts/gen-homebrew-formula.sh" "$VERSION" "$dist/SHA256SUMS" >"$stage/kanjo.rb"
mv "$stage/kanjo.rb" "$dist/kanjo.rb"
mkdir -p "$stage/zip"
"$scripts/gen-winget-manifests.sh" "$VERSION" "$dist/SHA256SUMS" "$stage/zip/manifests/k/Kyotu/Kanjo/${VERSION#v}"
find "$stage/zip" -type d -exec chmod 0755 {} +
find "$stage/zip" -type f -exec chmod 0644 {} +
find "$stage/zip" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
zip="$dist/winget-manifests-${VERSION#v}.zip"
rm -f "$zip"
(cd "$stage/zip" && find manifests -print | LC_ALL=C sort | TZ=UTC zip -X -9 -q -@ "$zip")
echo "release-packages: $dist/kanjo.rb"
echo "release-packages: $zip"
