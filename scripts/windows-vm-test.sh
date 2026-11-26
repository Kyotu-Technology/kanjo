#!/bin/sh
# Installs and runs Kanjo on a real Windows 11 in a throwaway virtual machine:
# cross-builds kanjo.exe (x86_64-pc-windows-gnu) and packs it with
# scripts/release-package.sh, or takes a release zip given with --zip, lets
# Windows install itself unattended (dockurr/windows, evaluation image
# downloaded from Microsoft, needs KVM) and runs scripts/windows-vm-test.ps1
# inside it at first logon.
#
# With --zip the build is skipped. A SHA256SUMS next to the zip is used when it
# lists the zip; otherwise the checksum is computed here. With
# --winget-manifest the guest also installs Kanjo with winget from that local
# manifest directory, which downloads the archive the manifest names
# (non-elevated first, elevated when that is refused), runs
# kanjo up with the WinGet copy and records the executable path written into
# settings.json and the Run key.
#
# Takes about half an hour and 20 GB of disk in a temporary directory, which is
# removed at the end together with the machine. Needs Docker and /dev/kvm, and
# unzip for --zip.
# Usage: scripts/windows-vm-test.sh [--zip kanjo-<version>-<target>.zip] [--winget-manifest <dir>]
# Prints the result and exits 0 when every check passed, 1 on a failure, and 2
# when the checks passed but the WinGet step did not run because the Windows
# image has no WinGet.
set -eu

zip=''
manifest=''
while [ $# -gt 0 ]; do
	case $1 in
	--zip)
		[ $# -ge 2 ] || { echo "windows-vm-test: --zip needs a file" >&2; exit 1; }
		zip=$2
		shift 2
		;;
	--winget-manifest)
		[ $# -ge 2 ] || { echo "windows-vm-test: --winget-manifest needs a directory" >&2; exit 1; }
		manifest=$2
		shift 2
		;;
	*)
		echo "windows-vm-test: unknown argument $1 (usage: windows-vm-test.sh [--zip FILE] [--winget-manifest DIR])" >&2
		exit 1
		;;
	esac
done
if [ -n "$zip" ]; then
	[ -f "$zip" ] || { echo "windows-vm-test: $zip is not a file" >&2; exit 1; }
	case $zip in *.zip) ;; *) echo "windows-vm-test: $zip is not a .zip file" >&2; exit 1 ;; esac
	command -v unzip >/dev/null || { echo "windows-vm-test: --zip needs unzip to check the archive" >&2; exit 1; }
	unzip -Z1 "$zip" 2>/dev/null | grep -qE '(^|/)kanjo\.exe$' || { echo "windows-vm-test: $zip is not a zip that holds kanjo.exe" >&2; exit 1; }
	zip=$(cd "$(dirname "$zip")" && pwd)/$(basename "$zip")
fi
if [ -n "$manifest" ]; then
	[ -d "$manifest" ] || { echo "windows-vm-test: $manifest is not a directory" >&2; exit 1; }
	manifest=$(cd "$manifest" && pwd)
	grep -qs '^PackageIdentifier:' "$manifest"/*.yaml || { echo "windows-vm-test: $manifest holds no .yaml manifest with a PackageIdentifier" >&2; exit 1; }
fi

repo=$(cd "$(dirname "$0")/.." && pwd)
[ -n "$zip" ] || [ -d "$repo/web/dist" ] || { echo "windows-vm-test: build the UI first (cd web && npm ci && npm run build)" >&2; exit 1; }
[ -w /dev/kvm ] || { echo "windows-vm-test: /dev/kvm is not available" >&2; exit 1; }

work=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-winvm.XXXXXX")
name="kanjo-winvm-$$"
# shellcheck disable=SC2317,SC2329
cleanup() {
	docker rm -f "$name" >/dev/null 2>&1 || true
	docker run --rm -v "$work":/w alpine:3 rm -rf /w/storage /w/shared >/dev/null 2>&1 || true
	rm -rf "$work"
}
trap cleanup EXIT INT TERM

mkdir -p "$work/shared/bins/bin-x86_64-pc-windows-gnu" "$work/shared/dist" "$work/oem" "$work/storage"
chmod -R 777 "$work"

if [ -n "$zip" ]; then
	zipname=$(basename "$zip")
	cp "$zip" "$work/shared/dist/$zipname"
	sums="$(dirname "$zip")/SHA256SUMS"
	if [ -f "$sums" ] && awk -v n="$zipname" '{ f = $2; sub(/^\*/, "", f) } f == n { found = 1 } END { exit !found }' "$sums"; then
		awk -v n="$zipname" '{ f = $2; sub(/^\*/, "", f) } f == n' "$sums" > "$work/shared/dist/SHA256SUMS"
		(cd "$work/shared/dist" && sha256sum -c --quiet SHA256SUMS) || { echo "windows-vm-test: $zipname does not match its entry in $sums" >&2; exit 1; }
		echo "windows-vm-test: using $zipname with its entry from $sums, which matches"
	else
		(cd "$work/shared/dist" && sha256sum "$zipname" > SHA256SUMS)
		echo "windows-vm-test: using $zipname; no SHA256SUMS next to it lists it, so the checksum was computed here and proves only that the copy is intact"
	fi
else
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
fi
[ -z "$manifest" ] || cp -r "$manifest" "$work/shared/winget"
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
if ! grep -qE "watcher: requests before=([0-9]+) after=" "$result" || grep -qE "watcher: requests before=([0-9]+) after=\1 " "$result"; then
	echo "windows-vm-test: the watcher did not pick up the appended request" >&2
	fail=1
fi
if [ -n "$manifest" ]; then
	if grep -qF "winget: not available" "$result"; then
		if [ "$fail" -eq 0 ]; then
			echo "windows-vm-test: the other checks passed, but this Windows image has no WinGet, so the WinGet step did not run" >&2
			exit 2
		fi
		echo "windows-vm-test: this Windows image has no WinGet, so the WinGet step did not run" >&2
	else
		for expect in "winget installed: yes" "winget kanjo exe: " "winget recorded in settings.json: " "winget recorded in Run key: "; do
			grep -qF "$expect" "$result" || { echo "windows-vm-test: MISSING: $expect" >&2; fail=1; }
		done
		! grep -qE "^winget recorded in (settings.json|Run key): \(none\)" "$result" || { echo "windows-vm-test: kanjo up with the WinGet copy recorded no path" >&2; fail=1; }
		tr -d '\r' < "$result" | grep -E "^winget (installed|kanjo exe|recorded in)" | sed 's/^/windows-vm-test: /'
	fi
fi
[ "$fail" -eq 0 ] && echo "windows-vm-test: passed"
exit "$fail"
