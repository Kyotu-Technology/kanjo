---
paths:
  - "crates/kanjo-ingest/**"
---

# Ingest and parser

- The parser extracts usage numbers and metadata only. No prompt text, response text, tool input or tool output may reach a struct that is stored, logged or returned. Do not add such text to a `tracing` call or an error message either, not even at debug level.
- A record the parser does not understand goes to quarantine by its type, key signature, reason and Claude Code version (`write::record_quarantine`). The record itself is never kept.
- `transcript.rs` is the transcript viewer's reader. It reads a file on demand, is off by default and every view is audited; do not call it from the ingest pipeline.
- Fixtures under `tests/fixtures/` are synthetic: invented session ids, project names, paths, request ids and timestamps, and no message text. Never derive one by copying a real transcript, and never read the real `~/.claude` from a test; tests build their directories under a temporary directory.
- A parser test asserts the parsed usage numbers, not only that parsing succeeded. A fixture for an unknown record type asserts that it lands in quarantine.
- The `parser-fixture` skill has the full procedure.
