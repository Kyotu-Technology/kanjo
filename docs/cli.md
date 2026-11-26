# Command reference

Kanjo is an independent open-source project published by Kyotu Technology, not a product of Anthropic. "Claude" and "Claude Code" are Anthropic's names for its products.

Every command accepts `--home DIR` (the data directory), `--claude-dir DIR` (Claude Code's directory), `--lang en|pl`, `--no-color` and `--json`. `kanjo <command> --help` describes each one.

| Command | What it does |
| --- | --- |
| `kanjo up` | Connect Claude Code, install the background service and open the dashboard. `--yes`, `--no-hooks`, `--no-service`, `--no-open` |
| `kanjo demo` | The dashboard with invented data in a temporary directory. `--port N`, `--days N`, `--no-open` |
| `kanjo open` | Open the dashboard and sign the browser in |
| `kanjo status` | Limits, forecast and spending at a glance |
| `kanjo doctor` | Check the installation and say what to fix |
| `kanjo daily`, `weekly`, `monthly`, `session`, `blocks` | Cost reports. `--since` and `--until` (YYYY-MM-DD, local days), `--project NAME`, `--model NAME`, `--breakdown` (per model; `blocks` also lists the top three projects), `--csv`, or `--json` in the ccusage shape |
| `kanjo export VIEW --format csv\|json` | Write `usage`, `breakdown`, `sessions`, `blocks`, `turns`, `alerts` or `audit` to a file with `-o FILE`. `--by` picks the breakdown dimension (project, model, agent, skill, mcp, tool, branch, client, session, query-source), `--group` the bucket (hour, day, week, month) |
| `kanjo serve` | Run the server in this terminal (what the service runs). `--port N`, `--bind ADDR`, `--no-open` |
| `kanjo import` | Read transcripts into the database now; `--rescan` reads everything again without counting anything twice |
| `kanjo backup create`, `backup list`, `restore FILE` | Back up the database and restore a backup (restore refuses while Kanjo runs) |
| `kanjo pricing show`, `import FILE`, `reset` | Show, replace or reset the price table (see [Configuration](configuration.md)) |
| `kanjo purge` | Delete stored data: `--project NAME`, `--session ID`, `--before DATE` or `--all` |
| `kanjo retention apply` | Apply the retention settings now |
| `kanjo audit list`, `audit verify` | Show the audit log, check its hash chain |
| `kanjo service install`, `uninstall`, `status` | Manage the background service on its own |
| `kanjo uninstall` | Remove Kanjo from Claude Code and stop the service. `--purge-data`, `--yes` |
| `kanjo completions SHELL` | Shell completion for bash, zsh, fish, elvish or PowerShell |

`kanjo statusline` and `kanjo hook` are run by Claude Code, not by you.
