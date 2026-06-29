use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use clap::{Args, Parser, Subcommand, error::ErrorKind, value_parser};
use serde::Serialize;

use crate::{
    BroadcastType, ChannelType, ConnectionConfig, DuplicateTitleCheckScope, EdcbClient, EventKey,
    PluginKind, PostRecordingMode, ProgramGenreRange, ProgramSearchQuery, RecordSettingsPatch,
    RecordingMode, SearchDateInfo, ServiceKey, ServiceRecordingMode, TimeTable, TimeTableQuery,
    flows,
    types::{
        Channel, EventInfo, NotifySrvInfo, RecFileInfo, RecordSettings, RecordSettingsPresets,
        ReservationCondition, ReserveData, ServiceInfo, TunerProcessStatusInfo, TunerReserveInfo,
    },
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const CLI_EXAMPLES: &str = "EXAMPLES:
  edcb services
  edcb --json services
  edcb reserves --plain
  edcb recorded list
  edcb recorded get 1 --json
  edcb programs search --keyword ニュース --title-only
  edcb programs search --keyword ニュース --date-range 1:19:00-1:23:00
  edcb programs search --keyword ニュース --duration-min 30 --duration-max 120 --free-ca free
  edcb programs timetable --channel-type gr
  edcb programs timetable --service 32736:32736:1024 --start-time 2026-06-29T19:00:00+09:00 --end-time 2026-06-29T23:00:00+09:00
  edcb channels
  edcb recording defaults
  edcb recording presets --json
  edcb reservation-conditions --json
  edcb reservation-conditions create --keyword ニュース --genre 0:1 --priority 4 --yes
  edcb reservation-conditions update 77 --keyword ニュース --duplicate-title-check same-channel --yes
  edcb reserves get 1 --json
  edcb reserves preview --event 32736:32736:1024:4208
  edcb reserves create --event 32736:32736:1024:4208 --priority 4 --yes
  edcb reserves update 1 --disable --yes
  edcb reserves delete 1 --yes
  edcb plugins write
  edcb --host 172.18.0.7 notify-status";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CliCommand {
    Services,
    Reserves,
    RecordedList,
    RecordedGet(i32),
    ProgramsSearch(ProgramSearchQuery),
    ProgramsTimetable(TimeTableQuery),
    Channels,
    RecordingDefaults,
    RecordingPresets,
    ReservationConditionsList,
    ReservationConditionGet(i32),
    ReservationConditionCreate {
        query: ProgramSearchQuery,
        options: RecordSettingsPatch,
    },
    ReservationConditionUpdate {
        condition_id: i32,
        query: Option<ProgramSearchQuery>,
        options: RecordSettingsPatch,
    },
    ReservationConditionDelete(i32),
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
    pub connection: ConnectionConfig,
    pub output: OutputMode,
    pub command: CliCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CliAction {
    Run(CliInvocation),
    Help(String),
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

    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let raw = match RawCli::try_parse_from(args) {
        Ok(raw) => raw,
        Err(error) => match error.kind() {
            ErrorKind::DisplayHelp => return Ok(CliAction::Help(error.to_string())),
            ErrorKind::DisplayVersion => return Ok(CliAction::Version),
            _ => return Err(CliError::invalid_usage(clap_error_message(error))),
        },
    };

    Ok(CliAction::Run(raw.into_invocation(env)?))
}

fn clap_error_message(error: clap::Error) -> String {
    let message = error.to_string();
    message
        .strip_prefix("error: ")
        .unwrap_or(&message)
        .to_string()
}

#[derive(Debug, Parser)]
#[command(
    name = "edcb",
    version = VERSION,
    about = "EDCB CtrlCmd command line interface",
    after_long_help = CLI_EXAMPLES
)]
struct RawCli {
    #[arg(
        long,
        global = true,
        value_name = "host",
        help = "EDCB host (env: EDCB_HOST, default: 127.0.0.1)"
    )]
    host: Option<String>,
    #[arg(
        long,
        global = true,
        value_name = "port",
        help = "EDCB CtrlCmd port (env: EDCB_PORT, default: 4510)"
    )]
    port: Option<u16>,
    #[arg(
        long,
        global = true,
        value_name = "n",
        value_parser = value_parser!(u64).range(1..),
        help = "Request timeout in seconds (env: EDCB_TIMEOUT_SECONDS, default: 15)"
    )]
    timeout_seconds: Option<u64>,
    #[arg(
        long,
        global = true,
        conflicts_with = "plain",
        help = "Print pretty JSON"
    )]
    json: bool,
    #[arg(
        long,
        global = true,
        conflicts_with = "json",
        help = "Print stable line-based output"
    )]
    plain: bool,
    #[command(subcommand)]
    command: RawCommand,
}

