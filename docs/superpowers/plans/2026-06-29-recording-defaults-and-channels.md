# Recording Defaults And Channels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add DB-free recording defaults/presets and channel metadata surfaces.

**Architecture:** Keep `EdcbClient` as the raw CtrlCmd layer. Add flow functions
that decode current EDCB data into public typed structs, then expose them through
CLI and MCP thin adapters.

**Tech Stack:** Rust 2024, Tokio, serde/schemars, clap, rmcp, existing CtrlCmd
codec and mock TCP fixtures.

## Global Constraints

- Do not add a database or persistent cache.
- Do not add crates.io distribution assumptions.
- Keep `RecordSettingsPatch` as the write-side mutation input.
- Keep the first channel abstraction deterministic and stateless.
- Update README and `.agents/skills/cli/SKILL.md`.

---

### Task 1: Recording Defaults And Presets

**Files:**
- Modify: `src/types.rs`
- Modify: `src/flows.rs`
- Modify: `src/lib.rs`
- Modify: `tests/reservation.rs`

**Interfaces:**
- Produces: `RecordSettings`
- Produces: `RecordSettingsGlobalDefaults`
- Produces: `RecordSettingsPreset`
- Produces: `RecordSettingsPresets`
- Produces: `flows::record_settings_from_rec_setting(&RecSettingData) -> Result<RecordSettings>`
- Produces: `flows::get_recording_defaults(&EdcbClient) -> Result<RecordSettings>`
- Produces: `flows::get_recording_presets(&EdcbClient) -> Result<RecordSettingsPresets>`

- [x] Add failing tests for decoding `RecSettingData` into `RecordSettings`.
- [x] Add failing tests for fetching and parsing `EpgTimerSrv.ini`.
- [x] Implement public recording settings structs.
- [x] Implement INI parsing and preset decoding.
- [x] Run `cargo test --test reservation recording`.

### Task 2: Stateless Channels

**Files:**
- Modify: `src/types.rs`
- Modify: `src/flows.rs`
- Modify: `src/util.rs`
- Modify: `tests/reservation.rs`

**Interfaces:**
- Produces: `Channel`
- Produces: `flows::list_channels(&EdcbClient) -> Result<Vec<Channel>>`

- [x] Add failing tests for channel construction from `ChSet5.txt` and `enum_service()`.
- [x] Include `remocon_id` in `ChSet5Item`.
- [x] Implement channel type, ID, display ID, radio/subchannel/watchable flags.
- [x] Fall back to `enum_service()` if `ChSet5.txt` cannot be read.
- [x] Run `cargo test --test reservation channel`.

### Task 3: CLI And MCP

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/mcp.rs`
- Modify: `tests/cli.rs`
- Modify: `tests/mcp_server.rs`

**Interfaces:**
- Produces CLI: `recording defaults`, `recording presets`, `channels`
- Produces MCP tools: `get_recording_defaults`, `get_recording_presets`, `list_channels`

- [x] Add failing CLI parse tests.
- [x] Add failing MCP tool listing tests.
- [x] Implement clap commands and execution branches.
- [x] Implement MCP tools.
- [x] Run `cargo test --test cli recording channels` and `cargo test --test mcp_server`.

### Task 4: Docs And Verification

**Files:**
- Modify: `README.md`
- Modify: `.agents/skills/cli/SKILL.md`

- [x] Document new CLI and MCP surfaces.
- [x] Run `cargo fmt --check`.
- [x] Run `cargo test`.
- [x] Run `cargo clippy --all-targets -- -D warnings`.
- [x] Run `nix build`.
- [x] Run `git diff --check`.
