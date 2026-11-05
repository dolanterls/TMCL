//! `completions <SHELL>`: print a script the shell can source.
//!
//! Generated from the same `clap::Command` the parser uses, so a new subcommand
//! shows up in the script without anyone remembering to update a list here.

use clap::CommandFactory;

use crate::cli::args::CompletionsArgs;
use crate::cli::output::OutputOptions;
use crate::core::error::Result;

pub(super) fn run(args: CompletionsArgs, _output: OutputOptions) -> Result<()> {
    let mut command = crate::cli::args::Cli::command();
    let name = command.get_name().to_owned();
    // Straight to stdout: the script is the command's result, not progress.
    clap_complete::generate(args.shell, &mut command, name, &mut std::io::stdout());
    Ok(())
}
