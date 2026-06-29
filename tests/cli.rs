use std::time::Duration;

use chrono::DateTime;
use edcb_tools::{
    BroadcastType, ChannelType, ConnectionConfig, DuplicateTitleCheckScope, EventKey, PluginKind,
    ProgramGenreRange, ProgramSearchQuery, RecordingMode, SearchDateInfo, ServiceKey,
    ServiceRecordingMode, TimeTableQuery,
    cli::{CliAction, CliCommand, CliInvocation, OutputMode, format_services_plain},
    types::ServiceInfo,
};

fn empty_env() -> std::iter::Empty<(&'static str, &'static str)> {
    std::iter::empty()
}

fn expect_help(action: CliAction) -> String {
    match action {
        CliAction::Help(text) => text,
        other => panic!("expected help action, got {other:?}"),
    }
}

fn connection(host: &str, port: u16, timeout_seconds: u64) -> ConnectionConfig {
    ConnectionConfig::new(host, port).with_timeout(Duration::from_secs(timeout_seconds))
}

#[test]
fn parses_global_flags_env_and_recorded_get_command() {
    let action = CliAction::from_args_and_env(
        [
            "edcb",
            "--host",
            "cli-host",
            "--port",
            "4511",
            "--timeout-seconds",
            "2",
            "--json",
            "recorded",
            "get",
            "42",
        ],
        [
            ("EDCB_HOST", "env-host"),
            ("EDCB_PORT", "4510"),
            ("EDCB_TIMEOUT_SECONDS", "9"),
        ],
    )
    .expect("valid CLI invocation should parse");

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("cli-host", 4511, 2),
            output: OutputMode::Json,
            command: CliCommand::RecordedGet(42),
        })
    );
}

#[test]
fn parses_plain_plugin_command_without_cli_suffix() {
    let action =
        CliAction::from_args_and_env(["edcb", "plugins", "write", "--plain"], empty_env()).unwrap();

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("127.0.0.1", 4510, 15),
            output: OutputMode::Plain,
            command: CliCommand::Plugins(PluginKind::Write),
        })
    );
}

#[test]
fn help_ignores_other_arguments() {
    let action =
        CliAction::from_args_and_env(["edcb", "--help", "--host", "bad", "services"], empty_env())
            .unwrap();

    let help = expect_help(action);
    assert!(help.contains("Usage: edcb [OPTIONS] <COMMAND>"));
}

#[test]
fn parses_help_subcommand() {
    let action = CliAction::from_args_and_env(["edcb", "help"], empty_env()).unwrap();

    let help = expect_help(action);
    assert!(help.contains("Usage: edcb [OPTIONS] <COMMAND>"));
}

#[test]
fn parses_subcommand_help() {
    let action =
        CliAction::from_args_and_env(["edcb", "programs", "search", "--help"], empty_env())
            .unwrap();

    let help = expect_help(action);
    assert!(help.contains("Usage: edcb programs search [OPTIONS]"));
    assert!(help.contains("--keyword"));
}

#[test]
fn invalid_usage_uses_exit_code_2() {
    let error = CliAction::from_args_and_env(["edcb", "recorded", "get", "not-an-id"], empty_env())
        .expect_err("invalid info id should fail");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("info-id"));
}

#[test]
fn rejects_mutually_exclusive_output_modes() {
    let error =
        CliAction::from_args_and_env(["edcb", "--json", "--plain", "services"], empty_env())
            .expect_err("conflicting output modes should fail");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("--json"));
    assert!(error.message.contains("--plain"));
}

#[test]
fn parses_program_search_command() {
    let action = CliAction::from_args_and_env(
        [
            "edcb",
            "--json",
            "programs",
            "search",
            "--keyword",
            "ニュース",
            "--title-only",
            "--service",
            "32736:32736:1024",
        ],
        empty_env(),
    )
    .expect("program search command should parse");

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("127.0.0.1", 4510, 15),
            output: OutputMode::Json,
            command: CliCommand::ProgramsSearch(ProgramSearchQuery {
                keyword: "ニュース".to_string(),
                title_only: true,
                service_ranges: Some(vec![ServiceKey {
                    onid: 32736,
                    tsid: 32736,
                    sid: 1024,
                }]),
                ..ProgramSearchQuery::default()
            }),
        })
    );
}

