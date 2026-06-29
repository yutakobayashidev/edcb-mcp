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
- Get timetable programs: `edcb programs timetable [timetable options]`
- List channel metadata: `edcb channels`
- Get default recording settings: `edcb recording defaults`
- Get recording presets from `EpgTimerSrv.ini`: `edcb recording presets`
- Search program titles only: `edcb programs search --keyword <text> --title-only`
- Search within one service:
  `edcb programs search --keyword <text> --service <onid:tsid:sid>`
- Search by numeric EDCB genre:
  `edcb programs search --keyword <text> --genre <major:middle[:user_nibble]>`
- Search by EDCB recurring weekday/time range:
  `edcb programs search --keyword <text> --date-range <start-dow:HH:MM-end-dow:HH:MM>`
- List keyword auto reservation conditions: `edcb reservation-conditions`
- Get one keyword auto reservation condition: `edcb reservation-conditions get <condition-id>`
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
- `--genre <major:middle[:user_nibble]>` (repeatable)
- `--exclude-genre-ranges`
- `--date-range <start-dow:HH:MM-end-dow:HH:MM>` (repeatable, `0` is Sunday)
- `--exclude-date-ranges`
- `--duration-min <minutes>` and `--duration-max <minutes>`
- `--free-ca <all|free|paid>`
- `--search-enable` / `--search-disable`
- `--duplicate-title-check <none|same-channel|all-channels>`
- `--duplicate-title-check-days <days>`

Do not invent `--from` or `--to` for program search. EDCB/KonomiTV date ranges
are recurring weekday/time-of-day ranges, not absolute datetimes.

Use `programs timetable` for EnumPgInfoEx-backed program table retrieval:

```sh
edcb --json programs timetable --channel-type gr
edcb programs timetable --service 32736:32736:1024 --start-time 2026-06-29T19:00:00+09:00 --end-time 2026-06-29T23:00:00+09:00
```

Timetable options:

- `--service <onid:tsid:sid>` (repeatable)
- `--start-time <RFC3339 datetime>`
- `--end-time <RFC3339 datetime>`
- `--channel-type <gr|bs|cs|catv|sky|bs4k>`

JSON timetable output includes `reservation_metadata_status`. Programs may be
present even when reservation metadata lookup failed, so do not treat missing
per-program reservations as definitive unless the status is `Ok`.

Use `channels` when the caller wants KonomiTV-style channel IDs without a DB:

```sh
edcb channels
edcb --json channels
```

Plain `channels` output is one line per channel. JSON output is an object with
`channels` and `epg_service_status`; check the status before assuming EPG
metadata such as remocon IDs was available.

Use `recording defaults` for the current EDCB default reservation settings, and
`recording presets` for `EpgTimerSrv.ini` presets:

```sh
edcb recording defaults
edcb --json recording presets
```

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

`reserves create` sends `AddReserve`, fetches reservations again, and returns
the newly assigned reservation ID when the created reservation can be resolved.
Plain reservation mutation output starts with an action label such as
`created`, `updated`, or `deleted`.

EDCB does not expose reservation dry-run. Use `reserves preview` for the
client-side preview that builds the reservation payload without sending a
mutation command.

Keyword auto reservations are exposed as reservation conditions:

```sh
edcb reservation-conditions
edcb reservation-conditions get 77
edcb reservation-conditions create --keyword news --genre 0:1 --priority 4 --yes
edcb reservation-conditions update 77 --keyword news --duplicate-title-check same-channel --yes
edcb reservation-conditions delete 77 --yes
```

`reservation-conditions create` sends EDCB AutoAdd data. EDCB does not return
the assigned AutoAdd ID from the add command, so list conditions afterwards when
the assigned ID is needed.

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
- If behavior or command availability is uncertain, run `edcb --help` or
  `edcb help <command>` from the current build before answering.
