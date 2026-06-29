use std::process;

use edcb_tools::cli::{CliAction, execute, help_text, version_text};

#[tokio::main]
async fn main() {
    match CliAction::from_env_args() {
        Ok(CliAction::Help) => {
            print!("{}", help_text());
        }
        Ok(CliAction::Version) => {
            print!("{}", version_text());
        }
        Ok(CliAction::Run(invocation)) => match execute(invocation).await {
            Ok(output) => print!("{output}"),
            Err(error) => {
                eprintln!("error: {error}");
                process::exit(error.exit_code);
            }
        },
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("Use `edcb --help` for usage.");
            process::exit(error.exit_code);
        }
    }
}
