use rmcp::{ServiceExt, transport::stdio};

use edcb_mcp::mcp::{EdcbMcpServer, ServerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env_args().map_err(std::io::Error::other)?;
    let service = EdcbMcpServer::new(config).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
