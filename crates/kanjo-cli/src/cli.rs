use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "kanjo",
    version,
    about = "Kanjo: an offline cost and limit monitor for Claude Code",
    long_about = "Kanjo: an offline cost and limit monitor for Claude Code.\n\nStart with \"kanjo up\". Everything stays on this computer.",
    arg_required_else_help = true
)]
pub struct Cli {
    /// Kanjo's data directory (default: the platform data directory, or KANJO_HOME)
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,
    /// Claude Code's directory (default: ~/.claude, or KANJO_CLAUDE_DIR / CLAUDE_CONFIG_DIR)
    #[arg(long, global = true, value_name = "DIR")]
    pub claude_dir: Option<PathBuf>,
    /// Language of the messages: en or pl (default: the Kanjo setting, then LANG)
    #[arg(long, global = true, value_name = "LANG", value_parser = ["en", "pl"])]
    pub lang: Option<String>,
    /// No colours in the output
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Print data as JSON
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Set everything up and open the dashboard (safe to run again)
    Up {
        /// Do not ask before changing anything
        #[arg(long, short = 'y')]
        yes: bool,
        /// Do not open the browser
        #[arg(long)]
        no_open: bool,
        /// Leave Claude Code's settings.json alone (no status line, no hooks)
        #[arg(long)]
        no_hooks: bool,
        /// No background service; run Kanjo in this terminal instead
        #[arg(long)]
        no_service: bool,
    },
    /// Run the dashboard server in this terminal (what the background service runs)
    Serve {
        #[arg(long, value_name = "N")]
        port: Option<u16>,
        #[arg(long, value_name = "ADDR")]
        bind: Option<String>,
        /// Do not open the browser
        #[arg(long)]
        no_open: bool,
    },
    /// Show the dashboard with invented data, without touching your own
    Demo {
        #[arg(long, value_name = "N")]
        port: Option<u16>,
        #[arg(long)]
        no_open: bool,
        /// Days of invented history
        #[arg(long, value_name = "N")]
        days: Option<u32>,
        /// Only write the demo database, do not start the server
        #[arg(long, hide = true)]
        seed_only: bool,
        #[arg(long, hide = true, default_value_t = 42)]
        seed: u64,
    },
    /// Open the dashboard in your browser
    Open,
    /// Limits, forecast and spending at a glance
    Status,
    /// Check the installation and say what to fix
    Doctor,
    /// Used by Claude Code's status line (reads JSON on stdin)
    Statusline,
    /// Used by Claude Code's session hooks (reads JSON on stdin)
    Hook,
    /// Cost per day
    Daily(ReportArgs),
    /// Cost per week
    Weekly(ReportArgs),
    /// Cost per month
    Monthly(ReportArgs),
    /// Cost per session
    Session(ReportArgs),
    /// Five-hour usage windows
    Blocks(ReportArgs),
    /// Write a view to a CSV or JSON file
    Export {
        #[arg(value_enum)]
        view: ExportView,
        #[arg(long, value_enum)]
        format: ExportFormat,
        /// Output file (default: print)
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
        #[command(flatten)]
        filters: Filters,
        /// Dimension for the breakdown view
        #[arg(long, value_enum, default_value = "project")]
        by: ExportBy,
        /// Bucket size for the usage view
        #[arg(long, value_enum, default_value = "day")]
        group: ExportGroup,
    },
    /// Read Claude Code transcripts into the database now
    Import {
        /// Read every transcript again from the start (nothing is counted twice)
        #[arg(long)]
        rescan: bool,
    },
    /// Make or list backups of the database
    Backup {
        #[command(subcommand)]
        action: Option<BackupAction>,
    },
    /// Replace the database with a backup
    Restore {
        /// A backup file, or a name from "kanjo backup list"
        file: PathBuf,
    },
    /// Show, import or reset the price table
    Pricing {
        #[command(subcommand)]
        action: Option<PricingAction>,
    },
    /// Delete stored usage data
    Purge {
        #[command(flatten)]
        target: PurgeTarget,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Apply the retention settings now
    Retention {
        #[command(subcommand)]
        action: RetentionAction,
    },
    /// Show or check the audit log
    Audit {
        #[command(subcommand)]
        action: Option<AuditAction>,
    },
    /// Manage the background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
    /// Remove Kanjo from Claude Code and stop the background service
    Uninstall {
        /// Also delete Kanjo's data directory
        #[arg(long)]
        purge_data: bool,
        #[arg(long, short = 'y')]
        yes: bool,
        /// Leave the background service alone
        #[arg(long, hide = true)]
        no_service: bool,
    },
    /// Print a shell completion script
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Args, Clone, Default)]
pub struct Filters {
    /// First day, YYYY-MM-DD or YYYYMMDD
    #[arg(long, value_name = "DATE")]
    pub since: Option<String>,
    /// Last day (included), YYYY-MM-DD or YYYYMMDD
    #[arg(long, value_name = "DATE")]
    pub until: Option<String>,
    /// Only this project (name as shown in the dashboard)
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
    /// Only this model, e.g. claude-opus-5-5
    #[arg(long, value_name = "NAME")]
    pub model: Option<String>,
}