impl RawCli {
    fn into_invocation(self, env: BTreeMap<String, String>) -> Result<CliInvocation, CliError> {
        let host = self
            .host
            .or_else(|| env.get("EDCB_HOST").cloned())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = match self.port {
            Some(port) => port,
            None => env
                .get("EDCB_PORT")
                .map(|value| parse_port(value))
                .transpose()?
                .unwrap_or(4510),
        };
        let timeout_seconds = match self.timeout_seconds {
            Some(timeout_seconds) => timeout_seconds,
            None => env
                .get("EDCB_TIMEOUT_SECONDS")
                .map(|value| parse_timeout(value))
                .transpose()?
                .unwrap_or(15),
        };
        let timeout = Duration::from_secs(timeout_seconds);
        let output = if self.json {
            OutputMode::Json
        } else if self.plain {
            OutputMode::Plain
        } else {
            OutputMode::Human
        };

        Ok(CliInvocation {
            connection: ConnectionConfig {
                host,
                port,
                timeout,
            },
            output,
            command: self.command.try_into_command()?,
        })
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum RawCommand {
    #[command(about = "List services")]
    Services,
    #[command(about = "List or manage reservations")]
    Reserves(ReservesArgs),
    #[command(about = "List or inspect recorded items")]
    Recorded(RecordedArgs),
    #[command(about = "Search programs or retrieve timetable data")]
    Programs(ProgramsArgs),
    #[command(about = "List KonomiTV-style channel metadata")]
    Channels,
    #[command(about = "Inspect recording defaults and presets")]
    Recording(RecordingArgs),
    #[command(about = "List or manage keyword auto reservation conditions")]
    ReservationConditions(ReservationConditionsArgs),
    #[command(about = "List tuner reservation state")]
    TunerReserves,
    #[command(about = "List tuner process state")]
    TunerProcesses,
    #[command(about = "List EDCB plugins")]
    Plugins {
        #[arg(value_name = "write|rec_name", help = "Plugin kind")]
        kind: PluginKind,
    },
    #[command(about = "Get EDCB notify server status")]
    NotifyStatus,
}

impl RawCommand {
    fn try_into_command(self) -> Result<CliCommand, CliError> {
        match self {
            Self::Services => Ok(CliCommand::Services),
            Self::Reserves(args) => args.try_into_command(),
            Self::Recorded(args) => args.try_into_command(),
            Self::Programs(args) => args.try_into_command(),
            Self::Channels => Ok(CliCommand::Channels),
            Self::Recording(args) => Ok(args.into_command()),
            Self::ReservationConditions(args) => args.try_into_command(),
            Self::TunerReserves => Ok(CliCommand::TunerReserves),
            Self::TunerProcesses => Ok(CliCommand::TunerProcesses),
            Self::Plugins { kind } => Ok(CliCommand::Plugins(kind)),
            Self::NotifyStatus => Ok(CliCommand::NotifyStatus),
        }
    }
}

#[derive(Debug, Args)]
struct RecordingArgs {
    #[command(subcommand)]
    command: RecordingCommand,
}

impl RecordingArgs {
    fn into_command(self) -> CliCommand {
        match self.command {
            RecordingCommand::Defaults => CliCommand::RecordingDefaults,
            RecordingCommand::Presets => CliCommand::RecordingPresets,
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum RecordingCommand {
    Defaults,
    Presets,
}

#[derive(Debug, Args)]
struct RecordedArgs {
    #[command(subcommand)]
    command: RecordedCommand,
}

impl RecordedArgs {
    fn try_into_command(self) -> Result<CliCommand, CliError> {
        match self.command {
            RecordedCommand::List => Ok(CliCommand::RecordedList),
            RecordedCommand::Get { info_id } => Ok(CliCommand::RecordedGet(info_id)),
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum RecordedCommand {
    List,
    Get {
        #[arg(value_name = "info-id")]
        info_id: i32,
    },
}

#[derive(Debug, Args)]
struct ProgramsArgs {
    #[command(subcommand)]
    command: ProgramsCommand,
}

impl ProgramsArgs {
    fn try_into_command(self) -> Result<CliCommand, CliError> {
        match self.command {
            ProgramsCommand::Search(search) => Ok(CliCommand::ProgramsSearch(search.into_query())),
            ProgramsCommand::Timetable(timetable) => {
                Ok(CliCommand::ProgramsTimetable(timetable.into_query()))
            }
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum ProgramsCommand {
    Search(SearchOptions),
    Timetable(TimetableOptions),
}

#[derive(Debug, Args)]
struct ReservationConditionsArgs {
    #[command(subcommand)]
    command: Option<ReservationConditionsCommand>,
}

impl ReservationConditionsArgs {
    fn try_into_command(self) -> Result<CliCommand, CliError> {
        match self.command {
            None => Ok(CliCommand::ReservationConditionsList),
            Some(ReservationConditionsCommand::Get { condition_id }) => {
                Ok(CliCommand::ReservationConditionGet(condition_id))
            }
            Some(ReservationConditionsCommand::Create {
                search,
                recording,
                yes,
            }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reservation-conditions create requires --yes to confirm mutation",
                    ));
                }
                Ok(CliCommand::ReservationConditionCreate {
                    query: search.into_query(),
                    options: recording.into_patch(),
                })
            }
            Some(ReservationConditionsCommand::Update {
                condition_id,
                search,
                recording,
                yes,
            }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reservation-conditions update requires --yes to confirm mutation",
                    ));
                }
                let query = search.has_any().then(|| search.into_query());
                Ok(CliCommand::ReservationConditionUpdate {
                    condition_id,
                    query,
                    options: recording.into_patch(),
                })
            }
            Some(ReservationConditionsCommand::Delete { condition_id, yes }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reservation-conditions delete requires --yes to confirm mutation",
                    ));
                }
                Ok(CliCommand::ReservationConditionDelete(condition_id))
            }
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum ReservationConditionsCommand {
    Get {
        #[arg(value_name = "condition-id")]
        condition_id: i32,
    },
    Create {
        #[command(flatten)]
        search: SearchOptions,
        #[command(flatten)]
        recording: RecordingOptions,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
    Update {
        #[arg(value_name = "condition-id")]
        condition_id: i32,
        #[command(flatten)]
        search: SearchOptions,
        #[command(flatten)]
        recording: RecordingOptions,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
    Delete {
        #[arg(value_name = "condition-id")]
        condition_id: i32,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
}

#[derive(Debug, Args)]
struct ReservesArgs {
    #[command(subcommand)]
    command: Option<ReservesCommand>,
}

impl ReservesArgs {
    fn try_into_command(self) -> Result<CliCommand, CliError> {
        match self.command {
            None => Ok(CliCommand::Reserves),
            Some(ReservesCommand::Get { reserve_id }) => Ok(CliCommand::ReserveGet(reserve_id)),
            Some(ReservesCommand::Preview { event, recording }) => Ok(CliCommand::ReservePreview {
                event_key: event,
                options: recording.into_patch(),
            }),
            Some(ReservesCommand::Create {
                event,
                recording,
                yes,
            }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reserves create requires --yes to confirm mutation",
                    ));
                }
                Ok(CliCommand::ReserveCreate {
                    event_key: event,
                    options: recording.into_patch(),
                })
            }
            Some(ReservesCommand::Update {
                reserve_id,
                recording,
                yes,
            }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reserves update requires --yes to confirm mutation",
                    ));
                }
                Ok(CliCommand::ReserveUpdate {
                    reserve_id,
                    options: recording.into_patch(),
                })
            }
            Some(ReservesCommand::Delete { reserve_id, yes }) => {
                if !yes {
                    return Err(CliError::invalid_usage(
                        "reserves delete requires --yes to confirm mutation",
                    ));
                }
                Ok(CliCommand::ReserveDelete(reserve_id))
            }
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
enum ReservesCommand {
    Get {
        #[arg(value_name = "reserve-id")]
        reserve_id: i32,
    },
    Preview {
        #[arg(long, value_name = "onid:tsid:sid:eid", help = "Event key to reserve")]
        event: EventKey,
        #[command(flatten)]
        recording: RecordingOptions,
    },
    Create {
        #[arg(long, value_name = "onid:tsid:sid:eid", help = "Event key to reserve")]
        event: EventKey,
        #[command(flatten)]
        recording: RecordingOptions,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
    Update {
        #[arg(value_name = "reserve-id")]
        reserve_id: i32,
        #[command(flatten)]
        recording: RecordingOptions,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
    Delete {
        #[arg(value_name = "reserve-id")]
        reserve_id: i32,
        #[arg(long, help = "Confirm the mutation")]
        yes: bool,
    },
}

#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Search Options")]
struct SearchOptions {
    #[arg(long, help = "Keyword to search")]
    keyword: Option<String>,
    #[arg(long, help = "Keyword to exclude")]
    exclude_keyword: Option<String>,
    #[arg(long, help = "Match only program titles")]
    title_only: bool,
    #[arg(long, help = "Use case-sensitive keyword matching")]
    case_sensitive: bool,
    #[arg(long, help = "Treat keyword as a regular expression")]
    regex: bool,
    #[arg(long, help = "Enable EDCB fuzzy search")]
    fuzzy: bool,
    #[arg(
        long,
        value_name = "onid:tsid:sid",
        help = "Service key to search; repeatable"
    )]
    service: Vec<ServiceKey>,
    #[arg(
        long,
        value_name = "major:middle[:user_nibble]",
        value_parser = parse_program_genre_range,
        help = "EDCB genre range; repeatable"
    )]
    genre: Vec<ProgramGenreRange>,
    #[arg(long, help = "Exclude matching genre ranges")]
    exclude_genre_ranges: bool,
    #[arg(
        long,
        value_name = "start-dow:HH:MM-end-dow:HH:MM",
        value_parser = parse_search_date_range,
        help = "EDCB recurring weekday/time range; repeatable"
    )]
    date_range: Vec<SearchDateInfo>,
    #[arg(long, help = "Exclude matching date ranges")]
    exclude_date_ranges: bool,
    #[arg(long, value_name = "minutes", help = "Minimum duration in minutes")]
    duration_min: Option<u16>,
    #[arg(long, value_name = "minutes", help = "Maximum duration in minutes")]
    duration_max: Option<u16>,
    #[arg(long, value_name = "all|free|paid", help = "Broadcast fee filter")]
    free_ca: Option<BroadcastType>,
    #[arg(
        long,
        conflicts_with = "search_disable",
        help = "Enable this condition"
    )]
    search_enable: bool,
    #[arg(
        long,
        conflicts_with = "search_enable",
        help = "Disable this condition"
    )]
    search_disable: bool,
    #[arg(
        long,
        value_name = "none|same-channel|all-channels",
        help = "Duplicate title check scope"
    )]
    duplicate_title_check: Option<DuplicateTitleCheckScope>,
    #[arg(
        long,
        value_name = "days",
        help = "Duplicate title check period in days"
    )]
    duplicate_title_check_days: Option<u16>,
}

