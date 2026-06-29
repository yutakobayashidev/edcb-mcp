use std::collections::BTreeMap;
use std::time::Duration;

use crate::{
    BroadcastType, EdcbClient, EventKey, PluginKind, ProgramSearchQuery, RecordSettingsPatch,
    SearchDateInfo, ServiceKey, flows,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 4510,
            timeout: Duration::from_secs(15),
        }
    }
}

impl ServerConfig {
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

        let mut config = Self::default();
        if let Some(host) = env.get("EDCB_HOST") {
            config.host.clone_from(host);
        }
        if let Some(port) = env.get("EDCB_PORT") {
            config.port = parse_port(port)?;
        }
        if let Some(timeout) = env.get("EDCB_TIMEOUT_SECONDS") {
            config.timeout = Duration::from_secs(parse_timeout(timeout)?);
        }

        let args: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect();
        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--host" => {
                    index += 1;
                    config.host = args
                        .get(index)
                        .ok_or_else(|| "--host requires a value".to_string())?
                        .clone();
                }
                "--port" => {
                    index += 1;
                    config.port = parse_port(
                        args.get(index)
                            .ok_or_else(|| "--port requires a value".to_string())?,
                    )?;
                }
                "--timeout-seconds" => {
                    index += 1;
                    config.timeout = Duration::from_secs(parse_timeout(
                        args.get(index)
                            .ok_or_else(|| "--timeout-seconds requires a value".to_string())?,
                    )?);
                }
                unknown => return Err(format!("unknown argument {unknown}")),
            }
            index += 1;
        }

        Ok(config)
    }
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
pub struct PluginKindParam {
    pub kind: String,
}

impl PluginKindParam {
    pub fn try_into_plugin_kind(&self) -> Result<PluginKind, String> {
        match self.kind.as_str() {
            "write" => Ok(PluginKind::Write),
            "rec_name" => Ok(PluginKind::RecName),
            value => Err(format!(
                "unsupported plugin kind {value}; expected \"write\" or \"rec_name\""
            )),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsParam {
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
    pub date_ranges: Option<Vec<SearchProgramsDateParam>>,
    #[serde(default)]
    pub is_exclude_date_ranges: bool,
    pub duration_range_min: Option<u16>,
    pub duration_range_max: Option<u16>,
    #[serde(default)]
    pub broadcast_type: BroadcastType,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchProgramsServiceParam {
    pub network_id: u16,
    pub transport_stream_id: u16,
    pub service_id: u16,
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
                .map(|service| ServiceKey {
                    onid: service.network_id,
                    tsid: service.transport_stream_id,
                    sid: service.service_id,
                })
                .collect()
        });
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
        Ok(ProgramSearchQuery {
            keyword: self.keyword.clone(),
            exclude_keyword: self.exclude_keyword.clone(),
            title_only: self.is_title_only,
            case_sensitive: self.is_case_sensitive,
            regex: self.is_regex_search_enabled,
            fuzzy: self.is_fuzzy_search_enabled,
            service_ranges,
            date_ranges,
            exclude_date_ranges: self.is_exclude_date_ranges,
            duration_min: self.duration_range_min,
            duration_max: self.duration_range_max,
            broadcast_type: self.broadcast_type,
        })
    }
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

#[derive(Debug, Clone)]
pub struct EdcbMcpServer {
    config: ServerConfig,
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
    pub fn new(config: ServerConfig) -> Self {
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
        let mut client = EdcbClient::new(self.config.host.clone(), self.config.port);
        client.set_timeout(self.config.timeout);
        client
    }
}

fn to_call_tool_result<T: Serialize>(result: crate::Result<T>) -> Result<CallToolResult, String> {
    let value = result.map_err(|error| error.to_string())?;
    serde_json::to_value(value)
        .map(CallToolResult::structured)
        .map_err(|error| format!("failed to serialize tool response: {error}"))
}
