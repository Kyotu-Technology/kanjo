#!/bin/sh
# Checks that the documentation describes what exists. Markdown files, code
# comments and UI string files are scanned for plan and promise wording (the
# term list is in TERMS below). A line carrying the marker docs-check: allow is
# skipped. --self-test runs the check against a
# passing and a failing fixture in a temporary directory.
#
# Usage: scripts/check-docs.sh [--self-test]
set -eu

# shellcheck disable=SC2016
TERMS='
function term(re, label, level) { t[++n] = re; l[n] = label; v[n] = level }
BEGIN {
	b = "(^|[^a-z0-9_])"; e = "([^a-z0-9_]|$)"; s = "[[:space:]]+"
	term(b "planned", "planned", 1)
	term(b "roadmap", "roadmap", 1)
	term(b "coming" s "soon", "coming soon", 1)
	term(b "future" s "(version|release|edition|desktop)", "future version/release/edition/desktop", 2)
	term(b "later" s "(version|release)", "later version/release", 2)
	term(b "yet" e, "yet", 3)
	term(b "will" s "(need|say|be|get|have)" e, "will need/say/be/get/have", 3)
	term(b "once[[:space:]][^.]*[[:space:]]ships" e, "once ... ships", 3)
	term("[(]plan[)]", "(plan)", 3)
	term(b "w" s "planach", "w planach", 3)
	term(b "(za)?planowan", "planowan", 3)
	term(b "jeszcze" s "nie", "jeszcze nie", 3)
	term(b "nie" s "[^[:space:].]+" s "jeszcze", "nie ... jeszcze", 3)
	term(b "nikt" s "jeszcze", "nikt jeszcze", 3)
	term(b "wkrótce", "wkrótce", 3)
	term(b "w" s "przyszłej" s "wersji", "w przyszłej wersji", 3)
}
{
	f = $0; lvl = 0; code = 0
	if (f ~ /\.md$/) {
		if (f == "CHANGELOG.md" || f ~ /^\.claude\//) next
		lvl = (f == "docs/decisions.md") ? 1 : 3
	} else if (f ~ /^web\/src\/lib\/i18n\// && f !~ /\.test\.ts$/) {
		lvl = 2
	} else if (f ~ /^crates\/.*\.rs$/ || f ~ /^web\/src\// || f ~ /^scripts\/[^\/]*\.sh$/ || f ~ /^install\//) {
		lvl = 2; code = 1
	} else next
	ln = 0
	while ((getline line < f) > 0) {
		ln++
		if (index(line, "docs-check: allow")) continue
		if (code && line !~ /^[[:space:]]*(\/\/|#|<#|\/\*|\*|<!--)/) continue
		low = tolower(line)
		for (i = 1; i <= n; i++) if (v[i] <= lvl && low ~ t[i]) printf "%s:%d: %s\n", f, ln, l[i]
	}
	close(f)
}'

list_files() {
	if [ -e .git ]; then
		git -c core.quotePath=false --git-dir=.git ls-files --cached --others --exclude-standard
	else
		find . -type f | sed 's|^\./||'
	fi
}

check() {
	status=0
	files=$(list_files) || {
		echo "check-docs: listing the files failed" >&2
		return 2
	}
	hits=$(printf '%s\n' "$files" | awk "$TERMS") || {
		echo "check-docs: the scan failed" >&2
		return 2
	}
	if [ -n "$hits" ]; then
		printf '%s\n' "$hits"
		echo "check-docs: the lines above carry plan or promise wording. Documentation describes what exists."
		echo "Reword a limitation as a fact (has not been, does not, there is no); never delete a true limitation to pass."
		echo "Plans and ideas belong in GitHub issues. A sentence about runtime state may carry the marker docs-check: allow"
		echo "(inside <!-- --> in Markdown)."
		status=1
	fi
	return "$status"
}

put() {
	mkdir -p "$(dirname "$1")"
	f=$1
	shift
	printf '%s\n' "$@" >"$f"
}

self_test() {
	WORK=$(mktemp -d "${TMPDIR:-/tmp}/kanjo-check-docs.XXXXXX")
	trap 'rm -rf "$WORK"' EXIT INT TERM
	mkdir -p "$WORK/pass" "$WORK/fail"
	cd "$WORK/pass"
	put README.md 'A later rescan picks the file up.' 'The spool file is deleted once it is consumed.' 'A yeti is not a term.' \
		'There is no team view yet. <!-- docs-check: allow -->' 'It has not been run on macOS.' \
		'Pliki, które są jeszcze na dysku.' 'Ograniczenie jest jeszcze ważne.'
	put CHANGELOG.md 'Planned and released, both fine here.'
	put .claude/rules/x.md 'The roadmap words are quoted here.'
	put docs/decisions.md 'Releases will be signed with cosign.'
	put crates/a/src/lib.rs '// The key is not created yet on first start.' 'let planned = 1;'
	put web/src/lib/i18n/pl.ts "  noKey: 'Klucz jeszcze nie istnieje',"
	if check >"$WORK/pass.out" 2>&1; then echo "self-test: passing fixture passes"; else
		cat "$WORK/pass.out"
		echo "self-test: FAIL, the passing fixture failed" >&2
		return 1
	fi
	cd "$WORK/fail"
	put README.md 'Windows support is not there yet.' 'Kanjo will be signed.' 'Once the tap ships, brew works.' \
		'# Desktop (plan)' 'See the roadmap.' 'Funkcja jest w planach.' 'Wkrótce dostępne.' 'Funkcja planowana.' \
		'Dostępne w przyszłej wersji.' 'Instalacja nie została jeszcze sprawdzona.' 'Zaplanowano na marzec.'
	put docs/decisions.md 'Coming soon: more.'
	put crates/a/src/lib.rs '// planned for a future release'
	put web/src/lib/i18n/en.ts "  hint: 'Available in a later version',"
	put scripts/x.sh '# Not done yet.' 'echo planned'
	if check >"$WORK/fail.out" 2>&1; then
		echo "self-test: FAIL, the failing fixture passed" >&2
		return 1
	fi
	missing=0
	for want in 'README.md:1: yet' 'README.md:2: will need/say/be/get/have' 'README.md:3: once ... ships' 'README.md:4: (plan)' \
		'README.md:5: roadmap' 'README.md:6: w planach' 'README.md:7: wkrótce' 'README.md:8: planowan' \
		'README.md:9: w przyszłej wersji' 'README.md:10: nie ... jeszcze' 'README.md:11: planowan' 'docs/decisions.md:1: coming soon' \
		'crates/a/src/lib.rs:1: planned' 'crates/a/src/lib.rs:1: future version/release/edition/desktop' \
		'web/src/lib/i18n/en.ts:1: later version/release'; do
		grep -F -x -q "$want" "$WORK/fail.out" || {
			echo "self-test: FAIL, missing: $want" >&2
			missing=1
		}
	done
	if grep -q '^scripts/x.sh' "$WORK/fail.out"; then
		echo "self-test: FAIL, a yet in a script comment or a plain code line was reported" >&2
		missing=1
	fi
	[ "$missing" = 0 ] || {
		cat "$WORK/fail.out"
		return 1
	}
	echo "self-test: failing fixture fails with every expected hit"
	mkdir "$WORK/none" && cd "$WORK/none"
	put README.md 'Fine.'
	if ! check >"$WORK/none.out" 2>&1; then
		echo "self-test: FAIL, a tree with nothing to report failed" >&2
		return 1
	fi
	mkdir .git
	if check >"$WORK/none.out" 2>&1; then
		echo "self-test: FAIL, a tree whose file list cannot be read passed" >&2
		return 1
	fi
	echo "self-test: an unreadable file list fails"
}

case "${1:-}" in
--self-test)
	self_test
	;;
"")
	cd "$(dirname "$0")/.."
	if check; then echo "check-docs: ok"; else exit 1; fi
	;;
*)
	echo "usage: scripts/check-docs.sh [--self-test]" >&2
	exit 2
	;;
esac
