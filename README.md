# edcb-mcp

Rust client library, command line interface, and MCP server for EDCB/EpgTimer
CtrlCmd.

This crate currently provides a Tokio-based TCP client, binary codec, `edcb`
CLI, and `edcb-mcp` stdio MCP server for CtrlCmd APIs used by EDCB
integrations. The implementation is ported from `xtne6f/edcb.py`, with
KonomiTV's async usage used as a secondary reference.

## Supported in v1

- TCP transport
- `edcb` command line interface
- stdio MCP server surface
- EDCB primitive, string, vector, struct, and `SYSTEMTIME` codec
- Service, EPG, reserve, recorded-file, tuner, plugin, auto-add, manual-add,
  and notify-status read APIs
- Program search, recorded item detail retrieval, reservation detail retrieval,
  and event-based reservation preview/create/update/delete with recording
  options
- Utility parsers for `ChSet5.txt`, `LogoData.ini`, logo directory indexes, and
  program extended text

## Architecture

`EdcbClient` is a raw CtrlCmd client: its methods map closely to EDCB commands
and wire data structures. Application-level operations live in `flows`, such as
program search and event-based reservation preview/create. The CLI and MCP
server call these flows instead of embedding CtrlCmd orchestration directly.

## To Do

- [ ] Unix domain socket transport
- [ ] Windows named pipe transport
- [ ] View app stream / SrvPipe stream helpers
- [ ] Recorded-file, auto-add, and manual-add mutation APIs
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

## Command Line Interface

Run the `edcb` CLI with CLI options:

```sh
cargo run --bin edcb -- --host 127.0.0.1 --port 4510 services
```

The same connection settings can be supplied through environment variables:

```sh
EDCB_HOST=127.0.0.1 EDCB_PORT=4510 EDCB_TIMEOUT_SECONDS=15 cargo run --bin edcb -- services
```

CLI options take precedence over environment variables. Defaults are
`127.0.0.1`, port `4510`, and a 15 second timeout.

Output is a stable line-based summary by default. Use `--json` for full
structured output.

Available commands:

- `services`
- `reserves`
- `recorded list`
- `recorded get <info-id>`
- `programs search --keyword <text> [--title-only] [--service <onid:tsid:sid>]`
- `reserves get <reserve-id>`
- `reserves preview --event <onid:tsid:sid:eid> [recording options]`
- `reserves create --event <onid:tsid:sid:eid> [recording options] --yes`
- `reserves update <reserve-id> [recording options] --yes`
- `reserves delete <reserve-id> --yes`
- `tuner-reserves`
- `tuner-processes`
- `plugins <write|rec_name>`
- `notify-status`

`reserves preview` is a client-side preview that fetches the EDCB default
reservation settings and the target event, then builds the `ReserveData` that
would be sent. EDCB does not expose a reservation dry-run command. Use
`reserves create ... --yes` to send the actual add-reservation command.
`reserves update ... --yes` fetches the existing reservation, applies recording
option changes, sends the full updated reservation to EDCB, and returns the
updated reservation data.
`reserves delete ... --yes` first fetches the reservation by ID, then sends the
delete command and returns the deleted reservation data.
`programs search` prints event keys as `onid:tsid:sid:eid`, which can be passed
to `reserves preview` or `reserves create`.

Common recording options:

- `--priority <1-5>`
- `--enable` / `--disable`
- `--recording-mode <all|all-without-decoding|specified|specified-without-decoding|view>`
- `--start-margin <seconds>` and `--end-margin <seconds>`
- `--caption <default|enable|disable>` and `--data <default|enable|disable>`
- `--post-recording <default|nothing|standby|standby-and-reboot|suspend|suspend-and-reboot|shutdown>`

## MCP Server

Run the `edcb-mcp` stdio MCP server with CLI options:

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
- `get_reservation`
- `list_recorded`
- `get_recorded_info`
- `search_programs`
- `preview_reservation`
- `create_reservation`
- `update_reservation`
- `delete_reservation`
- `list_tuner_reserves`
- `list_tuner_processes`
- `list_plugins`
- `get_notify_status`

`preview_reservation` does not mutate EDCB state. `create_reservation` creates
one reservation from an event key and the server's default reservation settings.
Both accept an optional `options` object using KonomiTV-style recording setting
names:

```json
{
  "event": "32737:32737:1032:9285",
  "options": {
    "priority": 4,
    "recording_start_margin": 60,
    "recording_end_margin": 120
  }
}
```

`update_reservation` accepts `reserve_id` and required `options`.
`delete_reservation` fetches the reservation before deleting it and returns the
deleted reservation data.

## Development

Use the Nix dev shell through direnv, then run:

```sh
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
