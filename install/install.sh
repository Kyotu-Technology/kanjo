#!/bin/sh
# Kanjo installer for Linux and macOS.
#
# Downloads the release archive for this machine from GitHub, verifies it
# against SHA256SUMS and installs the kanjo binary into ~/.local/bin. No sudo,
# no changes outside the install directory. SHA256SUMS itself is checked
# against its Sigstore signature when cosign is installed (the certificate must
# name this repository's release workflow and tag), and against its minisign
# signature when minisign is installed and a public key is set below. A failed
# check stops the installer; a skipped one is reported with the reason.
# Download it from the latest release of the repository in REPO_URL below, read
# it, and then run:
#
#   sh install.sh
#
# Options:
#   --version <x.y.z>   install this version instead of the latest release
#   --dir <path>        install directory (default: ~/.local/bin)
#   --archive <file>    offline mode: install from a local kanjo-<version>-<target>.tar.gz;
#                       a SHA256SUMS file next to it is used for verification when present
#   --sums <file>       SHA256SUMS to verify an offline archive against
#   --no-verify         offline mode only: install an archive that has no SHA256SUMS
#   --skip-signature    do not run cosign or minisign (SHA-256 is still checked), for
#                       machines where cosign is installed but cannot reach Sigstore
#   --help
set -eu

REPO_URL="https://github.com/kyotu/kanjo"
# Minisign public key of the release signer (the base64 line of minisign.pub).
# Empty until the project publishes its key; signature checks are skipped then.
MINISIGN_PUBKEY="${KANJO_MINISIGN_PUBKEY:-}"
# Releases are signed by .github/workflows/release.yml with a GitHub OIDC token.
COSIGN_ISSUER="https://token.actions.githubusercontent.com"
TARGETS="x86_64-unknown-linux-musl aarch64-unknown-linux-musl x86_64-apple-darwin aarch64-apple-darwin"

say() { printf 'kanjo-install: %s\n' "$*"; }
die() {
	printf 'kanjo-install: error: %s\n' "$*" >&2
	exit 1
}
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
	cat <<'EOF'
Usage: install.sh [--version <x.y.z>] [--dir <path>] [--archive <file> [--sums <file> | --no-verify]] [--skip-signature]

  --version <x.y.z>   install this version instead of the latest release
  --dir <path>        install directory (default: ~/.local/bin)
  --archive <file>    offline mode: install from a local kanjo-<version>-<target>.tar.gz;
                      a SHA256SUMS file next to it is used for verification when present
  --sums <file>       SHA256SUMS to verify an offline archive against
  --no-verify         offline mode only: install an archive that has no SHA256SUMS
  --skip-signature    do not run cosign or minisign (SHA-256 is still checked), for
                      machines where cosign is installed but cannot reach Sigstore
EOF
}

version=""
dir="${HOME:?HOME is not set}/.local/bin"
archive=""
sums=""
no_verify=0
skip_signature=0
while [ $# -gt 0 ]; do
	case "$1" in
	--version)
		[ $# -ge 2 ] || die "--version needs a value"
		version=${2#v}
		shift 2
		;;
	--dir)
		[ $# -ge 2 ] || die "--dir needs a value"
		dir=$2
		shift 2
		;;
	--archive)
		[ $# -ge 2 ] || die "--archive needs a value"
		archive=$2
		shift 2
		;;
	--sums)
		[ $# -ge 2 ] || die "--sums needs a value"
		sums=$2
		shift 2
		;;
	--no-verify)
		no_verify=1
		shift
		;;
	--skip-signature)
		skip_signature=1
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*) die "unknown option: $1 (see --help)" ;;
	esac
done

check_version() {
	case "$1" in
	"" | [0-9]*.[0-9]*.[0-9]*) ;;
	*) die "not a version number: $1" ;;
	esac
	case "$1" in
	*[!0-9A-Za-z.+-]*) die "not a version number: $1" ;;
	esac
}
check_version "$version"

detect_target() {
	os=$(uname -s)
	arch=$(uname -m)
	case "$arch" in
	x86_64 | amd64) arch=x86_64 ;;
	aarch64 | arm64) arch=aarch64 ;;
	*) die "unsupported architecture: $arch (release builds exist for x86_64 and aarch64)" ;;
	esac
	case "$os" in
	Linux)
		libc=glibc
		if ldd --version 2>&1 | grep -qi musl; then
			libc=musl
		elif [ -n "$(find /lib /lib64 -maxdepth 1 -name 'ld-musl-*' 2>/dev/null)" ]; then
			libc=musl
		fi
		say "detected Linux $arch with $libc; the release binary is statically linked against musl and runs on both"
		target="$arch-unknown-linux-musl"
		;;
	Darwin)
		if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
			arch=aarch64
			say "running under Rosetta; installing the native Apple silicon build"
		fi
		target="$arch-apple-darwin"
		;;
	*) die "unsupported operating system: $os (use install.ps1 on Windows)" ;;
	esac
}

