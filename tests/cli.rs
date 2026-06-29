use std::time::Duration;

use edcb_tools::{
    EventKey, PluginKind, ProgramSearchQuery, RecordingMode, ServiceKey, ServiceRecordingMode,
    cli::{CliAction, CliCommand, CliInvocation, OutputMode, format_services_plain},
    types::ServiceInfo,
};

fn empty_env() -> std::iter::Empty<(&'static str, &'static str)> {
    std::iter::empty()
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
            host: "cli-host".to_string(),
            port: 4511,
            timeout: Duration::from_secs(2),
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
            host: "127.0.0.1".to_string(),
            port: 4510,
            timeout: Duration::from_secs(15),
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

    assert_eq!(action, CliAction::Help);
}

#[test]
fn invalid_usage_uses_exit_code_2() {
    let error = CliAction::from_args_and_env(["edcb", "recorded", "get", "not-an-id"], empty_env())
        .expect_err("invalid info id should fail");

    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("info-id"));
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
            host: "127.0.0.1".to_string(),
            port: 4510,
            timeout: Duration::from_secs(15),
            output: OutputMode::Json,
            command: CliCommand::ProgramsSearch(ProgramSearchQuery {
                keyword: "ニュース".to_string(),
                title_only: true,
                service: Some(ServiceKey {
                    onid: 32736,
                    tsid: 32736,
                    sid: 1024,
                }),
            }),
        })
    );
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
