#!/bin/sh
# Runs cargo audit against a fresh shallow clone of the RustSec advisory
# database instead of ~/.cargo/advisory-db, so a stale or broken local copy can
# neither hide an advisory nor fail the run for a reason unrelated to the
# dependencies. The clone is checked before it is used: a database without
# advisories would otherwise report "no vulnerabilities".
#
# Usage: scripts/cargo-audit.sh                 audit Cargo.lock
#        scripts/cargo-audit.sh bin <binary>    audit the dependency list embedded by cargo-auditable
#   KANJO_ADVISORY_DB  use this existing clone instead (it is checked the same way, not updated)
set -eu

say() { printf 'cargo-audit: %s\n' "$*"; }
fail() {
	printf 'cargo-audit: FAIL: %s\n' "$*" >&2
	exit 1
}

db=${KANJO_ADVISORY_DB:-}
work=""
trap '[ -z "$work" ] || rm -rf "$work"' EXIT
trap 'exit 130' INT TERM
if [ -z "$db" ]; then
	work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/kanjo-advisory-db.XXXXXX")
	db="$work/advisory-db"
	git clone --quiet --depth 1 https://github.com/rustsec/advisory-db.git "$db" ||
		fail "could not clone the RustSec advisory database"
fi
[ -d "$db/crates" ] || fail "$db is not a RustSec advisory database (it has no crates/ directory)"
count=$(find "$db/crates" -name 'RUSTSEC-*.md' | wc -l | tr -d ' ')
[ "$count" -ge 1000 ] || fail "$db holds only $count advisories; refusing to audit against an incomplete database"
say "advisory database $db: $count advisories, last commit $(git -C "$db" log -1 --format=%cs 2>/dev/null || echo unknown)"

cd "$(dirname "$0")/.."
status=0
cargo audit --db "$db" --no-fetch "$@" || status=$?
exit "$status"
