#!/bin/sh
# Installs and runs Kanjo on a real Windows 11 in a throwaway virtual machine:
# cross-builds kanjo.exe (x86_64-pc-windows-gnu), packs it with
# scripts/release-package.sh, lets Windows install itself unattended
# (dockurr/windows, evaluation image downloaded from Microsoft, needs KVM) and
# runs scripts/windows-vm-test.ps1 inside it at first logon.
#
# Takes about half an hour and 20 GB of disk in a temporary directory, which is
# removed at the end together with the machine. Needs Docker and /dev/kvm.
# Usage: scripts/windows-vm-test.sh        (prints the result, exits 1 on failure)
set -eu

repo=$(cd "$(dirname "$0")/.." && pwd)
[ -d "$repo/web/dist" ] || { echo "windows-vm-test: build the UI first (cd web && npm ci && npm run build)" >&2; exit 1; }
[ -w /dev/kvm ] || { echo "windows-vm-test: /dev/kvm is not available" >&2; exit 1; }

work=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-winvm.XXXXXX")
name="kanjo-winvm-$$"
cleanup() {
	docker rm -f "$name" >/dev/null 2>&1 || true
	docker run --rm -v "$work":/w alpine:3 rm -rf /w/storage /w/shared >/dev/null 2>&1 || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM

mkdir -p "$work/shared/bins/bin-x86_64-pc-windows-gnu" "$work/shared/dist" "$work/oem" "$work/storage"
chmod -R 777 "$work"

echo "windows-vm-test: building kanjo.exe"
docker run --rm -v "$repo":/src:ro -v "$work/shared/bins/bin-x86_64-pc-windows-gnu":/out -w /src -e CARGO_TARGET_DIR=/tmp/target rust:1 sh -ec '
	apt-get update -qq >/dev/null
	apt-get install -y -qq mingw-w64 perl make >/dev/null
	rustup target add x86_64-pc-windows-gnu >/dev/null
	cargo build --release --locked -p kanjo --target x86_64-pc-windows-gnu
	cp /tmp/target/x86_64-pc-windows-gnu/release/kanjo.exe /out/
	chmod 666 /out/kanjo.exe'

VERSION=0.1.0 SOURCE_DATE_EPOCH=$(git -C "$repo" log -1 --format=%ct) sh "$repo/scripts/release-package.sh" "$work/shared/bins" "$work/shared/dist"
(cd "$work/shared/dist" && sha256sum kanjo-* > SHA256SUMS)
cp "$repo/install/install.ps1" "$work/shared/"
cp -r "$repo/crates/kanjo-ingest/tests/fixtures/claude/projects" "$work/shared/projects"
now=$(date +%s)
printf '{"cwd":"C:\\\\work\\\\app","session_id":"winvm-session-1","model":{"id":"claude-opus-5-5","display_name":"Opus"},"version":"2.1.332","cost":{"total_cost_usd":2.5},"context_window":{"context_window_size":200000,"used_percentage":30},"rate_limits":{"five_hour":{"used_percentage":47,"resets_at":%s},"seven_day":{"used_percentage":63,"resets_at":%s}}}\n' "$((now + 14400))" "$((now + 345600))" > "$work/shared/payload.json"
cp "$repo/scripts/windows-vm-test.ps1" "$work/oem/run-test.ps1"
printf '@echo off\r\npowershell -NoProfile -ExecutionPolicy Bypass -File "C:\\OEM\\run-test.ps1" > "C:\\OEM\\bat.log" 2>&1\r\n' > "$work/oem/install.bat"
chmod -R a+rwX "$work" 2>/dev/null || true

echo "windows-vm-test: installing Windows 11 (this is the slow part)"
docker run -d --name "$name" --device=/dev/kvm --device=/dev/net/tun --cap-add NET_ADMIN \
	-e VERSION=11 -e RAM_SIZE=8G -e CPU_CORES=4 -e DISK_SIZE=48G \
	-v "$work/storage":/storage -v "$work/oem":/oem -v "$work/shared":/data \
	--stop-timeout 60 dockurr/windows >/dev/null

waited=0
while [ ! -f "$work/shared/done.txt" ]; do
	[ "$waited" -lt 5400 ] || { echo "windows-vm-test: no result after 90 minutes" >&2; exit 1; }
	[ "$(docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null)" = true ] || { echo "windows-vm-test: the virtual machine stopped" >&2; exit 1; }
	sleep 20
	waited=$((waited + 20))
done

result="$work/shared/result.txt"
tr -d '\r' < "$result"
fail=0
for expect in "SHA-256 checksum: verified" "exe exists: True" "health: {\"status\":\"ok\"" "ui: 200" "encrypted=True" "watcher=True" "service=True" "confidence=official" "kanjo processes after uninstall: 0"; do
	grep -qF "$expect" "$result" || { echo "windows-vm-test: MISSING: $expect" >&2; fail=1; }
done
grep -qE "watcher: requests before=([0-9]+) after=" "$result" && ! grep -qE "watcher: requests before=([0-9]+) after=\1 " "$result" || { echo "windows-vm-test: the watcher did not pick up the appended request" >&2; fail=1; }
[ "$fail" -eq 0 ] && echo "windows-vm-test: passed"
exit "$fail"
