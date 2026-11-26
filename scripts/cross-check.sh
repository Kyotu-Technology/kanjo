#!/bin/sh
# Builds Kanjo for the platforms a Linux machine cannot run natively, as far as
# containers allow, so platform breakage is found before CI or a release:
#
#   musl     static Linux binary built and run in Alpine
#   windows  x86_64-pc-windows-gnu lint and build, the binary and the test suite run under Wine
#   macos    aarch64 and x86_64 Apple binaries compiled and linked with cargo-zigbuild
#            (they cannot be executed here)
#
# Needs Docker. The source tree is mounted read-only; nothing is written to it.
# Usage: scripts/cross-check.sh [musl|windows|macos]...   (default: all three)
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
[ -d "$repo/web/dist" ] || { echo "cross-check: build the UI first (cd web && npm ci && npm run build)" >&2; exit 1; }
[ $# -gt 0 ] || set -- musl windows macos

run() {
	docker run --rm -v "$repo":/src:ro -w /src -e CARGO_TARGET_DIR=/tmp/target "$@"
}

for what in "$@"; do
	case "$what" in
	musl)
		run rust:1-alpine sh -ec '
			apk add --no-cache musl-dev perl make linux-headers >/dev/null
			cargo build --release --locked -p kanjo
			/tmp/target/release/kanjo --version
			KANJO_HOME=/tmp/home KANJO_NO_KEYRING=1 /tmp/target/release/kanjo demo --seed-only
			KANJO_HOME=/tmp/home KANJO_NO_KEYRING=1 /tmp/target/release/kanjo status'
		;;
	windows)
		# The watcher test is expected to fail under Wine, which does not deliver
		# change notifications for subdirectories; everything else must pass.
		run -e WINEDEBUG=-all -e KANJO_NO_KEYRING=1 -e TS_RS_EXPORT_DIR=/tmp/bindings \
			-e CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine rust:1 sh -ec '
			apt-get update -qq >/dev/null
			apt-get install -y -qq mingw-w64 perl make wine wine64 >/dev/null
			rustup target add x86_64-pc-windows-gnu >/dev/null
			rustup component add clippy >/dev/null 2>&1
			cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
			cargo build --release --locked -p kanjo --target x86_64-pc-windows-gnu
			exe=/tmp/target/x86_64-pc-windows-gnu/release/kanjo.exe
			wine "$exe" --version
			wine "$exe" --home "C:\\kanjo-home" demo --seed-only
			wine "$exe" --home "C:\\kanjo-home" status
			cargo test --workspace --locked --no-fail-fast --target x86_64-pc-windows-gnu \
				-- --skip watcher_picks_up_appends'
		;;
	macos)
		run ghcr.io/rust-cross/cargo-zigbuild:latest sh -ec '
			rustup update stable >/dev/null 2>&1
			rustup default stable >/dev/null 2>&1
			rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
			for target in aarch64-apple-darwin x86_64-apple-darwin; do
				cargo zigbuild --release --locked -p kanjo --target "$target"
				ls -l "/tmp/target/$target/release/kanjo"
			done'
		;;
	*)
		echo "cross-check: unknown platform $what (musl, windows or macos)" >&2
		exit 2
		;;
	esac
done
