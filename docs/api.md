# HTTP API

The `kanjo` binary serves a JSON API under `/api/v1` for its own web UI, for scripts and for CI. It listens on the loopback interface unless told otherwise. All request and response bodies are the types defined in `crates/kanjo-core/src/api.rs` and `settings.rs`; the TypeScript equivalents are generated into `web/src/lib/api/bindings`.

Conventions: timestamps are Unix epoch milliseconds; money is USD; ratios run 0..1 and percentages 0..100. Read endpoints take the optional query parameters of `Query` (`from`, `to`, `project`, `model`, `agent`, `client`, `group`, `by`, `q`, `sort`, `limit`, `offset`, `period`, `lang`, `format`) and ignore the ones that do not apply. Without `from`/`to` an endpoint uses its natural default range (30 days for reports). `limit` is clamped to 1..1000 and `offset` to 0..10000000; `from` after `to` and text parameters longer than 200 characters are refused with `400 invalid`.

## Signing in

Unless authentication is disabled for loopback in `config.toml`, every `/api` request except `/api/v1/health` needs a credential: the `kanjo_session` cookie or `Authorization: Bearer <access token>`. The access token lives in `access.token` in the data directory (owner-only). A missing or wrong credential yields `401 unauthorized`.

A browser signs in by opening `/?token=<value>` on any UI path. The server answers `303` to the same path without the `token` parameter and, when the value is right, sets the `kanjo_session` cookie (`HttpOnly; SameSite=Strict; Path=/`, plus `Secure` under TLS). The cookie is a derivative of the token, never the token itself. The value can be the access token, but neither `kanjo open` nor `kanjo up` ever puts the token into a URL. They use a login code instead: 32 random bytes as 64 lowercase hex characters, valid once and for two minutes, and accepted only by the server instance it was written for.

The login code flow works through two files in the data directory. A running server writes `server.json` (owner-only) with its process id, the address it listens on, its base URL, whether it asks for a credential, whether it is a demo, a random instance id, the version and its start time. It holds a shared lock on the file while it runs and removes it on a clean shutdown; a file nobody holds the lock on is stale and ignored, so a crashed server never blocks anything. A second server for the same data directory refuses to start. `kanjo open`, `kanjo up`, `kanjo restore`, `kanjo service status` and `kanjo uninstall --purge-data` find the server through this file, so a server started with `kanjo serve --port N` or `kanjo demo --home DIR` is found, and a process that merely holds the configured port is not mistaken for Kanjo. To sign a browser in, the command line writes a file named after the code into `login/` (owner-only) that names the instance id from `server.json`, and opens `<url>?token=<code>`. The server deletes the file when the code is presented, whatever the outcome, and clears `login/` when it starts. A code that reaches some other process listening on the port is worthless to it. The browser the server opens itself, and the sign-in link that `kanjo demo` and an interactive `kanjo serve --no-open` print, use an equivalent code the server keeps in memory.

Wrong credentials are rate limited per peer: all of loopback counts as one peer and IPv6 peers count per /64. Ten distinct wrong values within five minutes lock the peer out for five minutes; one stale value repeated does not count as guessing. During a lockout further wrong values get `429` with code `unauthorized` and a `Retry-After` header in seconds. The right token, cookie or login code is always accepted, so nobody else's failures can lock the owner out. A wrong login link counts only when the browser marks it as a navigation that did not come from another site (`Sec-Fetch-Site` not `cross-site` or `same-site`, `Sec-Fetch-Mode` absent or `navigate`), so a web page that makes the browser fetch `/?token=...` cannot start a lockout. When more than 4096 peers are being tracked, the others share one overflow entry, and its lockout is enforced like any other.

Other guards: requests whose `Host` header does not name this server (a loopback name, the bind address or the address the connection arrived on, with the listening port) get `403 forbidden`, as do requests with a foreign `Origin` and `/api` requests marked `Sec-Fetch-Site: cross-site` or `same-site`. Requests that change state (`POST`, `PUT`, `PATCH`, `DELETE`) must carry `X-Kanjo-CSRF: 1`. At most 256 connections are open at once; when all are taken, the oldest connection that has not yet presented a valid credential is closed to make room, so idle or unauthenticated sockets cannot keep the owner out.

## Errors and limits

