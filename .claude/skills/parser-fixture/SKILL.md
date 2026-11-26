---
name: parser-fixture
description: Add a synthetic Claude Code transcript fixture and its test to the ingest crate. Use when the parser must handle a new record type, a new field, a new Claude Code version or a parsing bug.
---

# Adding a parser fixture

Fixtures are small JSONL files that imitate Claude Code transcript records. They live under `crates/kanjo-ingest/tests/fixtures/<root>/projects/<project-dir>/<session-id>.jsonl`, where each `<root>` stands for one Claude Code directory (`claude` for the current format, `cc-1.0` and `cc-2.4` for older and newer ones).

**Never copy a real transcript.** Do not read files under the real `~/.claude` to build a fixture, and do not accept one pasted by a user either. If the structure of a real record is needed, ask the person for the key names and value types only.

1. Describe the case in one sentence (a duplicated message, a sidechain, an unknown record type, a renamed usage field). That sentence becomes the test name.
2. Write the records by hand. Keep the keys the parser reads, the token counts and the model names that matter for the case. Invent everything else: session ids, project names, paths, request ids, timestamps. Leave every message text out or empty.
3. Add it to an existing root when it extends that format, or create a new root for a different Claude Code version.
4. Add a test in `crates/kanjo-ingest/tests/formats.rs` or `ingest.rs` that imports the root into a temporary store and asserts the parsed numbers: tokens per category, cost or unpriced, session and sidechain attribution. A test that only checks that parsing did not fail is not enough.
5. For a record type Kanjo does not understand, assert that it lands in quarantine with its type and key signature and that no usage row was written.
6. Assert that a second scan of the same files changes nothing (`rescan_changes_nothing` in `formats.rs` does this).
7. Run `cargo test -p kanjo-ingest --locked`.

Before finishing, search the new files for anything real:

```sh
grep -rnE "/home/|/Users/|C:\\\\Users|@[a-z0-9-]+\.[a-z]{2,}" crates/kanjo-ingest/tests/fixtures
```
