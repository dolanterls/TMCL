use std::process::ExitCode;

use clap::Parser;

use tmcl::cli::args::{Cli, Command};
use tmcl::cli::exit_code;
use tmcl::cli::output::OutputOptions;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let output = OutputOptions::from_cli(&cli);
    let interactive = matches!(&cli.command, None | Some(Command::Tui));
    // Diagnostics go to stderr, which the TUI already owns: enabling them there
    // would tear the frame instead of helping.
    if !interactive {
        tmcl::core::diagnostics::set_enabled(
            cli.verbose || tmcl::core::diagnostics::wanted_by_environment(),
        );
    }
    let result = if interactive {
        tmcl::tui::run().await
    } else {
        tmcl::cli::commands::run(cli).await
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", output.error_message(&error));
            ExitCode::from(exit_code::for_category(error.category()))
        }
    }
}
