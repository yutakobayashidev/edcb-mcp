# Reservation Options Design

## Goal

Add KonomiTV-style reservation recording options without exposing EDCB raw
`RecSettingData` flags to CLI, MCP, or library users.

## Scope

This change covers one-shot event reservation preview/create and existing
reservation update. It does not cover keyword auto-add/manual-add settings,
recording presets, or bulk reservation editing.

## Public Model

Expose a `RecordSettingsPatch` model. Every field is optional, so callers can
change only the setting they care about while the flow keeps EDCB defaults or
the existing reservation values for the rest.

Supported fields:

- `is_enabled`
- `priority` from `1` to `5`
- `recording_mode`
- `recording_start_margin` and `recording_end_margin`
- `caption_recording_mode`
- `data_broadcasting_recording_mode`
- `post_recording_mode`
- `post_recording_bat_file_path`
- `recording_folders`
- `is_event_relay_follow_enabled`
- `is_exact_recording_enabled`
- `is_oneseg_separate_output_enabled`
- `is_sequential_recording_in_single_file_enabled`
- `forced_tuner_id`

`recording_mode`, caption/data modes, and post-recording mode are enums with
human-readable names based on KonomiTV's API. The raw EDCB meanings of
`rec_mode`, `service_mode`, `suspend_mode`, and `reboot_flag` stay internal.

## Flows

`preview_reservation_with_options(client, event_key, options)` builds the same
reservation preview as today, then applies the patch to `rec_setting`.

`create_reservation_with_options(client, event_key, options)` previews with
options, sends `AddReserve`, and returns the sent reservation data.

`update_reservation(client, reserve_id, options)` fetches the current
reservation, applies the patch to the full `ReserveData`, sends `ChgReserve`,
then fetches and returns the updated reservation. This follows KonomiTV's rule:
send the full reservation object, not a partial raw structure.

## CLI

Keep the CLI compact. Add common flags to `reserves preview`, `reserves create`,
and `reserves update`:

- `--priority <1-5>`
- `--enable`
- `--disable`
- `--recording-mode <all|all-without-decoding|specified|specified-without-decoding|view>`
- `--start-margin <seconds>`
- `--end-margin <seconds>`
- `--caption <default|enable|disable>`
- `--data <default|enable|disable>`
- `--post-recording <default|nothing|standby|standby-and-reboot|suspend|suspend-and-reboot|shutdown>`

`reserves update <reserve-id> ... --yes` is the new mutation command.

Folder lists and less common booleans are first-class in the library and MCP
JSON model. They are intentionally omitted from CLI flags for now to avoid a
large command surface.

## MCP

Use the same conceptual model:

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

`update_reservation` accepts:

```json
{
  "reserve_id": 518,
  "options": {
    "is_enabled": false
  }
}
```

## Validation

- `priority` must be in `1..=5`.
- If one margin is set, both start and end margins must be set.
- Caption and data modes must both be `Default`, or both be explicit
  `Enable`/`Disable` values.
- `recording_folders` use `Write_Default.dll` and `RecName_Macro.dll` in the
  generated EDCB structure, matching KonomiTV's practical default.
- `update_reservation` rejects an empty patch so accidental no-op mutation calls
  are visible to the caller.

## Tests

Unit/integration tests should cover:

- `RecordSettingsPatch` to raw `RecSettingData` mapping.
- Validation failures for invalid priority, one-sided margins, and mixed
  caption/data defaults.
- `create_reservation_with_options` applying options before `AddReserve`.
- `update_reservation` sending full `ReserveData` through `ChgReserve2`.
- CLI parsing for create/preview/update flags.
- MCP tool list and request parameter schema surface.