#[derive(Args, Clone)]
pub struct ReportArgs {
    #[command(flatten)]
    pub filters: Filters,
    /// One line per model under each row (blocks also list their top projects)
    #[arg(long)]
    pub breakdown: bool,
    /// Print CSV instead of a table
    #[arg(long)]
    pub csv: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportView {
    Usage,
    Breakdown,
    Sessions,
    Blocks,
    Turns,
    Alerts,
    Audit,
}

impl ExportView {
    pub fn name(self) -> &'static str {
        match self {
            ExportView::Usage => "usage",
            ExportView::Breakdown => "breakdown",
            ExportView::Sessions => "sessions",
            ExportView::Blocks => "blocks",
            ExportView::Turns => "turns",
            ExportView::Alerts => "alerts",
            ExportView::Audit => "audit",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportBy {
    Project,
    Model,
    Agent,
    Skill,
    Mcp,
    Tool,
    Branch,
    Client,
    Session,
    /// The part of Claude Code that sent the request (main conversation, compaction, ...)
    QuerySource,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ExportGroup {
    Hour,
    Day,
    Week,
    Month,
}

#[derive(Subcommand)]
pub enum BackupAction {
    /// Make a backup now
    Create,
    /// List backups
    List,
}

#[derive(Subcommand)]
pub enum PricingAction {
    /// Show the price table in use
    Show,
    /// Use a price table from a file
    Import {
        file: PathBuf,
        /// The minisign signature of the file (.minisig)
        #[arg(long, value_name = "FILE")]
        sig: Option<PathBuf>,
        /// A trusted minisign public key, or a .pub file (repeatable)
        #[arg(long, value_name = "KEY")]
        pubkey: Vec<String>,
        /// Accept a file without a signature
        #[arg(long)]
        allow_unsigned: bool,
    },
    /// Go back to the price table built into Kanjo
    Reset,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
pub struct PurgeTarget {
    /// Everything of one project
    #[arg(long, value_name = "NAME")]
    pub project: Option<String>,
    /// One session
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
    /// Everything before this day, YYYY-MM-DD
    #[arg(long, value_name = "DATE")]
    pub before: Option<String>,
    /// All usage data
    #[arg(long)]
    pub all: bool,
}

#[derive(Subcommand)]
pub enum RetentionAction {
    /// Delete what is older than the retention settings allow
    Apply,
}

#[derive(Subcommand)]
pub enum AuditAction {
    /// The newest entries
    List {
        #[arg(long, value_name = "N", default_value_t = 20)]
        limit: i64,
    },
    /// Check that no entry was changed or removed
    Verify,
}

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install and start the background service
    Install,
    /// Stop and remove the background service
    Uninstall,
    /// Is the background service running?
    Status,
}
