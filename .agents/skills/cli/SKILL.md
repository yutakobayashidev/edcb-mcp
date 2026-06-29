---
name: cli
description: Use the edcb CLI in this repository. Use when the user wants to run, explain, demonstrate, or troubleshoot `edcb` commands for EDCB CtrlCmd operations, including service listing, program search, recorded item lookup, reservation preview/create/update/delete, recording options, JSON output, connection flags, and Nix flake invocation.
---

# edcb CLI

## Overview

Use the `edcb` binary as the first-class command line surface for EDCB CtrlCmd
operations in this repository. Prefer Nix flake invocation in user-facing
examples, then Cargo invocation for development.

## Invocation

Use local flake apps from this repo:

```sh
nix run .#edcb -- --host 127.0.0.1 --port 4510 services
```

Use GitHub flake apps for installed/distributed usage:

```sh
nix run github:yutakobayashidev/edcb-tools#edcb -- --host 127.0.0.1 services
```

Use Cargo only for development:

```sh
cargo run --bin edcb -- --host 127.0.0.1 --port 4510 services
```

Global flags:

- `--host <host>`: EDCB host, env `EDCB_HOST`, default `127.0.0.1`
- `--port <port>`: CtrlCmd port, env `EDCB_PORT`, default `4510`
- `--timeout-seconds <n>`: request timeout, env `EDCB_TIMEOUT_SECONDS`, default
  `15`
- `--json`: pretty JSON output
- `--plain`: stable line-based output

CLI flags take precedence over environment variables.

## Read Commands

- List services: `edcb services`
- List reservations: `edcb reserves`
- Get one reservation: `edcb reserves get <reserve-id>`
- List recorded items: `edcb recorded list`
- Get one recorded item: `edcb recorded get <info-id>`
- Search programs: `edcb programs search [search options]`
- Search program titles only: `edcb programs search --keyword <text> --title-only`
- Search within one service:
  `edcb programs search --keyword <text> --service <onid:tsid:sid>`
- Search by EDCB recurring weekday/time range:
  `edcb programs search --keyword <text> --date-range <start-dow:HH:MM-end-dow:HH:MM>`
- List tuner reservations: `edcb tuner-reserves`
- List tuner processes: `edcb tuner-processes`
- List plugins: `edcb plugins <write|rec_name>`
- Get notify status: `edcb notify-status`

Use `--json` when the caller needs structured data:

```sh
edcb --json programs search --keyword news --title-only
```

Program search options use EDCB `SearchKeyInfo`/`SearchPg` semantics:

- `--keyword <text>`
- `--exclude-keyword <text>`
- `--title-only`
- `--case-sensitive`
- `--regex`
- `--fuzzy`
- `--service <onid:tsid:sid>` (repeatable)
- `--date-range <start-dow:HH:MM-end-dow:HH:MM>` (repeatable, `0` is Sunday)
- `--exclude-date-ranges`
- `--duration-min <minutes>` and `--duration-max <minutes>`
- `--free-ca <all|free|paid>`

Do not invent `--from` or `--to` for program search. EDCB/KonomiTV date ranges
are recurring weekday/time-of-day ranges, not absolute datetimes.

## Reservation Commands

Program search prints event keys as `onid:tsid:sid:eid`. Reuse that key for
reservation commands.

Preview a reservation without mutating EDCB:

```sh
edcb reserves preview --event 32736:32736:1024:4208
```

Create, update, and delete require `--yes`:

```sh
edcb reserves create --event 32736:32736:1024:4208 --priority 4 --yes
edcb reserves update 1 --disable --yes
edcb reserves delete 1 --yes
```

EDCB does not expose reservation dry-run. Use `reserves preview` for the
client-side preview that builds the reservation payload without sending a
mutation command.

Recording options:

- `--priority <1-5>`
- `--enable` / `--disable`
- `--recording-mode <all|all-without-decoding|specified|specified-without-decoding|view>`
- `--start-margin <seconds>` and `--end-margin <seconds>`
- `--caption <default|enable|disable>` and `--data <default|enable|disable>`
- `--post-recording <default|nothing|standby|standby-and-reboot|suspend|suspend-and-reboot|shutdown>`

## Troubleshooting

- If a connection fails, check host, port, timeout, and whether EDCB CtrlCmd is
  reachable from the current environment.
- If a mutation command is rejected locally, check that `--yes` is present.
- If a reservation command needs an event key, run `programs search` first.
- If output is hard to parse, rerun with `--json`.
- If behavior or command availability is uncertain, run `edcb --help` from the
  current build before answering.
