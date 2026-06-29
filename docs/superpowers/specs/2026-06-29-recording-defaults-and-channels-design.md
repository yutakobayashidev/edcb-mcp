# Recording Defaults And Channels Design

## Goal

Add DB-free KonomiTV-inspired surfaces for recording defaults/presets and channel
metadata while keeping EDCB CtrlCmd as the only state source.

## Scope

This change exposes current snapshots only. It does not persist channel rows,
user display ordering, pinned channels, recorded-only historical channels,
jikkyo state, or viewer counts.

## Recording Defaults

Expose a full `RecordSettings` value for read APIs. `RecordSettingsPatch`
continues to be the mutation input for reservation create/update flows.

Two sources are supported:

- `get_recording_defaults()` calls `EdcbClient::get_default_reserve()` and
  decodes the returned `RecSettingData`.
- `get_recording_presets()` fetches `EpgTimerSrv.ini` with `file_copy2()` and
  parses `[SET]`, `[REC_DEF]`, `[REC_DEF{id}]`, `REC_DEF_FOLDER*`, and
  `REC_DEF_FOLDER_1SEG*` sections in the same broad shape as KonomiTV.

The preset response contains `global_defaults` plus `presets`, including ID 0.
Malformed custom preset IDs are skipped. Failure to fetch or parse the default
preset is an error.

## Channels

Expose a `Channel` model built from current EDCB data. The flow first attempts
to read `ChSet5.txt` because it includes services that may not currently have
EPG events. It also reads `enum_service()` to fill remote control IDs when
available. If `ChSet5.txt` is unavailable, the flow falls back to
`enum_service()`.

Each channel includes:

- `id`: `NID{onid}-SID{sid}`
- `display_channel_id`: channel type lower-case plus calculated channel number
- `service_key`: `onid:tsid:sid`
- `network_id`, `transport_stream_id`, `service_id`
- `remocon_id`, `channel_number`, `channel_type`, `name`
- `is_subchannel`, `is_radiochannel`, `is_watchable`

The channel-number calculation is deterministic but stateless. It may differ
from KonomiTV in environments that rely on persisted branch numbers or
preferred terrestrial region settings.

## CLI And MCP

Add CLI commands:

- `edcb recording defaults`
- `edcb recording presets`
- `edcb channels`

Add MCP tools:

- `get_recording_defaults`
- `get_recording_presets`
- `list_channels`

JSON output returns the typed structs. Plain output is compact and intended for
inspection.

## Testing

Tests cover:

- `RecSettingData` to `RecordSettings` decoding.
- `EpgTimerSrv.ini` preset parsing from an EDCB file-copy response.
- channel construction from `ChSet5.txt` plus `enum_service()`.
- CLI parsing/execution and MCP tool exposure/parameter-free calls.
