use std::collections::BTreeMap;
use std::time::Duration;

use crate::{
    BroadcastType, ChannelType, ConnectionConfig, DuplicateTitleCheckScope, EdcbClient, EventKey,
    PluginKind, ProgramGenreRange, ProgramSearchQuery, RecordSettingsPatch, SearchDateInfo,
    ServiceKey, TimeTableQuery, flows,
};
use chrono::{DateTime, FixedOffset};
use clap::{Parser, error::ErrorKind, value_parser};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerAction {
    Run(ConnectionConfig),
    Help(String),
    Version(String),
}

impl ServerAction {
    pub fn from_env_args() -> Result<Self, String> {
        Self::from_args_and_env(std::env::args(), std::env::vars())
    }

    pub fn from_args_and_env<A, S, E, K, V>(args: A, env: E) -> Result<Self, String>
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
        let raw = match RawServerArgs::try_parse_from(args) {
            Ok(raw) => raw,
            Err(error) => match error.kind() {
                ErrorKind::DisplayHelp => return Ok(Self::Help(error.to_string())),
                ErrorKind::DisplayVersion => return Ok(Self::Version(error.to_string())),
                _ => return Err(clap_error_message(error)),
            },
        };

        Ok(Self::Run(raw.into_config(env)?))
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "edcb-mcp",
    version = VERSION,
    about = "EDCB CtrlCmd stdio MCP server"
)]
struct RawServerArgs {
    #[arg(
        long,
        value_name = "host",
        help = "EDCB host (env: EDCB_HOST, default: 127.0.0.1)"
    )]
    host: Option<String>,
    #[arg(
        long,
        value_name = "port",
        help = "EDCB CtrlCmd port (env: EDCB_PORT, default: 4510)"
    )]
    port: Option<u16>,
    #[arg(
        long,
        value_name = "n",
        value_parser = value_parser!(u64).range(1..),
        help = "Request timeout in seconds (env: EDCB_TIMEOUT_SECONDS, default: 15)"
    )]
    timeout_seconds: Option<u64>,
}

impl RawServerArgs {
    fn into_config(self, env: BTreeMap<String, String>) -> Result<ConnectionConfig, String> {
        let mut config = ConnectionConfig::default();
        if let Some(host) = env.get("EDCB_HOST") {
            config.host.clone_from(host);
        }
        if let Some(port) = env.get("EDCB_PORT") {
            config.port = parse_port(port)?;
        }
        if let Some(timeout) = env.get("EDCB_TIMEOUT_SECONDS") {
            config.timeout = Duration::from_secs(parse_timeout(timeout)?);
        }
        if let Some(host) = self.host {
            config.host = host;
        }
        if let Some(port) = self.port {
            config.port = port;
        }
        if let Some(timeout_seconds) = self.timeout_seconds {
            config.timeout = Duration::from_secs(timeout_seconds);
        }

        Ok(config)
    }
}

fn clap_error_message(error: clap::Error) -> String {
    let message = error.to_string();
    message
        .strip_prefix("error: ")
        .unwrap_or(&message)
        .to_string()
}

fn parse_port(value: &str) -> Result<u16, String> {
    value
        .parse()
        .map_err(|_| format!("port must be a number in 0..=65535: {value}"))
}

