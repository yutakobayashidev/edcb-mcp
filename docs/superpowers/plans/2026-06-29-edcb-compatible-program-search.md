# EDCB-Compatible Program Search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework program search around EDCB/KonomiTV-compatible `SearchKeyInfo` semantics and expose those supported search conditions through the Rust flow, `edcb` CLI, and MCP server.

**Architecture:** Keep `EdcbClient` as the raw CtrlCmd layer and add `EdcbClient::search_pg()` for `CMD_EPG_SRV_SEARCH_PG`. Keep `flows::search_programs()` as the application layer: it converts a KonomiTV-style `ProgramSearchQuery` into `SearchKeyInfo` and calls `search_pg`. If no services are specified, the flow resolves default search services with `enum_service()` before calling `search_pg`. CLI and MCP expose conditions EDCB already supports.

**Tech Stack:** Rust 2024, Tokio, chrono, serde, schemars, rmcp, existing EDCB CtrlCmd codec and mock TCP fixtures.

## Global Constraints

- Keep `edcb` and `edcb-mcp` binary names unchanged.
- Keep Nix flake as the primary distribution surface.
- Prefer EDCB/KonomiTV search semantics over ad hoc client-side keyword filtering.
- Use `SearchKeyInfo` for keyword, exclusion keyword, title-only, service, recurring date ranges, duration range, fuzzy, regex, case-sensitive, and free/paid filters.
- Treat `date_ranges` as EDCB/KonomiTV recurring weekday/time-of-day ranges, not absolute datetimes.
- Do not add `--from` or `--to` in this change.
- Do not add genre/content filters in this first pass; `SearchKeyInfo.content_list` can be exposed later with a deliberate genre interface.
- Update README and `.agents/skills/cli/SKILL.md` when CLI usage changes.

---

### Task 1: Raw `search_pg` Client API

**Files:**
- Modify: `src/client.rs`
- Modify: `src/test_support.rs`
- Modify: `tests/reservation.rs`

**Interfaces:**
- Consumes: existing `SearchKeyInfo`
- Consumes: existing `write_search_key_info(writer: &mut Writer, value: &SearchKeyInfo)`
- Produces: `EdcbClient::search_pg(&self, keys: &[SearchKeyInfo]) -> Result<Vec<EventInfo>>`
- Produces test helper `encode_event_list_for_test(event: &EventInfo) -> Vec<u8>`

- [ ] **Step 1: Add event-list test fixture**

In `src/test_support.rs`, add this public helper near `encode_service_event_list_for_test`:

```rust
#[doc(hidden)]
pub fn encode_event_list_for_test(event: &EventInfo) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.write_vector(std::slice::from_ref(event), |writer, event| {
        write_event_info_for_test(writer, event)
    });
    writer.into_inner()
}
```

- [ ] **Step 2: Write failing raw client test**

Add imports in `tests/reservation.rs`:

```rust
use edcb_tools::types::{SearchDateInfo, SearchKeyInfo};
```

Add this test near the other program-search tests:

```rust
#[tokio::test]
async fn search_pg_sends_search_key_info_and_decodes_events() {
    let (_service, event) = service_event_fixture_for_test();
    let key = SearchKeyInfo {
        and_key: "Program".to_string(),
        not_key: "ignore".to_string(),
        title_only_flag: true,
        service_list: vec![ServiceKey {
            onid: event.onid,
            tsid: event.tsid,
            sid: event.sid,
        }
        .to_search_id()],
        date_list: vec![SearchDateInfo {
            start_day_of_week: 1,
            start_hour: 19,
            start_min: 0,
            end_day_of_week: 1,
            end_hour: 23,
            end_min: 0,
        }],
        chk_duration_min: 30,
        chk_duration_max: 120,
        ..SearchKeyInfo::default()
    };
    let (addr, server) =
        spawn_single_command_server(1025, encode_event_list_for_test(&event)).await;
    let mut client = EdcbClient::new(addr.ip().to_string(), addr.port());
    client.set_timeout(Duration::from_secs(1));

    let programs = client
        .search_pg(std::slice::from_ref(&key))
        .await
        .expect("SearchPg should return matching events");
    let payload = server
        .await
        .expect("mock EDCB server task should complete without panicking");

    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].eid, event.eid);
    let program_bytes = "Program"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    assert!(payload
        .windows(program_bytes.len())
        .any(|window| window == program_bytes));
}
```

- [ ] **Step 3: Verify RED**

Run:

```bash
cargo test --test reservation search_pg_sends_search_key_info_and_decodes_events
```

