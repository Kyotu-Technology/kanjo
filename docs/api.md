# HTTP API

The `kanjo` binary serves a JSON API under `/api/v1` for its own web UI, for scripts and for CI. It listens on the loopback interface unless told otherwise. All request and response bodies are the types defined in `crates/kanjo-core/src/api.rs` and `settings.rs`; the TypeScript equivalents are generated into `web/src/lib/api/bindings`.

Conventions: timestamps are Unix epoch milliseconds; money is USD; ratios run 0..1 and percentages 0..100. Read endpoints take the optional query parameters of `Query` (`from`, `to`, `project`, `model`, `agent`, `client`, `group`, `by`, `q`, `sort`, `limit`, `offset`, `period`, `lang`, `format`) and ignore the ones that do not apply. Without `from`/`to` an endpoint uses its natural default range (30 days for reports).

Authentication: unless disabled for loopback in `config.toml`, every `/api` request needs the access token, either as the `kanjo_session` cookie (set by opening `/?token=<token>`, which is what `kanjo open` does) or as `Authorization: Bearer <token>`. A missing or wrong token yields `401` with code `unauthorized`. Requests that change state (`POST`, `PUT`, `PATCH`, `DELETE`) must carry the header `X-Kanjo-CSRF: 1`. Requests whose `Host` header is not the address the server listens on are rejected.

Errors: any non-2xx response carries an `ApiError` body (`code`, `message`). Codes: `unauthorized`, `forbidden`, `not_found`, `invalid`, `disabled`, `internal`.

| Method | Path | Body | Response |
| --- | --- | --- | --- |
| GET | `/api/v1/health` | | `{ "status": "ok", "version": string }` (no auth) |
| GET | `/api/v1/overview` | | `Overview` |
| GET | `/api/v1/usage` | | `UsageReport` (`group`, range, filters) |
| GET | `/api/v1/breakdown` | | `Breakdown` (`by`, range, filters) |
| GET | `/api/v1/sessions` | | `SessionList` (range, `project`, `q`, `sort`, `limit`, `offset`) |
| GET | `/api/v1/sessions/{id}` | | `SessionDetail` |
| GET | `/api/v1/sessions/{id}/transcript` | | `Transcript`; `403 disabled` unless the viewer is enabled; audited |
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
| POST | `/api/v1/alerts/ack` | `{ "id": number or null }` (null = all) | `{ "acknowledged": number }` |
| GET | `/api/v1/budgets` | | `Budget[]` |
| POST | `/api/v1/budgets` | `BudgetInput` | `Budget` |
| PUT | `/api/v1/budgets/{id}` | `BudgetInput` | `Budget` |
| DELETE | `/api/v1/budgets/{id}` | | `204` |
| GET | `/api/v1/settings` | | `Settings` |
| PUT | `/api/v1/settings` | `Settings` | `Settings`; audited |
| GET | `/api/v1/system` | | `SystemInfo` |
| POST | `/api/v1/system/hooks` | `HookInstallRequest` | `HookStatus`; audited |
| DELETE | `/api/v1/system/hooks` | | `HookStatus`; audited |
| GET | `/api/v1/system/backups` | | `BackupInfo[]` |
| POST | `/api/v1/system/backup` | | `BackupInfo`; audited |
| POST | `/api/v1/system/purge` | `PurgeRequest` | `PurgeReport`; audited |
| POST | `/api/v1/system/rescan` | | `IngestStatus` |
| GET | `/api/v1/audit` | | `AuditPage` (`limit`, `offset`) |
| GET | `/api/v1/search` | | `SearchResult[]` (`q`) |
| GET | `/api/v1/export/{view}` | | File download, `format=csv` or `json`; views: `usage`, `breakdown`, `sessions`, `blocks`, `turns`, `alerts`, `audit`; audited |
| GET | `/api/v1/events` | | Server-sent events; each `data:` line is one `LiveEvent` |

Everything outside `/api` serves the embedded web UI, with `index.html` as the fallback for unknown paths.