fn parse_timeout(value: &str) -> Result<u64, String> {
    let timeout = value
        .parse::<u64>()
        .map_err(|_| format!("timeout must be a positive integer: {value}"))?;
    if timeout == 0 {
        Err("timeout must be greater than zero".to_string())
    } else {
        Ok(timeout)
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RecordedInfoParam {
    pub info_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationIdParam {
    pub reserve_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationConditionIdParam {
    pub condition_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PluginKindParam {
    pub kind: String,
}

impl PluginKindParam {
    pub fn try_into_plugin_kind(&self) -> Result<PluginKind, String> {
        self.kind.parse()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsParam {
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub keyword: String,
    #[serde(default)]
    pub exclude_keyword: String,
    #[serde(default)]
    pub is_title_only: bool,
    #[serde(default)]
    pub is_case_sensitive: bool,
    #[serde(default)]
    pub is_fuzzy_search_enabled: bool,
    #[serde(default)]
    pub is_regex_search_enabled: bool,
    pub service_ranges: Option<Vec<SearchProgramsServiceParam>>,
    pub genre_ranges: Option<Vec<SearchProgramsGenreParam>>,
    #[serde(default)]
    pub is_exclude_genre_ranges: bool,
    pub date_ranges: Option<Vec<SearchProgramsDateParam>>,
    #[serde(default)]
    pub is_exclude_date_ranges: bool,
    pub duration_range_min: Option<u16>,
    pub duration_range_max: Option<u16>,
    #[serde(default)]
    pub broadcast_type: BroadcastType,
    #[serde(default)]
    pub duplicate_title_check_scope: DuplicateTitleCheckScope,
    #[serde(default = "default_duplicate_title_check_period_days")]
    pub duplicate_title_check_period_days: u16,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsServiceParam {
    pub network_id: u16,
    pub transport_stream_id: u16,
    pub service_id: u16,
}

impl SearchProgramsServiceParam {
    fn to_service_key(&self) -> ServiceKey {
        ServiceKey {
            onid: self.network_id,
            tsid: self.transport_stream_id,
            sid: self.service_id,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsGenreParam {
    pub major: u8,
    pub middle: u8,
    pub user_nibble: Option<u16>,
}

impl SearchProgramsGenreParam {
    fn to_genre_range(&self) -> ProgramGenreRange {
        ProgramGenreRange {
            major: self.major,
            middle: self.middle,
            user_nibble: self.user_nibble,
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsDateParam {
    pub start_day_of_week: u8,
    pub start_hour: u16,
    pub start_minute: u16,
    pub end_day_of_week: u8,
    pub end_hour: u16,
    pub end_minute: u16,
}

impl SearchProgramsParam {
    pub fn try_into_query(&self) -> Result<ProgramSearchQuery, String> {
        let service_ranges = self.service_ranges.as_ref().map(|services| {
            services
                .iter()
                .map(SearchProgramsServiceParam::to_service_key)
                .collect()
        });
        let genre_ranges = self
            .genre_ranges
            .as_ref()
            .into_iter()
            .flatten()
            .map(SearchProgramsGenreParam::to_genre_range)
            .collect();
        let date_ranges = self
            .date_ranges
            .as_ref()
            .map(|ranges| {
                ranges
                    .iter()
                    .map(|range| range.try_into_search_date())
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        if let (Some(min), Some(max)) = (self.duration_range_min, self.duration_range_max)
            && min > max
        {
            return Err(
                "program search duration_range_min must be less than or equal to duration_range_max"
                    .to_string(),
            );
        }
        if self.duplicate_title_check_period_days > 9999 {
            return Err(
                "program search duplicate_title_check_period_days must be in 0..=9999".to_string(),
            );
        }
        Ok(ProgramSearchQuery {
            is_enabled: self.is_enabled,
            keyword: self.keyword.clone(),
            exclude_keyword: self.exclude_keyword.clone(),
            title_only: self.is_title_only,
            case_sensitive: self.is_case_sensitive,
            regex: self.is_regex_search_enabled,
            fuzzy: self.is_fuzzy_search_enabled,
            service_ranges,
            genre_ranges,
            exclude_genre_ranges: self.is_exclude_genre_ranges,
            date_ranges,
            exclude_date_ranges: self.is_exclude_date_ranges,
            duration_min: self.duration_range_min,
            duration_max: self.duration_range_max,
            broadcast_type: self.broadcast_type,
            duplicate_title_check_scope: self.duplicate_title_check_scope,
            duplicate_title_check_period_days: self.duplicate_title_check_period_days,
        })
    }
}

fn default_true() -> bool {
    true
}

fn default_duplicate_title_check_period_days() -> u16 {
    6
}

impl SearchProgramsDateParam {
    fn try_into_search_date(&self) -> Result<SearchDateInfo, String> {
        if self.start_day_of_week > 6 || self.end_day_of_week > 6 {
            return Err("date range day_of_week must be in 0..=6".to_string());
        }
        if self.start_hour > 23 || self.end_hour > 23 {
            return Err("date range hour must be in 0..=23".to_string());
        }
        if self.start_minute > 59 || self.end_minute > 59 {
            return Err("date range minute must be in 0..=59".to_string());
        }
        Ok(SearchDateInfo {
            start_day_of_week: self.start_day_of_week,
            start_hour: self.start_hour,
            start_min: self.start_minute,
            end_day_of_week: self.end_day_of_week,
            end_hour: self.end_hour,
            end_min: self.end_minute,
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetTimetableParam {
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
    pub channel_type: Option<ChannelType>,
    pub services: Option<Vec<SearchProgramsServiceParam>>,
}

impl GetTimetableParam {
    pub fn try_into_query(&self) -> Result<TimeTableQuery, String> {
        let services = self
            .services
            .as_ref()
            .into_iter()
            .flatten()
            .map(SearchProgramsServiceParam::to_service_key)
            .collect();
        if let (Some(start), Some(end)) = (self.start_time, self.end_time)
            && end <= start
        {
            return Err("timetable end_time must be later than start_time".to_string());
        }
        Ok(TimeTableQuery {
            start_time: self.start_time,
            end_time: self.end_time,
            channel_type: self.channel_type,
            services,
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationEventParam {
    pub event: String,
    pub options: Option<RecordSettingsPatch>,
}

impl ReservationEventParam {
    fn try_into_event_key(&self) -> Result<EventKey, String> {
        self.event.parse()
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReservationUpdateParam {
    pub reserve_id: i32,
    pub options: RecordSettingsPatch,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateReservationConditionParam {
    pub condition: SearchProgramsParam,
    pub options: Option<RecordSettingsPatch>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateReservationConditionParam {
    pub condition_id: i32,
    pub condition: Option<SearchProgramsParam>,
    pub options: Option<RecordSettingsPatch>,
}

#[derive(Debug, Clone)]
pub struct EdcbMcpServer {
    config: ConnectionConfig,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for EdcbMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("EDCB CtrlCmd MCP server")
    }
}

#[tool_router(router = tool_router)]
impl EdcbMcpServer {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            tool_router: Self::tool_router(),
        }
    }

    pub fn tool_names(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        names.sort();
        names
    }

    #[tool(name = "list_services", description = "List EDCB services")]
    pub async fn list_services(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_service().await)
    }

    #[tool(name = "list_reserves", description = "List EDCB reserves")]
    pub async fn list_reserves(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_reserve().await)
    }

    #[tool(
        name = "get_reservation",
        description = "Get one EDCB reservation by reserve ID"
    )]
    pub async fn get_reservation(
        &self,
        Parameters(params): Parameters<ReservationIdParam>,
    ) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(flows::get_reservation(&client, params.reserve_id).await)
    }

    #[tool(name = "list_recorded", description = "List EDCB recorded file info")]
    pub async fn list_recorded(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_rec_info_basic().await)
    }

    #[tool(
        name = "get_recorded_info",
        description = "Get one EDCB recorded file info item"
    )]
    pub async fn get_recorded_info(
        &self,
        Parameters(params): Parameters<RecordedInfoParam>,
    ) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().get_rec_info(params.info_id).await)
    }

    #[tool(
        name = "search_programs",
        description = "Search EDCB programs with SearchKeyInfo-compatible conditions"
    )]
    pub async fn search_programs(
        &self,
        Parameters(params): Parameters<SearchProgramsParam>,
    ) -> Result<CallToolResult, String> {
        let query = params.try_into_query()?;
        let client = self.client();
        to_call_tool_result(flows::search_programs(&client, &query).await)
    }

    #[tool(
        name = "get_timetable",
        description = "Get EDCB timetable programs grouped by service with optional channel/service/time filters"
    )]
    pub async fn get_timetable(
        &self,
        Parameters(params): Parameters<GetTimetableParam>,
    ) -> Result<CallToolResult, String> {
        let query = params.try_into_query()?;
        let client = self.client();
        to_call_tool_result(flows::get_timetable(&client, &query).await)
    }

    #[tool(
        name = "list_reservation_conditions",
        description = "List EDCB keyword auto reservation conditions"
    )]
    pub async fn list_reservation_conditions(&self) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(flows::list_reservation_conditions(&client).await)
    }

    #[tool(
        name = "get_reservation_condition",
        description = "Get one EDCB keyword auto reservation condition by condition ID"
    )]
    pub async fn get_reservation_condition(
        &self,
        Parameters(params): Parameters<ReservationConditionIdParam>,
    ) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(flows::get_reservation_condition(&client, params.condition_id).await)
    }

    #[tool(
        name = "create_reservation_condition",
        description = "Create an EDCB keyword auto reservation condition"
    )]
    pub async fn create_reservation_condition(
        &self,
        Parameters(params): Parameters<CreateReservationConditionParam>,
    ) -> Result<CallToolResult, String> {
        let query = params.condition.try_into_query()?;
        let options = params.options.unwrap_or_default();
        let client = self.client();
        to_call_tool_result(flows::create_reservation_condition(&client, &query, &options).await)
    }

    #[tool(
        name = "update_reservation_condition",
        description = "Update one EDCB keyword auto reservation condition"
    )]
    pub async fn update_reservation_condition(
        &self,
        Parameters(params): Parameters<UpdateReservationConditionParam>,
    ) -> Result<CallToolResult, String> {
        let query = params
            .condition
            .as_ref()
            .map(SearchProgramsParam::try_into_query)
            .transpose()?;
        let options = params.options.unwrap_or_default();
        let client = self.client();
        to_call_tool_result(
            flows::update_reservation_condition(
                &client,
                params.condition_id,
                query.as_ref(),
                &options,
            )
            .await,
        )
    }

    #[tool(
        name = "delete_reservation_condition",
        description = "Delete one EDCB keyword auto reservation condition by condition ID after fetching it"
    )]
    pub async fn delete_reservation_condition(
        &self,
        Parameters(params): Parameters<ReservationConditionIdParam>,
    ) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(flows::delete_reservation_condition(&client, params.condition_id).await)
    }

    #[tool(
        name = "preview_reservation",
        description = "Build an EDCB reservation from an event without sending a mutation command"
    )]
    pub async fn preview_reservation(
        &self,
        Parameters(params): Parameters<ReservationEventParam>,
    ) -> Result<CallToolResult, String> {
        let event_key = params.try_into_event_key()?;
        let options = params.options.unwrap_or_default();
        let client = self.client();
        to_call_tool_result(
            flows::preview_reservation_with_options(&client, event_key, &options).await,
        )
    }

    #[tool(
        name = "create_reservation",
        description = "Create an EDCB reservation from an event using the server default reservation settings"
    )]
    pub async fn create_reservation(
        &self,
        Parameters(params): Parameters<ReservationEventParam>,
    ) -> Result<CallToolResult, String> {
        let event_key = params.try_into_event_key()?;
        let options = params.options.unwrap_or_default();
        let client = self.client();
        to_call_tool_result(
            flows::create_reservation_with_options(&client, event_key, &options).await,
        )
    }

    #[tool(
        name = "update_reservation",
        description = "Update one EDCB reservation's recording settings by reserve ID"
    )]
    pub async fn update_reservation(
        &self,
        Parameters(params): Parameters<ReservationUpdateParam>,
    ) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(
            flows::update_reservation(&client, params.reserve_id, &params.options).await,
        )
    }

    #[tool(
        name = "delete_reservation",
        description = "Delete one EDCB reservation by reserve ID after fetching it"
    )]
    pub async fn delete_reservation(
        &self,
        Parameters(params): Parameters<ReservationIdParam>,
    ) -> Result<CallToolResult, String> {
        let client = self.client();
        to_call_tool_result(flows::delete_reservation(&client, params.reserve_id).await)
    }

    #[tool(
        name = "list_tuner_reserves",
        description = "List EDCB tuner reserve state"
    )]
    pub async fn list_tuner_reserves(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_tuner_reserve().await)
    }

    #[tool(
        name = "list_tuner_processes",
        description = "List EDCB tuner process state"
    )]
    pub async fn list_tuner_processes(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().enum_tuner_process().await)
    }

    #[tool(name = "list_plugins", description = "List EDCB plugins by kind")]
    pub async fn list_plugins(
        &self,
        Parameters(params): Parameters<PluginKindParam>,
    ) -> Result<CallToolResult, String> {
        let kind = params.try_into_plugin_kind()?;
        to_call_tool_result(self.client().enum_plugin(kind).await)
    }

    #[tool(
        name = "get_notify_status",
        description = "Get current EDCB notify status"
    )]
    pub async fn get_notify_status(&self) -> Result<CallToolResult, String> {
        to_call_tool_result(self.client().get_notify_srv_status().await)
    }

    fn client(&self) -> EdcbClient {
        EdcbClient::new(self.config.clone())
    }
}

fn to_call_tool_result<T: Serialize>(result: crate::Result<T>) -> Result<CallToolResult, String> {
    let value = result.map_err(|error| error.to_string())?;
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|error| format!("failed to serialize tool response: {error}"))
}
