#!/bin/sh
# Tests install/install.sh in a temporary directory, offline and online, with
# stub curl, cosign and minisign on a PATH that contains nothing else from the
# machine except basic tools. Never touches the real HOME.
#
# Usage: scripts/test-install.sh
set -eu

REPO=$(cd "$(dirname "$0")/.." && pwd)
INSTALLER="$REPO/install/install.sh"
VERSION=9.9.9
URL=https://github.com/kyotu/kanjo
IDENTITY="$URL/.github/workflows/release.yml@refs/tags/v$VERSION"
ISSUER=https://token.actions.githubusercontent.com

case "$(uname -s)/$(uname -m)" in
Linux/x86_64) TARGET=x86_64-unknown-linux-musl ;;
Linux/aarch64) TARGET=aarch64-unknown-linux-musl ;;
Darwin/arm64) TARGET=aarch64-apple-darwin ;;
Darwin/x86_64) TARGET=x86_64-apple-darwin ;;
*)
	echo "test-install: unsupported platform $(uname -s)/$(uname -m)" >&2
	exit 1
	;;
esac
NAME="kanjo-$VERSION-$TARGET"

WORK=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-install-test.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

mkdir -p "$WORK/rel/$NAME" "$WORK/sys" "$WORK/stubs" "$WORK/home"
printf '#!/bin/sh\necho "kanjo %s (test)"\n' "$VERSION" >"$WORK/rel/$NAME/kanjo"
chmod 0755 "$WORK/rel/$NAME/kanjo"
(cd "$WORK/rel" && tar -czf "$NAME.tar.gz" "$NAME" && rm -rf "$NAME")
sum() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{ print $1 }'; else shasum -a 256 "$1" | awk '{ print $1 }'; fi
}
printf '%s  %s\n' "$(sum "$WORK/rel/$NAME.tar.gz")" "$NAME.tar.gz" >"$WORK/rel/SHA256SUMS"
echo '{"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json"}' >"$WORK/rel/SHA256SUMS.sigstore.json"
echo 'untrusted comment: test' >"$WORK/rel/SHA256SUMS.minisig"

