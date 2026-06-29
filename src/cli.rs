use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::Serialize;

use crate::{
    EdcbClient, EventKey, PluginKind, ProgramSearchQuery, ServiceKey, flows,
    types::{
        EventInfo, NotifySrvInfo, RecFileInfo, ReserveData, ServiceInfo, TunerProcessStatusInfo,
        TunerReserveInfo,
    },
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Services,
    Reserves,
    RecordedList,
    RecordedGet(i32),
    ProgramsSearch(ProgramSearchQuery),
    ReservePreview(EventKey),
    ReserveCreate(EventKey),
    TunerReserves,
    TunerProcesses,
    Plugins(PluginKind),
    NotifyStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInvocation {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
    pub output: OutputMode,
    pub command: CliCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run(CliInvocation),
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub exit_code: i32,
    pub message: String,
}

impl CliError {
    fn invalid_usage(message: impl Into<String>) -> Self {
        Self {
            exit_code: 2,
            message: message.into(),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            message: message.into(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

impl CliAction {
    pub fn from_env_args() -> Result<Self, CliError> {
        from_args_and_env_pairs(std::env::args(), std::env::vars())
    }

    pub fn from_args_and_env<A, S, E, K, V>(args: A, env: E) -> Result<Self, CliError>
    where
        A: IntoIterator<Item = S>,
        S: AsRef<str>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        from_args_and_env_pairs(args, env)
    }
}

fn from_args_and_env_pairs<A, S, E, K, V>(args: A, env: E) -> Result<CliAction, CliError>
where
    A: IntoIterator<Item = S>,
    S: AsRef<str>,
    E: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let env: BTreeMap<String, String> = env
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_string(), value.as_ref().to_string()))
        .collect();

    let mut invocation = CliInvocation {
        host: env
            .get("EDCB_HOST")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string()),
        port: env
            .get("EDCB_PORT")
            .map(|value| parse_port(value))
            .transpose()?
            .unwrap_or(4510),
        timeout: Duration::from_secs(
            env.get("EDCB_TIMEOUT_SECONDS")
                .map(|value| parse_timeout(value))
                .transpose()?
                .unwrap_or(15),
        ),
        output: OutputMode::Human,
        command: CliCommand::Services,
    };

    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let mut positionals = Vec::new();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" => return Ok(CliAction::Help),
            "--version" => return Ok(CliAction::Version),
            "--host" => {
                index += 1;
                invocation.host = args
                    .get(index)
                    .ok_or_else(|| CliError::invalid_usage("--host requires a value"))?
                    .clone();
            }
            "--port" => {
                index += 1;
                invocation.port = parse_port(
                    args.get(index)
                        .ok_or_else(|| CliError::invalid_usage("--port requires a value"))?,
                )?;
            }
            "--timeout-seconds" => {
                index += 1;
                invocation.timeout =
                    Duration::from_secs(parse_timeout(args.get(index).ok_or_else(|| {
                        CliError::invalid_usage("--timeout-seconds requires a value")
                    })?)?);
            }
            "--json" => invocation.output = OutputMode::Json,
            "--plain" => invocation.output = OutputMode::Plain,
            "--keyword" | "--service" | "--event" => {
                let key = args[index].clone();
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| CliError::invalid_usage(format!("{key} requires a value")))?;
                positionals.push(key);
                positionals.push(value.clone());
            }
            "--title-only" | "--yes" => positionals.push(args[index].clone()),
            value if value.starts_with('-') => {
                return Err(CliError::invalid_usage(format!("unknown argument {value}")));
            }
            value => positionals.push(value.to_string()),
        }
        index += 1;
    }

    invocation.command = parse_command(&positionals)?;
    Ok(CliAction::Run(invocation))
}

