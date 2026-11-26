#!/bin/sh
# Checks a built kanjo binary before it is published: it starts, it carries
# its cargo-auditable dependency list, and it links nothing that the operating
# system does not ship. On Linux that means no NEEDED entry at all (static
# musl), on macOS only /usr/lib and /System/Library, and on Windows only system
# DLLs, so a dynamic MSVC runtime (vcruntime140.dll, the api-ms-win-crt-* UCRT
# sets) fails the check.
#
# Usage: scripts/check-binary.sh <binary> <target triple>
#        scripts/check-binary.sh --self-check
#   OBJDUMP  tool that lists PE imports (default: llvm-objdump from the Rust
#            toolchain's llvm-tools component, then llvm-objdump or objdump on PATH)
set -eu

say() { printf 'check-binary: %s\n' "$*"; }
fail() {
	printf 'check-binary: FAIL: %s\n' "$*" >&2
	exit 1
}

# Reads DLL names, one per line, and prints the ones that are not part of Windows.
windows_violations() {
	tr -d '\r' | awk '
	{ $0 = tolower($0) }
	NF == 0 { next }
	/^api-ms-win-crt-/ { print; next }
	/^(api-ms-win-core-|api-ms-win-security-|api-ms-win-eventing-|ext-ms-win-)/ { next }
	/^(kernel32|kernelbase|ntdll|advapi32|user32|gdi32|ws2_32|mswsock|crypt32|bcrypt|bcryptprimitives|ncrypt|secur32|userenv|shell32|shlwapi|ole32|oleaut32|combase|propsys|runtimeobject|iphlpapi|powrprof|psapi|dbghelp|version|rpcrt4|netapi32|wintrust|uxtheme|dwmapi|comctl32|winmm|synchronization)\.dll$/ { next }
	{ print }'
}

self_check() {
	good=$(printf '%s\n' KERNEL32.dll ntdll.dll bcryptprimitives.dll api-ms-win-core-synch-l1-2-0.dll \
		ADVAPI32.dll WS2_32.dll USER32.dll CRYPT32.dll ole32.dll | windows_violations | wc -l | tr -d ' ')
	bad=$(printf '%s\n' VCRUNTIME140.dll vcruntime140_1.dll MSVCP140.dll ucrtbase.dll \
		api-ms-win-crt-runtime-l1-1-0.dll libssl-3-x64.dll sqlite3.dll | windows_violations | wc -l | tr -d ' ')
	[ "$good" -eq 0 ] || fail "classifier self-check: $good system DLL(s) flagged"
	[ "$bad" -eq 7 ] || fail "classifier self-check: flagged $bad of 7 non-system DLLs"
	say "classifier self-check passed"
}

find_objdump() {
	if [ -n "${OBJDUMP:-}" ]; then
		printf '%s\n' "$OBJDUMP"
		return
	fi
	if command -v rustc >/dev/null 2>&1; then
		host=$(rustc -vV | sed -n 's/^host: //p')
		for exe in llvm-objdump llvm-objdump.exe; do
			p="$(rustc --print sysroot)/lib/rustlib/$host/bin/$exe"
			if [ -x "$p" ]; then
				printf '%s\n' "$p"
				return
			fi
		done
	fi
	for exe in llvm-objdump objdump; do
		if command -v "$exe" >/dev/null 2>&1; then
			command -v "$exe"
			return
		fi
	done
	fail "no tool to read the import table; run rustup component add llvm-tools or set OBJDUMP"
}

check_windows() {
	objdump=$(find_objdump)
	dlls=$("$objdump" -p "$1" | sed -n 's/^[[:space:]]*DLL Name:[[:space:]]*//p' | tr -d '\r')
	[ -n "$dlls" ] || fail "$objdump found no imported DLLs in $1, so the check cannot be trusted"
	printf '%s\n' "$dlls" | sed 's/^/  imports /'
	bad=$(printf '%s\n' "$dlls" | windows_violations)
	if [ -n "$bad" ]; then
		printf '%s\n' "$bad" | sed 's/^/  not part of Windows: /' >&2
		fail "$1 imports DLLs that a clean Windows installation does not have (a dynamic C runtime needs the Visual C++ redistributable); build with scripts/release-build.sh, which links the CRT statically"
	fi
	say "$1 imports only Windows system DLLs"
}

check_linux() {
	if readelf -d "$1" | grep -q NEEDED; then
		readelf -d "$1" >&2
		fail "$1 is not statically linked"
	fi
	say "$1 is statically linked"
}

check_macos() {
	otool -L "$1"
	if otool -L "$1" | tail -n +2 | grep -v -E '^[[:space:]]+/(usr/lib|System/Library)/'; then
		fail "$1 links a library outside the OS"
	fi
	say "$1 links only system libraries"
}

if [ "${1:-}" = --self-check ]; then
	self_check
	exit 0
fi
[ $# -eq 2 ] || fail "usage: scripts/check-binary.sh <binary> <target triple>"
bin=$1
target=$2
[ -f "$bin" ] || fail "no such file: $bin"

self_check
case "$target" in
*-linux-*) check_linux "$bin" ;;
*-apple-*) check_macos "$bin" ;;
*-windows-*) check_windows "$bin" ;;
*) fail "no linkage check for target $target" ;;
esac
grep -q -a -F '.dep-v0' "$bin" || fail "no cargo-auditable data in $bin"
say "cargo-auditable data present"
"$bin" --version
