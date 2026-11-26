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
- State only what is true today and can be checked: a platform is "tested" when a CI job or a named script tested it, and a status note ("not released yet") is removed when it stops being true.
- Examples are synthetic: no real project names, paths, account ids or e-mail addresses.
- A change a user would notice gets an entry under Unreleased in `CHANGELOG.md`.
- The security documents in `docs/security/` describe the code as it is. A change to what is stored, to a trust boundary or to the release process updates the matching document in the same pull request.
