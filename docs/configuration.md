# Configuration

Kanjo is an independent open-source project published by Kyotu Technology, not a product of Anthropic. "Claude" and "Claude Code" are Anthropic's names for its products.

Most settings (your plan and what you pay for it, display currency, alert thresholds, privacy, retention, language, theme, first day of the week) are changed in the dashboard and stored in the database. A few things that are needed before the database opens live in `config.toml` in the data directory.

| Platform | Data directory |
| --- | --- |
| Linux | `$XDG_DATA_HOME/kanjo`, by default `~/.local/share/kanjo` |
| macOS | `~/Library/Application Support/io.Kyotu.kanjo` |
| Windows | `%APPDATA%\Kyotu\kanjo\data` |

`--home DIR` or the `KANJO_HOME` environment variable choose another directory, and `--claude-dir DIR`, `KANJO_CLAUDE_DIR` or `CLAUDE_CONFIG_DIR` point Kanjo at a Claude Code directory other than `~/.claude`. `KANJO_NO_KEYRING=1` keeps the database key in the key file instead of the system keyring.

```toml
[server]
bind = "127.0.0.1"   # another address needs tls_cert, tls_key and auth = "token"
port = 4747
auth = "token"       # "none" is accepted on loopback only
# tls_cert = "/path/to/cert.pem"
# tls_key = "/path/to/key.pem"

[storage]
encryption = "auto"  # "keyring", "keyfile" or "off"; set before the first start

[statusline]
chain_command = "..."  # your previous status line, written by kanjo up
show_segment = false   # also print Kanjo's own segment when chaining
```

Encryption has to be decided before the database is created: Kanjo cannot encrypt an existing unencrypted database. To start over encrypted, move the database out of the data directory, set `encryption = "auto"` in the `[storage]` section of `config.toml` and run `kanjo import`; history older than the transcripts still on disk is lost. `kanjo doctor` and the System and privacy page say the same.

**Prices.** Costs use a price table compiled into the binary, generated from LiteLLM's model price data and versioned by date; `kanjo pricing show` shows which one is in use. To update prices without a network connection, import a file that uses the same LiteLLM field names. A signed file is checked against the minisign public key you trust:

```sh
kanjo pricing import prices.json --sig prices.json.minisig --pubkey minisign.pub
```

A file without a signature is accepted only with `--allow-unsigned`, and is recorded as unsigned. Kanjo has no built-in trusted key, so the trust decision is yours. An import reprices all stored history in one step and is written to the audit log; `kanjo pricing reset` goes back to the built-in table.

**Currency.** Prices are always computed in US dollars. In Settings you can pick a display currency and enter the exchange rate yourself, because Kanjo never fetches rates. The dashboard and alert texts use it; command-line reports and exports stay in US dollars.