fn parse_command(positionals: &[String]) -> Result<CliCommand, CliError> {
    match positionals {
        [] => Err(CliError::invalid_usage("missing command")),
        [command] if command == "services" => Ok(CliCommand::Services),
        [command] if command == "reserves" => Ok(CliCommand::Reserves),
        [command] if command == "tuner-reserves" => Ok(CliCommand::TunerReserves),
        [command] if command == "tuner-processes" => Ok(CliCommand::TunerProcesses),
        [command] if command == "notify-status" => Ok(CliCommand::NotifyStatus),
        [command, subcommand] if command == "recorded" && subcommand == "list" => {
            Ok(CliCommand::RecordedList)
        }
        [command, subcommand, info_id] if command == "recorded" && subcommand == "get" => {
            Ok(CliCommand::RecordedGet(info_id.parse().map_err(|_| {
                CliError::invalid_usage(format!("info-id must be an integer: {info_id}"))
            })?))
        }
        [command, subcommand, rest @ ..] if command == "programs" && subcommand == "search" => {
            Ok(CliCommand::ProgramsSearch(parse_program_search(rest)?))
        }
        [command, subcommand, rest @ ..] if command == "reserves" && subcommand == "preview" => {
            Ok(CliCommand::ReservePreview(parse_event_arg(rest)?))
        }
        [command, subcommand, rest @ ..] if command == "reserves" && subcommand == "create" => {
            let event_key = parse_event_arg(rest)?;
            if !rest.iter().any(|value| value == "--yes") {
                return Err(CliError::invalid_usage(
                    "reserves create requires --yes to confirm mutation",
                ));
            }
            Ok(CliCommand::ReserveCreate(event_key))
        }
        [command, kind] if command == "plugins" => {
            Ok(CliCommand::Plugins(parse_plugin_kind(kind)?))
        }
        [command, ..] => Err(CliError::invalid_usage(format!(
            "unknown or incomplete command: {command}"
        ))),
    }
}

fn parse_program_search(args: &[String]) -> Result<ProgramSearchQuery, CliError> {
    let mut keyword = None;
    let mut title_only = false;
    let mut service = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--keyword" => {
                index += 1;
                keyword = Some(
                    args.get(index)
                        .ok_or_else(|| CliError::invalid_usage("--keyword requires a value"))?
                        .clone(),
                );
            }
            "--title-only" => title_only = true,
            "--service" => {
                index += 1;
                service = Some(parse_service_key(args.get(index).ok_or_else(|| {
                    CliError::invalid_usage("--service requires onid:tsid:sid")
                })?)?);
            }
            value => {
                return Err(CliError::invalid_usage(format!(
                    "unknown programs search argument {value}"
                )));
            }
        }
        index += 1;
    }
    let keyword = keyword
        .ok_or_else(|| CliError::invalid_usage("programs search requires --keyword <text>"))?;
    Ok(ProgramSearchQuery {
        keyword,
        title_only,
        service,
    })
}

fn parse_event_arg(args: &[String]) -> Result<EventKey, CliError> {
    let mut event = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--event" => {
                index += 1;
                event = Some(parse_event_key(args.get(index).ok_or_else(|| {
                    CliError::invalid_usage("--event requires onid:tsid:sid:eid")
                })?)?);
            }
            "--yes" => {}
            value => {
                return Err(CliError::invalid_usage(format!(
                    "unknown reservation argument {value}"
                )));
            }
        }
        index += 1;
    }
    event.ok_or_else(|| CliError::invalid_usage("reservation command requires --event"))
}

fn parse_service_key(value: &str) -> Result<ServiceKey, CliError> {
    value.parse().map_err(CliError::invalid_usage)
}

fn parse_event_key(value: &str) -> Result<EventKey, CliError> {
    value.parse().map_err(CliError::invalid_usage)
}

fn parse_plugin_kind(value: &str) -> Result<PluginKind, CliError> {
    match value {
        "write" => Ok(PluginKind::Write),
        "rec_name" => Ok(PluginKind::RecName),
        _ => Err(CliError::invalid_usage(format!(
            "plugin kind must be write or rec_name: {value}"
        ))),
    }
}

