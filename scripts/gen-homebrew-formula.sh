#!/bin/sh
# Writes the Homebrew formula kanjo.rb for a release to stdout. The formula
# installs the prebuilt binaries from the GitHub release archives for macOS and
# Linux on arm64 and x86_64, pinned to the checksums in that release's
# SHA256SUMS. The release workflow attaches the result to the release, and the
# workflow in packaging/homebrew/tap copies it into the tap repository.
#
# Usage: scripts/gen-homebrew-formula.sh <version> <SHA256SUMS> >kanjo.rb
#   KANJO_REPO_URL overrides the repository (default https://github.com/Kyotu-Technology/kanjo).
set -eu

[ $# -eq 2 ] || {
	echo "usage: ${0##*/} <version> <SHA256SUMS>" >&2
	exit 2
}
version=${1#v}
sums=$2
repo=${KANJO_REPO_URL:-https://github.com/Kyotu-Technology/kanjo}

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

base="$repo/releases/download/v$version"
mac_arm=$(sha256_for "kanjo-$version-aarch64-apple-darwin.tar.gz")
mac_intel=$(sha256_for "kanjo-$version-x86_64-apple-darwin.tar.gz")
linux_arm=$(sha256_for "kanjo-$version-aarch64-unknown-linux-musl.tar.gz")
linux_intel=$(sha256_for "kanjo-$version-x86_64-unknown-linux-musl.tar.gz")

cat <<EOF
class Kanjo < Formula
  desc "Offline cost and limit monitor for Claude Code"
  homepage "$repo"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "$base/kanjo-$version-aarch64-apple-darwin.tar.gz"
      sha256 "$mac_arm"
    end
    on_intel do
      url "$base/kanjo-$version-x86_64-apple-darwin.tar.gz"
      sha256 "$mac_intel"
    end
  end

  on_linux do
    on_arm do
      url "$base/kanjo-$version-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$base/kanjo-$version-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "kanjo"
    prefix.install "LICENSE", "NOTICE"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/kanjo --version")
  end
end
EOF
