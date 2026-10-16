//! Command handlers: one `cmd_*` function per subcommand.

use std::io::Write;

use anyhow::{Context as _, Result};
use os_shim::System;
use serde_json::json;

use officekit_core::packaging::package::Package;
use officekit_core::packaging::part_name::PartName;

use crate::cli::PartsArgs;

/// `officekit parts <FILE>`: list every part in the package, one per line
/// or as a JSON object with `--json`.
pub fn cmd_parts<S>(system: &S, args: &PartsArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let path = args.file();
    let package =
        Package::open(system, path).with_context(|| format!("opening {}", path.display()))?;
    if args.json() {
        let parts: Vec<&str> = package.part_names().map(PartName::as_str).collect();
        let payload = json!({ "file": path.display().to_string(), "parts": parts });
        writeln!(stdout, "{}", serde_json::to_string_pretty(&payload)?)?;
    } else {
        for name in package.part_names() {
            writeln!(stdout, "{name}")?;
        }
    }
    Ok(())
}