fn parse_port(value: &str) -> Result<u16, CliError> {
    value.parse().map_err(|_| {
        CliError::invalid_usage(format!("port must be a number in 0..=65535: {value}"))
    })
}

fn parse_timeout(value: &str) -> Result<u64, CliError> {
    let timeout = value.parse::<u64>().map_err(|_| {
        CliError::invalid_usage(format!("timeout must be a positive integer: {value}"))
    })?;
    if timeout == 0 {
        Err(CliError::invalid_usage("timeout must be greater than zero"))
    } else {
        Ok(timeout)
    }
}

pub async fn execute(invocation: CliInvocation) -> Result<String, CliError> {
    let mut client = EdcbClient::new(invocation.host.clone(), invocation.port);
    client.set_timeout(invocation.timeout);

    match invocation.command {
        CliCommand::Services => {
            let value = client.enum_service().await.map_err(runtime_error)?;
            render(&invocation.output, &value, || format_services_plain(&value))
        }
        CliCommand::Reserves => {
            let value = client.enum_reserve().await.map_err(runtime_error)?;
            render(&invocation.output, &value, || format_reserves_plain(&value))
        }
        CliCommand::RecordedList => {
            let value = client.enum_rec_info_basic().await.map_err(runtime_error)?;
            render(&invocation.output, &value, || format_recorded_plain(&value))
        }
        CliCommand::RecordedGet(info_id) => {
            let value = client.get_rec_info(info_id).await.map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_recorded_info_plain(&value)
            })
        }
        CliCommand::ProgramsSearch(query) => {
            let value = flows::search_programs(&client, &query)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || format_programs_plain(&value))
        }
        CliCommand::ReservePreview(event_key) => {
            let value = flows::preview_reservation(&client, event_key)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::ReserveCreate(event_key) => {
            let value = flows::create_reservation(&client, event_key)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::TunerReserves => {
            let value = client.enum_tuner_reserve().await.map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_tuner_reserves_plain(&value)
            })
        }
        CliCommand::TunerProcesses => {
            let value = client.enum_tuner_process().await.map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_tuner_processes_plain(&value)
            })
        }
        CliCommand::Plugins(kind) => {
            let value = client.enum_plugin(kind).await.map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_string_list_plain(&value)
            })
        }
        CliCommand::NotifyStatus => {
            let value = client
                .get_notify_srv_status()
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_notify_status_plain(&value)
            })
        }
    }
}

fn format_programs_plain(programs: &[EventInfo]) -> String {
    programs
        .iter()
        .map(|event| {
            let title = event
                .short_info
                .as_ref()
                .map(|info| info.event_name.as_str())
                .unwrap_or("");
            let start = event
                .start_time
                .map(|time| time.to_rfc3339())
                .unwrap_or_else(|| "-".to_string());
            format!(
                "{}:{}:{}:{}\t{}\t{}\n",
                event.onid, event.tsid, event.sid, event.eid, start, title
            )
        })
        .collect()
}

fn format_reservation_plain(reserve: &ReserveData) -> String {
    format!(
        "{}:{}:{}:{}\t{}\t{}\t{}\n",
        reserve.onid,
        reserve.tsid,
        reserve.sid,
        reserve.eid,
        reserve.start_time.to_rfc3339(),
        reserve.station_name,
        reserve.title
    )
}

fn runtime_error(error: crate::EdcbError) -> CliError {
    CliError::runtime(error.to_string())
}

fn render<T: Serialize>(
    output: &OutputMode,
    value: &T,
    plain: impl FnOnce() -> String,
) -> Result<String, CliError> {
    match output {
        OutputMode::Json => serde_json::to_string_pretty(value)
            .map(|json| format!("{json}\n"))
            .map_err(|error| CliError::runtime(format!("failed to serialize output: {error}"))),
        OutputMode::Human | OutputMode::Plain => Ok(plain()),
    }
}

