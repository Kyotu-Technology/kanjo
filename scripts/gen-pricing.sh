#!/bin/sh
# Dev-time only: regenerates assets/pricing.json from a LiteLLM price file.
# Kanjo itself never downloads anything; the result is embedded at build time.
# Usage: scripts/gen-pricing.sh path/to/model_prices_and_context_window.json YYYY-MM-DD
set -eu
src="$1"
version="$2"
jq --arg v "$version" '{
  version: $v,
  source: "LiteLLM model_prices_and_context_window.json (litellm_provider=anthropic)",
  currency: "USD",
  models: (to_entries
    | map(select(.value.litellm_provider == "anthropic" and (.key | startswith("claude-")) and .value.input_cost_per_token != null))
    | map({key: .key, value: (.value | {
        input_cost_per_token,
        output_cost_per_token,
        cache_read_input_token_cost,
        cache_creation_input_token_cost,
        cache_creation_input_token_cost_above_1hr,
        max_input_tokens,
        max_output_tokens
      } | with_entries(select(.value != null)))})
    | sort_by(.key) | from_entries)
}' "$src" > "$(dirname "$0")/../assets/pricing.json"
