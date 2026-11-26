#!/bin/sh
# Packs the per-target binaries downloaded by the release workflow into
# kanjo-<version>-<target>.tar.gz (Unix) or .zip (Windows), with fixed
# ordering, ownership and timestamps so the archives depend only on their
# contents and SOURCE_DATE_EPOCH. Each archive holds one directory,
# kanjo-<target>/, without the version, so the path of the binary inside an
# unpacked archive stays the same from one release to the next.
#
# Usage: VERSION=0.1.0 SOURCE_DATE_EPOCH=<unix time> scripts/release-package.sh <bins dir> <dist dir>
#   <bins dir> holds one bin-<target>/ directory per target with kanjo or kanjo.exe inside.
set -eu

bins=$1
dist=$2
: "${VERSION:?set VERSION}"
: "${SOURCE_DATE_EPOCH:?set SOURCE_DATE_EPOCH}"

repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$dist"
dist=$(cd "$dist" && pwd)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

found=0
for dir in "$bins"/bin-*; do
	[ -d "$dir" ] || continue
	target=${dir##*/bin-}
	name="kanjo-$VERSION-$target"
	top="kanjo-$target"
	root="$stage/$top"
	mkdir -p "$root"
	chmod 0755 "$root"
	case "$target" in
	*windows*) exe=kanjo.exe ;;
	*) exe=kanjo ;;
	esac
	[ -f "$dir/$exe" ] || {
		echo "release-package: $dir/$exe is missing" >&2
		exit 1
	}
	cp "$dir/$exe" "$root/$exe"
	chmod 0755 "$root/$exe"
	for doc in LICENSE NOTICE CHANGELOG.md README.md SECURITY.md; do
		if [ -f "$repo/$doc" ]; then
			cp "$repo/$doc" "$root/$doc"
			chmod 0644 "$root/$doc"
		fi
	done
	find "$root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
	case "$target" in
	*windows*)
		(cd "$stage" && find "$top" -print | LC_ALL=C sort | TZ=UTC zip -X -9 -q -@ "$dist/$name.zip")
		;;
	*)
		(cd "$stage" && LC_ALL=C tar --sort=name --format=gnu --owner=0 --group=0 --numeric-owner \
			--mtime="@$SOURCE_DATE_EPOCH" -cf - "$top" | gzip -9 -n >"$dist/$name.tar.gz")
		;;
	esac
	echo "release-package: $name"
	found=$((found + 1))
done

[ "$found" -gt 0 ] || {
	echo "release-package: no bin-* directories in $bins" >&2
	exit 1
}
