---
paths:
  - "BACKLOG.md"
  - "docs/decisions.md"
---

# Backlog and decisions

- `BACKLOG.md` has the sections Now, Proposed and Later, and the section an entry is in is its status. There is no status field and no in-progress state. It lists only work that is meant to be done: an idea that is dropped is deleted, not kept.
- An entry is `### K-NNN Short title`, then `Size: S.` (S, M, L or XL, defined in the header) with `Depends: K-NNN.` only when it applies, then a `Why:` line and a `Done when:` line that can be checked. An entry under Later also has a `Trigger:` line. An entry does not say who does it.
- A new entry goes under Proposed with the ID after `Last ID used`, and that line is raised in the same edit. IDs are never reused, and no ID is above `Last ID used`.
- Only the maintainer moves entries out of Proposed, to Now or Later, or deletes them.
- The pull request that ships an entry deletes it, adds a line under Unreleased in `CHANGELOG.md` when a user would notice the change, and rewrites the documentation sentence it makes untrue.
- Nothing private goes into either file: no personal matters, credentials, hardware or schedules.
- `docs/decisions.md` holds only decisions in force: `## D-NNN Title (YYYY-MM-DD)`, then Context, Decision and Consequences in three to six lines. A decision that no longer holds is removed with the change that ends it.
- `sh scripts/check-docs.sh` checks the backlog format and the wording of `docs/decisions.md`.