Any non-2xx response carries an `ApiError` body (`code`, `message`). Codes: `unauthorized` (401, and 429 during a lockout), `forbidden` (403), `not_found` (404), `invalid` (400, 405, 413 and other client errors), `disabled` (403, a feature that is switched off: the conversation viewer, or the hook installer in demo mode), `internal` (500, and 503 on a timeout).

Request bodies are limited to 256 KiB; a larger one gets `413` with code `invalid` and the message "request body too large". A handler that runs longer than 30 seconds is answered with `503`, code `internal`, message "the request took too long"; state-changing requests under `/api/v1/system/` (backup, purge, hooks, rescan) get 300 seconds, and the event stream has no limit.

## Endpoints

| Method | Path | Body | Response |
| --- | --- | --- | --- |
| GET | `/api/v1/health` | | `{ "status": "ok", "version": string }`; no credential needed |
| GET | `/api/v1/overview` | | `Overview` |
| GET | `/api/v1/usage` | | `UsageReport` (`group`, range, filters) |
| GET | `/api/v1/breakdown` | | `Breakdown` (`by`, range, filters) |
| GET | `/api/v1/sessions` | | `SessionList` (range, `project`, `q`, `sort`, `limit`, `offset`) |
| GET | `/api/v1/sessions/{id}` | | `SessionDetail` |
| GET | `/api/v1/sessions/{id}/transcript` | | `Transcript`; `403 disabled` unless the viewer is enabled, `404` in demo mode; audited |
| GET | `/api/v1/turns` | | `Turn[]`, most expensive first (range, `project`, `limit`) |
| GET | `/api/v1/projects` | | `Project[]` |
| PATCH | `/api/v1/projects/{id}` | `ProjectPatch` | `Project` |
| GET | `/api/v1/blocks` | | `Block[]` (range) |
| GET | `/api/v1/limits` | | `LimitsDetail` |
| GET | `/api/v1/heatmap` | | `Heatmap` (range, `project`) |
| GET | `/api/v1/compare` | | `Comparison` (`period` = `week` or `month`, `offset` = periods back) |
| GET | `/api/v1/plan` | | `PlanFit` |
| GET | `/api/v1/insights` | | `Insights` |
| GET | `/api/v1/alerts` | | `AlertList` (`limit`, `offset`, `q=open` for unacknowledged, `lang`) |
| POST | `/api/v1/alerts/ack` | `{ "id": number or null }` (null or absent = all open alerts) | `{ "acknowledged": number }`, the number of alerts acknowledged by this call |
| GET | `/api/v1/budgets` | | `Budget[]` |
| POST | `/api/v1/budgets` | `BudgetInput` | `Budget` |
| PUT | `/api/v1/budgets/{id}` | `BudgetInput` | `Budget` |
| DELETE | `/api/v1/budgets/{id}` | | `204` |
| GET | `/api/v1/settings` | | `Settings` |
| PUT | `/api/v1/settings` | `Settings` | `Settings`; validated (below); audited |
| GET | `/api/v1/system` | | `SystemInfo`; `lang=en` or `pl` (also `en-*`, `pl-*`) chooses the language of the health checks; without it, or for another language, the language setting decides |
| POST | `/api/v1/system/hooks` | `HookInstallRequest` | `HookStatus`; `403 disabled` in demo mode; audited |
| DELETE | `/api/v1/system/hooks` | | `HookStatus`; `403 disabled` in demo mode; audited |
| GET | `/api/v1/system/backups` | | `BackupInfo[]` |
| POST | `/api/v1/system/backup` | | `BackupInfo`; audited |
| POST | `/api/v1/system/purge` | `PurgeRequest` | `PurgeReport`; exactly one selector or `400 invalid`; audited |
| POST | `/api/v1/system/rescan` | | `IngestStatus` |
| GET | `/api/v1/audit` | | `AuditPage` (`limit`, `offset`, `from`, `to`, `q`) |
| GET | `/api/v1/search` | | `SearchResult[]` (`q`) |
| GET | `/api/v1/export/{view}` | | File download (below); audited |
| GET | `/api/v1/events` | | Server-sent events; each `data:` line is one `LiveEvent`, starting with the current `Ingest` status (except in demo mode) |