Expected: compile failure because `encode_event_list_for_test` and `EdcbClient::search_pg` do not exist.

- [ ] **Step 4: Implement `EdcbClient::search_pg`**

In `src/client.rs`, add this method near `enum_pg_info_ex`:

```rust
pub async fn search_pg(&self, keys: &[SearchKeyInfo]) -> Result<Vec<EventInfo>> {
    let body = self
        .send_cmd(CMD_EPG_SRV_SEARCH_PG, |writer| {
            writer.write_vector(keys, write_search_key_info)
        })
        .await?;
    let mut reader = Reader::new(&body);
    reader.read_vector(read_event_info)
}
```

- [ ] **Step 5: Verify GREEN**

Run:

```bash
cargo test --test reservation search_pg_sends_search_key_info_and_decodes_events
```

Expected: the test passes.

### Task 2: KonomiTV-Style Search Query Model

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/types.rs`
- Modify: `src/flows.rs`
- Modify: `tests/reservation.rs`

**Interfaces:**
- Produces: `BroadcastType`
- Produces: expanded `ProgramSearchQuery`
- Produces public re-exports for `BroadcastType`, `SearchDateInfo`, and `SearchKeyInfo`
- Produces: `program_search_query_to_search_key(query: &ProgramSearchQuery) -> Result<SearchKeyInfo>`
- Updates: `flows::search_programs(client, query)` to call `client.search_pg(&[search_key])`

- [ ] **Step 1: Expand search types**

In `src/types.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub enum BroadcastType {
    #[default]
    All,
    FreeOnly,
    PaidOnly,
}
```

Replace `ProgramSearchQuery` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ProgramSearchQuery {
    pub keyword: String,
    pub exclude_keyword: String,
    pub title_only: bool,
    pub case_sensitive: bool,
    pub regex: bool,
    pub fuzzy: bool,
    pub service_ranges: Option<Vec<ServiceKey>>,
    pub date_ranges: Vec<SearchDateInfo>,
    pub exclude_date_ranges: bool,
    pub duration_min: Option<u16>,
    pub duration_max: Option<u16>,
    pub broadcast_type: BroadcastType,
}
```

Update `src/lib.rs` exports:

```rust
pub use types::{
    BroadcastType, EventKey, PostRecordingMode, ProgramSearchQuery, RecordSettingsPatch,
    RecordingFolder, RecordingMode, SearchDateInfo, SearchKeyInfo, ServiceKey,
    ServiceRecordingMode,
};
```

- [ ] **Step 2: Write failing conversion tests**

Update existing `ProgramSearchQuery` literals in tests to use `..ProgramSearchQuery::default()` for newly added fields.

Add imports in `tests/reservation.rs`:

```rust
use edcb_tools::{BroadcastType, SearchDateInfo};
```

Add tests in `tests/reservation.rs`:

```rust
#[test]
fn program_search_query_builds_search_key_info() {
    let query = ProgramSearchQuery {
        keyword: "Program".to_string(),
        exclude_keyword: "ignore".to_string(),
        title_only: true,
        case_sensitive: true,
        regex: true,
        fuzzy: true,
        service_ranges: Some(vec![ServiceKey {
            onid: 1,
            tsid: 2,
            sid: 3,
        }]),
        date_ranges: vec![SearchDateInfo {
            start_day_of_week: 1,
            start_hour: 19,
            start_min: 0,
            end_day_of_week: 1,
            end_hour: 23,
            end_min: 0,
        }],
        duration_min: Some(30),
        duration_max: Some(120),
        broadcast_type: BroadcastType::FreeOnly,
        ..ProgramSearchQuery::default()
    };

    let key = program_search_query_to_search_key(&query)
        .expect("search query should convert to SearchKeyInfo");

    assert_eq!(key.and_key, "Program");
    assert_eq!(key.not_key, "ignore");
    assert!(key.title_only_flag);
    assert!(key.case_sensitive);
    assert!(key.reg_exp_flag);
    assert!(key.aimai_flag);
    assert_eq!(key.service_list, vec![0x0001_0002_0003]);
    assert_eq!(key.date_list.len(), 1);
    assert_eq!(key.chk_duration_min, 30);
    assert_eq!(key.chk_duration_max, 120);
    assert_eq!(key.free_ca_flag, 1);
}

#[test]
fn program_search_query_rejects_invalid_ranges() {
    let duration_error = program_search_query_to_search_key(&ProgramSearchQuery {
        duration_min: Some(120),
        duration_max: Some(30),
        ..ProgramSearchQuery::default()
    })
    .expect_err("reversed duration range should fail");
    assert!(duration_error.to_string().contains("duration_min"));
}
```