#[test]
fn parses_extended_program_search_conditions() {
    let action = CliAction::from_args_and_env(
        [
            "edcb",
            "programs",
            "search",
            "--keyword",
            "ニュース",
            "--exclude-keyword",
            "スポーツ",
            "--title-only",
            "--case-sensitive",
            "--regex",
            "--fuzzy",
            "--service",
            "1:2:3",
            "--date-range",
            "1:19:00-1:23:00",
            "--exclude-date-ranges",
            "--duration-min",
            "30",
            "--duration-max",
            "120",
            "--free-ca",
            "free",
            "--genre",
            "0:1",
            "--genre",
            "14:0:4660",
            "--exclude-genre-ranges",
            "--search-disable",
            "--duplicate-title-check",
            "all-channels",
            "--duplicate-title-check-days",
            "6",
        ],
        empty_env(),
    )
    .expect("extended program search command should parse");

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("127.0.0.1", 4510, 15),
            output: OutputMode::Human,
            command: CliCommand::ProgramsSearch(ProgramSearchQuery {
                keyword: "ニュース".to_string(),
                exclude_keyword: "スポーツ".to_string(),
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
                exclude_date_ranges: true,
                duration_min: Some(30),
                duration_max: Some(120),
                broadcast_type: BroadcastType::FreeOnly,
                genre_ranges: vec![
                    ProgramGenreRange {
                        major: 0,
                        middle: 1,
                        user_nibble: None,
                    },
                    ProgramGenreRange {
                        major: 14,
                        middle: 0,
                        user_nibble: Some(4660),
                    },
                ],
                exclude_genre_ranges: true,
                is_enabled: false,
                duplicate_title_check_scope: DuplicateTitleCheckScope::AllChannels,
                duplicate_title_check_period_days: 6,
            }),
        })
    );
}

#[test]
fn parses_reservation_condition_commands() {
    let create = CliAction::from_args_and_env(
        [
            "edcb",
            "reservation-conditions",
            "create",
            "--keyword",
            "ニュース",
            "--genre",
            "0:1",
            "--priority",
            "4",
            "--yes",
        ],
        empty_env(),
    )
    .expect("reservation condition create command should parse");
    let update = CliAction::from_args_and_env(
        [
            "edcb",
            "reservation-conditions",
            "update",
            "77",
            "--keyword",
            "スポーツ",
            "--disable",
            "--yes",
        ],
        empty_env(),
    )
    .expect("reservation condition update command should parse");
    let delete = CliAction::from_args_and_env(
        ["edcb", "reservation-conditions", "delete", "77", "--yes"],
        empty_env(),
    )
    .expect("reservation condition delete command should parse");

    assert!(matches!(
        create,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReservationConditionCreate { query, options },
            ..
        }) if query.keyword == "ニュース"
            && query.genre_ranges == vec![ProgramGenreRange {
                major: 0,
                middle: 1,
                user_nibble: None,
            }]
            && options.priority == Some(4)
    ));
    assert!(matches!(
        update,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReservationConditionUpdate {
                condition_id: 77,
                query: Some(query),
                options,
            },
            ..
        }) if query.keyword == "スポーツ" && options.is_enabled == Some(false)
    ));
    assert!(matches!(
        delete,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReservationConditionDelete(77),
            ..
        })
    ));
}

#[test]
fn reservation_condition_mutations_require_confirmation() {
    let error = CliAction::from_args_and_env(
        [
            "edcb",
            "reservation-conditions",
            "create",
            "--keyword",
            "ニュース",
        ],
        empty_env(),
    )
    .expect_err("reservation condition creation should require --yes");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("--yes"));
}

#[test]
fn parses_program_timetable_command() {
    let action = CliAction::from_args_and_env(
        [
            "edcb",
            "--json",
            "programs",
            "timetable",
            "--service",
            "32736:32736:1024",
            "--start-time",
            "2026-06-29T19:00:00+09:00",
            "--end-time",
            "2026-06-29T23:00:00+09:00",
            "--channel-type",
            "gr",
        ],
        empty_env(),
    )
    .expect("program timetable command should parse");

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("127.0.0.1", 4510, 15),
            output: OutputMode::Json,
            command: CliCommand::ProgramsTimetable(TimeTableQuery {
                start_time: Some(
                    DateTime::parse_from_rfc3339("2026-06-29T19:00:00+09:00")
                        .expect("test start time should parse")
                ),
                end_time: Some(
                    DateTime::parse_from_rfc3339("2026-06-29T23:00:00+09:00")
                        .expect("test end time should parse")
                ),
                channel_type: Some(ChannelType::Gr),
                services: vec![ServiceKey {
                    onid: 32736,
                    tsid: 32736,
                    sid: 1024,
                }],
            }),
        })
    );
}

#[test]
fn parses_recording_defaults_and_presets_commands() {
    let defaults = CliAction::from_args_and_env(["edcb", "recording", "defaults"], empty_env())
        .expect("recording defaults command should parse");
    let presets =
        CliAction::from_args_and_env(["edcb", "--json", "recording", "presets"], empty_env())
            .expect("recording presets command should parse");

    assert!(matches!(
        defaults,
        CliAction::Run(CliInvocation {
            command: CliCommand::RecordingDefaults,
            ..
        })
    ));
    assert!(matches!(
        presets,
        CliAction::Run(CliInvocation {
            output: OutputMode::Json,
            command: CliCommand::RecordingPresets,
            ..
        })
    ));
}

