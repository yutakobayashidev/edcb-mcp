use std::process;

use edcb_tools::cli::{CliAction, execute, version_text};

#[tokio::main]
async fn main() {
    match CliAction::from_env_args() {
        Ok(CliAction::Help(text)) => {
            print!("{text}");
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
            if !error.message.contains("Usage:") {
                eprintln!("Use `edcb --help` for usage.");
            }
            process::exit(error.exit_code);
        }
    }
}