- [ ] **Step 3: Verify RED**

Run:

```bash
cargo test --test reservation program_search_query_builds_search_key_info
cargo test --test reservation program_search_query_rejects_invalid_ranges
```

Expected: compile failure because `BroadcastType` and `program_search_query_to_search_key` do not exist.

- [ ] **Step 4: Implement conversion and SearchPg-backed flow**

In `src/flows.rs`, add:

```rust
pub fn program_search_query_to_search_key(query: &ProgramSearchQuery) -> Result<SearchKeyInfo> {
    validate_program_search_query(query)?;
    Ok(SearchKeyInfo {
        and_key: query.keyword.clone(),
        not_key: query.exclude_keyword.clone(),
        case_sensitive: query.case_sensitive,
        reg_exp_flag: query.regex,
        title_only_flag: query.title_only,
        date_list: query.date_ranges.clone(),
        service_list: query
            .service_ranges
            .as_ref()
            .into_iter()
            .flatten()
            .map(|service| service.to_search_id())
            .collect(),
        aimai_flag: query.fuzzy,
        not_date_flag: query.exclude_date_ranges,
        free_ca_flag: match query.broadcast_type {
            BroadcastType::All => 0,
            BroadcastType::FreeOnly => 1,
            BroadcastType::PaidOnly => 2,
        },
        chk_duration_min: query.duration_min.unwrap_or_default(),
        chk_duration_max: query.duration_max.unwrap_or_default(),
        ..SearchKeyInfo::default()
    })
}

fn validate_program_search_query(query: &ProgramSearchQuery) -> Result<()> {
    if let (Some(min), Some(max)) = (query.duration_min, query.duration_max)
        && min > max
    {
        return Err(EdcbError::InvalidInput(
            "program search duration_min must be less than or equal to duration_max".to_string(),
        ));
    }
    Ok(())
}
```

Change `search_programs`:

```rust
pub async fn search_programs(
    client: &EdcbClient,
    query: &ProgramSearchQuery,
) -> Result<Vec<EventInfo>> {
    let key = if query.service_ranges.is_none() {
        let mut query = query.clone();
        query.service_ranges = Some(default_search_services(client).await?);
        program_search_query_to_search_key(&query)?
    } else {
        program_search_query_to_search_key(query)?
    };
    client.search_pg(&[key]).await
}
```

Do not keep keyword filtering in Rust. EDCB performs keyword and date/duration filtering through `SearchPg`.

- [ ] **Step 5: Update existing flow tests**

Update `search_programs_filters_enum_pg_info_ex_results` into a `SearchPg` test:

- use command ID `1025`
- use `encode_event_list_for_test(&event)`
- assert the payload contains the UTF-16LE search keyword
- remove `EnumPgInfoEx` service-time-list payload assertions

- [ ] **Step 6: Verify GREEN**

Run:

```bash
cargo test --test reservation program_search_query_builds_search_key_info
cargo test --test reservation program_search_query_rejects_invalid_ranges
cargo test --test reservation search_programs_filters_enum_pg_info_ex_results
```

Expected: tests pass and `search_programs` uses `CMD_EPG_SRV_SEARCH_PG`.

### Task 3: CLI Search Options

**Files:**
- Modify: `src/cli.rs`
- Modify: `tests/cli.rs`

**Interfaces:**
- Produces CLI flags:
  - `--exclude-keyword <text>`
  - `--case-sensitive`
  - `--regex`
  - `--fuzzy`
  - `--duration-min <minutes>`
  - `--duration-max <minutes>`
  - `--free-ca <all|free|paid>`
  - `--date-range <start-dow:HH:MM-end-dow:HH:MM>`
  - `--exclude-date-ranges`

- [ ] **Step 1: Write failing CLI tests**

Add tests that parse:

```bash
edcb programs search --keyword news --exclude-keyword sports --title-only --case-sensitive --regex --fuzzy --service 1:2:3 --duration-min 30 --duration-max 120 --free-ca free --date-range 1:19:00-1:23:00
```

