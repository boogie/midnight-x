//! CLI parser. v1 surface is tiny on purpose; if it grows past 4-5 flags
//! we'll switch to `clap`.

use std::path::PathBuf;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run(RunOpts),
    PrintDefaultConfig,
    PrintVersion,
    PrintHelp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunOpts {
    pub config: Option<PathBuf>,
}

/// Parse argv-style arguments into a `CliAction`.
///
/// # Errors
///
/// Returns an error when an unknown flag is supplied, when `--config` is
/// missing its value, or when leftover positional arguments are present.
pub fn parse<I, S>(args: I) -> Result<CliAction>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let mut a = pico_args::Arguments::from_vec(args.into_iter().map(Into::into).collect());
    if a.contains(["-h", "--help"]) {
        return Ok(CliAction::PrintHelp);
    }
    if a.contains(["-V", "--version"]) {
        return Ok(CliAction::PrintVersion);
    }
    if a.contains("--print-default-config") {
        return Ok(CliAction::PrintDefaultConfig);
    }

    let config: Option<PathBuf> = a
        .opt_value_from_str("--config")
        .map_err(|e| anyhow!("--config: {e}"))?;

    let leftover = a.finish();
    if !leftover.is_empty() {
        return Err(anyhow!("unexpected arguments: {leftover:?}"));
    }
    Ok(CliAction::Run(RunOpts { config }))
}

pub const HELP_TEXT: &str = "\
mx — Midnight X, a modern Rust two-pane terminal file manager.

USAGE:
    mx [--config PATH]
    mx --print-default-config
    mx -V | --version
    mx -h | --help

OPTIONS:
    --config PATH              Use the given config file instead of the default location.
    --print-default-config     Write the canonical default config to stdout and exit.
    -V, --version              Print version and exit.
    -h, --help                 Print this help and exit.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_runs_with_defaults() {
        let a = parse::<_, &str>(std::iter::empty()).unwrap();
        assert_eq!(a, CliAction::Run(RunOpts::default()));
    }

    #[test]
    fn version_flag() {
        assert_eq!(parse(["-V"]).unwrap(), CliAction::PrintVersion);
        assert_eq!(parse(["--version"]).unwrap(), CliAction::PrintVersion);
    }

    #[test]
    fn print_default_config_flag() {
        assert_eq!(
            parse(["--print-default-config"]).unwrap(),
            CliAction::PrintDefaultConfig
        );
    }

    #[test]
    fn config_flag_takes_path() {
        let a = parse(["--config", "/etc/mx/x.toml"]).unwrap();
        match a {
            CliAction::Run(o) => assert_eq!(o.config.unwrap(), PathBuf::from("/etc/mx/x.toml")),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_arg_errors() {
        assert!(parse(["--gibberish"]).is_err());
    }
}