pub fn format_services_plain(services: &[ServiceInfo]) -> String {
    services
        .iter()
        .map(|service| {
            format!(
                "{}\t{}\t{}\t{}\t{}\n",
                service.onid,
                service.tsid,
                service.sid,
                service.remote_control_key_id,
                service.service_name
            )
        })
        .collect()
}

fn format_reserves_plain(reserves: &[ReserveData]) -> String {
    reserves
        .iter()
        .map(|reserve| {
            format!(
                "{}\t{}\t{}\t{}\n",
                reserve.reserve_id,
                reserve.start_time.to_rfc3339(),
                reserve.station_name,
                reserve.title
            )
        })
        .collect()
}

fn format_recorded_plain(items: &[RecFileInfo]) -> String {
    items
        .iter()
        .map(|item| {
            format!(
                "{}\t{}\t{}\t{}\n",
                item.id,
                item.start_time.to_rfc3339(),
                item.service_name,
                item.title
            )
        })
        .collect()
}

fn format_recorded_info_plain(item: &RecFileInfo) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\n",
        item.id,
        item.start_time.to_rfc3339(),
        item.service_name,
        item.title,
        item.rec_file_path
    )
}

fn format_tuner_reserves_plain(items: &[TunerReserveInfo]) -> String {
    items
        .iter()
        .map(|item| {
            format!(
                "{}\t{}\t{}\n",
                item.tuner_id,
                item.tuner_name,
                item.reserve_list.len()
            )
        })
        .collect()
}

fn format_tuner_processes_plain(items: &[TunerProcessStatusInfo]) -> String {
    items
        .iter()
        .map(|item| {
            format!(
                "{}\t{}\t{}\t{}\n",
                item.tuner_id, item.process_id, item.drop, item.signal_lv
            )
        })
        .collect()
}

fn format_string_list_plain(items: &[String]) -> String {
    items.iter().map(|item| format!("{item}\n")).collect()
}

fn format_notify_status_plain(value: &NotifySrvInfo) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\n",
        value.notify_id,
        value.time.to_rfc3339(),
        value.param1,
        value.param2,
        value.count
    )
}

pub fn help_text() -> &'static str {
    "EDCB CtrlCmd command line interface

USAGE:
  edcb [global flags] <command>
  edcb [global flags] recorded get <info-id>
  edcb [global flags] programs search --keyword <text>
  edcb [global flags] reserves create --event <onid:tsid:sid:eid> --yes
  edcb [global flags] plugins <write|rec_name>

COMMANDS:
  services
  reserves
  recorded list
  recorded get <info-id>
  programs search --keyword <text> [--title-only] [--service <onid:tsid:sid>]
  reserves preview --event <onid:tsid:sid:eid>
  reserves create --event <onid:tsid:sid:eid> --yes
  tuner-reserves
  tuner-processes
  plugins <write|rec_name>
  notify-status

GLOBAL FLAGS:
  -h, --help                 Show this help
      --version              Show version
      --host <host>          EDCB host (env: EDCB_HOST, default: 127.0.0.1)
      --port <port>          EDCB CtrlCmd port (env: EDCB_PORT, default: 4510)
      --timeout-seconds <n>  Request timeout (env: EDCB_TIMEOUT_SECONDS, default: 15)
      --json                 Print pretty JSON
      --plain                Print stable line-based output

EXAMPLES:
  edcb services
  edcb --json services
  edcb reserves --plain
  edcb recorded list
  edcb recorded get 1 --json
  edcb programs search --keyword ニュース --title-only
  edcb reserves preview --event 32736:32736:1024:4208
  edcb reserves create --event 32736:32736:1024:4208 --yes
  edcb plugins write
  edcb --host 172.18.0.7 notify-status
"
}

pub fn version_text() -> String {
    format!("edcb {VERSION}\n")
}
