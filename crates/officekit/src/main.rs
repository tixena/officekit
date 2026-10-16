//! Officekit CLI binary.

mod cli;
mod dispatch;
mod handlers;

use std::io::{stderr as stderr_handle, stdout as stdout_handle};
use std::process::ExitCode;

use clap::Parser as _;
use os_shim::real::RealSystem;

use crate::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let system = RealSystem::new();
    let mut stdout = stdout_handle().lock();
    let mut stderr = stderr_handle().lock();
    dispatch::run(&cli, &system, &mut stdout, &mut stderr)
}
