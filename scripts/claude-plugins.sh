#!/bin/sh
# Sets up the Claude Code plugins this repository uses, for people who work on
# Kanjo with Claude Code. It reads .claude/settings.json, adds the plugin
# marketplaces declared there at their pinned commits, and installs every plugin
# the file enables at project scope. Safe to run again. Needs claude, jq, git
# and network access to GitHub; it changes Claude Code's own configuration
# (~/.claude or CLAUDE_CONFIG_DIR), never the repository.
#
# A marketplace that is already configured under the same name is left as it
# is, so a copy you added yourself keeps following its own branch.
#
# Usage: scripts/claude-plugins.sh
set -eu

say() { printf 'claude-plugins: %s\n' "$*"; }
fail() {
	printf 'claude-plugins: FAIL: %s\n' "$*" >&2
	exit 1
}

root=$(cd "$(dirname "$0")/.." && pwd)
settings="$root/.claude/settings.json"
[ -f "$settings" ] || fail "$settings not found"
for tool in claude jq git; do
	command -v "$tool" >/dev/null 2>&1 || fail "$tool is not installed"
done

cd "$root"
known=$(claude plugin marketplace list --json | jq -r '.[].name')

add_marketplace() {
	if printf '%s\n' "$known" | grep -qx "$1"; then
		say "marketplace $1 is already configured, leaving it as it is"
	else
		say "adding marketplace $1 from $2"
		claude plugin marketplace add "$2"
	fi
}

add_marketplace claude-plugins-official anthropics/claude-plugins-official
jq -r '.extraKnownMarketplaces // {} | to_entries[] | "\(.key) \(.value.source.repo)#\(.value.source.ref)"' "$settings" |
	while read -r name source; do
		add_marketplace "$name" "$source"
	done

jq -r '.enabledPlugins // {} | to_entries[] | select(.value == true) | .key' "$settings" |
	while read -r plugin; do
		say "installing $plugin"
		claude plugin install "$plugin" --scope project
	done

for server in rust-analyzer typescript-language-server svelteserver; do
	command -v "$server" >/dev/null 2>&1 || say "note: $server is not on PATH; its language server plugin stays idle until it is"
done
say "done; start a new Claude Code session to load the plugins"