impl SearchOptions {
    fn has_any(&self) -> bool {
        self.keyword.is_some()
            || self.exclude_keyword.is_some()
            || self.title_only
            || self.case_sensitive
            || self.regex
            || self.fuzzy
            || !self.service.is_empty()
            || !self.genre.is_empty()
            || self.exclude_genre_ranges
            || !self.date_range.is_empty()
            || self.exclude_date_ranges
            || self.duration_min.is_some()
            || self.duration_max.is_some()
            || self.free_ca.is_some()
            || self.search_enable
            || self.search_disable
            || self.duplicate_title_check.is_some()
            || self.duplicate_title_check_days.is_some()
    }

    fn into_query(self) -> ProgramSearchQuery {
        let mut query = ProgramSearchQuery {
            keyword: self.keyword.unwrap_or_default(),
            exclude_keyword: self.exclude_keyword.unwrap_or_default(),
            title_only: self.title_only,
            case_sensitive: self.case_sensitive,
            regex: self.regex,
            fuzzy: self.fuzzy,
            exclude_genre_ranges: self.exclude_genre_ranges,
            exclude_date_ranges: self.exclude_date_ranges,
            ..ProgramSearchQuery::default()
        };
        if !self.service.is_empty() {
            query.service_ranges = Some(self.service);
        }
        query.genre_ranges = self.genre;
        query.date_ranges = self.date_range;
        query.duration_min = self.duration_min;
        query.duration_max = self.duration_max;
        if let Some(value) = self.free_ca {
            query.broadcast_type = value;
        }
        if self.search_disable {
            query.is_enabled = false;
        } else if self.search_enable {
            query.is_enabled = true;
        }
        if let Some(value) = self.duplicate_title_check {
            query.duplicate_title_check_scope = value;
        }
        if let Some(value) = self.duplicate_title_check_days {
            query.duplicate_title_check_period_days = value;
        }
        query
    }
}

