---
name: backlog-wave
description: Ship several entries of BACKLOG.md in one pass (parallel builders in isolated worktrees, independent review, one integration branch, one pull request). Use when asked to implement the Now section, a list of K-IDs or "everything that is ready".
---

# Shipping a wave of backlog entries

The section "How a request becomes a change" in `CLAUDE.md` and the rule in `.claude/rules/backlog.md` apply; this adds what is specific to doing several entries at once. Treat the steps as the shape that has worked, and change it when the entries call for something else: two small entries in one crate need no workflow at all.

## Choose the wave

Read the whole of Now. An entry belongs to the wave when its `Depends:` entries are shipped or in the same wave and its `Done when:` can be reached from this machine. Entries that need hardware, another operating system, an action in the GitHub organisation or a maintainer decision are left out and named in the report with what they wait for. Then look at which files each entry is likely to touch: entries that overlap go to one builder, in dependency order, and the rest run in parallel. Entries that turn out larger than their `Size:` suggests are better dropped from the wave than half done.

## Brief and build

Write one brief file outside the repository with what every builder needs: the non-negotiable rules that matter for these entries, the decisions already taken, the skills to use (`api-change`, `add-migration`, `parser-fixture`), and the rule that a builder commits nothing outside its worktree and changes no GitHub state. Give each builder its entry text, its own worktree on its own branch cut from the current `origin/main`, and a required result: the commit, the checks it ran with their output, the CHANGELOG line, the documentation sentences it rewrote, and what it could not verify. `BACKLOG.md`, `CHANGELOG.md` and other files every entry touches are edited only at integration, which avoids conflicts between builders.

Each finished worktree is then read by a reviewer that did not build it, against the entry's `Done when:` line and the rules; the reviewer confirms a finding by reproducing it before fixing it. Use the `invariant-reviewer` when the entry touches the parser, the store, the server, dependencies or fixtures. Model and effort follow the work: writing and review need a strong model, running checks and collecting facts do not.

## Integrate

Bring the entries onto one branch, one commit per entry, each commit deleting its entry from `BACKLOG.md` and carrying its CHANGELOG line. What the builders or reviewers found and did not fix goes under Proposed with the next ID. Run `preflight` with every group that applies on the integrated branch, because checks that passed per worktree say nothing about the combination. Push, open one pull request that lists the entries with their K-IDs, and watch CI.

## Finish

Remove the worktrees and the per-entry branches, and stop anything started for a check. Report per entry: shipped, verified how, and what remains unverified (a platform that only CI ran, a script that was parsed and never executed). List the entries left out of the wave and the reason for each.
