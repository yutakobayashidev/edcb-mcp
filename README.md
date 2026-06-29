# edcb-tools

[![DeepWiki](https://img.shields.io/badge/DeepWiki-yutakobayashidev%2Fedcb--tools-blue.svg?logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAACwAAAAyCAYAAAAnWDnqAAAAAXNSR0IArs4c6QAAA05JREFUaEPtmUtyEzEQhtWTQyQLHNak2AB7ZnyXZMEjXMGeK/AIi+QuHrMnbChYY7MIh8g01fJoopFb0uhhEqqcbWTp06/uv1saEDv4O3n3dV60RfP947Mm9/SQc0ICFQgzfc4CYZoTPAswgSJCCUJUnAAoRHOAUOcATwbmVLWdGoH//PB8mnKqScAhsD0kYP3j/Yt5LPQe2KvcXmGvRHcDnpxfL2zOYJ1mFwrryWTz0advv1Ut4CJgf5uhDuDj5eUcAUoahrdY/56ebRWeraTjMt/00Sh3UDtjgHtQNHwcRGOC98BJEAEymycmYcWwOprTgcB6VZ5JK5TAJ+fXGLBm3FDAmn6oPPjR4rKCAoJCal2eAiQp2x0vxTPB3ALO2CRkwmDy5WohzBDwSEFKRwPbknEggCPB/imwrycgxX2NzoMCHhPkDwqYMr9tRcP5qNrMZHkVnOjRMWwLCcr8ohBVb1OMjxLwGCvjTikrsBOiA6fNyCrm8V1rP93iVPpwaE+gO0SsWmPiXB+jikdf6SizrT5qKasx5j8ABbHpFTx+vFXp9EnYQmLx02h1QTTrl6eDqxLnGjporxl3NL3agEvXdT0WmEost648sQOYAeJS9Q7bfUVoMGnjo4AZdUMQku50McDcMWcBPvr0SzbTAFDfvJqwLzgxwATnCgnp4wDl6Aa+Ax283gghmj+vj7feE2KBBRMW3FzOpLOADl0Isb5587h/U4gGvkt5v60Z1VLG8BhYjbzRwyQZemwAd6cCR5/XFWLYZRIMpX39AR0tjaGGiGzLVyhse5C9RKC6ai42ppWPKiBagOvaYk8lO7DajerabOZP46Lby5wKjw1HCRx7p9sVMOWGzb/vA1hwiWc6jm3MvQDTogQkiqIhJV0nBQBTU+3okKCFDy9WwferkHjtxib7t3xIUQtHxnIwtx4mpg26/HfwVNVDb4oI9RHmx5WGelRVlrtiw43zboCLaxv46AZeB3IlTkwouebTr1y2NjSpHz68WNFjHvupy3q8TFn3Hos2IAk4Ju5dCo8B3wP7VPr/FGaKiG+T+v+TQqIrOqMTL1VdWV1DdmcbO8KXBz6esmYWYKPwDL5b5FA1a0hwapHiom0r/cKaoqr+27/XcrS5UwSMbQAAAABJRU5ErkJggg==)](https://deepwiki.com/yutakobayashidev/edcb-tools)

Rust client library, command line interface, and MCP server for EDCB/EpgTimer
CtrlCmd.

This crate currently provides a Tokio-based TCP client, binary codec, `edcb`
CLI, and `edcb-mcp` stdio MCP server for CtrlCmd APIs used by EDCB
integrations. The implementation is ported from `xtne6f/edcb.py`, with
KonomiTV's async usage used as a secondary reference.

## Distribution

Nix flake is the primary distribution surface. The default package builds both
first-class binaries:

- `edcb`
- `edcb-mcp`

Run the CLI directly from GitHub:

```sh
nix run github:yutakobayashidev/edcb-tools#edcb -- --host 127.0.0.1 services
```

Run the stdio MCP server directly from GitHub:

```sh
nix run github:yutakobayashidev/edcb-tools#edcb-mcp -- --host 127.0.0.1 --port 4510
```

Install both binaries into a Nix profile:

```sh
nix profile install github:yutakobayashidev/edcb-tools#edcb-tools
```

Use the package from another flake:

```nix
{
  inputs.edcb-tools.url = "github:yutakobayashidev/edcb-tools";

  outputs = { edcb-tools, ... }: {
    # edcb-tools.packages.${system}.default
    # edcb-tools.packages.${system}.edcb-tools
    # edcb-tools.apps.${system}.edcb
    # edcb-tools.apps.${system}.edcb-mcp
  };
}
```

The Rust client library is intended to be consumed from this repository, not
published to crates.io:

```toml
[dependencies]
edcb-tools = { git = "https://github.com/yutakobayashidev/edcb-tools" }
```

## Supported in v1

- TCP transport
- `edcb` command line interface
- stdio MCP server surface
- EDCB primitive, string, vector, struct, and `SYSTEMTIME` codec
- Service, EPG, reserve, recorded-file, tuner, plugin, auto-add, manual-add,
  and notify-status read APIs
- Program search, timetable retrieval, recorded item detail retrieval,
  reservation detail retrieval, and event-based reservation
  preview/create/update/delete with recording options
- Utility parsers for `ChSet5.txt`, `LogoData.ini`, logo directory indexes, and
  program extended text

## Architecture

`EdcbClient` is a raw CtrlCmd client: its methods map closely to EDCB commands
and wire data structures. Application-level operations such as program search
and event-based reservation preview/create are exported from the crate root. The
CLI and MCP server call these operations instead of embedding CtrlCmd
orchestration directly. TCP transport is isolated behind an internal transport
boundary so additional transports can be added without rewriting command
encoding.

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

use edcb_tools::{ConnectionConfig, EdcbClient};

#[tokio::main]
async fn main() -> edcb_tools::Result<()> {
    let client = EdcbClient::new(
        ConnectionConfig::new("127.0.0.1", 4510).with_timeout(Duration::from_secs(5)),
    );

    let services = client.enum_service().await?;
    for service in services {
        println!("{}: {}", service.sid, service.service_name);
    }

    Ok(())
}
```

## Command Line Interface

Run the `edcb` CLI through the flake:

```sh
nix run .#edcb -- --host 127.0.0.1 --port 4510 services
```

During development, the same CLI can be run with Cargo:

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

Run `edcb --help`, `edcb help`, or `edcb help <command>` for clap-generated
usage, options, and examples from the current build.

Available commands:

- `services`
- `reserves`
- `recorded list`
- `recorded get <info-id>`
- `programs search [search options]`
- `programs timetable [timetable options]`
- `channels`
- `recording defaults`
- `recording presets`
- `reservation-conditions`
- `reservation-conditions get <condition-id>`
- `reservation-conditions create [search options] [recording options] --yes`
- `reservation-conditions update <condition-id> [search options] [recording options] --yes`
- `reservation-conditions delete <condition-id> --yes`
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
`reserves create ... --yes` to send the actual add-reservation command. After
creation, the CLI fetches reservations again and returns the newly assigned
reservation ID when it can be resolved from the before/after difference.
`reserves update ... --yes` fetches the existing reservation, applies recording
option changes, sends the full updated reservation to EDCB, and returns the
updated reservation data.
`reserves delete ... --yes` first fetches the reservation by ID, then sends the
delete command and returns the deleted reservation data.
`programs search` prints event keys as `onid:tsid:sid:eid`, which can be passed
to `reserves preview` or `reserves create`.

Preview JSON has the same `ReserveData` shape that create/get return. The
previewed reservation has not been sent to EDCB yet. Abridged example:

```json
{
  "reserve_id": 0,
  "onid": 32736,
  "tsid": 32736,
  "sid": 1024,
  "eid": 4208,
  "start_time": "2026-06-29T22:00:00+09:00",
  "duration_second": 1800,
  "station_name": "Example Service",
  "title": "Example Program",
  "rec_setting": {
    "rec_mode": 1,
    "priority": 2
  }
}
```

Useful reservation preview selectors:

```sh
edcb --json reserves preview --event 32736:32736:1024:4208 \
  | jq '{event: "\(.onid):\(.tsid):\(.sid):\(.eid)", start_time, title, priority: .rec_setting.priority}'
```

Program search uses EDCB's `SearchKeyInfo`/`SearchPg` semantics. Date ranges are
recurring weekday/time-of-day ranges, not absolute datetimes. If no service is
specified, the CLI first fetches EDCB's service list and searches those services.
Use `programs timetable` when you want the program table for services/time
windows instead of keyword search.
`reservation-conditions` manages EDCB keyword auto reservations (`AutoAddData`)
with the same search options and recording options. EDCB does not return the
newly assigned AutoAdd ID from the add command, so create returns the condition
payload that was sent with `id` set to `0`; list or get conditions afterwards to
see assigned IDs.

Program search options:

- `--keyword <text>`
- `--exclude-keyword <text>`
- `--title-only`
- `--case-sensitive`
- `--regex`
- `--fuzzy`
- `--service <onid:tsid:sid>` (repeatable)
- `--genre <major:middle[:user_nibble]>` (repeatable)
- `--exclude-genre-ranges`
- `--date-range <start-dow:HH:MM-end-dow:HH:MM>` (repeatable, `0` is Sunday)
- `--exclude-date-ranges`
- `--duration-min <minutes>` and `--duration-max <minutes>`
- `--free-ca <all|free|paid>`
- `--search-enable` / `--search-disable`
- `--duplicate-title-check <none|same-channel|all-channels>`
- `--duplicate-title-check-days <days>`

Examples:

```sh
edcb programs search --keyword news --title-only
edcb programs search --keyword news --genre 0:1
edcb programs search --keyword news --date-range 1:19:00-1:23:00
edcb programs search --keyword news --duration-min 30 --duration-max 120 --free-ca free
edcb reservation-conditions create --keyword news --genre 0:1 --priority 4 --yes
edcb reservation-conditions update 77 --keyword news --duplicate-title-check same-channel --yes
```

Program timetable uses EDCB's `EnumPgInfoEx` semantics. It returns programs
grouped by service, nests short same-TS subchannels under their main channel,
and attaches reservation metadata when a matching reservation can be found.
JSON output includes `reservation_metadata_status`; if reservation lookup fails,
programs are still returned and the status contains the failure message.

Timetable options:

- `--service <onid:tsid:sid>` (repeatable)
- `--start-time <RFC3339 datetime>`
- `--end-time <RFC3339 datetime>`
- `--channel-type <gr|bs|cs|catv|sky|bs4k>`

Examples:

```sh
edcb programs timetable --channel-type gr
edcb programs timetable --service 32736:32736:1024 --start-time 2026-06-29T19:00:00+09:00 --end-time 2026-06-29T23:00:00+09:00
```

Timetable JSON nests program details under `channels[].programs[].event`.
Reservation metadata is optional per program; check
`reservation_metadata_status` before treating `reservation: null` as definitive.
Abridged example:

```json
{
  "channels": [
    {
      "service": {
        "onid": 32736,
        "tsid": 32736,
        "sid": 1024,
        "service_name": "Example Service"
      },
      "programs": [
        {
          "event": {
            "onid": 32736,
            "tsid": 32736,
            "sid": 1024,
            "eid": 4208,
            "start_time": "2026-06-29T22:00:00+09:00",
            "short_info": {
              "event_name": "Example Program"
            }
          },
          "reservation": {
            "id": 77,
            "status": "Reserved",
            "recording_availability": "Full"
          }
        }
      ],
      "subchannels": null
    }
  ],
  "date_range": {
    "earliest": "2026-06-29T19:00:00+09:00",
    "latest": "2026-06-29T23:00:00+09:00"
  },
  "reservation_metadata_status": "Ok"
}
```

Useful timetable selectors:

```sh
edcb --json programs timetable --channel-type gr \
  | jq -r '.channels[].programs[] | [.event.onid, .event.tsid, .event.sid, .event.eid, .event.start_time, .event.short_info.event_name, (.reservation != null)] | @tsv'

edcb --json programs timetable --channel-type gr \
  | jq -r '.channels[].programs[] | select(.reservation == null) | "\(.event.onid):\(.event.tsid):\(.event.sid):\(.event.eid)\t\(.event.start_time)\t\(.event.short_info.event_name)"'
```

`channels` returns a DB-free KonomiTV-style channel snapshot built from
`ChSet5.txt` and `EnumService`. It includes `display_channel_id`, channel type,
service key, remocon ID, subchannel/radio flags, and watchability flags. Because
it is stateless, it does not preserve recorded-only historical channels, pinned
channels, jikkyo state, or viewer counts. Plain output is still one line per
channel; JSON output wraps the list in `channels` and includes
`epg_service_status` so callers can distinguish missing EPG metadata from an
empty channel list.

```sh
edcb channels
edcb --json channels
```

`recording defaults` decodes the EDCB default reservation settings returned by
`GetReserve2(0x7fffffff)`. `recording presets` reads `EpgTimerSrv.ini` through
`FileCopy2` and returns global defaults plus recording presets, including ID 0.
If EDCB returns an empty `EpgTimerSrv.ini`, use `recording defaults` for the
effective reservation default.

```sh
edcb recording defaults
edcb --json recording presets
```

Common recording options:

- `--priority <1-5>`
- `--enable` / `--disable`
- `--recording-mode <all|all-without-decoding|specified|specified-without-decoding|view>`
- `--start-margin <seconds>` and `--end-margin <seconds>`
- `--caption <default|enable|disable>` and `--data <default|enable|disable>`
- `--post-recording <default|nothing|standby|standby-and-reboot|suspend|suspend-and-reboot|shutdown>`

## MCP Server

Run the `edcb-mcp` stdio MCP server through the flake:

```sh
nix run .#edcb-mcp -- --host 127.0.0.1 --port 4510 --timeout-seconds 15
```

During development, the same server can be run with Cargo:

```sh
cargo run --bin edcb-mcp -- --host 127.0.0.1 --port 4510 --timeout-seconds 15
```

The same connection settings can be supplied through environment variables:

```sh
EDCB_HOST=127.0.0.1 EDCB_PORT=4510 EDCB_TIMEOUT_SECONDS=15 cargo run --bin edcb-mcp
```

CLI options take precedence over environment variables. Defaults are
`127.0.0.1`, port `4510`, and a 15 second timeout.

Run `edcb-mcp --help` for clap-generated server options from the current build.

Exposed MCP tools:

- `list_services`
- `list_reserves`
- `get_reservation`
- `list_recorded`
- `get_recorded_info`
- `list_channels`
- `get_recording_defaults`
- `get_recording_presets`
- `search_programs`
- `get_timetable`
- `list_reservation_conditions`
- `get_reservation_condition`
- `create_reservation_condition`
- `update_reservation_condition`
- `delete_reservation_condition`
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

`search_programs` accepts KonomiTV-style search condition fields:

```json
{
  "is_enabled": true,
  "keyword": "news",
  "exclude_keyword": "sports",
  "is_title_only": true,
  "is_case_sensitive": false,
  "is_fuzzy_search_enabled": true,
  "is_regex_search_enabled": false,
  "service_ranges": [
    {
      "network_id": 32736,
      "transport_stream_id": 32736,
      "service_id": 1024
    }
  ],
  "genre_ranges": [
    {
      "major": 0,
      "middle": 1,
      "user_nibble": null
    }
  ],
  "is_exclude_genre_ranges": false,
  "date_ranges": [
    {
      "start_day_of_week": 1,
      "start_hour": 19,
      "start_minute": 0,
      "end_day_of_week": 1,
      "end_hour": 23,
      "end_minute": 0
    }
  ],
  "is_exclude_date_ranges": false,
  "duration_range_min": 30,
  "duration_range_max": 120,
  "broadcast_type": "FreeOnly",
  "duplicate_title_check_scope": "None",
  "duplicate_title_check_period_days": 6
}
```

`create_reservation_condition` accepts a required `condition` object with the
same fields as `search_programs` and an optional `options` object with recording
settings. `update_reservation_condition` accepts `condition_id`, optional
`condition`, and optional `options`.

`get_timetable` accepts service/time/channel filters and returns channels with
programs, optional nested subchannels, and best-effort reservation metadata. The
response includes `reservation_metadata_status` so callers can distinguish "no
matching reservation" from "reservation lookup failed":

```json
{
  "start_time": "2026-06-29T19:00:00+09:00",
  "end_time": "2026-06-29T23:00:00+09:00",
  "channel_type": "GR",
  "services": [
    {
      "network_id": 32736,
      "transport_stream_id": 32736,
      "service_id": 1024
    }
  ]
}
```

## Development

Use the Nix dev shell through direnv, then run:

```sh
nix fmt
nix build
nix run .#edcb -- --version
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