Everything outside `/api` serves the embedded web UI, with `index.html` as the fallback for unknown paths.

`PUT /api/v1/settings` refuses values outside these ranges with `400 invalid`: `week_start` 0 (Monday) to 6 (Sunday); `currency.code` exactly three ASCII letters and `currency.usd_rate` above 0 and at most 1000000; `plan.monthly_usd` absent or at least 0; `alerts.limit_thresholds` one to ten values, each from 1 to 100; `alerts.context_threshold` from 10 to 100; every `retention` day count from 0 (keep forever) to 36500. Saving also applies the settings to the running server. Switching `privacy.session_titles` off deletes the stored session titles, and the server keeps `statusline.capture_titles` in `config.toml` in step with it (at start and on every save), because the statusline fast path never opens the database and drops the session name before anything reaches the spool unless that flag is set.

`POST /api/v1/system/hooks` writes Kanjo's `statusLine` and the `SessionStart`, `Stop` and `SessionEnd` hooks into Claude Code's `settings.json` (a copy of the previous file is kept next to it). When the data directory is not the platform default, the commands carry `--home <data directory>` so they write where this server reads. With `"service": true` it also installs the background service. `DELETE /api/v1/system/hooks` removes Kanjo's entries from `settings.json` and puts a statusline command Kanjo was chained to back in place; it leaves the background service installed and running, which `kanjo service uninstall` removes.

`POST /api/v1/system/rescan` starts a re-import in the background and answers at once with the current `IngestStatus`. It forgets the transcript cursors and reads all history Claude Code still keeps from the start; stored requests deduplicate, so nothing is counted twice. Progress arrives as `Ingest` events, and a `Usage` event follows when new requests were added. While an import is already running the call starts nothing and only reports the status. A demo server has no collector, so there it does nothing.

`GET /api/v1/export/{view}` serves one of `usage`, `breakdown`, `sessions`, `blocks`, `turns`, `alerts` and `audit` (anything else is `404`) with `Content-Disposition: attachment; filename="kanjo-<view>-<YYYY-MM-DD>.<csv|json>"`. `format=csv` (the default) gives `text/csv; charset=utf-8`; `format=json` gives the view's API type as pretty-printed JSON (`UsageReport`, `Breakdown`, `SessionList`, `Block[]`, `Turn[]`, `AlertList`, `AuditPage`); any other format is `400 invalid`. The view takes the same query parameters as its endpoint. The list views `sessions`, `turns`, `alerts` and `audit` return up to 1000 rows per request (`limit` asks for fewer, `offset` pages further); `usage`, `breakdown` and `blocks` cover the whole range in one file.

## Audit log

Every entry has an actor, an action, a target and a JSON detail. Actors: `local` for a request to this API (the dashboard or a script), `cli` for a `kanjo` command in a terminal, and `service` for the server acting on its own (the daily retention). Entries also carry `detail.user`, the OS user that ran the process (`demo` in demo mode); API entries carry `detail.client`, the peer address, and command line entries `detail.via = "cli"`. Demo data contains invented entries.

| Action | Written by | Target | Detail |
| --- | --- | --- | --- |
| `export` | API, CLI | the view | `format`, `from`, `to`, `project`, `bytes` (the CLI adds `file` and `model`) |
| `transcript.view` | API | the session id | `messages`, `truncated` |
| `settings.update` | API | `settings` | `changed`: the top-level sections that changed |
| `data.purge` | API, CLI | `project:<id>`, `session:<id>`, `before:<ts>` or `everything` | `request`, `deleted` (the project's display name, or its alias under path redaction; the session id; the local date of the cutoff; or `everything`), `report` |
| `backup.create` | API, CLI | the backup file name | `size_bytes` |
| `backup.restore` | CLI | the backup file | |
| `pricing.import` | CLI | the price table version | `file`, `signed`, `models` |
| `pricing.reset` | CLI | the price table version | |
| `hooks.install` | API, CLI | the path of settings.json | `statusline`, `session_hooks`, `service`, `changed`, `settings_backup` |
| `hooks.uninstall` | API, CLI | the path of settings.json | `changed` |
| `service.install` | CLI | the executable | |
| `service.uninstall` | CLI | `service` | |
| `retention.apply` | CLI, service | `retention` | `report`, `settings` |
