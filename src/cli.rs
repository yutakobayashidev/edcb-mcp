use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::Serialize;

use crate::{
    BroadcastType, EdcbClient, EventKey, PluginKind, PostRecordingMode, ProgramSearchQuery,
    RecordSettingsPatch, RecordingMode, SearchDateInfo, ServiceKey, ServiceRecordingMode, flows,
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
    ReserveGet(i32),
    ReservePreview {
        event_key: EventKey,
        options: RecordSettingsPatch,
    },
    ReserveCreate {
        event_key: EventKey,
        options: RecordSettingsPatch,
    },
    ReserveUpdate {
        reserve_id: i32,
        options: RecordSettingsPatch,
    },
    ReserveDelete(i32),
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
            "--keyword" | "--exclude-keyword" | "--service" | "--date-range" | "--duration-min"
            | "--duration-max" | "--free-ca" | "--event" | "--priority" | "--recording-mode"
            | "--start-margin" | "--end-margin" | "--caption" | "--data" | "--post-recording" => {
                let key = args[index].clone();
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| CliError::invalid_usage(format!("{key} requires a value")))?;
                positionals.push(key);
                positionals.push(value.clone());
            }
            "--title-only"
            | "--case-sensitive"
            | "--regex"
            | "--fuzzy"
            | "--exclude-date-ranges"
            | "--yes"
            | "--enable"
            | "--disable" => positionals.push(args[index].clone()),
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
            let (event_key, options) = parse_event_and_options(rest)?;
            Ok(CliCommand::ReservePreview { event_key, options })
        }
        [command, subcommand, reserve_id] if command == "reserves" && subcommand == "get" => {
            Ok(CliCommand::ReserveGet(parse_reserve_id(reserve_id)?))
        }
        [command, subcommand, rest @ ..] if command == "reserves" && subcommand == "create" => {
            let (event_key, options) = parse_event_and_options(rest)?;
            if !rest.iter().any(|value| value == "--yes") {
                return Err(CliError::invalid_usage(
                    "reserves create requires --yes to confirm mutation",
                ));
            }
            Ok(CliCommand::ReserveCreate { event_key, options })
        }
        [command, subcommand, reserve_id, rest @ ..]
            if command == "reserves" && subcommand == "update" =>
        {
            let reserve_id = parse_reserve_id(reserve_id)?;
            let options = parse_record_settings_options(rest)?;
            if !rest.iter().any(|value| value == "--yes") {
                return Err(CliError::invalid_usage(
                    "reserves update requires --yes to confirm mutation",
                ));
            }
            Ok(CliCommand::ReserveUpdate {
                reserve_id,
                options,
            })
        }
        [command, subcommand, reserve_id, rest @ ..]
            if command == "reserves" && subcommand == "delete" =>
        {
            let reserve_id = parse_reserve_id(reserve_id)?;
            if let Some(value) = rest.iter().find(|value| value.as_str() != "--yes") {
                return Err(CliError::invalid_usage(format!(
                    "unknown reservation argument {value}"
                )));
            }
            if !rest.iter().any(|value| value == "--yes") {
                return Err(CliError::invalid_usage(
                    "reserves delete requires --yes to confirm mutation",
                ));
            }
            Ok(CliCommand::ReserveDelete(reserve_id))
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
    let mut query = ProgramSearchQuery::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--keyword" => {
                index += 1;
                query.keyword = args
                    .get(index)
                    .ok_or_else(|| CliError::invalid_usage("--keyword requires a value"))?
                    .clone();
            }
            "--exclude-keyword" => {
                index += 1;
                query.exclude_keyword = args
                    .get(index)
                    .ok_or_else(|| CliError::invalid_usage("--exclude-keyword requires a value"))?
                    .clone();
            }
            "--title-only" => query.title_only = true,
            "--case-sensitive" => query.case_sensitive = true,
            "--regex" => query.regex = true,
            "--fuzzy" => query.fuzzy = true,
            "--service" => {
                index += 1;
                query
                    .service_ranges
                    .get_or_insert_with(Vec::new)
                    .push(parse_service_key(args.get(index).ok_or_else(|| {
                        CliError::invalid_usage("--service requires onid:tsid:sid")
                    })?)?);
            }
            "--date-range" => {
                index += 1;
                query
                    .date_ranges
                    .push(parse_search_date_range(args.get(index).ok_or_else(
                        || {
                            CliError::invalid_usage(
                                "--date-range requires start-dow:HH:MM-end-dow:HH:MM",
                            )
                        },
                    )?)?);
            }
            "--exclude-date-ranges" => query.exclude_date_ranges = true,
            "--duration-min" => {
                index += 1;
                query.duration_min = Some(parse_u16_arg(args.get(index), "--duration-min")?);
            }
            "--duration-max" => {
                index += 1;
                query.duration_max = Some(parse_u16_arg(args.get(index), "--duration-max")?);
            }
            "--free-ca" => {
                index += 1;
                query.broadcast_type = parse_broadcast_type_arg(args.get(index), "--free-ca")?;
            }
            value => {
                return Err(CliError::invalid_usage(format!(
                    "unknown programs search argument {value}"
                )));
            }
        }
        index += 1;
    }
    Ok(query)
}

