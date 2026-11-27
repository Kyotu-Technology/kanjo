#!/bin/sh
# Gate and landing for the issue skill (.claude/skills/issue).
#
#   scripts/issue.sh gate <issue>... | ready   which issues can be worked on
#   scripts/issue.sh land <pull request>       auto-merge, or why not
#
# gate prints one line per issue, "<n> ok <title>" or "<n> skip <reason>", and
# exits 1 when no issue is ok. land waits until every check of the pull request
# has passed and only then merges it, as a squash and without the administrator
# bypass. A pull request that changes a path the maintainer merges (see
# protected below) is left open, and land exits 3 once its checks are green.
#
# Both guard against mistakes. Whoever runs this with administrator rights can
# go around it; the boundary is the ruleset of main.
set -euf

API='repos/{owner}/{repo}'

die() {
	echo "issue.sh: $*" >&2
	exit 1
}

protected() {
	case $1 in
	.github/* | .claude/* | CLAUDE.md | AGENTS.md | deny.toml | install/* | scripts/*) return 0 ;;
	Cargo.toml | Cargo.lock | crates/*/Cargo.toml | web/package.json | web/package-lock.json) return 0 ;;
	esac
	return 1
}

# shellcheck disable=SC2016
VERDICT='
(.labels | map(.name)) as $l
| .author_association as $a
| if .pull_request then "skip is a pull request"
  elif .state != "open" then "skip is closed"
  elif ($l | index("ready") | not) then "skip is not labelled ready"
  elif ($l | index("in-progress")) then "skip is in progress"
  elif ($l | index("blocked")) then "skip is blocked"
  elif ($l | index("question")) then "skip waits for an answer"
  elif ((.body // "") | test("(^|\n)Done when: *[^ \r\n]") | not) then "skip has no Done when; triage it"
  elif (["OWNER", "MEMBER", "COLLABORATOR"] | index($a) | not) then "skip was opened outside the project; rewrite it at triage"
  else "ok " + .title
  end,
  ([(.body // "") | scan("Depends:[^\n]*") | scan("#[0-9]+") | ltrimstr("#")] | join(" "))'

gate_one() {
	n=$1
	case $n in '' | 0* | *[!0-9]*)
		printf '%s skip is not an issue number\n' "$n"
		return
		;;
	esac
	out=$(gh api "$API/issues/$n" --jq "$VERDICT" 2>/dev/null) || out="skip cannot be read"
	verdict=$(printf '%s\n' "$out" | sed -n 1p)
	deps=$(printf '%s\n' "$out" | sed -n 2p)
	case $verdict in ok*)
		for d in $deps; do
			case " $passed " in *" $d "*) continue ;; esac
			[ "$(gh api "$API/issues/$d" --jq '.state + " " + (.state_reason // "")' 2>/dev/null)" = "closed completed" ] || {
				verdict="skip depends on #$d, which is not done"
				break
			}
		done
		;;
	esac
	case $verdict in ok*)
		[ -z "$(git ls-remote --heads origin "issue-$n-*")" ] || verdict="skip has a branch on origin"
		;;
	esac
	printf '%s %s\n' "$n" "$verdict"
}

# A dependency counts when it is done, or when it passed the gate earlier in the
# same run; issues are taken in ascending order.
gate() {
	[ $# -gt 0 ] || die "gate needs issue numbers or ready"
	if [ "$1" = ready ]; then
		run=$(gh issue list --label ready --state open --limit 200 --json number --jq '[.[].number] | sort | map(tostring) | join(" ")')
	else
		run=$(printf '%s\n' "$@" | sort -n | tr '\n' ' ')
	fi
	passed=
	for n in $run; do
		line=$(gate_one "$n")
		printf '%s\n' "$line"
		case $line in "$n ok "*) passed="$passed $n" ;; esac
	done
	[ -n "$passed" ]
}

# First line "total <n>", then one line per check that has not passed. Prints
# nothing when the checks cannot be read.
checks() {
	gh pr checks "$1" --json name,bucket \
		--jq '"total \(length)", (.[] | select(.bucket != "pass" and .bucket != "skipping") | "\(.bucket) \(.name)")' 2>/dev/null || true
}

wait_checks() {
	tries=0
	until checks "$1" | grep -q '^total [1-9]'; do
		tries=$((tries + 1))
		[ "$tries" -le 12 ] || die "no checks started on #$1"
		sleep 10
	done
	gh pr checks "$1" --watch --interval 20 >/dev/null 2>&1 || true
	result=$(checks "$1")
	printf '%s\n' "$result" | grep -q '^total [1-9]' || die "the checks of #$1 cannot be read"
	failed=$(printf '%s\n' "$result" | sed 1d)
	[ -z "$failed" ] || {
		printf '%s\n' "$failed"
		die "checks did not pass on #$1"
	}
}

disarm() {
	[ "$(gh pr view "$1" --json autoMergeRequest --jq '.autoMergeRequest == null')" = true ] || gh pr merge "$1" --disable-auto
}

land() {
	[ $# -eq 1 ] || die "land needs one pull request number"
	pr=$1
	state=$(gh pr view "$pr" --json state,isDraft,isCrossRepository,baseRefName --jq '"\(.state) \(.isDraft) \(.isCrossRepository) \(.baseRefName)"')
	[ "$state" = "OPEN false false main" ] || die "#$pr is not an open, ready pull request from this repository into main"
	head=$(gh pr view "$pr" --json headRefOid --jq .headRefOid)
	[ "$head" = "$(git rev-parse HEAD)" ] || die "the head of #$pr is not the local HEAD; push first"

	# Nothing merges while this run decides: a merge armed for an earlier head
	# would also take whatever was pushed since.
	disarm "$pr"
	files=$(gh api "$API/pulls/$pr/files" --paginate --jq '.[] | .filename, (.previous_filename // empty)')
	kept=$(printf '%s\n' "$files" | while IFS= read -r f; do
		if protected "$f"; then printf ' %s' "$f"; fi
	done)

	wait_checks "$pr"
	# shellcheck disable=SC2016
	threads=$(gh api graphql -F owner='{owner}' -F name='{repo}' -F number="$pr" \
		-f query='query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { reviewThreads(first: 100) { nodes { isResolved } } } } }' \
		--jq '[.data.repository.pullRequest.reviewThreads.nodes[] | select(.isResolved | not)] | length')
	[ "$threads" = 0 ] || die "#$pr has $threads unresolved review threads"
	if [ -n "$kept" ]; then
		echo "#$pr: checks passed; left for the maintainer because it changes:$kept"
		exit 3
	fi

	# Every check is green here, so gh merges at once; if GitHub still holds the
	# pull request back, --auto leaves the merge to it.
	gh pr merge "$pr" --squash --auto --match-head-commit "$head"
	tries=0
	until [ "$(gh pr view "$pr" --json state --jq .state)" = MERGED ]; do
		tries=$((tries + 1))
		[ "$tries" -le 12 ] || {
			disarm "$pr"
			[ "$(gh pr view "$pr" --json state --jq .state)" = MERGED ] && break
			die "checks passed but #$pr is not merged; see gh pr view $pr"
		}
		sleep 10
	done
	oid=$(gh pr view "$pr" --json mergeCommit --jq .mergeCommit.oid)
	echo "#$pr merged as $oid, signature verified: $(gh api "$API/commits/$oid" --jq .commit.verification.verified)"
}

cd "$(dirname "$0")/.."
cmd=${1:-}
[ $# -eq 0 ] || shift
case $cmd in
gate) gate "$@" ;;
land) land "$@" ;;
*)
	echo "usage: scripts/issue.sh gate <issue>... | ready" >&2
	echo "       scripts/issue.sh land <pull request>" >&2
	exit 2
	;;
esac
