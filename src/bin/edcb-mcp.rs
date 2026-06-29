use std::process;

use rmcp::{ServiceExt, transport::stdio};

use edcb_tools::mcp::{EdcbMcpServer, ServerAction};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match ServerAction::from_env_args() {
        Ok(ServerAction::Run(config)) => config,
        Ok(ServerAction::Help(text) | ServerAction::Version(text)) => {
            print!("{text}");
            return Ok(());
        }
        Err(error) => {
            eprintln!("error: {error}");
            process::exit(2);
        }
    };
    let service = EdcbMcpServer::new(config).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