sha256_of() {
	if have sha256sum; then
		sha256sum "$1" | awk '{ print $1 }'
	elif have shasum; then
		shasum -a 256 "$1" | awk '{ print $1 }'
	elif have openssl; then
		openssl dgst -sha256 -r "$1" | awk '{ print $1 }'
	else
		die "no SHA-256 tool found (sha256sum, shasum or openssl)"
	fi
}

fetch() {
	if have curl; then
		curl --proto '=https' --tlsv1.2 -fsSL --retry 3 -o "$2" "$1"
	elif have wget; then
		wget --https-only -q -O "$2" "$1"
	else
		die "neither curl nor wget is available; download the archive yourself and use --archive"
	fi
}

latest_version() {
	# The /releases/latest page redirects to /releases/tag/v<version>.
	if have curl; then
		effective=$(curl --proto '=https' --tlsv1.2 -fsSLI -o /dev/null -w '%{url_effective}' "$REPO_URL/releases/latest")
	else
		effective=$(wget --https-only -q --max-redirect=5 -S --spider "$REPO_URL/releases/latest" 2>&1 | awk '/^ *[Ll]ocation: / { l = $2 } END { print l }')
	fi
	v=${effective##*/tag/v}
	case "$v" in
	[0-9]*.[0-9]*.[0-9]*) printf '%s\n' "$v" ;;
	*) die "could not determine the latest release from $REPO_URL (got: $effective); pass --version" ;;
	esac
}

# $1 = archive, $2 = SHA256SUMS
verify_sha256() {
	file=${1##*/}
	expected=$(awk -v f="$file" '$2 == f || $2 == "*" f { print $1; n++ } END { if (n != 1) exit 1 }' "$2") ||
		die "SHA256SUMS has no single entry for $file"
	[ -n "$expected" ] || die "SHA256SUMS has no entry for $file"
	actual=$(sha256_of "$1")
	[ "$actual" = "$expected" ] || die "checksum mismatch for $file (expected $expected, got $actual)"
	sha_status="verified"
}

sha_status="skipped"
cosign_status="skipped"
minisign_status="skipped"
signed=0

# $1 = SHA256SUMS, $2 = SHA256SUMS.sigstore.json (may be missing)
verify_cosign() {
	if [ "$skip_signature" = 1 ]; then
		cosign_status="skipped (--skip-signature)"
		return 0
	fi
	if ! have cosign; then
		cosign_status="skipped (cosign is not installed)"
		return 0
	fi
	[ -f "$2" ] || die "cosign is installed but there is no SHA256SUMS.sigstore.json next to SHA256SUMS, so the signature cannot be checked; copy it from the same release (or pass --skip-signature to rely on the checksum alone)"
	[ -n "$version" ] || die "cosign needs the release version, but it cannot be read from the archive name $name (expected kanjo-<version>-<target>.tar.gz)"
	identity="$REPO_URL/.github/workflows/release.yml@refs/tags/v$version"
	cosign_version=$(cosign version 2>&1 | awk '/^GitVersion:/ { print $2; exit }')
	bundle_flag=""
	case "$cosign_version" in
	v0.* | v1.* | 0.* | 1.*) die "cosign $cosign_version cannot read the release's Sigstore bundle; install cosign 3 or later (or pass --skip-signature)" ;;
	v2.* | 2.*) bundle_flag=--new-bundle-format ;;
	esac
	if cosign verify-blob ${bundle_flag:+"$bundle_flag"} --bundle "$2" \
		--certificate-oidc-issuer "$COSIGN_ISSUER" --certificate-identity "$identity" "$1" >"$tmp/cosign.log" 2>&1; then
		cosign_status="verified (signed by $identity)"
		signed=1
	else
		sed 's/^/  cosign: /' "$tmp/cosign.log" >&2
		die "cosign signature check failed: SHA256SUMS was not signed by $identity; do not install this download. If cosign failed only because this machine cannot reach Sigstore, pass --skip-signature to rely on the checksum alone"
	fi
}

# $1 = SHA256SUMS, $2 = SHA256SUMS.minisig (may be missing)
verify_minisign() {
	if [ "$skip_signature" = 1 ]; then
		minisign_status="skipped (--skip-signature)"
		return 0
	fi
	if [ -z "$MINISIGN_PUBKEY" ]; then
		minisign_status="skipped (no minisign public key is configured in this installer yet)"
		return 0
	fi
	if ! have minisign; then
		minisign_status="skipped (minisign is not installed)"
		return 0
	fi
	[ -f "$2" ] || die "minisign is installed and a public key is configured, but there is no SHA256SUMS.minisig to check"
	minisign -Vm "$1" -x "$2" -P "$MINISIGN_PUBKEY" >/dev/null || die "minisign signature check failed; do not install this download"
	minisign_status="verified"
	signed=1
}

