//! Dispatch layer: routes parsed `Commands` to handlers and maps failures to exit codes.
//!
//! `main.rs` calls [`run`] as its sole action after parsing the CLI.

use std::io::{Read, Write};
use std::process::ExitCode;

use os_shim::System;

use crate::cli::{Cli, Commands};
use crate::handlers::{self, PackageValidator};

pub const EXIT_ERROR: u8 = 2;
pub const EXIT_OK: u8 = 0;

pub fn run<S>(
    cli: &Cli,
    system: &S,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> ExitCode
where
    S: System + ?Sized,
{
    let outcome = match cli.cmd() {
        Commands::Cat(args) => handlers::cmd_cat(system, args, stdout).map(|()| EXIT_OK),
        Commands::Down(args) => handlers::cmd_down(system, args, stdout).map(|()| EXIT_OK),
        Commands::Format(args) => handlers::cmd_format(system, args, stdout).map(|()| EXIT_OK),
        Commands::Get(args) => {
            handlers::cmd_get(system, args, stdin, stdout, stderr).map(|()| EXIT_OK)
        }
        Commands::Save(args) => {
            handlers::cmd_save(system, args, stdin, stdout, stderr).map(|()| EXIT_OK)
        }
        Commands::Up(args) => handlers::cmd_up(system, args, stdout).map(|()| EXIT_OK),
        Commands::Parts(args) => handlers::cmd_parts(system, args, stdout).map(|()| EXIT_OK),
        Commands::Validate(args) => {
            let validator = PackageValidator::new(system);
            handlers::cmd_validate(&validator, args, stdout, stderr).map_err(anyhow::Error::from)
        }
    };
    match outcome {
        Ok(status) => ExitCode::from(status),
        Err(err) => {
            let _: Result<(), _> = writeln!(stderr, "error: {err:#}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}