fn parse_event_and_options(args: &[String]) -> Result<(EventKey, RecordSettingsPatch), CliError> {
    let mut event = None;
    let mut options = RecordSettingsPatch::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--event" => {
                index += 1;
                event = Some(parse_event_key(args.get(index).ok_or_else(|| {
                    CliError::invalid_usage("--event requires onid:tsid:sid:eid")
                })?)?);
            }
            _ => parse_record_settings_option(args, &mut index, &mut options)?,
        }
        index += 1;
    }
    Ok((
        event.ok_or_else(|| CliError::invalid_usage("reservation command requires --event"))?,
        options,
    ))
}

fn parse_record_settings_options(args: &[String]) -> Result<RecordSettingsPatch, CliError> {
    let mut options = RecordSettingsPatch::default();
    let mut index = 0;
    while index < args.len() {
        parse_record_settings_option(args, &mut index, &mut options)?;
        index += 1;
    }
    Ok(options)
}

fn parse_record_settings_option(
    args: &[String],
    index: &mut usize,
    options: &mut RecordSettingsPatch,
) -> Result<(), CliError> {
    match args[*index].as_str() {
        "--yes" => {}
        "--priority" => {
            *index += 1;
            options.priority = Some(parse_u8_arg(args.get(*index), "--priority")?);
        }
        "--enable" => options.is_enabled = Some(true),
        "--disable" => options.is_enabled = Some(false),
        "--recording-mode" => {
            *index += 1;
            options.recording_mode = Some(parse_recording_mode_arg(
                args.get(*index),
                "--recording-mode",
            )?);
        }
        "--start-margin" => {
            *index += 1;
            options.recording_start_margin =
                Some(parse_i32_arg(args.get(*index), "--start-margin")?);
        }
        "--end-margin" => {
            *index += 1;
            options.recording_end_margin = Some(parse_i32_arg(args.get(*index), "--end-margin")?);
        }
        "--caption" => {
            *index += 1;
            options.caption_recording_mode = Some(parse_service_recording_mode_arg(
                args.get(*index),
                "--caption",
            )?);
        }
        "--data" => {
            *index += 1;
            options.data_broadcasting_recording_mode = Some(parse_service_recording_mode_arg(
                args.get(*index),
                "--data",
            )?);
        }
        "--post-recording" => {
            *index += 1;
            options.post_recording_mode = Some(parse_post_recording_mode_arg(
                args.get(*index),
                "--post-recording",
            )?);
        }
        value => {
            return Err(CliError::invalid_usage(format!(
                "unknown reservation argument {value}"
            )));
        }
    }
    Ok(())
}

fn parse_service_key(value: &str) -> Result<ServiceKey, CliError> {
    value.parse().map_err(CliError::invalid_usage)
}

fn parse_event_key(value: &str) -> Result<EventKey, CliError> {
    value.parse().map_err(CliError::invalid_usage)
}

fn parse_reserve_id(value: &str) -> Result<i32, CliError> {
    value
        .parse()
        .map_err(|_| CliError::invalid_usage(format!("reserve-id must be an integer: {value}")))
}

fn parse_u8_arg(value: Option<&String>, name: &str) -> Result<u8, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(|_| CliError::invalid_usage(format!("{name} must be a number")))
}

fn parse_u16_arg(value: Option<&String>, name: &str) -> Result<u16, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(|_| CliError::invalid_usage(format!("{name} must be a number")))
}

fn parse_i32_arg(value: Option<&String>, name: &str) -> Result<i32, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(|_| CliError::invalid_usage(format!("{name} must be an integer")))
}

fn parse_broadcast_type_arg(value: Option<&String>, name: &str) -> Result<BroadcastType, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(CliError::invalid_usage)
}

fn parse_search_date_range(value: &str) -> Result<SearchDateInfo, CliError> {
    let (start, end) = value.split_once('-').ok_or_else(|| {
        CliError::invalid_usage(format!(
            "date range must be start-dow:HH:MM-end-dow:HH:MM: {value}"
        ))
    })?;
    let (start_day_of_week, start_hour, start_min) = parse_search_date_endpoint(start, value)?;
    let (end_day_of_week, end_hour, end_min) = parse_search_date_endpoint(end, value)?;
    Ok(SearchDateInfo {
        start_day_of_week,
        start_hour,
        start_min,
        end_day_of_week,
        end_hour,
        end_min,
    })
}