for tool in sh awk tar gzip sha256sum shasum openssl mktemp find cp mv chmod mkdir rm dirname head cat uname sed grep ldd sysctl tr wc sort date env; do
	p=$(command -v "$tool" 2>/dev/null) || continue
	case "$p" in /*) ln -s "$p" "$WORK/sys/$tool" ;; esac
done

cat >"$WORK/stubs/curl" <<'EOF'
#!/bin/sh
out=""
head=0
prev=""
for a in "$@"; do
	case "$prev" in -o) out=$a ;; esac
	case "$a" in -fsSLI) head=1 ;; esac
	prev=$a
	url=$a
done
echo "$url" >>"$STUB_LOG/curl"
if [ "$head" = 1 ]; then
	printf '%s' "$STUB_LATEST"
	exit 0
fi
file=${url#"$STUB_BASE"/}
[ "$file" != "$url" ] && [ -f "$STUB_REL/$file" ] || exit 22
cp "$STUB_REL/$file" "$out"
EOF
cat >"$WORK/stubs/cosign" <<'EOF'
#!/bin/sh
if [ "$1" = version ]; then
	printf 'GitVersion:    %s\n' "${STUB_COSIGN_VERSION:-v3.1.5}"
	exit 0
fi
printf '%s\n' "$*" >>"$STUB_LOG/cosign"
if [ "${STUB_COSIGN_EXIT:-0}" = 0 ]; then echo "Verified OK" >&2; else echo "Error: none of the expected identities matched" >&2; fi
exit "${STUB_COSIGN_EXIT:-0}"
EOF
cat >"$WORK/stubs/minisign" <<'EOF'
#!/bin/sh
printf '%s\n' "$*" >>"$STUB_LOG/minisign"
exit "${STUB_MINISIGN_EXIT:-0}"
EOF
chmod 0755 "$WORK/stubs/curl" "$WORK/stubs/cosign" "$WORK/stubs/minisign"

failures=0
n=0
# $1 = case name, $2 = stubs to provide (space separated), rest = installer arguments.
# Sets $out (combined output), $code (exit status), $dir (install directory).
run() {
	n=$((n + 1))
	label=$1
	stubs=$2
	shift 2
	case_dir="$WORK/case$n"
	dir="$case_dir/bin"
	mkdir -p "$case_dir/path" "$case_dir/log"
	for s in $stubs; do ln -s "$WORK/stubs/$s" "$case_dir/path/$s"; done
	set +e
	out=$(env -i HOME="$WORK/home" TMPDIR="$case_dir" PATH="${EXTRA_PATH:+$EXTRA_PATH:}$case_dir/path:$WORK/sys" \
		STUB_LOG="$case_dir/log" STUB_REL="$WORK/rel" STUB_BASE="$URL/releases/download/v$VERSION" \
		STUB_LATEST="$URL/releases/tag/v$VERSION" STUB_COSIGN_EXIT="${COSIGN_EXIT:-0}" \
		STUB_COSIGN_VERSION="${COSIGN_VERSION:-v3.1.5}" STUB_MINISIGN_EXIT="${MINISIGN_EXIT:-0}" \
		KANJO_MINISIGN_PUBKEY="${PUBKEY:-}" sh "$INSTALLER" --dir "$dir" "$@" 2>&1)
	code=$?
	set -e
}
fail() {
	failures=$((failures + 1))
	printf 'FAIL [%s]: %s\n--- output ---\n%s\n--------------\n' "$label" "$1" "$out"
}
expect_exit() { [ "$code" = "$1" ] || fail "exit $code, expected $1"; }
expect_out() { printf '%s\n' "$out" | grep -q -F -- "$1" || fail "output lacks: $1"; }
expect_no_out() { if printf '%s\n' "$out" | grep -q -F -- "$1"; then fail "output contains: $1"; fi; }
expect_installed() { [ -x "$dir/kanjo" ] || fail "kanjo was not installed"; }
expect_not_installed() { [ ! -e "$dir/kanjo" ] || fail "kanjo was installed although verification failed"; }
expect_log() { grep -q -F -- "$2" "$case_dir/log/$1" 2>/dev/null || fail "$1 was not called with: $2"; }
expect_no_log() { [ ! -s "$case_dir/log/$1" ] || fail "$1 was called: $(cat "$case_dir/log/$1")"; }

ARCHIVE="$WORK/rel/$NAME.tar.gz"

run "offline, no cosign" "" --archive "$ARCHIVE"
expect_exit 0
expect_installed
expect_out "SHA-256 checksum: verified"
expect_out "cosign signature: skipped (cosign is not installed)"
expect_out "WARNING: no signature was verified"

run "offline, cosign verifies" "cosign" --archive "$ARCHIVE"
expect_exit 0
expect_installed
expect_out "cosign signature: verified"
expect_no_out "WARNING: no signature was verified"
expect_log cosign "verify-blob"
expect_log cosign "--certificate-identity $IDENTITY"
expect_log cosign "--certificate-oidc-issuer $ISSUER"
expect_log cosign "--bundle "
expect_log cosign "SHA256SUMS.sigstore.json"

COSIGN_EXIT=1 run "offline, cosign rejects" "cosign" --archive "$ARCHIVE"
expect_exit 1
expect_not_installed
expect_out "cosign signature check failed"
expect_out "none of the expected identities matched"

mkdir -p "$WORK/nobundle"
cp "$ARCHIVE" "$WORK/rel/SHA256SUMS" "$WORK/nobundle/"
run "offline, cosign but no bundle" "cosign" --archive "$WORK/nobundle/$NAME.tar.gz"
expect_exit 1
expect_not_installed
expect_out "SHA256SUMS.sigstore.json"

COSIGN_EXIT=1 run "offline, --skip-signature" "cosign" --archive "$ARCHIVE" --skip-signature
expect_exit 0
expect_installed
expect_no_log cosign
expect_out "cosign signature: skipped (--skip-signature)"
expect_out "WARNING: no signature was verified"

COSIGN_VERSION=v2.6.5 run "offline, cosign 2" "cosign" --archive "$ARCHIVE"
expect_exit 0
expect_log cosign "--new-bundle-format"

COSIGN_VERSION=2.6.5 run "offline, cosign 2 without v" "cosign" --archive "$ARCHIVE"
expect_exit 0
expect_log cosign "--new-bundle-format"

COSIGN_VERSION=v1.13.1 run "offline, cosign 1" "cosign" --archive "$ARCHIVE"
expect_exit 1
expect_not_installed
expect_out "cannot read the release's Sigstore bundle"

COSIGN_VERSION=v3.1.5 run "offline, cosign 3" "cosign" --archive "$ARCHIVE"
expect_exit 0
expect_no_out "--new-bundle-format"
if grep -q -F -- "--new-bundle-format" "$case_dir/log/cosign"; then fail "cosign 3 got --new-bundle-format"; fi

mkdir -p "$WORK/renamed"
cp "$ARCHIVE" "$WORK/renamed/kanjo-local.tar.gz"
run "offline, --no-verify with a renamed archive" "cosign" --archive "$WORK/renamed/kanjo-local.tar.gz" --no-verify
expect_exit 0
expect_installed
expect_no_log cosign

run "online, cosign verifies" "curl cosign"
expect_exit 0
expect_installed
expect_out "cosign signature: verified"
expect_log cosign "--certificate-identity $IDENTITY"
expect_log curl "$URL/releases/download/v$VERSION/SHA256SUMS.sigstore.json"

mkdir -p "$WORK/rel-nobundle"
mv "$WORK/rel/SHA256SUMS.sigstore.json" "$WORK/rel-nobundle/"
run "online, bundle missing from the release" "curl cosign" --version "$VERSION"
expect_exit 1
expect_not_installed
expect_out "SHA256SUMS.sigstore.json"
run "online, no cosign, bundle missing" "curl" --version "$VERSION"
expect_exit 0
expect_installed
expect_out "cosign signature: skipped (cosign is not installed)"
mv "$WORK/rel-nobundle/SHA256SUMS.sigstore.json" "$WORK/rel/"

COSIGN_EXIT=1 run "online, cosign rejects" "curl cosign" --version "$VERSION"
expect_exit 1
expect_not_installed

PUBKEY=RWtestkey run "minisign verifies" "curl minisign" --version "$VERSION"
expect_exit 0
expect_installed
expect_out "minisign signature: verified"
expect_log minisign "-P RWtestkey"

PUBKEY=RWtestkey MINISIGN_EXIT=1 run "minisign rejects" "curl minisign" --version "$VERSION"
expect_exit 1
expect_not_installed
expect_out "minisign signature check failed"

PUBKEY=RWtestkey run "minisign key but no minisign" "curl" --version "$VERSION"
expect_exit 0
expect_out "minisign signature: skipped (minisign is not installed)"

run "no minisign key" "curl minisign" --version "$VERSION"
expect_exit 0
expect_out "minisign signature: skipped (no minisign public key is configured"
expect_no_log minisign

mkdir -p "$WORK/tampered"
cp "$ARCHIVE" "$WORK/rel/SHA256SUMS" "$WORK/rel/SHA256SUMS.sigstore.json" "$WORK/tampered/"
printf 'x' >>"$WORK/tampered/$NAME.tar.gz"
run "tampered archive" "cosign" --archive "$WORK/tampered/$NAME.tar.gz"
expect_exit 1
expect_not_installed
expect_out "checksum mismatch"

run "install directory not on PATH" "" --archive "$ARCHIVE"
expect_exit 0
expect_out "next step: run \"$dir/kanjo\" up"
expect_out "is not on your PATH"
expect_out "bash:"
expect_out "zsh:"
expect_out "fish_add_path"
expect_out "PowerShell:"
expect_no_out 'next step: run "kanjo up"'

EXTRA_PATH="$WORK/case$((n + 1))/bin" run "install directory on PATH" "" --archive "$ARCHIVE"
expect_exit 0
expect_out 'next step: run "kanjo up"'
expect_no_out "is not on your PATH"

if [ "$failures" -gt 0 ]; then
	echo "test-install: $failures failed check(s) in $n case(s)"
	exit 1
fi
echo "test-install: all $n cases passed"
