---
paths:
  - "README.md"
  - "README.pl.md"
  - "docs/**"
  - "CHANGELOG.md"
  - "SECURITY.md"
  - "CONTRIBUTING.md"
---

# Documentation

- `README.md` and `README.pl.md` say the same thing. A change to one is made in the other in the same commit.
- Documentation is descriptive prose in paragraphs. Use a list or a table only where it really organises the content.
- Documentation describes what exists. Plans, ideas and their status live only in `BACKLOG.md`, reasons for choices in `docs/decisions.md`. A limitation is stated as a fact (`has not been run on macOS`), never as a promise (`not yet`, `planned`, `will`). The pull request that closes a limitation rewrites or removes its sentence. `scripts/check-docs.sh` enforces the wording; to pass it, reword a limitation as a fact (has not been, does not); never delete a true limitation, and use the allow marker (`docs-check: allow`, inside an HTML comment in Markdown) for a sentence about runtime state.
- A platform is "tested" when a CI job or a named script tested it.
- Each fact has one home; the map is in `CLAUDE.md`.
- Examples are synthetic: no real project names, paths, account ids or e-mail addresses.
- A change a user would notice gets an entry under Unreleased in `CHANGELOG.md`.
- The security documents in `docs/security/` describe the code as it is. A change to what is stored, to a trust boundary or to the release process updates the matching document in the same pull request.
