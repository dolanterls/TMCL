//! `completions <SHELL>`: a script for the user's shell.

use clap::Args;

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    #[arg(value_name = "SHELL", help = "Which shell to generate for")]
    pub shell: clap_complete::Shell,
}