fn parse_search_date_endpoint(value: &str, source: &str) -> Result<(u8, u16, u16), CliError> {
    let mut parts = value.split(':');
    let day = parts
        .next()
        .ok_or_else(|| invalid_search_date_range(source))?
        .parse::<u8>()
        .map_err(|_| invalid_search_date_range(source))?;
    let hour = parts
        .next()
        .ok_or_else(|| invalid_search_date_range(source))?
        .parse::<u16>()
        .map_err(|_| invalid_search_date_range(source))?;
    let minute = parts
        .next()
        .ok_or_else(|| invalid_search_date_range(source))?
        .parse::<u16>()
        .map_err(|_| invalid_search_date_range(source))?;
    if parts.next().is_some() || day > 6 || hour > 23 || minute > 59 {
        return Err(invalid_search_date_range(source));
    }
    Ok((day, hour, minute))
}

fn invalid_search_date_range(value: &str) -> CliError {
    CliError::invalid_usage(format!(
        "date range must be start-dow:HH:MM-end-dow:HH:MM with day 0..=6, hour 0..=23, and minute 0..=59: {value}"
    ))
}

fn parse_recording_mode_arg(value: Option<&String>, name: &str) -> Result<RecordingMode, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(CliError::invalid_usage)
}

fn parse_service_recording_mode_arg(
    value: Option<&String>,
    name: &str,
) -> Result<ServiceRecordingMode, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(CliError::invalid_usage)
}

fn parse_post_recording_mode_arg(
    value: Option<&String>,
    name: &str,
) -> Result<PostRecordingMode, CliError> {
    value
        .ok_or_else(|| CliError::invalid_usage(format!("{name} requires a value")))?
        .parse()
        .map_err(CliError::invalid_usage)
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
        CliCommand::ReserveGet(reserve_id) => {
            let value = flows::get_reservation(&client, reserve_id)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::ReservePreview { event_key, options } => {
            let value = flows::preview_reservation_with_options(&client, event_key, &options)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::ReserveCreate { event_key, options } => {
            let value = flows::create_reservation_with_options(&client, event_key, &options)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::ReserveUpdate {
            reserve_id,
            options,
        } => {
            let value = flows::update_reservation(&client, reserve_id, &options)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_plain(&value)
            })
        }
        CliCommand::ReserveDelete(reserve_id) => {
            let value = flows::delete_reservation(&client, reserve_id)
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
  edcb [global flags] programs search [search options]
  edcb [global flags] reserves create --event <onid:tsid:sid:eid> [recording options] --yes
  edcb [global flags] reserves update <reserve-id> [recording options] --yes
  edcb [global flags] reserves delete <reserve-id> --yes
  edcb [global flags] plugins <write|rec_name>

COMMANDS:
  services
  reserves
  recorded list
  recorded get <info-id>
  programs search [search options]
  reserves get <reserve-id>
  reserves preview --event <onid:tsid:sid:eid> [recording options]
  reserves create --event <onid:tsid:sid:eid> [recording options] --yes
  reserves update <reserve-id> [recording options] --yes
  reserves delete <reserve-id> --yes
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

PROGRAM SEARCH OPTIONS:
      --keyword <text>
      --exclude-keyword <text>
      --title-only
      --case-sensitive
      --regex
      --fuzzy
      --service <onid:tsid:sid>                     Repeatable
      --date-range <start-dow:HH:MM-end-dow:HH:MM> Repeatable; 0 is Sunday
      --exclude-date-ranges
      --duration-min <minutes>
      --duration-max <minutes>
      --free-ca <all|free|paid>

RECORDING OPTIONS:
      --priority <1-5>
      --enable | --disable
      --recording-mode <all|all-without-decoding|specified|specified-without-decoding|view>
      --start-margin <seconds> --end-margin <seconds>
      --caption <default|enable|disable> --data <default|enable|disable>
      --post-recording <default|nothing|standby|standby-and-reboot|suspend|suspend-and-reboot|shutdown>

EXAMPLES:
  edcb services
  edcb --json services
  edcb reserves --plain
  edcb recorded list
  edcb recorded get 1 --json
  edcb programs search --keyword ニュース --title-only
  edcb programs search --keyword ニュース --date-range 1:19:00-1:23:00
  edcb programs search --keyword ニュース --duration-min 30 --duration-max 120 --free-ca free
  edcb reserves get 1 --json
  edcb reserves preview --event 32736:32736:1024:4208
  edcb reserves create --event 32736:32736:1024:4208 --priority 4 --yes
  edcb reserves update 1 --disable --yes
  edcb reserves delete 1 --yes
  edcb plugins write
  edcb --host 172.18.0.7 notify-status
"
}

pub fn version_text() -> String {
    format!("edcb {VERSION}\n")
}
