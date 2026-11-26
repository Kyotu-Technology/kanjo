---
name: update-pricing
description: Regenerate the embedded price table (assets/pricing.json) from a local copy of LiteLLM's model price file.
argument-hint: <path/to/model_prices_and_context_window.json> <YYYY-MM-DD>
disable-model-invocation: true
---

# Updating the price table

Prices are compiled into the binary from `assets/pricing.json`. A wrong price silently changes every report, so this is a manual, reviewed step.

The source is a copy of LiteLLM's `model_prices_and_context_window.json` that the maintainer downloaded. Do not download it yourself: ask for the path if `$ARGUMENTS` does not give one, and ask for the date of the source file, which becomes the table's version.

1. Run the generator (it needs `jq`):

   ```sh
   scripts/gen-pricing.sh <path> <YYYY-MM-DD>
   ```

   It keeps only Anthropic models whose name starts with `claude-` and overwrites `assets/pricing.json`.
2. Show `git diff assets/pricing.json` and list, model by model, what was added, removed and changed. A removed model or a changed price of an existing model needs the maintainer's explicit confirmation before going further; older transcripts still refer to old models.
3. Do not add placeholder prices. A model missing from the table stays unpriced, which is the intended behaviour.
4. Run `cargo test -p kanjo-core --locked`.
5. Add a CHANGELOG entry under Unreleased that names the table version, and say in the pull request where the source file came from and which models changed.
