//! Command handlers: one `cmd_*` function per subcommand.

use std::io::Write;

use anyhow::{Context as _, Result};
use os_shim::System;
use serde_json::json;

use officekit_core::packaging::package::Package;

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
    let parts = archive_entries(&package)?;
    if args.json() {
        let payload = json!({ "file": path.display().to_string(), "parts": parts });
        writeln!(stdout, "{}", serde_json::to_string_pretty(&payload)?)?;
    } else {
        for name in &parts {
            writeln!(stdout, "{name}")?;
        }
    }
    Ok(())
}

/// What the archive holds: `[Content_Types].xml`, every relationship part,
/// and every other part, sorted by case-folded name.
fn archive_entries(package: &Package) -> Result<Vec<String>> {
    let mut entries = vec!["/[Content_Types].xml".to_owned()];
    entries.extend(package.relationship_sets().into_iter().map(|(source, _)| {
        source.map_or_else(
            || "/_rels/.rels".to_owned(),
            |part| part.relationships_part(),
        )
    }));
    entries.extend(
        package
            .part_names()?
            .iter()
            .map(|name| name.as_str().to_owned()),
    );
    entries.sort_by_cached_key(|entry| entry.to_ascii_lowercase());
    Ok(entries)
}
