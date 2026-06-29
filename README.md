# edcb-mcp

Rust client library for EDCB/EpgTimer CtrlCmd.

This crate currently provides a Tokio-based TCP client and binary codec for the
read-oriented CtrlCmd APIs used by EDCB integrations. The implementation is
ported from `xtne6f/edcb.py`, with KonomiTV's async usage used as a secondary
reference.

## Supported in v1

- TCP transport
- stdio MCP server surface
- EDCB primitive, string, vector, struct, and `SYSTEMTIME` codec
- Service, EPG, reserve, recorded-file, tuner, plugin, auto-add, manual-add,
  and notify-status read APIs
- Utility parsers for `ChSet5.txt`, `LogoData.ini`, logo directory indexes, and
  program extended text

## To Do

- [ ] Unix domain socket transport
- [ ] Windows named pipe transport
- [ ] View app stream / SrvPipe stream helpers
- [ ] Reserve, recorded-file, auto-add, and manual-add mutation APIs
- [x] MCP server surface
- [ ] HTTP MCP transport

## Example

```rust
use std::time::Duration;

use edcb_mcp::EdcbClient;

#[tokio::main]
async fn main() -> edcb_mcp::Result<()> {
    let mut client = EdcbClient::new("127.0.0.1", 4510);
    client.set_timeout(Duration::from_secs(5));

    let services = client.enum_service().await?;
    for service in services {
        println!("{}: {}", service.sid, service.service_name);
    }

    Ok(())
}
```

## MCP Server

Run the stdio MCP server with CLI options:

```sh
cargo run --bin edcb-mcp -- --host 127.0.0.1 --port 4510 --timeout-seconds 15
```

The same connection settings can be supplied through environment variables:

```sh
EDCB_HOST=127.0.0.1 EDCB_PORT=4510 EDCB_TIMEOUT_SECONDS=15 cargo run --bin edcb-mcp
```

CLI options take precedence over environment variables. Defaults are
`127.0.0.1`, port `4510`, and a 15 second timeout.

Exposed MCP tools:

- `list_services`
- `list_reserves`
- `list_recorded`
- `get_recorded_info`
- `list_tuner_reserves`
- `list_tuner_processes`
- `list_plugins`
- `get_notify_status`

## Development

Use the Nix dev shell through direnv, then run:

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
