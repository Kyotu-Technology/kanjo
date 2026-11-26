# Instructions for coding agents

The project instructions are in [CLAUDE.md](CLAUDE.md): what Kanjo is, the crate map, the rules that are not negotiable, the style and the commands. Read that file before changing anything; it applies to every agent, whatever tool runs it.

More specific guidance sits next to it. `.claude/rules/` holds short rules for one part of the tree each (the web UI, the store, the API contract, the parser, the server, dependencies, workflows, scripts, documentation); the `paths` list at the top of each file says which files it covers. `.claude/skills/` holds step-by-step procedures (checking a change, changing an API shape, adding a migration, adding a parser fixture, running the demo). [CONTRIBUTING.md](CONTRIBUTING.md) describes how to build, test and open a pull request.
