#!/bin/sh
# Zero-egress test for the kanjo binary (Linux only).
#
# Phase 1 starts "kanjo demo" inside a network namespace that has nothing but
# loopback and proves the UI answers there. Phase 2 runs it on the normal
# network under "strace -f -e trace=network", exercises the UI and fails if any
# connect(), sendto() or sendmsg() targets anything other than loopback, a Unix
# socket or netlink, or port 53 on any address (the resolver stub is loopback).
#
# Usage: scripts/egress-test.sh [path/to/kanjo]
#   KANJO_BIN         binary to test (default target/release/kanjo, then target/debug/kanjo)
#   EGRESS_REQUIRE=1  turn every skip into a failure (CI sets this)
#   EGRESS_SECONDS    how long to exercise the UI under strace (default 10)
#   EGRESS_PORT       port for "kanjo demo" (default: random 20000-59999)
set -eu

REQUIRE="${EGRESS_REQUIRE:-0}"
TRACE_SECONDS="${EGRESS_SECONDS:-10}"

say() { printf 'egress-test: %s\n' "$*"; }
fail() {
	printf 'egress-test: FAIL: %s\n' "$*" >&2
	exit 1
}
skip_or_fail() {
	if [ "$REQUIRE" = 1 ]; then
		fail "EGRESS_REQUIRE=1 but $*"
	fi
	printf 'egress-test: SKIP: %s\n' "$*"
	SKIPPED=yes
}
have() { command -v "$1" >/dev/null 2>&1; }