#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Timetable Options")]
struct TimetableOptions {
    #[arg(
        long,
        value_name = "onid:tsid:sid",
        help = "Service key to include; repeatable"
    )]
    service: Vec<ServiceKey>,
    #[arg(
        long,
        value_name = "RFC3339 datetime",
        value_parser = parse_datetime_value,
        help = "Start time filter"
    )]
    start_time: Option<DateTime<FixedOffset>>,
    #[arg(
        long,
        value_name = "RFC3339 datetime",
        value_parser = parse_datetime_value,
        help = "End time filter"
    )]
    end_time: Option<DateTime<FixedOffset>>,
    #[arg(
        long,
        value_name = "gr|bs|cs|catv|sky|bs4k",
        help = "Channel type filter"
    )]
    channel_type: Option<ChannelType>,
}

impl TimetableOptions {
    fn into_query(self) -> TimeTableQuery {
        TimeTableQuery {
            start_time: self.start_time,
            end_time: self.end_time,
            channel_type: self.channel_type,
            services: self.service,
        }
    }
}

#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Recording Options")]
struct RecordingOptions {
    #[arg(long, value_name = "1-5", help = "Reservation priority")]
    priority: Option<u8>,
    #[arg(long, conflicts_with = "disable", help = "Enable the reservation")]
    enable: bool,
    #[arg(long, conflicts_with = "enable", help = "Disable the reservation")]
    disable: bool,
    #[arg(long, value_name = "mode", help = "Recording mode")]
    recording_mode: Option<RecordingMode>,
    #[arg(long, value_name = "seconds", help = "Recording start margin")]
    start_margin: Option<i32>,
    #[arg(long, value_name = "seconds", help = "Recording end margin")]
    end_margin: Option<i32>,
    #[arg(
        long,
        value_name = "default|enable|disable",
        help = "Caption recording mode"
    )]
    caption: Option<ServiceRecordingMode>,
    #[arg(
        long,
        value_name = "default|enable|disable",
        help = "Data broadcasting recording mode"
    )]
    data: Option<ServiceRecordingMode>,
    #[arg(long, value_name = "mode", help = "Post-recording action")]
    post_recording: Option<PostRecordingMode>,
}

