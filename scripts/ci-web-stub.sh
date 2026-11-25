#!/bin/sh
# Rust-only CI jobs compile the server, which embeds web/dist. They do not need
# the real UI, so give them a one-file placeholder when no build is present.
set -eu
dist="$(dirname "$0")/../web/dist"
mkdir -p "$dist"
if [ ! -f "$dist/index.html" ]; then
	printf '<!doctype html><html><head><title>kanjo</title></head><body>UI not built</body></html>\n' >"$dist/index.html"
fi
