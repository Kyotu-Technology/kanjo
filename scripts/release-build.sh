#!/bin/sh
# Builds the kanjo binary for one target with the flags a release uses, so the
# release workflow and the CI check of the Windows build cannot drift apart.
# Paths are remapped (the checkout to /build/kanjo, the cargo home to /cargo),
# and the MSVC C runtime is linked statically so kanjo.exe needs no Visual C++
# redistributable. --target is always passed, so the flags never reach build
# scripts or proc macros.
#
# Usage: TARGET=<target triple> scripts/release-build.sh [cargo profile, default release]
set -eu

: "${TARGET:?set TARGET to a target triple}"
profile=${1:-release}

ws=$(cd "$(dirname "$0")/.." && pwd)
ch="${CARGO_HOME:-$HOME/.cargo}"
if command -v cygpath >/dev/null 2>&1; then
	ws=$(cygpath -w "$ws")
	ch=$(cygpath -w "$ch")
fi
us=$(printf '\037')
flags="--remap-path-prefix=$ws=/build/kanjo$us--remap-path-prefix=$ch=/cargo"
case "$TARGET" in
*-windows-msvc) flags="$flags$us-Ctarget-feature=+crt-static" ;;
esac
CARGO_ENCODED_RUSTFLAGS=$flags
export CARGO_ENCODED_RUSTFLAGS

cd "$(dirname "$0")/.."
cargo auditable build --profile "$profile" --locked -p kanjo --target "$TARGET"