impl RecordingOptions {
    fn into_patch(self) -> RecordSettingsPatch {
        RecordSettingsPatch {
            is_enabled: if self.disable {
                Some(false)
            } else if self.enable {
                Some(true)
            } else {
                None
            },
            priority: self.priority,
            recording_mode: self.recording_mode,
            recording_start_margin: self.start_margin,
            recording_end_margin: self.end_margin,
            caption_recording_mode: self.caption,
            data_broadcasting_recording_mode: self.data,
            post_recording_mode: self.post_recording,
            ..RecordSettingsPatch::default()
        }
    }
}

fn parse_program_genre_range(value: &str) -> Result<ProgramGenreRange, CliError> {
    let parts: Vec<_> = value.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(CliError::invalid_usage(format!(
            "genre must be major:middle[:user_nibble]: {value}"
        )));
    }
    Ok(ProgramGenreRange {
        major: parts[0]
            .parse()
            .map_err(|_| CliError::invalid_usage(format!("genre major must be u8: {value}")))?,
        middle: parts[1]
            .parse()
            .map_err(|_| CliError::invalid_usage(format!("genre middle must be u8: {value}")))?,
        user_nibble: parts
            .get(2)
            .map(|part| {
                part.parse().map_err(|_| {
                    CliError::invalid_usage(format!("genre user_nibble must be u16: {value}"))
                })
            })
            .transpose()?,
    })
}

