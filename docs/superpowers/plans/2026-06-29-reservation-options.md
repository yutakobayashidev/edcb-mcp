# Reservation Options Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add human-friendly reservation recording options for reservation preview/create/update without exposing EDCB raw flags.

**Architecture:** Add common option types in `types.rs`, apply them in `flows.rs`, and keep `EdcbClient` as the raw CtrlCmd layer. CLI parses a compact set of common flags, while MCP exposes the full JSON-friendly patch model.

**Tech Stack:** Rust 2024, Tokio, serde, schemars, rmcp, existing EDCB CtrlCmd codec.

## Global Constraints

- Prefer `RecordSettingsPatch` over a full required settings object.
- Keep raw `RecSettingData` fields internal to conversion code.
- Require `--yes` for CLI update/create mutations.
- Do not add keyword auto-add/manual-add option editing in this change.
- Do not run destructive real EDCB update commands during smoke tests unless explicitly requested.

---

### Task 1: Common Option Types And Patch Logic

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/types.rs`
- Modify: `src/flows.rs`
- Test: `tests/reservation.rs`

**Interfaces:**
- Produces: `RecordSettingsPatch`
- Produces: `RecordingFolder`
- Produces: `RecordingMode`
- Produces: `ServiceRecordingMode`
- Produces: `PostRecordingMode`
- Produces: `apply_record_settings_patch(settings: &mut RecSettingData, patch: &RecordSettingsPatch) -> Result<()>`

- [x] **Step 1: Write failing tests**

Add tests in `tests/reservation.rs` that call `apply_record_settings_patch` with priority, disabled specified-service mode, margins, caption/data modes, and a recording folder.

- [x] **Step 2: Verify RED**

Run:

```bash
cargo test --test reservation applies_record_settings_patch_to_edcb_rec_setting
```

Expected: compile failure because the option types and patch function do not exist.

- [x] **Step 3: Implement common types and patch function**

Add the public patch model and enum conversion logic. Validation failures return `EdcbError::InvalidInput`.

- [x] **Step 4: Verify GREEN**

Run:

```bash
cargo test --test reservation applies_record_settings_patch_to_edcb_rec_setting
cargo test --test reservation rejects_invalid_record_settings_patch_values
```

Expected: both pass.

### Task 2: Reservation Create/Update Flows

**Files:**
- Modify: `src/client.rs`
- Modify: `src/flows.rs`
- Test: `tests/reservation.rs`

**Interfaces:**
- Produces: `EdcbClient::change_reserve(&self, reserve: &ReserveData) -> Result<()>`
- Produces: `EdcbClient::change_reserves(&self, reserves: &[ReserveData]) -> Result<()>`
- Produces: `preview_reservation_with_options(client, event_key, options) -> Result<ReserveData>`
- Produces: `create_reservation_with_options(client, event_key, options) -> Result<ReserveData>`
- Produces: `update_reservation(client, reserve_id, options) -> Result<ReserveData>`

- [x] **Step 1: Write failing tests**

Add tests proving create applies options before `AddReserve`, and update performs `GetReserve2`, `ChgReserve2`, then `GetReserve2`.

- [x] **Step 2: Verify RED**

Run:

```bash
cargo test --test reservation create_reservation_with_options_applies_recording_options
cargo test --test reservation update_reservation_changes_existing_record_settings
```

Expected: compile failure because the flow and client methods do not exist.

- [x] **Step 3: Implement flows and raw change reserve command**

Use `CMD_EPG_SRV_CHG_RESERVE2` with the versioned reserve vector writer.

- [x] **Step 4: Verify GREEN**

Run the two focused tests again. Expected: both pass.

### Task 3: CLI And MCP Surface

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/mcp.rs`
- Test: `tests/cli.rs`
- Test: `tests/mcp_server.rs`

**Interfaces:**
- Updates: `reserves preview --event <key> [options]`
- Updates: `reserves create --event <key> [options] --yes`
- Produces: `reserves update <reserve-id> [options] --yes`
- Updates MCP `preview_reservation` and `create_reservation` with `options`
- Produces MCP `update_reservation`

- [x] **Step 1: Write failing tests**

Add CLI parsing tests for create/preview/update option flags and add `update_reservation` to MCP tool-list expectations.

- [x] **Step 2: Verify RED**

Run:

```bash
cargo test --test cli parses_reservation_commands_with_recording_options
cargo test --test mcp_server mcp_server_exposes_v1_tools
```

Expected: failures because the command variants and tool do not exist.

- [x] **Step 3: Implement CLI/MCP parsing and dispatch**

Keep CLI flags compact and use common `RecordSettingsPatch` values. MCP uses the same patch type for JSON input and schema generation.

- [x] **Step 4: Verify GREEN**

Run the focused CLI and MCP tests again. Expected: both pass.

### Task 4: Docs And Full Verification

**Files:**
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-06-29-reservation-options.md`

**Interfaces:**
- Documents new CLI flags.
- Documents MCP option payload.
- Marks the implementation plan complete.

- [x] **Step 1: Update docs**

Update README supported features, command list, MCP tool list, and examples.

- [x] **Step 2: Run full verification**

Run:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Expected: all commands exit 0.