report_verification() {
	say "verification:"
	say "  SHA-256 checksum: $sha_status"
	say "  cosign signature: $cosign_status"
	say "  minisign signature: $minisign_status"
	if [ "$signed" = 0 ]; then
		say "WARNING: no signature was verified. The checksum shows that the archive matches SHA256SUMS from the same source, not that the Kanjo release workflow built it. Install cosign and run the installer again, or verify the download by hand as described in docs/release.md."
	fi
}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-install.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
trap 'exit 130' INT TERM

if [ -n "$archive" ]; then
	[ -f "$archive" ] || die "archive not found: $archive"
	name=${archive##*/}
	case "$name" in
	kanjo-*.tar.gz) ;;
	*) die "expected an archive named kanjo-<version>-<target>.tar.gz, got $name" ;;
	esac
	stem=${name#kanjo-}
	stem=${stem%.tar.gz}
	version=""
	for t in $TARGETS; do
		case "$stem" in *"-$t") version=${stem%-"$t"} ;; esac
	done
	check_version "$version"
	cp "$archive" "$tmp/$name"
	if [ -z "$sums" ] && [ -f "$(dirname "$archive")/SHA256SUMS" ]; then
		sums="$(dirname "$archive")/SHA256SUMS"
	fi
	if [ -n "$sums" ]; then
		[ -f "$sums" ] || die "SHA256SUMS not found: $sums"
		cp "$sums" "$tmp/SHA256SUMS"
		if [ -f "$sums.minisig" ]; then cp "$sums.minisig" "$tmp/SHA256SUMS.minisig"; fi
		if [ -f "$sums.sigstore.json" ]; then cp "$sums.sigstore.json" "$tmp/SHA256SUMS.sigstore.json"; fi
		verify_cosign "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.sigstore.json"
		verify_minisign "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.minisig"
		verify_sha256 "$tmp/$name" "$tmp/SHA256SUMS"
		report_verification
	elif [ "$no_verify" = 1 ]; then
		say "WARNING: --no-verify given; installing an archive whose checksum and signature were not verified"
	else
		die "no SHA256SUMS next to $archive; pass --sums <file>, or --no-verify to install it unverified"
	fi
else
	detect_target
	if [ -z "$version" ]; then version=$(latest_version); fi
	name="kanjo-$version-$target.tar.gz"
	base="$REPO_URL/releases/download/v$version"
	say "downloading $name"
	fetch "$base/$name" "$tmp/$name"
	fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"
	if [ "$skip_signature" = 0 ] && have cosign; then
		fetch "$base/SHA256SUMS.sigstore.json" "$tmp/SHA256SUMS.sigstore.json" ||
			die "cosign is installed but SHA256SUMS.sigstore.json could not be downloaded from the release, so the signature cannot be checked"
	fi
	if [ "$skip_signature" = 0 ] && have minisign && [ -n "$MINISIGN_PUBKEY" ]; then
		fetch "$base/SHA256SUMS.minisig" "$tmp/SHA256SUMS.minisig" || die "could not download SHA256SUMS.minisig"
	fi
	verify_cosign "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.sigstore.json"
	verify_minisign "$tmp/SHA256SUMS" "$tmp/SHA256SUMS.minisig"
	verify_sha256 "$tmp/$name" "$tmp/SHA256SUMS"
	report_verification
fi

mkdir -p "$tmp/x"
tar -xzf "$tmp/$name" -C "$tmp/x"
bin=$(find "$tmp/x" -type f -name kanjo | head -n 1)
[ -n "$bin" ] || die "the archive does not contain a kanjo binary"

mkdir -p "$dir"
cp "$bin" "$dir/kanjo.tmp.$$"
chmod 0755 "$dir/kanjo.tmp.$$"
mv -f "$dir/kanjo.tmp.$$" "$dir/kanjo"
say "installed $dir/kanjo"

case ":${PATH:-}:" in
*":$dir:"*) say "next step: run \"kanjo up\"" ;;
*)
	say "$dir is not on your PATH, so your shell will not find kanjo by name yet"
	say "next step: run \"$dir/kanjo\" up"
	say "to put kanjo on your PATH, add $dir to the start-up file of your shell:"
	say "  bash:       echo 'export PATH=\"$dir:\$PATH\"' >> ~/.bashrc   (~/.bash_profile on macOS)"
	say "  zsh:        echo 'export PATH=\"$dir:\$PATH\"' >> ~/.zshrc"
	say "  fish:       fish_add_path '$dir'"
	say "  PowerShell: Add-Content \$PROFILE '\$env:PATH = \"$dir:\" + \$env:PATH'"
	say "then open a new terminal, and \"kanjo up\" works from anywhere"
	;;
esac