Assert the resulting `ProgramSearchQuery` fields exactly match the flags. Also add one usage-error test for malformed `--date-range 1:19-1:23`.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --test cli parses_program_search_extended_conditions
cargo test --test cli rejects_invalid_program_search_date_range
```

Expected: failures because flags are not implemented.

- [ ] **Step 3: Implement CLI parsing**

Add parse helpers in `src/cli.rs`:

```rust
fn parse_broadcast_type(value: &str) -> Result<BroadcastType, CliError>
fn parse_search_date_range(value: &str) -> Result<SearchDateInfo, CliError>
```

Use compact CLI syntax for date ranges:

```text
<start-dow>:<HH>:<MM>-<end-dow>:<HH>:<MM>
```

Validate day of week `0..=6`, hour `0..=23`, minute `0..=59`.

- [ ] **Step 4: Update CLI help**

Update `programs search` help to include the new flags. Include one example for EDCB/KonomiTV-style recurring date range.

- [ ] **Step 5: Verify GREEN**

Run:

```bash
cargo test --test cli parses_program_search_extended_conditions
cargo test --test cli rejects_invalid_program_search_date_range
```

Expected: both pass.

### Task 4: MCP Search Options

**Files:**
- Modify: `src/mcp.rs`
- Modify: `tests/mcp_server.rs`

**Interfaces:**
- Extends `SearchProgramsParam` with JSON-friendly fields matching `ProgramSearchQuery`.
- Keeps `service` as `Option<String>` using `onid:tsid:sid`.
- Represents `date_ranges` as objects using KonomiTV-style names:
  - `start_day_of_week`
  - `start_hour`
  - `start_minute`
  - `end_day_of_week`
  - `end_hour`
  - `end_minute`

- [ ] **Step 1: Write failing MCP tests**

Add tests that construct `SearchProgramsParam` with extended fields and assert `try_into_query()` produces the expected `ProgramSearchQuery`. Include one invalid date range test.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test --test mcp_server search_programs_param_parses_extended_conditions
cargo test --test mcp_server search_programs_param_rejects_invalid_date_ranges
```

Expected: failures because fields and conversion are not implemented.

- [ ] **Step 3: Implement MCP parameter conversion**

Add:

```rust
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsDateRangeParam {
    pub start_day_of_week: u8,
    pub start_hour: u16,
    pub start_minute: u16,
    pub end_day_of_week: u8,
    pub end_hour: u16,
    pub end_minute: u16,
}
```

Extend `SearchProgramsParam` with the same search fields as CLI. Convert them into `ProgramSearchQuery`, validating ranges before returning.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
cargo test --test mcp_server search_programs_param_parses_extended_conditions
cargo test --test mcp_server search_programs_param_rejects_invalid_date_ranges
```

Expected: both pass.

### Task 5: Docs, Local Skill, And Full Verification

**Files:**
- Modify: `README.md`
- Modify: `.agents/skills/cli/SKILL.md`
- Modify: `docs/superpowers/plans/2026-06-29-edcb-compatible-program-search.md`

**Interfaces:**
- Documents `SearchPg`-backed program search.
- Documents recurring `--date-range` as EDCB/KonomiTV-style weekday/time-of-day filtering.

- [ ] **Step 1: Update README**

Document the expanded `programs search` command:

```text
programs search --keyword <text> [search options]
```

Add a "Program search options" list for all new flags. Explicitly state that `--date-range` is recurring weekday/time-of-day filtering compatible with EDCB/KonomiTV search conditions.

- [ ] **Step 2: Update `.agents/skills/cli/SKILL.md`**

Add examples for:

```bash
edcb programs search --keyword news --duration-min 30 --duration-max 120
edcb programs search --keyword news --date-range 1:19:00-1:23:00
edcb programs search --keyword news --free-ca free --fuzzy
```

- [ ] **Step 3: Verify**

Run:

```bash
git diff --check
env XDG_CACHE_HOME=/tmp/codex-cache nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (ps: [ ps.pyyaml ])' --command python3 /home/yuta/.config/codex/skills/.system/skill-creator/scripts/quick_validate.py .agents/skills/cli
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
env XDG_CACHE_HOME=/tmp/codex-cache nix fmt
env XDG_CACHE_HOME=/tmp/codex-cache nix build .#edcb-tools --no-link
env XDG_CACHE_HOME=/tmp/codex-cache nix run .#edcb -- --version
```

Expected: all commands exit 0, and the last command prints:

```text
edcb 0.1.0
```

- [ ] **Step 4: Commit**

Run:

```bash
git add src/client.rs src/test_support.rs src/lib.rs src/types.rs src/flows.rs src/cli.rs src/mcp.rs tests/reservation.rs tests/cli.rs tests/mcp_server.rs README.md .agents/skills/cli/SKILL.md docs/superpowers/plans/2026-06-29-edcb-compatible-program-search.md
git commit -m "feat: add edcb-backed program search filters"
```

Expected: commit succeeds.
