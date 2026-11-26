//! `kanjo`: the command line front end over the Kanjo engine crates.

#[macro_use]
mod i18n;
mod app;
mod cli;
mod data;
mod error;
mod fast;
mod fmt;
mod reports;
mod setup;

use app::{App, Global};
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, RetentionAction};
use error::CliResult;
use i18n::Lang;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let (home, command) = match args.as_slice() {
        [command] => (None, command.to_str()),
        [flag, home, command] if flag == "--home" => (Some(std::path::PathBuf::from(home)), command.to_str()),
        _ => (None, None),
    };
    match command {
        Some("statusline") => return fast::statusline(home, None, false),
        Some("hook") => return fast::hook(home, None),
        _ => {}
    }
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map(|l| format!(" ({}:{})", l.file(), l.line())).unwrap_or_default();
        eprintln!("Kanjo stopped because of an internal error{place}. Please report it at https://github.com/kyotu/kanjo/issues.");
    }));
    let cli = Cli::parse();
    let global = Global {
        home: cli.home.clone(),
        claude_dir: cli.claude_dir.clone(),
        lang: cli.lang.as_deref().and_then(Lang::from_code),
        no_color: cli.no_color,
        json: cli.json,
    };
    let fallback_lang = global.lang.unwrap_or_else(Lang::from_env);
    let mut lang = fallback_lang;
    let result = dispatch(cli.command, &global, &mut lang);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            if let Some(text) = f.render(lang) {
                eprintln!("{text}");
            }
            ExitCode::from(f.exit_code())
        }
    }
}

fn dispatch(command: Command, g: &Global, lang: &mut Lang) -> CliResult<()> {
    match command {
        Command::Statusline => {
            let _ = fast::statusline(g.home.clone(), g.claude_dir.clone(), g.no_color);
            return Ok(());
        }
        Command::Hook => {
            let _ = fast::hook(g.home.clone(), g.claude_dir.clone());
            return Ok(());
        }
        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "kanjo", &mut std::io::stdout());
            return Ok(());
        }
        Command::Demo { port, no_open, days, seed_only, seed } => return setup::demo(g, port, no_open, days, seed_only, seed),
        _ => {}
    }
    let mut app = App::new(g, true)?;
    *lang = app.lang;
    let result = run(&mut app, command);
    *lang = app.lang;
    result
}

fn run(app: &mut App, command: Command) -> CliResult<()> {
    use reports::Report;
    match command {
        Command::Up { yes, no_open, no_hooks, no_service } => setup::up(app, yes, no_open, no_hooks, no_service),
        Command::Serve { port, bind, no_open } => setup::serve(app, port, bind, no_open),
        Command::Open => setup::open_dashboard(app),
        Command::Status => data::status(app),
        Command::Doctor => data::doctor(app),
        Command::Daily(a) => reports::run(app, Report::Daily, &a),
        Command::Weekly(a) => reports::run(app, Report::Weekly, &a),
        Command::Monthly(a) => reports::run(app, Report::Monthly, &a),
        Command::Session(a) => reports::run(app, Report::Session, &a),
        Command::Blocks(a) => reports::run(app, Report::Blocks, &a),
        Command::Export { view, format, output, filters, by, group } => {
            reports::export_view(app, view, format, output.as_deref(), &filters, by, group)
        }
        Command::Import { rescan } => data::import(app, rescan),
        Command::Backup { action } => data::backup(app, action.as_ref()),
        Command::Restore { file } => data::restore(app, &file),
        Command::Pricing { action } => data::pricing(app, action.as_ref()),
        Command::Purge { target, yes } => data::purge(app, &target, yes),
        Command::Retention { action: RetentionAction::Apply } => data::retention_apply(app),
        Command::Audit { action } => data::audit(app, action.as_ref()),
        Command::Service { action } => setup::service(app, &action),
        Command::Uninstall { purge_data, yes, no_service } => setup::uninstall(app, purge_data, yes, no_service),
        Command::Statusline | Command::Hook | Command::Completions { .. } | Command::Demo { .. } => Ok(()),
    }
}
