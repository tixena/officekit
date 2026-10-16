//! Dispatch layer: routes parsed `Commands` to handlers and maps failures to exit codes.
//!
//! `main.rs` calls [`run`] as its sole action after parsing the CLI.

use std::io::Write;
use std::process::ExitCode;

use os_shim::System;

use crate::cli::{Cli, Commands};
use crate::handlers;

pub const EXIT_ERROR: u8 = 1;

pub fn run<S>(cli: &Cli, system: &S, stdout: &mut dyn Write, stderr: &mut dyn Write) -> ExitCode
where
    S: System + ?Sized,
{
    let outcome = match cli.cmd() {
        Commands::Parts(args) => handlers::cmd_parts(system, args, stdout),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            let _: Result<(), _> = writeln!(stderr, "error: {err:#}");
            ExitCode::from(EXIT_ERROR)
        }
    }
}
