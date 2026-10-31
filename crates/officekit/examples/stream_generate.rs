//! Writes section 3.12's streamed generation to a file: a template's
//! `placeholder` row written once per item into a package streamed straight
//! into the file, every entry with its sizes in a data descriptor.
//!
//! ```text
//! cargo run -p officekit --example stream_generate -- 100000 target/stream.docx
//! ```

#[path = "../tests/stream_generation/file_sink.rs"]
mod file_sink;
#[path = "../tests/stream_generation/generation.rs"]
mod generation;

use core::error::Error;
use std::path::PathBuf;

use clap::Parser;
use officekit_xml::ready::ready_now;
use os_shim::System as _;
use os_shim::real::RealSystem;

use crate::file_sink::FileSink;

/// Stream a generated document into a file.
#[derive(Parser)]
struct Arguments {
    /// How many times the template's `placeholder` row is written.
    count: usize,
    /// The `.docx` file written.
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse();
    let system = RealSystem::new();
    let file = FileSink(ready_now(system.create(&arguments.output))?);
    generation::generate(&generation::template()?, arguments.count, file)?.close()?;
    Ok(())
}