#[test]
fn parses_channels_command() {
    let action = CliAction::from_args_and_env(["edcb", "--plain", "channels"], empty_env())
        .expect("channels command should parse");

    assert_eq!(
        action,
        CliAction::Run(CliInvocation {
            connection: connection("127.0.0.1", 4510, 15),
            output: OutputMode::Plain,
            command: CliCommand::Channels,
        })
    );
}

#[test]
fn rejects_invalid_program_search_date_range() {
    let error = CliAction::from_args_and_env(
        ["edcb", "programs", "search", "--date-range", "1:19-1:23"],
        empty_env(),
    )
    .expect_err("malformed date range should fail");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("date range"));
}

#[test]
fn parses_reservation_preview_and_create_commands() {
    let preview = CliAction::from_args_and_env(
        ["edcb", "reserves", "preview", "--event", "1:2:3:4"],
        empty_env(),
    )
    .expect("reservation preview command should parse");
    let create = CliAction::from_args_and_env(
        ["edcb", "reserves", "create", "--event", "1:2:3:4", "--yes"],
        empty_env(),
    )
    .expect("reservation create command should parse");
    let event = EventKey {
        service: ServiceKey {
            onid: 1,
            tsid: 2,
            sid: 3,
        },
        eid: 4,
    };

    assert!(matches!(
        preview,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReservePreview {
                event_key: parsed,
                ..
            },
            ..
        }) if parsed == event
    ));
    assert!(matches!(
        create,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReserveCreate {
                event_key: parsed,
                ..
            },
            ..
        }) if parsed == event
    ));
}

#[test]
fn parses_reservation_commands_with_recording_options() {
    let preview = CliAction::from_args_and_env(
        [
            "edcb",
            "reserves",
            "preview",
            "--event",
            "1:2:3:4",
            "--priority",
            "4",
            "--start-margin",
            "10",
            "--end-margin",
            "20",
            "--caption",
            "enable",
            "--data",
            "disable",
        ],
        empty_env(),
    )
    .expect("reservation preview options should parse");
    let update = CliAction::from_args_and_env(
        [
            "edcb",
            "reserves",
            "update",
            "42",
            "--disable",
            "--recording-mode",
            "specified-without-decoding",
            "--yes",
        ],
        empty_env(),
    )
    .expect("reservation update options should parse");

    assert!(matches!(
        preview,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReservePreview { options, .. },
            ..
        }) if options.priority == Some(4)
            && options.recording_start_margin == Some(10)
            && options.recording_end_margin == Some(20)
            && options.caption_recording_mode == Some(ServiceRecordingMode::Enable)
            && options.data_broadcasting_recording_mode == Some(ServiceRecordingMode::Disable)
    ));
    assert!(matches!(
        update,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReserveUpdate { reserve_id: 42, options },
            ..
        }) if options.is_enabled == Some(false)
            && options.recording_mode == Some(RecordingMode::SpecifiedServiceWithoutDecoding)
    ));
}

#[test]
fn parses_reservation_get_and_delete_commands() {
    let get = CliAction::from_args_and_env(["edcb", "reserves", "get", "42"], empty_env())
        .expect("reservation get command should parse");
    let delete =
        CliAction::from_args_and_env(["edcb", "reserves", "delete", "42", "--yes"], empty_env())
            .expect("reservation delete command should parse");

    assert!(matches!(
        get,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReserveGet(42),
            ..
        })
    ));
    assert!(matches!(
        delete,
        CliAction::Run(CliInvocation {
            command: CliCommand::ReserveDelete(42),
            ..
        })
    ));
}

#[test]
fn reserve_create_requires_confirmation() {
    let error = CliAction::from_args_and_env(
        ["edcb", "reserves", "create", "--event", "1:2:3:4"],
        empty_env(),
    )
    .expect_err("reservation creation should require --yes");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("--yes"));
}

#[test]
fn reserve_delete_requires_confirmation() {
    let error = CliAction::from_args_and_env(["edcb", "reserves", "delete", "42"], empty_env())
        .expect_err("reservation deletion should require --yes");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("--yes"));
}

#[test]
fn formats_services_as_stable_plain_lines() {
    let services = [ServiceInfo {
        onid: 32736,
        tsid: 32736,
        sid: 1024,
        service_type: 1,
        partial_reception_flag: 0,
        service_provider_name: String::new(),
        service_name: "NHK総合1・東京".to_string(),
        network_name: "関東広域0".to_string(),
        ts_name: "NHK総合・東京".to_string(),
        remote_control_key_id: 1,
    }];

    assert_eq!(
        format_services_plain(&services),
        "32736\t32736\t1024\t1\tNHK総合1・東京\n"
    );
}
