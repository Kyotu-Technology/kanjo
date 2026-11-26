---
paths:
  - "crates/kanjo-server/**"
---

# HTTP server

- The server listens on the loopback interface unless the user configures otherwise. It is the only crate that listens on the network, and nothing in the workspace connects out.
- All request guards live in `src/security.rs`: Host and Origin checks, the CSRF header on mutations, token authentication with its failure rate limit, the security headers, the handler timeout and the access log. A new route goes through the same layers; do not add a route that bypasses them, and never log a query string or a credential.
- A change to a guard, to authentication or to what the server exposes updates `docs/security/threat-model.md` in the same pull request.
- Handlers return types from `kanjo_core::api` and run database work in `spawn_blocking`.
- The Content-Security-Policy is strict (`style-src 'self'`, one hashed inline script). A UI change that needs it loosened is the wrong UI change.
- Integration tests are in `tests/api.rs`. A new endpoint is tested there, including how it answers a request without a valid session.
