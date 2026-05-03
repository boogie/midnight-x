#![forbid(unsafe_code)]

mod app;
mod cli;
mod panic_hook;

use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};

use mx_config::DEFAULT_CONFIG_TOML;

use crate::cli::{CliAction, RunOpts};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(args) {
        Ok(CliAction::PrintHelp) => {
            print!("{}", cli::HELP_TEXT);
            ExitCode::SUCCESS
        }
        Ok(CliAction::PrintVersion) => {
            println!("mx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(CliAction::PrintDefaultConfig) => {
            print!("{DEFAULT_CONFIG_TOML}");
            ExitCode::SUCCESS
        }
        Ok(CliAction::Run(opts)) => match run(&opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("mx: {e:#}");
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("mx: {e:#}\n\n{}", cli::HELP_TEXT);
            ExitCode::from(2)
        }
    }
}

fn run(opts: &RunOpts) -> Result<()> {
    install_logging();
    panic_hook::install();

    let (config, warnings) = mx_config::load(opts.config.as_deref()).context("loading config")?;
    if !warnings.is_empty() {
        for w in &warnings {
            tracing::warn!(key = %w.key, msg = %w.message, "config warning");
        }
        // Phase 2: also push these into a startup Modal::Error.
    }

    if let Err(e) = app::run(config) {
        return Err(anyhow!(e.to_string()));
    }
    Ok(())
}

fn install_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
