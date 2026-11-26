---
name: run-demo
description: Launch Kanjo on synthetic demo data to see a change working, in the real binary or in the web UI's mock mode, and take screenshots. Use when asked to run, start, try or screenshot the app.
---

# Running Kanjo on demo data

Never start `kanjo up` or `kanjo serve` to try a change: they read the real `~/.claude` and the real data directory. `kanjo demo` uses invented data in a temporary directory.

## The real binary

The server embeds `web/dist`, so build the UI first when the change is in `web/` (otherwise the binary serves the previous build or the stub page):

```sh
(cd web && npm ci && npm run build)
cargo run -p kanjo -- demo --no-open --port 47800
```

The command keeps running and prints a one-time login link (`http://127.0.0.1:47800/?token=...`). Run it in the background, read the link from its output and open that link; the code in it is short lived and works once. `--days <N>` changes how much history is invented. Check that the server is up with:

```sh
curl -s http://127.0.0.1:47800/api/v1/health
```

Stop the process when done.

## The UI alone

For work on the UI that needs no server, in `web/`:

```sh
npm run dev
```

Open `http://localhost:5173/?mock`. `?mock=<scenario>` picks a state (`default`, `onboarding`, `demo`, `empty`, `import`, `nostatusline`, `unpriced`, `unauthorized`, `redacted`, `viewer`, `gateway`, `expired`, `tampered`), and `?lang=en|pl` and `?theme=light|dark` override the saved preferences. Check a UI change in both languages, both themes and at 360 px width.

## Screenshots

With a demo server running, `node web/scripts/screenshots.mjs "<login link>"` retakes the documentation screenshots in `docs/screenshots`. It needs a local Chromium (`--chrome <path>` picks the binary).