# Prints one line per network syscall whose destination is not local.
# Input: an strace log written with -f -e trace=network.
classify() {
	awk '
	function verdict(line,    fam, addr, port) {
		if (match(line, /sa_family=AF_[A-Z0-9]+/) == 0) return "undecoded destination"
		fam = substr(line, RSTART + 10, RLENGTH - 10)
		if (match(line, /_port=htons\([0-9]+\)/)) {
			port = substr(line, RSTART, RLENGTH)
			sub(/.*htons\(/, "", port)
			sub(/\)/, "", port)
			if (port == "53") return "DNS port 53"
		}
		if (fam == "AF_UNIX" || fam == "AF_LOCAL" || fam == "AF_NETLINK" || fam == "AF_UNSPEC") return ""
		if (fam == "AF_INET") {
			if (match(line, /inet_addr\("[0-9.]+"\)/) == 0) return "undecoded IPv4 address"
			addr = substr(line, RSTART + 11, RLENGTH - 13)
			if (addr ~ /^127\./) return ""
			return "IPv4 " addr
		}
		if (fam == "AF_INET6") {
			if (match(line, /inet_pton\(AF_INET6, "[^"]*"/) == 0) return "undecoded IPv6 address"
			addr = substr(line, RSTART + 21, RLENGTH - 22)
			if (addr == "::1" || addr ~ /^::ffff:127\./) return ""
			return "IPv6 " addr
		}
		return "address family " fam
	}
	/(^|[^a-z_])connect\(/ { v = verdict($0); if (v != "") print v " | " $0; next }
	/(^|[^a-z_])(sendto|sendmsg|sendmmsg)\(/ && /sa_family=/ { v = verdict($0); if (v != "") print v " | " $0 }
	' "$1"
}

# Proves the classifier flags what it must and nothing else, so a broken
# pattern cannot turn the real check into a silent pass.
self_check() {
	cat >"$WORK/good.trace" <<'EOF'
101 connect(5, {sa_family=AF_UNIX, sun_path="/run/user/1000/bus"}, 110) = 0
101 connect(6, {sa_family=AF_INET, sin_port=htons(4747), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
101 connect(7, {sa_family=AF_INET6, sin6_port=htons(4747), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::1", &sin6_addr), sin6_scope_id=0}, 28) = 0
101 connect(7, {sa_family=AF_INET6, sin6_port=htons(4747), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "::ffff:127.0.0.1", &sin6_addr), sin6_scope_id=0}, 28) = 0
101 sendto(3, [{nlmsg_len=20, nlmsg_type=RTM_GETADDR}], 20, 0, {sa_family=AF_NETLINK, nl_pid=0, nl_groups=00000000}, 12) = 20
101 connect(8, {sa_family=AF_UNSPEC, sa_data="\0\0\0\0"}, 16) = 0
102 sendto(9, "x", 1, MSG_NOSIGNAL, NULL, 0) = 1
101 <... connect resumed>) = 0
101 bind(5, {sa_family=AF_INET, sin_port=htons(4747), sin_addr=inet_addr("127.0.0.1")}, 16) = 0
EOF
	cat >"$WORK/bad.trace" <<'EOF'
connect(5, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("192.0.2.10")}, 16) = -1 EINPROGRESS (Operation now in progress)
[pid 77] connect(5, {sa_family=AF_INET6, sin6_port=htons(443), sin6_flowinfo=htonl(0), inet_pton(AF_INET6, "2001:db8::1", &sin6_addr), sin6_scope_id=0}, 28 <unfinished ...>
78 connect(5, {sa_family=AF_INET, sin_port=htons(53), sin_addr=inet_addr("127.0.0.53")}, 16) = 0
78 sendmsg(5, {msg_name={sa_family=AF_INET, sin_port=htons(123), sin_addr=inet_addr("192.0.2.20")}, msg_namelen=16, msg_iov=[], msg_iovlen=0, msg_controllen=0, msg_flags=0}, 0) = 48
78 connect(5, 0x7ffd5e0c1230, 16) = -1 EFAULT (Bad address)
78 connect(5, {sa_family=AF_PACKET, sll_protocol=htons(ETH_P_ALL), sll_ifindex=2}, 20) = 0
EOF
	good=$(classify "$WORK/good.trace" | wc -l)
	bad=$(classify "$WORK/bad.trace" | wc -l)
	[ "$good" -eq 0 ] || fail "classifier self-check: $good false positive(s): $(classify "$WORK/good.trace")"
	[ "$bad" -eq 6 ] || fail "classifier self-check: flagged $bad of 6 violations"
	say "classifier self-check passed"
}

isolate_env() {
	mkdir -p "$WORK/home" "$WORK/kanjo-home" "$WORK/claude" "$WORK/runtime"
	chmod 700 "$WORK/runtime"
	HOME="$WORK/home"
	XDG_CONFIG_HOME="$WORK/home/.config"
	XDG_DATA_HOME="$WORK/home/.local/share"
	XDG_CACHE_HOME="$WORK/home/.cache"
	XDG_STATE_HOME="$WORK/home/.local/state"
	XDG_RUNTIME_DIR="$WORK/runtime"
	KANJO_HOME="$WORK/kanjo-home"
	KANJO_CLAUDE_DIR="$WORK/claude"
	CLAUDE_CONFIG_DIR="$WORK/claude"
	export HOME XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME XDG_STATE_HOME XDG_RUNTIME_DIR
	export KANJO_HOME KANJO_CLAUDE_DIR CLAUDE_CONFIG_DIR
	unset DBUS_SESSION_BUS_ADDRESS DISPLAY WAYLAND_DISPLAY BROWSER http_proxy https_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY 2>/dev/null || true
}

url() { printf 'http://127.0.0.1:%s%s' "$PORT" "$1"; }

# $1 = pid of the process to watch, $2 = its log
wait_ready() {
	i=0
	while [ "$i" -lt 60 ]; do
		if curl -fsS -m 2 -o /dev/null "$(url /api/v1/health)" 2>/dev/null; then
			return 0
		fi
		if ! kill -0 "$1" 2>/dev/null; then
			tail -n 20 "$2" >&2 || true
			fail "kanjo exited before the UI answered"
		fi
		sleep 0.5
		i=$((i + 1))
	done
	tail -n 20 "$2" >&2 || true
	fail "the UI did not answer on port $PORT within 30 s"
}

probe_ui() {
	health=$(curl -fsS -m 5 "$(url /api/v1/health)") || fail "GET /api/v1/health failed"
	case "$health" in
	*'"status"'*ok*) ;;
	*) fail "unexpected /api/v1/health body: $health" ;;
	esac
	curl -fsS -m 5 -o "$WORK/index.html" "$(url /)" || fail "GET / failed"
	grep -qi '<html\|<!doctype html' "$WORK/index.html" || fail "GET / did not return an HTML page"
}

exercise_ui() {
	probe_ui
	for path in /api/v1/overview /api/v1/limits /api/v1/sessions /api/v1/system /no-such-page; do
		curl -s -m 5 -o /dev/null "$(url "$path")" || true
	done
	curl -s -m 2 -o /dev/null "$(url /api/v1/events)" || true
}

# $1 = pid
stop_pid() {
	kill -TERM "$1" 2>/dev/null || return 0
	i=0
	while kill -0 "$1" 2>/dev/null && [ "$i" -lt 20 ]; do
		sleep 0.25
		i=$((i + 1))
	done
	kill -KILL "$1" 2>/dev/null || true
	wait "$1" 2>/dev/null || true
}

children_of() {
	if have pgrep; then
		pgrep -P "$1" || true
	else
		ps -o pid= --ppid "$1" || true
	fi
}

inside_namespace() {
	BIN=$1
	PORT=$2
	WORK=$3
	ip link set lo up
	addrs=$(ip -o addr show | awk '$2 != "lo" { print $2 " " $4 }')
	[ -z "$addrs" ] || fail "the namespace has non-loopback addresses: $addrs"
	routes=$(ip route show; ip -6 route show 2>/dev/null | grep -v '^::1 \|^fe80:' || true)
	[ -z "$routes" ] || fail "the namespace has routes beyond loopback: $routes"
	if curl -s -m 3 -o /dev/null http://192.0.2.1/ 2>/dev/null; then
		fail "the namespace reached a non-loopback address; isolation is broken"
	fi
	"$BIN" demo --port "$PORT" --no-open >"$WORK/namespace.log" 2>&1 &
	pid=$!
	trap 'kill -KILL "$pid" 2>/dev/null || true' EXIT
	wait_ready "$pid" "$WORK/namespace.log"
	probe_ui
	stop_pid "$pid"
	say "namespace phase passed: the UI answers with only loopback available"
}

if [ "${1:-}" = "--inside-namespace" ]; then
	shift
	inside_namespace "$@"
	exit 0
fi

SKIPPED=""
PIDS=""
WORK=""
cleanup() {
	for p in $PIDS; do
		for c in $(children_of "$p"); do kill -KILL "$c" 2>/dev/null || true; done
		kill -KILL "$p" 2>/dev/null || true
	done
	if [ -n "$WORK" ]; then rm -rf "$WORK"; fi
}
trap cleanup EXIT
trap 'exit 130' INT TERM

if [ "$(uname -s)" != Linux ]; then
	skip_or_fail "the egress test needs Linux (namespaces and strace)"
	exit 0
fi

SELF=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
REPO=$(cd "$(dirname "$0")/.." && pwd)
BIN="${1:-${KANJO_BIN:-}}"
if [ -z "$BIN" ]; then
	for candidate in "$REPO/target/release/kanjo" "$REPO/target/debug/kanjo"; do
		if [ -x "$candidate" ]; then
			BIN=$candidate
			break
		fi
	done
fi
[ -n "$BIN" ] || fail "no kanjo binary found; build it or pass its path"
[ -x "$BIN" ] || fail "not an executable: $BIN"
BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")

have curl || fail "curl is required to probe the UI"
have awk || fail "awk is required"

WORK=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-egress.XXXXXX")
isolate_env
self_check

PORT="${EGRESS_PORT:-$(awk 'BEGIN { srand(); print 20000 + int(rand() * 40000) }')}"
if curl -s -m 2 -o /dev/null "$(url /)" 2>/dev/null; then
	fail "port $PORT is already in use; set EGRESS_PORT"
fi
say "testing $BIN on port $PORT"

if ! have unshare || ! have ip; then
	skip_or_fail "namespace phase needs unshare and ip (util-linux, iproute2)"
elif ! unshare --user --map-root-user --net true 2>/dev/null; then
	skip_or_fail "namespace phase: unprivileged user namespaces are blocked (Ubuntu: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0)"
else
	unshare --user --map-root-user --net sh "$SELF" --inside-namespace "$BIN" "$PORT" "$WORK" &
	ns_pid=$!
	PIDS="$PIDS $ns_pid"
	wait "$ns_pid" || fail "namespace phase failed"
fi

if ! have strace; then
	skip_or_fail "strace phase needs strace"
elif ! strace -f -qq -e trace=network -o /dev/null true 2>/dev/null; then
	skip_or_fail "strace phase: ptrace is not permitted here"
else
	TRACE="$WORK/network.trace"
	strace -f -qq -e trace=network -s 256 -o "$TRACE" "$BIN" demo --port "$PORT" --no-open >"$WORK/strace.log" 2>&1 &
	spid=$!
	PIDS="$PIDS $spid"
	wait_ready "$spid" "$WORK/strace.log"
	end=$(($(date +%s) + TRACE_SECONDS))
	while [ "$(date +%s)" -lt "$end" ]; do
		exercise_ui
		sleep 1
	done
	for c in $(children_of "$spid"); do stop_pid "$c"; done
	stop_pid "$spid"
	grep -q "bind(.*htons($PORT)" "$TRACE" || fail "the trace does not show kanjo binding port $PORT, so it cannot be trusted"
	calls=$(grep -c -E '(^|[^a-z_])(connect|sendto|sendmsg|sendmmsg|bind|accept4?)\(' "$TRACE" || true)
	violations=$(classify "$TRACE")
	if [ -n "$violations" ]; then
		printf '%s\n' "$violations" >&2
		fail "kanjo attempted network traffic that is not loopback ($(printf '%s\n' "$violations" | wc -l) syscall(s) above)"
	fi
	say "strace phase passed: $calls traced socket calls, all loopback, Unix or netlink"
fi

if [ -n "$SKIPPED" ]; then
	say "finished with skipped phases; this is not a full pass"
else
	say "PASS"
fi
