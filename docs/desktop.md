# Desktop edition (plan)

This document describes how a desktop edition of Kanjo can be added later without changing the engine. Nothing here is implemented yet; it records the plan and the seams that already exist, so that work on the engine keeps them intact. The desktop edition is a thin shell around the same server and the same web UI that `kanjo` already ships, not a second application.

## Shape

The desktop edition is one new crate, `kanjo-desktop`, built with Tauri 2. It depends on `kanjo-server` and nothing below it directly. At start-up it resolves `kanjo_core::paths::Paths` exactly as the CLI does, starts the Kanjo server in-process on the loopback interface and opens a Tauri webview pointed at that server. The UI code in `web/` does not change: the webview loads the same embedded SPA over HTTP from `127.0.0.1` and talks to the same `/api/v1` endpoints, including the server-sent events stream, exactly as a browser does. No Tauri commands or JavaScript bridge are needed, which keeps the attack surface of the webview to what the browser version already has.

The crate map in `CLAUDE.md` already commits `kanjo-server` to exposing `router()` and `serve()` so that a desktop shell can host the UI; this is the seam the shell is built on. The shell uses `serve()` when it wants the server's own runtime behaviour (ingest and alert loops, SSE) and `router()` only if it ever needs to wrap the router in extra layers. Either way the shell binds `127.0.0.1` on port 0, reads back the port the operating system assigned, and builds the webview URL from it, so it never collides with a `kanjo serve` or `kanjo up` instance on the configured port. Tauri 2 runs on tokio, so the server runs on the same runtime as the shell, and the rule that blocking database work goes through `spawn_blocking` applies unchanged.

Authentication stays as described in `docs/api.md`. The shell reads the access token through the same code the CLI's `kanjo open` uses and opens `/?token=<token>`, which sets the session cookie inside the webview. The server's `Host` check accepts the request because the webview addresses the server by the address it listens on. Nothing has to be weakened for the desktop: the shell is a browser that happens to be bundled.

Only one process should own the database and the ingest loop. If a background service installed by `kanjo service` is already running, the shell should detect it through `/api/v1/health` on the configured port and open its webview against that instance instead of starting a second server. Only when no instance answers does it start its own. Whether the desktop app then also offers to install the background service is a product decision for later.

## Tray icon

The tray icon shows the share of the current limit that has been used. That number already exists in the API: `GET /api/v1/limits` returns `LimitsDetail`, whose `current` field carries the five-hour and seven-day windows (`LimitWindow`, with `used_pct`, `stale` and the forecast fields). The shell subscribes to `/api/v1/events` (or polls `/api/v1/limits` when the stream drops), renders the higher of the two percentages as tray text or a generated icon, and marks a stale reading visibly instead of showing an old number as current. The tray menu needs little more than "Open dashboard" and "Quit"; anything richer belongs in the web UI. Desktop notifications keep going through the existing local delivery in `kanjo-alerts`; the shell does not add a second notification path.

## Packaging and signing

The engine is the same, but packaging is where the desktop edition really differs from the CLI, and most of the work is there.

On macOS the app ships as a `.app` bundle inside a `.dmg`. It must be signed with a Developer ID Application certificate, built with the hardened runtime and notarized by Apple, otherwise Gatekeeper blocks it for most users. That needs an Apple developer account and signing secrets in a protected release environment, and the bundle is a universal binary or one build per architecture. On Windows the app ships as an MSI or NSIS installer, needs an Authenticode certificate (an EV or cloud HSM based certificate to avoid SmartScreen warnings), and depends on the WebView2 runtime, which Windows 11 includes and the installer can bootstrap on Windows 10. On Linux the app ships as an AppImage and a `.deb`; it links WebKitGTK dynamically, so unlike the CLI it cannot be a static musl binary and needs a glibc baseline.

Release automation gets a second job set next to the existing one, using the Tauri bundler and the platform signing tools, with the same pinned actions, least-privilege permissions, SHA256SUMS, cosign signature, provenance attestation and SBOM as the CLI. The CLI archives stay exactly as they are and remain the supported way to run Kanjo on servers and over SSH.

## What must not change

Rule 2 of the project (no outbound traffic) applies to the desktop edition in full. The Tauri updater and HTTP plugins are not used, the webview's content security policy allows only the loopback origin it was opened on, and navigation to any other origin is refused and opened in the system browser instead. `deny.toml` must pass with the desktop crate in the workspace; if a Tauri feature pulls in an HTTP client stack, that feature stays off. `scripts/egress-test.sh` gets a desktop variant that runs the app under `strace` with the same assertion.

Everything below the shell stays identical: the crates, the database and its migrations, the data directory from `Paths`, the API contract in `kanjo_core::api`, the web UI build, and the privacy rules. A user can switch between the CLI, the background service and the desktop app without migrating anything, because they all open the same database through the same code.