fn parse_datetime_value(value: &str) -> Result<DateTime<FixedOffset>, CliError> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|_| CliError::invalid_usage("datetime must be RFC 3339 datetime"))
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
    let client = EdcbClient::new(invocation.connection.clone());

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
        CliCommand::ProgramsTimetable(query) => {
            let value = flows::get_timetable(&client, &query)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_timetable_plain(&value)
            })
        }
        CliCommand::Channels => {
            let value = flows::list_channels(&client).await.map_err(runtime_error)?;
            render(&invocation.output, &value, || format_channels_plain(&value))
        }
        CliCommand::RecordingDefaults => {
            let value = flows::get_recording_defaults(&client)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_record_settings_plain(&value)
            })
        }
        CliCommand::RecordingPresets => {
            let value = flows::get_recording_presets(&client)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_recording_presets_plain(&value)
            })
        }
        CliCommand::ReservationConditionsList => {
            let value = flows::list_reservation_conditions(&client)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_conditions_plain(&value)
            })
        }
        CliCommand::ReservationConditionGet(condition_id) => {
            let value = flows::get_reservation_condition(&client, condition_id)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_condition_plain(&value)
            })
        }
        CliCommand::ReservationConditionCreate { query, options } => {
            let value = flows::create_reservation_condition(&client, &query, &options)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_condition_plain(&value)
            })
        }
        CliCommand::ReservationConditionUpdate {
            condition_id,
            query,
            options,
        } => {
            let value = flows::update_reservation_condition(
                &client,
                condition_id,
                query.as_ref(),
                &options,
            )
            .await
            .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_condition_plain(&value)
            })
        }
        CliCommand::ReservationConditionDelete(condition_id) => {
            let value = flows::delete_reservation_condition(&client, condition_id)
                .await
                .map_err(runtime_error)?;
            render(&invocation.output, &value, || {
                format_reservation_condition_plain(&value)
            })
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

fn format_timetable_plain(timetable: &TimeTable) -> String {
    let mut output = String::new();
    for channel in &timetable.channels {
        push_timetable_program_lines(
            &mut output,
            &channel.service.service_name,
            &channel.programs,
        );
        if let Some(subchannels) = &channel.subchannels {
            for subchannel in subchannels {
                push_timetable_program_lines(
                    &mut output,
                    &subchannel.service.service_name,
                    &subchannel.programs,
                );
            }
        }
    }
    output
}

fn push_timetable_program_lines(
    output: &mut String,
    service_name: &str,
    programs: &[crate::TimeTableProgram],
) {
    for program in programs {
        let event = &program.event;
        let title = event
            .short_info
            .as_ref()
            .map(|info| info.event_name.as_str())
            .unwrap_or("");
        let start = event
            .start_time
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());
        let duration = event
            .duration_sec
            .map(|duration| duration.to_string())
            .unwrap_or_else(|| "-".to_string());
        let reservation = program
            .reservation
            .as_ref()
            .map(|reservation| {
                format!(
                    "{}:{:?}:{:?}",
                    reservation.id, reservation.status, reservation.recording_availability
                )
            })
            .unwrap_or_else(|| "-".to_string());
        output.push_str(&format!(
            "{}:{}:{}:{}\t{}\t{}\t{}\t{}\t{}\n",
            event.onid,
            event.tsid,
            event.sid,
            event.eid,
            start,
            duration,
            service_name,
            title,
            reservation
        ));
    }
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

fn format_reservation_conditions_plain(conditions: &[ReservationCondition]) -> String {
    conditions
        .iter()
        .map(format_reservation_condition_plain)
        .collect()
}

fn format_reservation_condition_plain(condition: &ReservationCondition) -> String {
    format!(
        "{}\t{}\t{}\t{}\n",
        condition.id,
        condition.reservation_count,
        condition.program_search_condition.is_enabled,
        condition.program_search_condition.keyword
    )
}

fn format_channels_plain(channels: &[Channel]) -> String {
    channels
        .iter()
        .map(|channel| {
            format!(
                "{}\t{}:{}:{}\t{:?}\t{}\t{}\n",
                channel.display_channel_id,
                channel.service_key.onid,
                channel.service_key.tsid,
                channel.service_key.sid,
                channel.channel_type,
                channel.remocon_id,
                channel.name
            )
        })
        .collect()
}

fn format_record_settings_plain(settings: &RecordSettings) -> String {
    format!(
        "enabled={}\tpriority={}\tmode={:?}\tmargin={}:{}\tcaption={:?}\tdata={:?}\tpost={:?}\tfolders={}\n",
        settings.is_enabled,
        settings.priority,
        settings.recording_mode,
        settings
            .recording_start_margin
            .map(|value| value.to_string())
            .unwrap_or_else(|| "default".to_string()),
        settings
            .recording_end_margin
            .map(|value| value.to_string())
            .unwrap_or_else(|| "default".to_string()),
        settings.caption_recording_mode,
        settings.data_broadcasting_recording_mode,
        settings.post_recording_mode,
        settings.recording_folders.len()
    )
}

fn format_recording_presets_plain(presets: &RecordSettingsPresets) -> String {
    let mut output = format!(
        "global\tmargin={}:{}\tcaption={:?}\tdata={:?}\tpost={:?}\n",
        presets.global_defaults.recording_start_margin,
        presets.global_defaults.recording_end_margin,
        presets.global_defaults.caption_recording_mode,
        presets.global_defaults.data_broadcasting_recording_mode,
        presets.global_defaults.post_recording_mode
    );
    for preset in &presets.presets {
        output.push_str(&format!(
            "{}\t{}\t{}",
            preset.id,
            preset.name,
            format_record_settings_plain(&preset.record_settings)
        ));
    }
    output
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

pub fn version_text() -> String {
    format!("edcb {VERSION}\n")
}
